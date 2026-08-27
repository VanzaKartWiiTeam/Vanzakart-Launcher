//! Adapter di piattaforma.
//!
//! È l'unico punto del progetto in cui compaiono API specifiche di un sistema
//! operativo. I crate di dominio ricevono i risultati come dati inerti
//! (vedi `docs/decisions.md` §D-001).

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

use std::path::Path;

/// Nome della piattaforma, mostrato nella pagina Debug.
pub const fn platform_name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// `true` se un eseguibile con quel percorso è in esecuzione.
///
/// Equivalente di `MainWindow.xaml.cs::IsExecutableRunning`: serve a impedire
/// l'avvio quando Dolphin è già aperto, perché non rileggerebbe i binding.
pub fn is_executable_running(executable: &Path) -> bool {
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

/// Radici in cui cercare installazioni portable di Dolphin.
pub fn dolphin_search_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Downloads"));
        roots.push(home.join("Desktop"));
    }
    if let Some(desktop) = dirs::desktop_dir() {
        roots.push(desktop);
    }
    roots.extend(extra_search_roots());

    roots.retain(|path| path.is_dir());
    roots.sort();
    roots.dedup();
    roots
}

/// Byte casuali dal generatore del sistema operativo.
///
/// È l'unica sorgente di entropia del progetto: i crate di dominio restano
/// deterministici e ricevono i valori casuali come argomenti (per esempio
/// `vk_save::mii::generate_mii_id`).
///
/// Se il generatore del sistema non risponde si ripiega sull'orologio ad alta
/// risoluzione: non è entropia crittografica, ma qui serve solo a non far
/// collidere due Mii creati sulla stessa macchina, e restituire zeri
/// garantirebbe la collisione.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    if getrandom::fill(&mut buffer).is_ok() {
        return buffer;
    }

    tracing::warn!("generatore di numeri casuali del sistema non disponibile");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = ((nanos >> ((index % 16) * 8)) & 0xFF) as u8;
    }
    buffer
}

/// Dati d'ambiente per la risoluzione dei percorsi di Dolphin.
pub fn path_probe() -> vk_dolphin::paths::PathProbe {
    vk_dolphin::paths::PathProbe {
        home: dirs::home_dir(),
        documents: dirs::document_dir(),
        app_data: dirs::config_dir(),
        local_app_data: dirs::data_local_dir(),
        search_roots: dolphin_search_roots(),
        registry_user_path: dolphin_registry_user_path(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_is_named() {
        assert!(["Windows", "macOS", "Linux"].contains(&platform_name()));
    }

    #[test]
    fn a_missing_executable_is_not_running() {
        assert!(!is_executable_running(Path::new("")));
        assert!(!is_executable_running(Path::new(
            "/percorso/inesistente/QuestoNonEsisteDavvero.exe"
        )));
    }

    #[test]
    fn the_current_process_is_detected_as_running() {
        let current = std::env::current_exe().expect("eseguibile corrente");
        assert!(is_executable_running(&current));
    }

    #[test]
    fn every_search_root_exists() {
        assert!(dolphin_search_roots().iter().all(|path| path.is_dir()));
    }

    #[test]
    fn random_bytes_are_not_all_zero() {
        // Due chiamate consecutive che coincidono segnalerebbero un
        // generatore fermo: con 16 byte la collisione è impossibile in pratica.
        let first = random_bytes::<16>();
        let second = random_bytes::<16>();
        assert_ne!(first, [0u8; 16]);
        assert_ne!(first, second);
    }

    #[test]
    fn the_probe_is_populated() {
        let probe = path_probe();
        assert!(probe.home.is_some() || probe.local_app_data.is_some());
    }
}
