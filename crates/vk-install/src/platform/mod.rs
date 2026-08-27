//! Integrazione con il sistema operativo.
//!
//! È l'unico punto del crate in cui compaiono registro di Windows, file
//! `.desktop` e bundle `.app`. Il resto del motore parla solo di percorsi e di
//! [`Artifact`](crate::record::Artifact): è quello che rende la stessa
//! procedura d'installazione valida su tre sistemi operativi che non hanno
//! nulla in comune su questo terreno.
//!
//! Ogni piattaforma espone le stesse funzioni:
//!
//! | Funzione | Windows | macOS | Linux |
//! | --- | --- | --- | --- |
//! | `create_shortcuts` | `.lnk` su desktop, menu Start, avvio veloce | alias sul desktop | `.desktop` in `~/.local/share/applications` |
//! | `register_uninstall` | chiave `Uninstall` in HKCU | registro su file | registro su file |
//! | `schedule_removal` | script differito, l'exe è bloccato | rimozione immediata | rimozione immediata |

use std::path::{Path, PathBuf};

use crate::error::InstallResult;
use crate::record::{Artifact, ArtifactKind};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(all(unix, not(target_os = "macos")))]
pub use linux::*;

/// Cosa creare, e per cosa.
#[derive(Debug, Clone)]
pub struct ShortcutRequest<'a> {
    /// Eseguibile del launcher (su macOS il bundle `.app`).
    pub executable: &'a Path,
    /// Cartella di lavoro: la cartella d'installazione.
    pub working_dir: &'a Path,
    /// Disinstallatore già copiato, se c'è.
    pub uninstaller: Option<&'a Path>,
    /// Icona da installare nel tema, se l'installer ne porta una con sé.
    pub icon: Option<&'a Path>,
    pub desktop: bool,
    pub start_menu: bool,
    pub quick_launch: bool,
    /// Voce "Disinstalla" accanto a quella del launcher.
    pub uninstall_entry: bool,
    /// Collegamento in una cartella del `PATH` (solo Linux).
    pub path_symlink: bool,
}

/// Dati della registrazione fra i programmi installati.
#[derive(Debug, Clone)]
pub struct UninstallRegistration<'a> {
    pub install_dir: &'a Path,
    pub executable: &'a Path,
    pub uninstaller: Option<&'a Path>,
    pub version: &'a str,
    pub size_bytes: u64,
}

/// Percorso dell'installer così come l'utente lo ha avviato.
///
/// Non è sempre `current_exe()`: dentro un bundle `.app` quello è il binario
/// in `Contents/MacOS`, e copiare *quello* non produrrebbe un'applicazione
/// avviabile; dentro un AppImage è il binario estratto nella cartella
/// temporanea di montaggio, che sparisce all'uscita.
pub fn self_bundle_path() -> InstallResult<PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        if appimage.exists() {
            return Ok(appimage);
        }
    }

    let executable =
        std::env::current_exe().map_err(|error| crate::error::InstallError::io(".", error))?;

    #[cfg(target_os = "macos")]
    {
        // …/VanzaKart Setup.app/Contents/MacOS/vanzakart-setup → …/VanzaKart Setup.app
        let mut current = executable.as_path();
        while let Some(parent) = current.parent() {
            if parent.extension().is_some_and(|ext| ext == "app") {
                return Ok(parent.to_path_buf());
            }
            current = parent;
        }
    }

    Ok(executable)
}

/// Rimuove ciò che l'installer aveva creato. Restituisce quanti elementi sono
/// spariti davvero.
pub fn remove_artifacts(artifacts: &[Artifact]) -> usize {
    artifacts
        .iter()
        .filter(|artifact| artifact.kind != ArtifactKind::Record)
        .filter(|artifact| remove_artifact(artifact))
        .count()
}

/// Rimuove una cartella rimasta vuota dopo la cancellazione di una
/// scorciatoia, e solo se vuota.
pub(crate) fn remove_parent_if_empty(path: &Path) {
    let Some(parent) = path.parent() else { return };
    if crate::fsops::is_dir_empty(parent) {
        crate::fsops::remove_path_best_effort(parent);
    }
}

/// `true` se l'eseguibile a quel percorso è in esecuzione.
///
/// Su Windows un file aperto non si sovrascrive: installare sopra un launcher
/// aperto fallirebbe a metà estrazione, con la cartella già mezza sostituita.
/// Meglio dirlo prima di cominciare.
///
/// Si confronta il **percorso**, non il nome: `VanzaKart Launcher.exe` può
/// essere anche l'installazione di qualcun altro, e su Linux il nome che il
/// sistema espone è troncato (vedi [`matches_process_name`]).
pub fn is_running(executable: &Path) -> bool {
    let Some(file_name) = executable
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return false;
    };
    if file_name.trim().is_empty() {
        return false;
    }
    let stem = executable
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.clone());
    let target = std::fs::canonicalize(executable).ok();

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system.processes().values().any(|process| {
        if let (Some(target), Some(path)) = (target.as_deref(), process.exe()) {
            // `starts_with` copre i bundle di macOS, dove il processo vive in
            // `Contents/MacOS/` dentro l'applicazione che si sta cercando.
            let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if resolved == target || resolved.starts_with(target) {
                return true;
            }
        }

        let name = process.name().to_string_lossy().to_string();
        matches_process_name(&name, &file_name) || matches_process_name(&name, &stem)
    })
}

/// Confronta il nome di un processo con quello atteso.
///
/// Su Linux il nome arriva da `/proc/<pid>/comm`, che il kernel **tronca a 15
/// caratteri**: `vanzakart-launcher.AppImage` si presenta come
/// `vanzakart-launc`, e un confronto per intero non lo riconoscerebbe mai.
fn matches_process_name(process_name: &str, expected: &str) -> bool {
    /// Lunghezza di `/proc/<pid>/comm`, senza il terminatore.
    const COMM_LIMIT: usize = 15;

    if process_name.eq_ignore_ascii_case(expected) {
        return true;
    }

    process_name.len() == COMM_LIMIT
        && expected.len() > COMM_LIMIT
        && expected.is_char_boundary(COMM_LIMIT)
        && expected[..COMM_LIMIT].eq_ignore_ascii_case(process_name)
}

/// Nome della cartella creata nel menu applicazioni.
pub const MENU_FOLDER_NAME: &str = "VanzaKart";

/// Descrizione mostrata nelle scorciatoie, come nel legacy.
pub const SHORTCUT_DESCRIPTION: &str = "VanzaKart Modpack Launcher";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installer_can_locate_itself() {
        let path = self_bundle_path().expect("percorso");
        assert!(path.is_absolute());
        assert!(path.exists());
    }

    #[test]
    fn a_truncated_process_name_is_still_recognised() {
        // Su Linux il kernel tiene 15 caratteri di `comm`: senza questa
        // regola l'installer non si accorgerebbe mai di un launcher aperto.
        assert!(matches_process_name(
            "vanzakart-launc",
            "vanzakart-launcher.AppImage"
        ));
        assert!(matches_process_name(
            "VanzaKart Launcher.exe",
            "vanzakart launcher.exe"
        ));
        assert!(!matches_process_name("vanzakart-launc", "vanzakart-lau"));
        assert!(!matches_process_name(
            "altro-programma",
            "vanzakart-launcher.AppImage"
        ));
    }

    #[test]
    fn the_running_installer_is_seen_as_running() {
        let current = std::env::current_exe().expect("eseguibile");
        assert!(is_running(&current));
        assert!(!is_running(Path::new("questo-non-esiste-davvero-12345")));
    }

    #[test]
    fn the_record_artifact_is_not_removed_with_the_others() {
        // Il registro va cancellato per ultimo, dal chiamante: se sparisse
        // qui, un errore a metà rimozione lascerebbe un'installazione senza
        // più traccia di cosa resta da togliere.
        let artifacts = vec![Artifact::new(ArtifactKind::Record, "install.json")];
        assert_eq!(remove_artifacts(&artifacts), 0);
    }
}
