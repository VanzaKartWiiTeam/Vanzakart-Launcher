//! Adapter Linux.

use std::path::PathBuf;

/// Su Linux Dolphin non usa il registro: nessun percorso da leggere.
pub fn dolphin_registry_user_path() -> Option<String> {
    None
}

/// Il launcher legacy è Windows-only: non c'è nulla da importare.
pub fn legacy_install_dir() -> Option<PathBuf> {
    None
}

/// Radici aggiuntive: i percorsi in cui finiscono AppImage e build portable.
pub fn extra_search_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("bin"));
        roots.push(home.join("Applications"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_nothing_to_read_from_a_registry() {
        assert!(dolphin_registry_user_path().is_none());
        assert!(legacy_install_dir().is_none());
    }

    #[test]
    fn common_install_prefixes_are_searched() {
        assert!(extra_search_roots().contains(&PathBuf::from("/opt")));
    }
}
