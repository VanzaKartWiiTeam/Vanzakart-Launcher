//! Operazioni su filesystem con semantica atomica.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CoreError, CoreResult};
use crate::hash::sha256_file;
use crate::manifest::ModManifestFile;
use crate::progress::CancelToken;
use crate::protect::{is_ignored_system_file, is_protected_relative, ProtectionRules};

/// Scrive un file in modo atomico: file temporaneo nella stessa directory,
/// `fsync`, poi `rename`.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CoreError::UnsafePath(format!("{} non ha una directory padre", path.display()))
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| CoreError::io(parent, e))?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let temp = parent.join(format!(".{file_name}.{}.tmp", unique_suffix()));

    let result = async {
        let mut handle = tokio::fs::File::create(&temp)
            .await
            .map_err(|e| CoreError::io(&temp, e))?;
        use tokio::io::AsyncWriteExt;
        handle
            .write_all(bytes)
            .await
            .map_err(|e| CoreError::io(&temp, e))?;
        handle.flush().await.map_err(|e| CoreError::io(&temp, e))?;
        handle
            .sync_all()
            .await
            .map_err(|e| CoreError::io(&temp, e))?;
        drop(handle);

        crate::zipx::clear_blocking_attributes(path);
        tokio::fs::rename(&temp, path)
            .await
            .map_err(|e| CoreError::io(path, e))
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

/// Serializza in JSON indentato e scrive atomicamente.
pub async fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let json = serde_json::to_vec_pretty(value)?;
    write_atomic(path, &json).await
}

/// Legge un file di testo tollerando BOM. `None` se il file non esiste.
pub async fn read_text_opt(path: &Path) -> CoreResult<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(Some(crate::json::strip_leading_noise(&text).to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

/// Copia un file creando le directory intermedie e rimuovendo gli attributi
/// bloccanti sulla destinazione.
pub async fn copy_file(source: &Path, destination: &Path) -> CoreResult<u64> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| CoreError::io(parent, e))?;
    }
    crate::zipx::clear_blocking_attributes(destination);
    tokio::fs::copy(source, destination)
        .await
        .map_err(|e| CoreError::io(source, e))
}

/// Sposta un file, con fallback su copia + rimozione quando l'origine e la
/// destinazione sono su volumi diversi (staging in `%TEMP%`).
pub async fn move_file(source: &Path, destination: &Path) -> CoreResult<()> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| CoreError::io(parent, e))?;
    }
    crate::zipx::clear_blocking_attributes(destination);

    match tokio::fs::rename(source, destination).await {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_file(source, destination).await?;
            tokio::fs::remove_file(source)
                .await
                .map_err(|e| CoreError::io(source, e))
        }
    }
}

/// Elenca ricorsivamente i file sotto `root`, restituendo percorsi relativi
/// normalizzati con `/`.
pub fn list_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => out.push(path),
                _ => {}
            }
        }
    }

    out.sort();
    out
}

/// Percorso relativo normalizzato con `/`.
pub fn relative_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Scansiona l'installazione locale producendo un manifest dei file gestiti.
///
/// Equivalente di `ModUpdateSafetyService::ScanLocalFilesAsync`: esclude i file
/// di sistema e i dati utente protetti.
pub async fn scan_managed_files(
    mod_sub_folder: &Path,
    cancel: &CancelToken,
) -> CoreResult<Vec<ModManifestFile>> {
    if !mod_sub_folder.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for path in list_files_recursive(mod_sub_folder) {
        cancel.check()?;
        let relative = relative_slash(mod_sub_folder, &path);

        if is_ignored_system_file(&relative) || is_protected_relative(&relative) {
            continue;
        }

        let size = tokio::fs::metadata(&path)
            .await
            .map(|meta| meta.len() as i64)
            .unwrap_or(0);

        out.push(ModManifestFile {
            path: relative,
            sha256: sha256_file(&path).await?,
            size,
        });
    }

    Ok(out)
}

/// Elenca i file di dati utente sotto la root della modpack.
///
/// Equivalente di `EnumerateUserDataFiles`.
pub fn list_user_data_files(mod_root: &Path) -> Vec<PathBuf> {
    if !mod_root.is_dir() {
        return Vec::new();
    }

    list_files_recursive(mod_root)
        .into_iter()
        .filter(|path| {
            let relative = relative_slash(mod_root, path);
            !is_ignored_system_file(&relative) && is_protected_relative(&relative)
        })
        .collect()
}

/// Rimuove le directory vuote sotto `root`, saltando quelle protette.
///
/// Equivalente di `RemoveEmptyDirectories`: `CTBRSTM` è mantenuta anche se vuota
/// perché è una directory richiesta dalla modpack.
pub fn remove_empty_directories(root: &Path, rules: &ProtectionRules) -> usize {
    if !root.is_dir() {
        return 0;
    }

    let mut directories: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let path = entry.path();
                stack.push(path.clone());
                directories.push(path);
            }
        }
    }

    // Dal più profondo al più superficiale, come l'ordinamento per lunghezza
    // decrescente del legacy.
    directories.sort_by_key(|path| std::cmp::Reverse(path.as_os_str().len()));

    let mut removed = 0usize;
    for directory in directories {
        let name = directory
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.eq_ignore_ascii_case("CTBRSTM") {
            continue;
        }
        if rules.is_absolute_protected(&directory) {
            continue;
        }
        let relative = relative_slash(&rules.layout().mod_root(), &directory);
        if is_protected_relative(&relative) {
            continue;
        }
        if std::fs::read_dir(&directory).is_ok_and(|mut it| it.next().is_none())
            && std::fs::remove_dir(&directory).is_ok()
        {
            removed += 1;
        }
    }

    removed
}

fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{seq:x}")
}

/// Timestamp `yyyyMMdd_HHmmss` in ora locale, usato per gli id di backup.
///
/// Stesso formato del legacy (`DateTime.Now.ToString("yyyyMMdd_HHmmss")`).
pub fn backup_timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let format = time::macros::format_description!("[year][month][day]_[hour][minute][second]");
    now.format(&format)
        .unwrap_or_else(|_| "00000000_000000".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protect::ModLayout;
    use crate::versions::Channel;

    #[tokio::test]
    async fn atomic_write_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");

        write_atomic(&path, b"{\"a\":1}").await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[tokio::test]
    async fn atomic_write_overwrites_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomic(&path, b"old").await.unwrap();
        write_atomic(&path, b"new").await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[tokio::test]
    async fn reads_optional_text_and_strips_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        assert!(read_text_opt(&path).await.unwrap().is_none());

        std::fs::write(&path, "\u{FEFF}ciao").unwrap();
        assert_eq!(read_text_opt(&path).await.unwrap().as_deref(), Some("ciao"));
    }

    #[tokio::test]
    async fn moves_files_across_directories() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/a.bin");
        let destination = dir.path().join("dst/nested/a.bin");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"payload").unwrap();

        move_file(&source, &destination).await.unwrap();

        assert!(!source.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"payload");
    }

    #[tokio::test]
    async fn scan_skips_protected_and_system_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("VanzaKart");
        std::fs::create_dir_all(root.join("Riivolution")).unwrap();
        std::fs::create_dir_all(root.join("Saves")).unwrap();
        std::fs::write(root.join("Riivolution/VanzaKart.xml"), b"<xml/>").unwrap();
        std::fs::write(root.join("Saves/rksys.dat"), b"user").unwrap();
        std::fs::write(root.join("desktop.ini"), b"[.]").unwrap();

        let files = scan_managed_files(&root, &CancelToken::new())
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Riivolution/VanzaKart.xml");
        assert_eq!(files[0].size, 6);
        assert_eq!(files[0].sha256, crate::hash::sha256_bytes(b"<xml/>"));
    }

    #[test]
    fn lists_only_user_data_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("VanzaKart");
        std::fs::create_dir_all(root.join("VanzaKart/My Stuff")).unwrap();
        std::fs::create_dir_all(root.join("Riivolution")).unwrap();
        std::fs::write(root.join("VanzaKart/My Stuff/custom.szs"), b"mine").unwrap();
        std::fs::write(root.join("Riivolution/VanzaKart.xml"), b"<xml/>").unwrap();

        let files = list_user_data_files(&root);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("custom.szs"));
    }

    #[test]
    fn removes_empty_directories_but_keeps_ctbrstm_and_protected() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ModLayout::new(dir.path(), Channel::Stable);
        let root = layout.mod_root();

        std::fs::create_dir_all(root.join("CTBRSTM")).unwrap();
        std::fs::create_dir_all(root.join("EmptyStage")).unwrap();
        std::fs::create_dir_all(root.join("Deep/Nested/Empty")).unwrap();
        std::fs::create_dir_all(layout.my_stuff()).unwrap();
        std::fs::create_dir_all(root.join("Riivolution")).unwrap();
        std::fs::write(root.join("Riivolution/VanzaKart.xml"), b"<xml/>").unwrap();

        let rules = ProtectionRules::build(layout.clone());
        let removed = remove_empty_directories(&root, &rules);

        assert!(removed >= 3);
        assert!(root.join("CTBRSTM").is_dir());
        assert!(layout.my_stuff().is_dir());
        assert!(root.join("Riivolution").is_dir());
        assert!(!root.join("EmptyStage").exists());
        assert!(!root.join("Deep").exists());
    }

    #[test]
    fn backup_timestamps_have_the_legacy_shape() {
        let stamp = backup_timestamp();
        assert_eq!(stamp.len(), 15);
        assert_eq!(&stamp[8..9], "_");
        assert!(stamp.chars().filter(|c| c.is_ascii_digit()).count() == 14);
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        let root = Path::new("/a/b");
        assert_eq!(relative_slash(root, Path::new("/a/b/c/d.txt")), "c/d.txt");
    }
}
