//! Backup e ripristino dei dati utente prima di un aggiornamento.
//!
//! Porta `CreateBackupAsync` / `RestoreBackupAsync` / `MigrateUserDataAsync` di
//! `ModUpdateSafetyService`, mantenendo il layout su disco
//! (`Backups/ModUpdates/<id>/files/…` + `manifest.json`) così che un backup
//! creato dal launcher legacy resti leggibile e viceversa.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::fsx;
use crate::hash::{hash_eq, sha256_file};
use crate::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
use crate::protect::{is_ignored_system_file, ModLayout};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFile {
    #[serde(rename = "RelativePath")]
    pub relative_path: String,
    #[serde(rename = "BackupPath")]
    pub backup_path: String,
    #[serde(rename = "Sha256")]
    pub sha256: String,
    #[serde(rename = "SizeBytes")]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSet {
    #[serde(rename = "BackupId")]
    pub backup_id: String,
    #[serde(rename = "BackupFolder")]
    pub backup_folder: String,
    #[serde(rename = "ModRoot")]
    pub mod_root: String,
    #[serde(rename = "UserDataRoot")]
    pub user_data_root: String,
    #[serde(rename = "Files", default)]
    pub files: Vec<BackupFile>,
}

impl BackupSet {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn folder(&self) -> PathBuf {
        PathBuf::from(&self.backup_folder)
    }
}

/// Crea un backup dei dati utente e ne rispecchia una copia in
/// `<Mod>_UserData`, come il launcher legacy.
pub async fn create_backup(
    layout: &ModLayout,
    backup_root: &Path,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<BackupSet> {
    let mod_root = layout.mod_root();
    let user_data_root = layout.user_data_root();
    let backup_id = fsx::backup_timestamp();
    let backup_folder = backup_root.join(&backup_id);

    tokio::fs::create_dir_all(&backup_folder)
        .await
        .map_err(|e| CoreError::io(&backup_folder, e))?;
    tokio::fs::create_dir_all(&user_data_root)
        .await
        .map_err(|e| CoreError::io(&user_data_root, e))?;

    let mut set = BackupSet {
        backup_id: backup_id.clone(),
        backup_folder: backup_folder.to_string_lossy().to_string(),
        mod_root: mod_root.to_string_lossy().to_string(),
        user_data_root: user_data_root.to_string_lossy().to_string(),
        files: Vec::new(),
    };

    if !mod_root.is_dir() {
        return Ok(set);
    }

    let candidates = fsx::list_user_data_files(&mod_root);
    let total = candidates.len() as u32;

    for (index, source) in candidates.iter().enumerate() {
        cancel.check()?;

        let relative = fsx::relative_slash(&mod_root, source);
        progress(
            ProgressUpdate::new(Phase::Backup, format!("Saving {relative}"))
                .with_files(index as u32 + 1, total),
        );

        let backup_path = backup_folder.join("files").join(&relative);
        let size = fsx::copy_file(source, &backup_path).await?;

        // Copia speculare in <Mod>_UserData, come nel legacy.
        fsx::copy_file(source, &user_data_root.join(&relative)).await?;

        set.files.push(BackupFile {
            relative_path: relative,
            backup_path: backup_path.to_string_lossy().to_string(),
            sha256: sha256_file(source).await?,
            size_bytes: size,
        });
    }

    fsx::write_json_atomic(&backup_folder.join("manifest.json"), &set).await?;
    Ok(set)
}

/// Ripristina un backup e ne verifica ogni file per hash.
///
/// Un fallimento di verifica è un errore: il chiamante deve mostrare il
/// percorso del backup senza cancellare nulla.
pub async fn restore_backup(
    set: &BackupSet,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<u32> {
    if set.files.is_empty() {
        return Ok(0);
    }

    let mod_root = PathBuf::from(&set.mod_root);
    tokio::fs::create_dir_all(&mod_root)
        .await
        .map_err(|e| CoreError::io(&mod_root, e))?;

    let total = set.files.len() as u32;
    let mut restored = 0u32;

    for file in &set.files {
        cancel.check()?;

        if is_ignored_system_file(&file.relative_path) {
            continue;
        }

        progress(
            ProgressUpdate::new(Phase::Rollback, format!("Restoring {}", file.relative_path))
                .with_files(restored + 1, total),
        );

        fsx::copy_file(
            Path::new(&file.backup_path),
            &mod_root.join(&file.relative_path),
        )
        .await?;
        restored += 1;
    }

    verify_restore(set, &mod_root).await?;
    Ok(restored)
}

async fn verify_restore(set: &BackupSet, mod_root: &Path) -> CoreResult<()> {
    for file in &set.files {
        if is_ignored_system_file(&file.relative_path) {
            continue;
        }

        let destination = mod_root.join(&file.relative_path);
        if !destination.is_file() {
            return Err(CoreError::RestoreFailed(format!(
                "{} is missing",
                file.relative_path
            )));
        }

        let actual = sha256_file(&destination).await?;
        if !hash_eq(&actual, &file.sha256) {
            return Err(CoreError::RestoreFailed(format!(
                "the hash of {} does not match",
                file.relative_path
            )));
        }
    }
    Ok(())
}

/// Copia i dati utente in `<Mod>_UserData` senza creare un backup datato.
///
/// Equivalente di `MigrateUserDataAsync`.
pub async fn mirror_user_data(layout: &ModLayout, cancel: &CancelToken) -> CoreResult<u32> {
    let mod_root = layout.mod_root();
    if !mod_root.is_dir() {
        return Ok(0);
    }

    let user_data_root = layout.user_data_root();
    tokio::fs::create_dir_all(&user_data_root)
        .await
        .map_err(|e| CoreError::io(&user_data_root, e))?;

    let mut migrated = 0u32;
    for source in fsx::list_user_data_files(&mod_root) {
        cancel.check()?;
        let relative = fsx::relative_slash(&mod_root, &source);
        fsx::copy_file(&source, &user_data_root.join(&relative)).await?;
        migrated += 1;
    }

    Ok(migrated)
}

/// Elenca i backup presenti, dal più recente.
pub fn list_backups(backup_root: &Path) -> Vec<BackupSummary> {
    let Ok(entries) = std::fs::read_dir(backup_root) else {
        return Vec::new();
    };

    let mut out: Vec<BackupSummary> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| {
            let path = entry.path();
            let file_count = std::fs::read_to_string(path.join("manifest.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<BackupSet>(&raw).ok())
                .map(|set| set.files.len() as u32)
                .unwrap_or(0);

            BackupSummary {
                id: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                file_count,
            }
        })
        .collect();

    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSummary {
    pub id: String,
    pub path: String,
    pub file_count: u32,
}

/// Rilegge un `BackupSet` dal suo `manifest.json`.
pub async fn load_backup(folder: &Path) -> CoreResult<BackupSet> {
    let manifest = folder.join("manifest.json");
    let raw = fsx::read_text_opt(&manifest).await?.ok_or_else(|| {
        CoreError::RestoreFailed(format!("manifest assente in {}", folder.display()))
    })?;
    Ok(serde_json::from_str(&raw)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::noop_sink;
    use crate::versions::Channel;

    fn seed_installation(root: &Path) -> ModLayout {
        let layout = ModLayout::new(root, Channel::Stable);
        let mod_root = layout.mod_root();

        std::fs::create_dir_all(mod_root.join("Riivolution")).unwrap();
        std::fs::create_dir_all(mod_root.join("Saves")).unwrap();
        std::fs::create_dir_all(layout.my_stuff()).unwrap();

        std::fs::write(mod_root.join("Riivolution/VanzaKart.xml"), b"<xml/>").unwrap();
        std::fs::write(mod_root.join("Saves/rksys.dat"), b"licenza-utente").unwrap();
        std::fs::write(layout.my_stuff().join("custom.szs"), b"mod-utente").unwrap();
        std::fs::write(mod_root.join("desktop.ini"), b"[.]").unwrap();

        layout
    }

    #[tokio::test]
    async fn backup_captures_only_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let layout = seed_installation(dir.path());
        let backup_root = dir.path().join("Backups/ModUpdates");

        let set = create_backup(&layout, &backup_root, &noop_sink(), &CancelToken::new())
            .await
            .unwrap();

        let paths: Vec<&str> = set.files.iter().map(|f| f.relative_path.as_str()).collect();
        assert_eq!(set.files.len(), 2, "trovati: {paths:?}");
        assert!(paths.contains(&"Saves/rksys.dat"));
        assert!(paths.iter().any(|p| p.ends_with("custom.szs")));
        assert!(!paths.iter().any(|p| p.contains("VanzaKart.xml")));
        assert!(!paths.iter().any(|p| p.contains("desktop.ini")));

        // Copia speculare in <Mod>_UserData.
        assert!(layout.user_data_root().join("Saves/rksys.dat").is_file());
        // Manifest rileggibile.
        let reloaded = load_backup(&set.folder()).await.unwrap();
        assert_eq!(reloaded, set);
    }

    #[tokio::test]
    async fn restore_puts_user_data_back() {
        let dir = tempfile::tempdir().unwrap();
        let layout = seed_installation(dir.path());
        let backup_root = dir.path().join("Backups/ModUpdates");

        let set = create_backup(&layout, &backup_root, &noop_sink(), &CancelToken::new())
            .await
            .unwrap();

        // Un update distruttivo cancella i dati utente.
        std::fs::remove_file(layout.mod_root().join("Saves/rksys.dat")).unwrap();
        std::fs::write(layout.my_stuff().join("custom.szs"), b"sovrascritto").unwrap();

        let restored = restore_backup(&set, &noop_sink(), &CancelToken::new())
            .await
            .unwrap();

        assert_eq!(restored, 2);
        assert_eq!(
            std::fs::read(layout.mod_root().join("Saves/rksys.dat")).unwrap(),
            b"licenza-utente"
        );
        assert_eq!(
            std::fs::read(layout.my_stuff().join("custom.szs")).unwrap(),
            b"mod-utente"
        );
    }

    #[tokio::test]
    async fn restore_fails_when_a_backup_file_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let layout = seed_installation(dir.path());
        let backup_root = dir.path().join("Backups/ModUpdates");

        let set = create_backup(&layout, &backup_root, &noop_sink(), &CancelToken::new())
            .await
            .unwrap();

        // Il file nel backup viene alterato dopo la creazione del manifest.
        let target = set
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("rksys.dat"))
            .unwrap();
        std::fs::write(&target.backup_path, b"danneggiato").unwrap();

        let error = restore_backup(&set, &noop_sink(), &CancelToken::new())
            .await
            .unwrap_err();
        assert!(matches!(error, CoreError::RestoreFailed(_)));
    }

    #[tokio::test]
    async fn backup_of_a_missing_installation_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ModLayout::new(dir.path(), Channel::Beta);

        let set = create_backup(
            &layout,
            &dir.path().join("Backups"),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

        assert!(set.is_empty());
        assert_eq!(
            restore_backup(&set, &noop_sink(), &CancelToken::new())
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn mirrors_user_data_without_a_dated_backup() {
        let dir = tempfile::tempdir().unwrap();
        let layout = seed_installation(dir.path());

        let migrated = mirror_user_data(&layout, &CancelToken::new())
            .await
            .unwrap();

        assert_eq!(migrated, 2);
        assert!(layout.user_data_root().join("Saves/rksys.dat").is_file());
    }

    #[tokio::test]
    async fn lists_backups_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let backup_root = dir.path().join("Backups");
        for id in ["20240101_000000", "20250101_000000"] {
            std::fs::create_dir_all(backup_root.join(id)).unwrap();
        }

        let listed = list_backups(&backup_root);
        assert_eq!(listed[0].id, "20250101_000000");
        assert_eq!(listed[1].id, "20240101_000000");
    }
}
