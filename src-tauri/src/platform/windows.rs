//! Adapter Windows.

use std::path::PathBuf;

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Su Windows non c'è nessuna sonda da eseguire: WebView2 non ha
/// configurazioni grafiche alternative da provare.
pub fn handle_probe_if_requested() {}

/// Su Windows non c'è una modalità grafica di ripiego: WebView2 è di sistema.
pub fn degrade_graphics() {}

/// Su Windows non c'è niente da controllare prima di aprire la finestra: la
/// webview è quella di sistema e non dipende da un server grafico esterno.
pub fn preflight() -> Result<(), String> {
    Ok(())
}

/// Nessun accorgimento sul rendering: WebView2 non ha il problema DMA-BUF di
/// WebKitGTK.
pub fn prepare_graphics() {}

/// `HKCU\Software\Dolphin Emulator\UserConfigPath`, se presente.
///
/// È lo stesso valore che leggeva `DolphinPathResolverService`.
pub fn dolphin_registry_user_path() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Dolphin Emulator")
        .ok()?
        .get_value::<String, _>("UserConfigPath")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Directory d'installazione del launcher legacy, dalla chiave di
/// disinstallazione che scriveva `WindowsInstallRegistryService`.
pub fn legacy_install_dir() -> Option<PathBuf> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\VanzaKartLauncher")
        .ok()?
        .get_value::<String, _>("InstallLocation")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

/// Radici aggiuntive: Program Files a 64 e 32 bit.
pub fn extra_search_roots() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|variable| std::env::var_os(variable).map(PathBuf::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookups_never_panic() {
        // Le chiavi possono non esistere: il risultato deve essere None, non
        // un panic.
        let _ = dolphin_registry_user_path();
        let _ = legacy_install_dir();
    }

    #[test]
    fn program_files_is_among_the_search_roots() {
        assert!(!extra_search_roots().is_empty());
    }
}
