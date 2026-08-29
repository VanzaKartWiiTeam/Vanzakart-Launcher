//! Adapter macOS.

use std::path::PathBuf;

/// Su macOS non c'è nessuna sonda da eseguire: la webview è di sistema e
/// WindowServer non ha configurazioni alternative da provare.
pub fn handle_probe_if_requested() {}

/// Su macOS non c'è una modalità grafica di ripiego: la webview è di sistema.
pub fn degrade_graphics() {}

/// Controlli d'ambiente prima di aprire la finestra.
///
/// Anche qui l'unico caso che si può riconoscere in anticipo è l'avvio con
/// `sudo`: il processo non riesce a parlare con WindowServer e muore con un
/// errore che parla di connessioni, non di permessi (§D-067).
pub fn preflight() -> Result<(), String> {
    if super::launched_as_root() {
        return Err("The launcher was started as root (sudo).

A graphical application started with sudo cannot talk to WindowServer and opens
no window; on top of that, settings and saves would end up in root's folder
instead of yours.

Start it normally, with a double click or with \"open -a\". The launcher needs no
administrator rights. If you want to continue anyway, set VK_ALLOW_ROOT=1."
            .into());
    }

    Ok(())
}

/// Su macOS non serve nessun accorgimento sul rendering: WKWebView è di
/// sistema e non ha il problema DMA-BUF di WebKitGTK.
pub fn prepare_graphics() {}

/// Su macOS Dolphin non usa il registro: nessun percorso da leggere.
pub fn dolphin_registry_user_path() -> Option<String> {
    None
}

/// Il launcher legacy è Windows-only: non c'è nulla da importare.
pub fn legacy_install_dir() -> Option<PathBuf> {
    None
}

/// Radici aggiuntive: le cartelle Applicazioni di sistema e utente.
pub fn extra_search_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::launched_as_root;

    #[test]
    fn there_is_nothing_to_read_from_a_registry() {
        assert!(dolphin_registry_user_path().is_none());
        assert!(legacy_install_dir().is_none());
    }

    #[test]
    fn a_normal_user_can_start_the_launcher() {
        // La suite non gira da root: il preflight non ha niente da dire.
        if !launched_as_root() {
            assert!(preflight().is_ok());
        }
    }

    #[test]
    fn the_applications_folder_is_searched() {
        assert!(extra_search_roots().contains(&PathBuf::from("/Applications")));
    }
}
