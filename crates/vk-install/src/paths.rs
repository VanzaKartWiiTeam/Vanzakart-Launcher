//! Dove finiscono le cose, su ogni sistema operativo.
//!
//! Un solo punto per tutte le posizioni predefinite: cartella
//! d'installazione, registro dell'installazione, backup, dati del launcher.
//! I default cambiano per piattaforma, la logica che li usa no.

use std::path::PathBuf;

/// Nome della cartella d'installazione predefinita su Windows.
///
/// È lo stesso che userebbe l'installer NSIS di Tauri in modalità
/// `currentUser` (`$LOCALAPPDATA\<productName>`): se l'utente non cambia
/// cartella, un aggiornamento passato dall'updater firmato sovrascrive
/// *questa* installazione invece di crearne una seconda (§D-052).
#[cfg(windows)]
const WINDOWS_DIR_NAME: &str = "VanzaKart Launcher";

/// Su Linux la cartella è in minuscolo e senza spazi, come vuole l'abitudine
/// del sistema: finisce dentro `Exec=` del file `.desktop`.
const LINUX_DIR_NAME: &str = "vanzakart-launcher";

/// Radice dei dati del launcher: **non** appartiene all'installer e non viene
/// toccata da un aggiornamento.
///
/// Deve restare identica a `AppPaths::discover()` del launcher, altrimenti il
/// disinstallatore cancellerebbe la cartella sbagliata.
pub fn launcher_data_root() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join("VanzaKart").join("Launcher"))
}

/// Radice dei dati dell'**installer**, separata da quella del launcher.
///
/// Ci vive il registro dell'installazione: se sta altrove, "cancella anche i
/// dati del launcher" non porta via con sé la traccia di ciò che va rimosso.
///
/// `VK_SETUP_DATA_ROOT` la sposta altrove. Serve ai test — che non devono
/// scrivere nel profilo di chi li esegue — e a un'installazione su chiavetta,
/// che si porta dietro il proprio registro.
pub fn installer_data_root() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os(DATA_ROOT_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(custom));
    }
    dirs::data_dir().map(|base| base.join("VanzaKart").join("Installer"))
}

/// Variabile che sposta la radice dei dati dell'installer.
pub const DATA_ROOT_ENV: &str = "VK_SETUP_DATA_ROOT";

/// Percorso del registro d'installazione condiviso.
pub fn record_path() -> Option<PathBuf> {
    installer_data_root().map(|root| root.join("install.json"))
}

/// Nome del registro copiato dentro la cartella d'installazione.
pub const RECORD_FILE_NAME: &str = "install.json";

/// Cartella d'installazione predefinita per la piattaforma corrente.
pub fn default_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local) = dirs::data_local_dir() {
            return local.join(WINDOWS_DIR_NAME);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home.join("Applications");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(".local").join("opt").join(LINUX_DIR_NAME);
        }
    }

    std::env::temp_dir().join(LINUX_DIR_NAME)
}

/// Alternative proposte nella UI accanto al percorso predefinito.
pub fn suggested_install_dirs() -> Vec<PathBuf> {
    let mut suggestions = vec![default_install_dir()];

    #[cfg(windows)]
    if let Some(programs) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
        // Richiede i permessi di amministratore: la UI lo segnala e l'utente
        // decide. L'installer non chiede l'elevazione da sé.
        suggestions.push(programs.join(WINDOWS_DIR_NAME));
    }

    #[cfg(target_os = "macos")]
    suggestions.push(PathBuf::from("/Applications"));

    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = dirs::home_dir() {
        suggestions.push(home.join("Applications"));
    }

    suggestions.retain(|path| !path.as_os_str().is_empty());
    suggestions.dedup();
    suggestions
}

/// `true` se la cartella d'installazione appartiene solo al launcher e quindi
/// può essere cancellata per intero dal disinstallatore.
///
/// Su macOS si installa *dentro* una cartella Applicazioni condivisa con il
/// resto del sistema: lì si rimuovono i singoli bundle, mai la cartella.
pub fn owns_install_dir(install_dir: &std::path::Path) -> bool {
    !is_applications_dir(install_dir)
}

/// `true` per `/Applications`, `~/Applications` e simili.
pub fn is_applications_dir(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.eq_ignore_ascii_case("Applications")
}

/// Cartella di backup predefinita, come nel setup legacy.
pub fn default_backup_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("VanzaKart_Backups")
}

/// Nome dell'eseguibile del launcher dentro il pacchetto.
pub fn launcher_executable_name() -> &'static str {
    if cfg!(windows) {
        "VanzaKart Launcher.exe"
    } else if cfg!(target_os = "macos") {
        "VanzaKart Launcher.app"
    } else {
        "vanzakart-launcher.AppImage"
    }
}

/// Nome con cui il disinstallatore viene copiato accanto al launcher.
pub fn uninstaller_name() -> &'static str {
    if cfg!(windows) {
        "VanzaKart Uninstaller.exe"
    } else if cfg!(target_os = "macos") {
        "VanzaKart Uninstaller.app"
    } else {
        "vanzakart-uninstaller"
    }
}

/// File temporaneo in cui scaricare il pacchetto.
///
/// Sta nella cartella temporanea del sistema, come il setup legacy, così un
/// download interrotto non lascia spazzatura nella cartella d'installazione.
///
/// Il nome è diverso a ogni chiamata: con un nome fisso due installazioni
/// avviate insieme si scrivevano l'una sopra l'altra e la seconda estraeva un
/// archivio troncato ("Could not find EOCD").
pub fn download_temp_path(extension: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "VanzaKart_Setup_payload_{}_{sequence}.{extension}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_install_dir_is_absolute_and_named() {
        let dir = default_install_dir();
        assert!(dir.is_absolute(), "{dir:?}");
        assert!(dir.file_name().is_some());
    }

    #[test]
    fn the_data_root_can_be_moved_with_an_environment_variable() {
        let previous = std::env::var_os(DATA_ROOT_ENV);
        let custom = std::env::temp_dir().join("vk-install-root-test");
        std::env::set_var(DATA_ROOT_ENV, &custom);
        assert_eq!(installer_data_root(), Some(custom.clone()));
        assert_eq!(record_path(), Some(custom.join("install.json")));

        match previous {
            Some(value) => std::env::set_var(DATA_ROOT_ENV, value),
            None => std::env::remove_var(DATA_ROOT_ENV),
        }
    }

    #[test]
    fn the_installer_record_lives_outside_the_launcher_data() {
        let (Some(launcher), Some(installer)) = (launcher_data_root(), installer_data_root())
        else {
            return;
        };
        assert!(!installer.starts_with(&launcher));
        assert!(!launcher.starts_with(&installer));
    }

    #[test]
    fn an_applications_folder_is_never_owned() {
        assert!(!owns_install_dir(std::path::Path::new("/Applications")));
        assert!(!owns_install_dir(std::path::Path::new(
            "/Users/tizio/Applications"
        )));
        assert!(owns_install_dir(std::path::Path::new(
            "/Users/tizio/Applications/VanzaKart Launcher"
        )));
    }

    #[test]
    fn the_default_is_among_the_suggestions() {
        assert!(suggested_install_dirs().contains(&default_install_dir()));
    }

    #[test]
    fn the_launcher_and_the_uninstaller_have_different_names() {
        assert_ne!(launcher_executable_name(), uninstaller_name());
    }

    #[test]
    fn the_download_lands_in_the_system_temp_dir() {
        let path = download_temp_path("zip");
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("zip"));
    }

    #[test]
    fn two_downloads_never_share_a_file() {
        assert_ne!(download_temp_path("zip"), download_temp_path("zip"));
    }
}
