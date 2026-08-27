//! La disinstallazione.
//!
//! Il disinstallatore legacy chiedeva due sole cose ("cancello anche la
//! modpack?", "cancello anche i dati utente?") e per il resto tirava a
//! indovinare quali cartelle fossero sue. Qui si parte dal registro
//! ([`crate::record`]) e si mostra all'utente **l'elenco esatto** di ciò che
//! verrà rimosso, con le dimensioni, prima di toccare qualsiasi cosa.
//!
//! Regola di fondo, uguale al legacy: i dati dell'utente non spariscono se non
//! lo chiede. Modpack, salvataggi e impostazioni sopravvivono a una
//! disinstallazione, così reinstallare riporta tutto com'era.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vk_core::progress::{Phase, ProgressSink, ProgressUpdate};

use crate::error::InstallResult;
use crate::record::{ArtifactKind, InstallRecord};
use crate::{fsops, paths, platform};

/// Che cosa togliere oltre al programma.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UninstallOptions {
    /// Impostazioni, licenze importate, Mii del launcher: tutta la cartella
    /// dati.
    pub remove_launcher_data: bool,
    /// Solo cache, log e download a metà. Ignorato se si rimuove tutto.
    pub remove_cache_and_logs: bool,
    /// Le modpack installate in Dolphin (Stable e Beta).
    pub remove_modpacks: bool,
    /// I dati di gioco dentro le modpack (`*_UserData`): salvataggi,
    /// personalizzazioni, addon locali.
    pub remove_modpack_user_data: bool,
}

/// Una voce dell'elenco mostrato prima di procedere.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalItem {
    pub label: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// `false` quando fa parte del programma e viene tolto comunque.
    pub optional: bool,
    pub exists: bool,
}

/// Una rimozione non riuscita: si prosegue, e si dice quale.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedRemoval {
    pub path: PathBuf,
    pub reason: String,
}

/// Esito della disinstallazione.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub removed: Vec<PathBuf>,
    pub failed: Vec<FailedRemoval>,
    /// `true` quando la cartella d'installazione sparirà all'uscita del
    /// processo, perché il disinstallatore ci si trova dentro (Windows).
    pub deferred: bool,
    pub bytes_freed: u64,
}

/// Elenco di ciò che verrà rimosso, con le dimensioni.
pub fn plan(record: &InstallRecord, options: &UninstallOptions) -> Vec<RemovalItem> {
    let mut items = Vec::new();

    for path in record.installed_paths() {
        items.push(item("Programma installato", path, false));
    }

    for artifact in &record.artifacts {
        if artifact.kind == ArtifactKind::RegistryKey {
            continue;
        }
        items.push(item(
            artifact.kind.label(),
            PathBuf::from(&artifact.path),
            false,
        ));
    }

    if let Some(data_root) = paths::launcher_data_root() {
        if options.remove_launcher_data {
            items.push(item("Impostazioni e dati del launcher", data_root, true));
        } else if options.remove_cache_and_logs {
            for (label, name) in [
                ("Cache", "cache"),
                ("Log", "logs"),
                ("Download interrotti", "downloads"),
            ] {
                items.push(item(label, data_root.join(name), true));
            }
        }
    }

    if options.remove_modpacks {
        for (label, path) in modpack_paths(false) {
            items.push(item(label, path, true));
        }
    }

    if options.remove_modpack_user_data {
        for (label, path) in modpack_paths(true) {
            items.push(item(label, path, true));
        }
    }

    items.retain(|entry| entry.exists);
    items
}

/// Esegue la rimozione.
pub fn run(
    record: &InstallRecord,
    options: &UninstallOptions,
    progress: &ProgressSink,
) -> InstallResult<UninstallReport> {
    let mut report = UninstallReport {
        removed: Vec::new(),
        failed: Vec::new(),
        deferred: false,
        bytes_freed: 0,
    };

    progress(ProgressUpdate::new(
        Phase::Updating,
        "Rimozione delle scorciatoie",
    ));
    platform::remove_artifacts(&record.artifacts);
    platform::unregister_uninstall(record.executable.file_name().and_then(|name| name.to_str()));

    // Prima i dati opzionali, poi il programma: se qualcosa va storto sui
    // dati, l'utente ha ancora un'installazione funzionante con cui riprovare.
    let optional: Vec<RemovalItem> = plan(record, options)
        .into_iter()
        .filter(|entry| entry.optional)
        .collect();

    for entry in optional {
        progress(ProgressUpdate::new(
            Phase::Updating,
            format!("Rimozione: {}", entry.label),
        ));
        remove_into(&entry.path, entry.bytes, &mut report);
    }

    progress(ProgressUpdate::new(
        Phase::Updating,
        "Rimozione del programma",
    ));
    let installed = record.installed_paths();
    let installed_bytes: u64 = installed.iter().map(|path| fsops::path_size(path)).sum();

    if installed.is_empty() {
        // Nessun percorso valido nel registro: non si tira a indovinare quale
        // cartella cancellare (§D-055).
        tracing::warn!("il registro non indica nessun programma da rimuovere");
    } else if contains_self(&installed) {
        // Su Windows l'eseguibile in esecuzione blocca la cartella: la
        // rimozione avviene dopo l'uscita. Altrove sparisce subito.
        match platform::schedule_removal(&installed) {
            // `false` vuol dire che non è stato rimandato niente: allora la
            // rimozione la si fa adesso e si conta solo ciò che sparisce
            // davvero. Contare a priori faceva dire "1 elemento rimosso" a
            // una disinstallazione che non aveva toccato niente.
            Ok(false) => {
                for path in &installed {
                    remove_into(path, fsops::path_size(path), &mut report);
                }
            }
            Ok(true) => {
                report.deferred = true;
                report.removed.extend(installed.iter().cloned());
                report.bytes_freed += installed_bytes;
            }
            Err(error) => report.failed.push(FailedRemoval {
                path: record.install_dir.clone(),
                reason: error.to_string(),
            }),
        }
    } else {
        for path in &installed {
            remove_into(path, fsops::path_size(path), &mut report);
        }
    }

    record.forget();

    progress(
        ProgressUpdate::new(Phase::Completed, "Disinstallazione completata").with_percent(100.0),
    );
    Ok(report)
}

fn remove_into(path: &Path, bytes: u64, report: &mut UninstallReport) {
    if std::fs::symlink_metadata(path).is_err() {
        return;
    }
    match fsops::remove_path(path) {
        Ok(()) => {
            report.removed.push(path.to_path_buf());
            report.bytes_freed += bytes;
        }
        Err(error) => report.failed.push(FailedRemoval {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }),
    }
}

fn item(label: &str, path: PathBuf, optional: bool) -> RemovalItem {
    let exists = std::fs::symlink_metadata(&path).is_ok();
    RemovalItem {
        label: label.to_string(),
        bytes: if exists { fsops::path_size(&path) } else { 0 },
        path,
        optional,
        exists,
    }
}

/// `true` se il disinstallatore in esecuzione si trova dentro uno dei
/// percorsi da rimuovere.
fn contains_self(paths: &[PathBuf]) -> bool {
    let Ok(current) = platform::self_bundle_path() else {
        return false;
    };
    let current = std::fs::canonicalize(&current).unwrap_or(current);
    paths
        .iter()
        // Un percorso vuoto o relativo è prefisso di tutto: non deve mai
        // rispondere "sì, sono qui dentro".
        .filter(|path| path.has_root() && path.exists())
        .any(|path| {
            let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            current.starts_with(&path)
        })
}

/// Le modpack installate in Dolphin, con l'etichetta da mostrare.
///
/// `user_data` sceglie fra le cartelle del gioco (`VanzaKart`, `VKBeta`) e
/// quelle dei dati dell'utente (`VanzaKart_UserData`, `VKBeta_UserData`), che
/// il launcher tiene separate proprio perché un aggiornamento o una
/// disinstallazione non le porti via.
pub fn modpack_paths(user_data: bool) -> Vec<(&'static str, PathBuf)> {
    let Some(riivolution) = riivolution_folder() else {
        return Vec::new();
    };

    if user_data {
        return vec![
            (
                "Dati di gioco della modpack (Stable)",
                riivolution.join("VanzaKart_UserData"),
            ),
            (
                "Dati di gioco della modpack (Beta)",
                riivolution.join("VKBeta_UserData"),
            ),
        ];
    }

    vec![
        ("Modpack VanzaKart (Stable)", riivolution.join("VanzaKart")),
        ("Modpack VanzaKart (Beta)", riivolution.join("VKBeta")),
    ]
}

/// `<cartella User di Dolphin>/Load/Riivolution`.
fn riivolution_folder() -> Option<PathBuf> {
    let user_folder = dolphin_user_folder()?;
    Some(user_folder.join("Load").join("Riivolution"))
}

/// Cartella User di Dolphin, letta dalle impostazioni del launcher.
///
/// Si guardano sia le impostazioni nuove sia quelle del launcher legacy in C#:
/// chi disinstalla può avere installato la modpack con l'uno o con l'altro.
pub fn dolphin_user_folder() -> Option<PathBuf> {
    let candidates = [
        paths::launcher_data_root().map(|root| root.join("settings.json")),
        dirs::data_local_dir().map(|local| {
            local
                .join("VanzaKart")
                .join("Launcher")
                .join("launcher_settings.json")
        }),
    ];

    for path in candidates.into_iter().flatten() {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) =
            serde_json::from_str::<serde_json::Value>(vk_core::json::strip_leading_noise(&raw))
        else {
            continue;
        };

        // `user_folder_path` è la chiave nuova, `UserFolderPath` quella del
        // launcher legacy.
        let folder = value
            .get("user_folder_path")
            .or_else(|| value.get("UserFolderPath"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(folder) = folder {
            return Some(PathBuf::from(folder));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Artifact;

    fn installed_record(temp: &Path) -> InstallRecord {
        let install_dir = temp.join("app").join("VanzaKart Launcher");
        std::fs::create_dir_all(&install_dir).expect("mkdir");
        std::fs::write(install_dir.join("launcher.bin"), b"MZMZMZ").expect("scritto");

        let mut record = InstallRecord::new("2.0.0", "test", install_dir.clone());
        record.executable = install_dir.join("launcher.bin");
        record.payload = vec![PathBuf::from("launcher.bin")];
        record
    }

    #[test]
    fn the_plan_lists_the_program_even_with_every_option_off() {
        let temp = tempfile::tempdir().expect("temp");
        let record = installed_record(temp.path());

        let items = plan(&record, &UninstallOptions::default());
        assert_eq!(items.len(), 1);
        assert!(!items[0].optional);
        assert_eq!(items[0].bytes, 6);
    }

    #[test]
    fn the_plan_never_lists_something_that_is_not_there() {
        let temp = tempfile::tempdir().expect("temp");
        let mut record = installed_record(temp.path());
        record.add_artifact(Artifact::file(
            ArtifactKind::DesktopShortcut,
            &temp.path().join("mai-creato.lnk"),
        ));

        let items = plan(&record, &UninstallOptions::default());
        assert!(items.iter().all(|entry| entry.exists));
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_shortcut_that_exists_is_listed_and_removed() {
        let temp = tempfile::tempdir().expect("temp");
        let mut record = installed_record(temp.path());
        let shortcut = temp.path().join("collegamenti").join("VanzaKart.lnk");
        std::fs::create_dir_all(shortcut.parent().expect("parent")).expect("mkdir");
        std::fs::write(&shortcut, b"lnk").expect("scritto");
        record.add_artifact(Artifact::file(ArtifactKind::DesktopShortcut, &shortcut));

        assert_eq!(plan(&record, &UninstallOptions::default()).len(), 2);

        let report = run(
            &record,
            &UninstallOptions::default(),
            &vk_core::progress::noop_sink(),
        )
        .expect("disinstallato");

        assert!(!shortcut.exists());
        assert!(!record.install_dir.exists());
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(report.bytes_freed >= 6);
    }

    #[test]
    fn optional_data_is_left_alone_by_default() {
        let temp = tempfile::tempdir().expect("temp");
        let record = installed_record(temp.path());
        let options = UninstallOptions::default();

        assert!(!options.remove_launcher_data);
        assert!(plan(&record, &options).iter().all(|entry| !entry.optional));
    }

    #[test]
    fn a_missing_settings_file_means_no_modpack_to_remove() {
        // Su una macchina senza launcher configurato non si deve dedurre
        // nessun percorso: cancellare a caso in Documenti sarebbe peggio che
        // lasciare qualcosa indietro.
        if dolphin_user_folder().is_none() {
            assert!(modpack_paths(false).is_empty());
            assert!(modpack_paths(true).is_empty());
        }
    }

    #[test]
    fn the_uninstaller_notices_when_it_is_inside_what_it_removes() {
        let temp = tempfile::tempdir().expect("temp");
        assert!(!contains_self(&[temp.path().to_path_buf()]));

        let current = platform::self_bundle_path().expect("percorso");
        let parent = current.parent().expect("cartella").to_path_buf();
        assert!(contains_self(&[parent]));
    }

    #[test]
    fn an_empty_path_is_never_where_the_uninstaller_lives() {
        assert!(!contains_self(&[PathBuf::new()]));
        assert!(!contains_self(&[PathBuf::from("relativo/qualsiasi")]));
    }

    #[test]
    fn a_record_that_points_nowhere_removes_nothing() {
        // Registro senza cartella: la disinstallazione non deve dichiarare
        // rimozioni che non ha fatto (§D-055).
        let record = InstallRecord::default();
        let report = run(
            &record,
            &UninstallOptions::default(),
            &vk_core::progress::noop_sink(),
        )
        .expect("nessun errore");

        assert!(report.removed.is_empty());
        assert!(report.failed.is_empty());
        assert!(!report.deferred);
        assert_eq!(report.bytes_freed, 0);
    }

    #[test]
    fn removing_something_twice_is_not_a_failure() {
        let temp = tempfile::tempdir().expect("temp");
        let mut report = UninstallReport {
            removed: Vec::new(),
            failed: Vec::new(),
            deferred: false,
            bytes_freed: 0,
        };
        let missing = temp.path().join("mai-esistito");
        remove_into(&missing, 0, &mut report);
        assert!(report.removed.is_empty());
        assert!(report.failed.is_empty());
    }
}
