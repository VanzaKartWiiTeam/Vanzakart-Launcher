//! Ritrovare un'installazione già presente.
//!
//! Tre sorgenti, in ordine di attendibilità: il registro scritto
//! dall'installer, la registrazione di sistema (solo Windows) e infine la
//! cartella predefinita. Le ultime due servono a chi ha installato con una
//! versione precedente dell'installer o con il setup legacy in C#: senza,
//! l'installer proporrebbe una seconda copia accanto a quella esistente.

use std::path::{Path, PathBuf};

use crate::record::InstallRecord;
use crate::{paths, platform};

/// Che cosa c'è già su questa macchina.
#[derive(Debug, Clone)]
pub struct ExistingInstall {
    /// Registro completo, quando l'installazione è stata fatta da questo
    /// installer.
    pub record: Option<InstallRecord>,
    pub install_dir: PathBuf,
    /// Vuota quando non è deducibile.
    pub version: String,
    pub executable: Option<PathBuf>,
    /// `true` quando c'è un registro: la disinstallazione sarà esatta.
    pub managed: bool,
}

impl ExistingInstall {
    fn from_record(record: InstallRecord) -> Self {
        let executable = Some(record.executable.clone()).filter(|path| path.exists());
        Self {
            install_dir: record.install_dir.clone(),
            version: record.version.clone(),
            executable,
            managed: true,
            record: Some(record),
        }
    }

    fn unmanaged(install_dir: PathBuf, version: Option<String>) -> Option<Self> {
        let executable = find_launcher_executable(&install_dir);
        if executable.is_none() && crate::fsops::is_dir_empty(&install_dir) {
            return None;
        }
        Some(Self {
            record: None,
            install_dir,
            version: version.unwrap_or_default(),
            executable,
            managed: false,
        })
    }

    /// Registro esistente, o registro ricostruito dal poco che si sa.
    ///
    /// Il ripiego permette di disinstallare anche ciò che è stato installato
    /// prima che il registro esistesse: si toglie la cartella, si tolgono le
    /// scorciatoie nei percorsi noti e si toglie la registrazione di sistema.
    pub fn record_or_reconstructed(&self) -> InstallRecord {
        if let Some(record) = &self.record {
            return record.clone();
        }

        let mut record = InstallRecord::new(
            self.version.clone(),
            crate::Target::current().key(),
            self.install_dir.clone(),
        );
        if let Some(executable) = &self.executable {
            record.executable = executable.clone();
            if let Ok(relative) = executable.strip_prefix(&self.install_dir) {
                record.payload = vec![relative.to_path_buf()];
            }
        }
        record.artifacts = well_known_artifacts();
        record
    }
}

/// Cerca un'installazione esistente.
pub fn find() -> Option<ExistingInstall> {
    if let Some(record) = load_shared_record() {
        if record.install_dir.is_dir() {
            return Some(ExistingInstall::from_record(record));
        }
    }

    if let Some(dir) = platform::registered_install_dir() {
        if let Some(record) = load_record_in(&dir) {
            return Some(ExistingInstall::from_record(record));
        }
        return ExistingInstall::unmanaged(dir, platform::registered_version());
    }

    let default_dir = paths::default_install_dir();
    if default_dir.is_dir() {
        if let Some(record) = load_record_in(&default_dir) {
            return Some(ExistingInstall::from_record(record));
        }
        return ExistingInstall::unmanaged(default_dir, None);
    }

    None
}

/// Cartella del launcher legacy in C#, se è ancora installato.
///
/// È un'informazione da mostrare, non una cartella in cui installare: il
/// launcher vecchio resta dov'è, e il nuovo ne importa le impostazioni al
/// primo avvio. Installarci sopra mescolerebbe due programmi nella stessa
/// cartella, e una reinstallazione pulita cancellerebbe quello vecchio senza
/// che nessuno l'abbia chiesto (§D-055).
pub fn legacy_install() -> Option<PathBuf> {
    platform::legacy_install_dir()
}

/// Cerca l'installazione da rimuovere.
///
/// Il disinstallatore vive *dentro* la cartella d'installazione: il registro
/// che gli sta accanto descrive quella copia, ed è più attendibile di quello
/// condiviso, che potrebbe riferirsi a un'installazione fatta altrove.
pub fn find_for_uninstall() -> Option<ExistingInstall> {
    if let Ok(current) = platform::self_bundle_path() {
        if let Some(record) = find_near(&current) {
            return Some(ExistingInstall::from_record(record));
        }
    }
    find()
}

/// Registro accanto a un eseguibile: è così che il disinstallatore copiato
/// nella cartella d'installazione sa cosa deve togliere.
pub fn find_near(executable: &Path) -> Option<InstallRecord> {
    let mut current = executable.parent();
    // Dentro un bundle `.app` il registro sta accanto al bundle, non dentro:
    // si risale di qualche livello prima di arrendersi.
    for _ in 0..4 {
        let directory = current?;
        if let Some(record) = load_record_in(directory) {
            return Some(record);
        }
        current = directory.parent();
    }
    None
}

fn load_shared_record() -> Option<InstallRecord> {
    let path = paths::record_path()?;
    InstallRecord::load(&path).ok()
}

fn load_record_in(directory: &Path) -> Option<InstallRecord> {
    let path = directory.join(paths::RECORD_FILE_NAME);
    let record = InstallRecord::load(&path).ok()?;
    record.install_dir.is_dir().then_some(record)
}

/// Cerca l'eseguibile del launcher dentro una cartella, con le regole del
/// setup legacy.
pub fn find_launcher_executable(directory: &Path) -> Option<PathBuf> {
    let expected = directory.join(paths::launcher_executable_name());
    if expected.exists() {
        return Some(expected);
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_launcher_candidate(path))
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn is_launcher_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.contains("uninstall") || lower.contains("setup") {
        return false;
    }
    if !lower.contains("launcher") && !lower.contains("vanzakart") {
        return false;
    }

    if cfg!(windows) {
        lower.ends_with(".exe")
    } else if cfg!(target_os = "macos") {
        lower.ends_with(".app")
    } else {
        path.is_file()
    }
}

/// Scorciatoie nei percorsi in cui le metteva l'installer: servono a ripulire
/// un'installazione senza registro.
fn well_known_artifacts() -> Vec<crate::record::Artifact> {
    use crate::record::{Artifact, ArtifactKind};

    let mut artifacts = Vec::new();

    #[cfg(windows)]
    {
        if let Some(desktop) = dirs::desktop_dir() {
            artifacts.push(Artifact::file(
                ArtifactKind::DesktopShortcut,
                &desktop.join("VanzaKart Launcher.lnk"),
            ));
        }
        if let Some(roaming) = dirs::data_dir() {
            let programs = roaming
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join(platform::MENU_FOLDER_NAME);
            artifacts.push(Artifact::file(
                ArtifactKind::StartMenuShortcut,
                &programs.join("VanzaKart Launcher.lnk"),
            ));
            // Il collegamento si chiamava «Disinstalla…» prima che
            // l'installer parlasse inglese: chi ha installato allora ha
            // ancora quel file, e va tolto lo stesso.
            for name in [
                "Uninstall VanzaKart Launcher.lnk",
                "Disinstalla VanzaKart Launcher.lnk",
            ] {
                artifacts.push(Artifact::file(
                    ArtifactKind::UninstallShortcut,
                    &programs.join(name),
                ));
            }
            artifacts.push(Artifact::file(
                ArtifactKind::QuickLaunchShortcut,
                &roaming
                    .join("Microsoft")
                    .join("Internet Explorer")
                    .join("Quick Launch")
                    .join("VanzaKart Launcher.lnk"),
            ));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(data) = dirs::data_dir() {
            let applications = data.join("applications");
            artifacts.push(Artifact::file(
                ArtifactKind::StartMenuShortcut,
                &applications.join("vanzakart-launcher.desktop"),
            ));
            artifacts.push(Artifact::file(
                ArtifactKind::UninstallShortcut,
                &applications.join("vanzakart-uninstaller.desktop"),
            ));
            artifacts.push(Artifact::file(
                ArtifactKind::Icon,
                &data
                    .join("icons")
                    .join("hicolor")
                    .join("256x256")
                    .join("apps")
                    .join("vanzakart-launcher.png"),
            ));
        }
        if let Some(desktop) = dirs::desktop_dir() {
            artifacts.push(Artifact::file(
                ArtifactKind::DesktopShortcut,
                &desktop.join("vanzakart-launcher.desktop"),
            ));
        }
        if let Some(home) = dirs::home_dir() {
            artifacts.push(Artifact::file(
                ArtifactKind::Symlink,
                &home.join(".local").join("bin").join("vanzakart-launcher"),
            ));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(desktop) = dirs::desktop_dir() {
            artifacts.push(Artifact::file(
                ArtifactKind::DesktopShortcut,
                &desktop.join("VanzaKart Launcher.app"),
            ));
        }
        if let Some(home) = dirs::home_dir() {
            artifacts.push(Artifact::file(
                ArtifactKind::StartMenuShortcut,
                &home.join("Applications").join("VanzaKart Launcher.app"),
            ));
        }
    }

    artifacts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_folder_is_not_an_installation() {
        let temp = tempfile::tempdir().expect("temp");
        assert!(ExistingInstall::unmanaged(temp.path().to_path_buf(), None).is_none());
    }

    #[test]
    fn a_folder_with_the_launcher_in_it_is_an_installation() {
        let temp = tempfile::tempdir().expect("temp");
        let executable = temp.path().join(paths::launcher_executable_name());
        if executable.extension().is_some_and(|ext| ext == "app") {
            std::fs::create_dir_all(&executable).expect("bundle");
        } else {
            std::fs::write(&executable, b"MZ").expect("scritto");
        }

        let found = ExistingInstall::unmanaged(temp.path().to_path_buf(), Some("1.9.0".into()))
            .expect("trovata");
        assert!(!found.managed);
        assert_eq!(found.version, "1.9.0");
        assert_eq!(found.executable, Some(executable));
    }

    #[test]
    fn a_reconstructed_record_still_knows_where_the_shortcuts_are() {
        let temp = tempfile::tempdir().expect("temp");
        let executable = temp.path().join(paths::launcher_executable_name());
        if executable.extension().is_some_and(|ext| ext == "app") {
            std::fs::create_dir_all(&executable).expect("bundle");
        } else {
            std::fs::write(&executable, b"MZ").expect("scritto");
        }

        let found = ExistingInstall::unmanaged(temp.path().to_path_buf(), None).expect("trovata");
        let record = found.record_or_reconstructed();
        assert_eq!(record.install_dir, temp.path());
        assert!(!record.artifacts.is_empty());
    }

    #[test]
    fn the_uninstaller_is_never_taken_for_the_launcher() {
        let temp = tempfile::tempdir().expect("temp");
        let name = if cfg!(windows) {
            "VanzaKart Uninstaller.exe"
        } else {
            "vanzakart-uninstaller"
        };
        std::fs::write(temp.path().join(name), b"MZ").expect("scritto");
        assert!(find_launcher_executable(temp.path()).is_none());
    }

    #[test]
    fn a_record_is_found_next_to_the_executable() {
        let temp = tempfile::tempdir().expect("temp");
        let mut record = InstallRecord::new("2.0.0", "test", temp.path().to_path_buf());
        record.executable = temp.path().join("app");
        std::fs::write(
            temp.path().join(paths::RECORD_FILE_NAME),
            serde_json::to_string(&record).expect("json"),
        )
        .expect("scritto");

        let found = find_near(&temp.path().join("app")).expect("registro");
        assert_eq!(found.version, "2.0.0");
    }

    #[test]
    fn looking_around_never_panics() {
        let _ = find();
        let _ = find_for_uninstall();
        let _ = legacy_install();
    }

    #[test]
    fn the_legacy_launcher_is_never_something_to_uninstall() {
        // `find` guarda solo le tracce lasciate da questo installer: il
        // launcher legacy, se c'è, non deve mai finire fra ciò che si rimuove.
        if let (Some(legacy), Some(found)) = (legacy_install(), find()) {
            assert_ne!(found.install_dir, legacy);
        }
    }
}
