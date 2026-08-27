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

/// `true` se un eseguibile con quel nome è in esecuzione.
///
/// Su Windows un file aperto non si sovrascrive: installare sopra un launcher
/// aperto fallirebbe a metà estrazione, con la cartella già mezza sostituita.
/// Meglio dirlo prima di cominciare.
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

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_string();
        name.eq_ignore_ascii_case(&file_name) || name.eq_ignore_ascii_case(&stem)
    })
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
