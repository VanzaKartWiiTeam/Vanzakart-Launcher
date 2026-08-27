//! Integrazione con macOS.
//!
//! Su macOS un'applicazione **è** una cartella: installare significa mettere
//! `VanzaKart Launcher.app` in una cartella Applicazioni, e disinstallare
//! significa toglierla. Non c'è un registro dei programmi installati da
//! aggiornare — il registro dell'installazione (`install.json`) fa quel
//! lavoro — e Launchpad indicizza da sé le cartelle Applicazioni.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{ShortcutRequest, UninstallRegistration};
use crate::error::{InstallError, InstallResult};
use crate::record::{Artifact, ArtifactKind};

pub fn create_shortcuts(request: &ShortcutRequest) -> Vec<Artifact> {
    let mut artifacts = Vec::new();

    if request.desktop {
        if let Some(desktop) = dirs::desktop_dir() {
            let link = desktop.join(file_name(request.executable));
            if symlink(request.executable, &link).is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::DesktopShortcut, &link));
            }
        }
    }

    // L'equivalente del menu Start è la cartella Applicazioni: se l'utente ha
    // installato altrove, ci si mette un collegamento perché Launchpad e
    // Spotlight trovino comunque il launcher.
    if request.start_menu && !crate::paths::is_applications_dir(request.working_dir) {
        if let Some(applications) = dirs::home_dir().map(|home| home.join("Applications")) {
            if crate::fsops::ensure_dir(&applications).is_ok() {
                let link = applications.join(file_name(request.executable));
                if symlink(request.executable, &link).is_ok() {
                    artifacts.push(Artifact::file(ArtifactKind::StartMenuShortcut, &link));
                }
            }
        }
    }

    artifacts
}

pub fn remove_artifact(artifact: &Artifact) -> bool {
    let path = PathBuf::from(&artifact.path);
    let removed = crate::fsops::remove_path_best_effort(&path);
    super::remove_parent_if_empty(&path);
    removed
}

/// Su macOS non c'è nulla da registrare: il registro d'installazione basta.
pub fn register_uninstall(_registration: &UninstallRegistration) -> InstallResult<Vec<Artifact>> {
    Ok(Vec::new())
}

pub fn unregister_uninstall(_executable_name: Option<&str>) -> bool {
    false
}

/// Su macOS l'installazione precedente si trova solo dal registro.
pub fn registered_install_dir() -> Option<PathBuf> {
    None
}

/// Il launcher legacy in C# era solo per Windows: non c'è nessuna
/// installazione vecchia da riconoscere.
pub fn legacy_install_dir() -> Option<PathBuf> {
    None
}

pub fn registered_version() -> Option<String> {
    None
}

/// Apre il launcher. `open` stacca il processo da sé: chiudere l'installer
/// subito dopo non chiude anche il launcher.
pub fn launch_detached(executable: &Path) -> InstallResult<()> {
    if executable.extension().is_some_and(|ext| ext == "app") {
        return Command::new("/usr/bin/open")
            .arg(executable)
            .status()
            .map(|_| ())
            .map_err(|error| InstallError::io(executable, error));
    }

    Command::new(executable)
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::io(executable, error))
}

/// Su macOS un binario in esecuzione si può cancellare: il file scompare dal
/// filesystem e il processo continua a girare sull'inode già aperto. Non
/// serve rimandare nulla a dopo l'uscita.
pub fn schedule_removal(paths: &[PathBuf]) -> InstallResult<bool> {
    for path in paths {
        crate::fsops::remove_path(path)?;
    }
    Ok(false)
}

/// Toglie l'attributo di quarantena da un bundle appena installato.
///
/// L'installer scarica il pacchetto da sé, quindi la quarantena di solito non
/// c'è: se però l'utente ha aperto l'installer scaricato dal browser, il
/// bundle estratto può ereditarla e Gatekeeper mostrerebbe "impossibile
/// aprire". È un tentativo, non un requisito.
pub fn clear_quarantine(path: &Path) {
    let _ = Command::new("/usr/bin/xattr")
        .arg("-dr")
        .arg("com.apple.quarantine")
        .arg(path)
        .status();
}

fn symlink(target: &Path, link: &Path) -> InstallResult<()> {
    crate::fsops::remove_path_best_effort(link);
    if let Some(parent) = link.parent() {
        crate::fsops::ensure_dir(parent)?;
    }
    std::os::unix::fs::symlink(target, link).map_err(|error| InstallError::io(link, error))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| crate::PRODUCT_NAME.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_link_points_at_the_bundle() {
        let temp = tempfile::tempdir().expect("temp");
        let bundle = temp.path().join("VanzaKart Launcher.app");
        std::fs::create_dir_all(bundle.join("Contents")).expect("bundle");
        let link = temp.path().join("collegamento.app");

        symlink(&bundle, &link).expect("collegamento");
        assert_eq!(std::fs::read_link(&link).expect("link"), bundle);
    }

    #[test]
    fn an_existing_link_is_replaced() {
        let temp = tempfile::tempdir().expect("temp");
        let first = temp.path().join("uno");
        let second = temp.path().join("due");
        std::fs::create_dir_all(&first).expect("uno");
        std::fs::create_dir_all(&second).expect("due");
        let link = temp.path().join("link");

        symlink(&first, &link).expect("primo");
        symlink(&second, &link).expect("secondo");
        assert_eq!(std::fs::read_link(&link).expect("link"), second);
    }

    #[test]
    fn removal_happens_at_once() {
        let temp = tempfile::tempdir().expect("temp");
        let bundle = temp.path().join("VanzaKart Launcher.app");
        std::fs::create_dir_all(&bundle).expect("bundle");

        assert!(!schedule_removal(std::slice::from_ref(&bundle)).expect("rimosso"));
        assert!(!bundle.exists());
    }

    #[test]
    fn nothing_is_registered_outside_the_record() {
        let registration = UninstallRegistration {
            install_dir: Path::new("/Applications"),
            executable: Path::new("/Applications/VanzaKart Launcher.app"),
            uninstaller: None,
            version: "2.0.0",
            size_bytes: 1024,
        };
        assert!(register_uninstall(&registration)
            .expect("nessuna")
            .is_empty());
        assert!(!unregister_uninstall(None));
    }
}
