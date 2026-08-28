//! Risoluzione dei percorsi di Dolphin.
//!
//! Porta `Launcher/Services/DolphinPathResolverService.cs` rendendola
//! cross-platform. La raccolta dei dati dipendenti dal sistema (home,
//! Documents, registro di Windows) resta nell'adapter di piattaforma: qui
//! arriva già come [`PathProbe`], così la logica è testabile senza toccare
//! il sistema reale.

use std::path::{Path, PathBuf};

/// Informazioni di ambiente fornite dall'adapter di piattaforma.
#[derive(Debug, Clone, Default)]
pub struct PathProbe {
    pub home: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    /// `%APPDATA%` su Windows, `~/Library/Application Support` su macOS,
    /// `~/.config` su Linux.
    pub app_data: Option<PathBuf>,
    /// `%LOCALAPPDATA%` su Windows, `~/.local/share` su Linux.
    pub local_app_data: Option<PathBuf>,
    /// Radici in cui cercare installazioni portable (Program Files, Desktop,
    /// Downloads, `/Applications`, …).
    pub search_roots: Vec<PathBuf>,
    /// Valore di `HKCU\Software\Dolphin Emulator\UserConfigPath` (solo Windows).
    pub registry_user_path: Option<String>,
}

/// Nomi comuni dell'eseguibile di Dolphin per piattaforma.
///
/// Su Linux **non** c'è `dolphin` senza suffisso: quello è il gestore di file
/// di KDE, che sta in `/usr/bin` come l'emulatore. Il confronto è senza
/// distinzione di maiuscole, quindi tenerlo nell'elenco faceva scegliere al
/// launcher il programma sbagliato, che poi moriva su `libKF6Archive`
/// (§D-073). L'emulatore lì si chiama `dolphin-emu`.
pub fn executable_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["Dolphin.exe", "DolphinWx.exe", "DolphinQt2.exe"]
    } else if cfg!(target_os = "macos") {
        &["Dolphin", "Dolphin.app"]
    } else {
        &["dolphin-emu", "dolphin-emu-qt2", "dolphin-emu-nogui"]
    }
}

/// `true` se il percorso è il gestore di file di KDE invece dell'emulatore.
///
/// Serve a dirlo a chi lo sceglie a mano: si chiamano quasi uguale, e
/// l'errore che si ottiene altrimenti parla di librerie KDE.
pub fn is_kde_file_manager(path: &Path) -> bool {
    if cfg!(windows) || cfg!(target_os = "macos") {
        return false;
    }

    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("dolphin"))
}

/// `true` se il nome è quello di un AppImage di Dolphin.
///
/// Su Linux Dolphin si distribuisce anche come AppImage, e il nome porta
/// versione e architettura — `Dolphin_Emulator-2503-x86_64.AppImage` — quindi
/// un confronto con un elenco di nomi fissi non lo troverebbe mai. Il nome
/// arriva già in minuscolo da chi chiama.
fn is_dolphin_appimage(lowercase_name: &str) -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && lowercase_name.ends_with(".appimage")
        && lowercase_name.contains("dolphin")
}

/// Cartelle `User` candidate, in ordine di priorità.
///
/// Ordine legacy conservato: installazione portable accanto all'eseguibile,
/// valore di registro, percorsi standard, ricerca nelle radici note.
pub fn user_folder_candidates(
    probe: &PathProbe,
    configured_dolphin: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(portable) = portable_user_folder(configured_dolphin) {
        candidates.push(portable);
    }

    if let Some(registry) = probe.registry_user_path.as_ref() {
        let normalized = registry.replace('/', std::path::MAIN_SEPARATOR_STR);
        if !normalized.trim().is_empty() {
            candidates.push(PathBuf::from(normalized));
        }
    }

    candidates.extend(standard_user_folders(probe));

    for root in &probe.search_roots {
        candidates.extend(portable_candidates_under(root));
    }

    dedupe_paths(candidates)
}

/// Percorsi standard della cartella User per piattaforma.
pub fn standard_user_folders(probe: &PathProbe) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if cfg!(windows) {
        if let Some(documents) = &probe.documents {
            out.push(documents.join("Dolphin Emulator"));
        }
        if let Some(app_data) = &probe.app_data {
            out.push(app_data.join("Dolphin Emulator"));
        }
        if let Some(local) = &probe.local_app_data {
            out.push(local.join("Dolphin Emulator"));
        }
        if let Some(home) = &probe.home {
            out.push(home.join("Documents").join("Dolphin Emulator"));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(app_data) = &probe.app_data {
            out.push(app_data.join("Dolphin"));
        }
        if let Some(home) = &probe.home {
            out.push(
                home.join("Library")
                    .join("Application Support")
                    .join("Dolphin"),
            );
        }
    } else {
        if let Some(local) = &probe.local_app_data {
            out.push(local.join("dolphin-emu"));
        }
        if let Some(home) = &probe.home {
            out.push(home.join(".local").join("share").join("dolphin-emu"));
            // Flatpak.
            out.push(
                home.join(".var")
                    .join("app")
                    .join("org.DolphinEmu.dolphin-emu")
                    .join("data")
                    .join("dolphin-emu"),
            );
            out.push(home.join(".dolphin-emu"));
        }
    }

    out
}

/// Cartella `User` di un'installazione portable accanto all'eseguibile.
///
/// Dolphin considera portable un'installazione con `portable.txt` nella
/// directory dell'eseguibile e una sottocartella `User`.
pub fn portable_user_folder(configured_dolphin: Option<&Path>) -> Option<PathBuf> {
    let executable = configured_dolphin?;
    if executable.as_os_str().is_empty() {
        return None;
    }

    let directory = executable_directory(executable)?;
    let flag = directory.join("portable.txt");
    let user = directory.join("User");

    (flag.is_file() && user.is_dir()).then_some(user)
}

/// Directory che contiene l'eseguibile, gestendo il bundle `.app` di macOS.
pub fn executable_directory(executable: &Path) -> Option<PathBuf> {
    if let Some(bundle) = app_bundle_root(executable) {
        return bundle.parent().map(Path::to_path_buf);
    }
    executable.parent().map(Path::to_path_buf)
}

/// Radice del bundle `.app` che contiene il percorso, se esiste.
pub fn app_bundle_root(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// Percorso da eseguire davvero.
///
/// Su macOS un `Dolphin.app` selezionato dall'utente va tradotto nel binario
/// `Contents/MacOS/Dolphin`; altrove il percorso è già quello giusto.
pub fn resolve_launch_executable(selected: &Path) -> PathBuf {
    if selected
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        let macos_dir = selected.join("Contents").join("MacOS");
        if let Ok(entries) = std::fs::read_dir(&macos_dir) {
            let mut binaries: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect();
            binaries.sort();

            if let Some(preferred) = binaries.iter().find(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("Dolphin"))
            }) {
                return preferred.clone();
            }
            if let Some(first) = binaries.first() {
                return first.clone();
            }
        }
        return macos_dir.join("Dolphin");
    }

    selected.to_path_buf()
}

/// Cartelle `User` di installazioni portable dentro una radice di ricerca.
///
/// Equivalente di `FindPortableCandidatesNearCommonInstallRoots`: cerca
/// directory il cui nome contiene "dolphin" e che ospitano una `User`.
pub fn portable_candidates_under(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("dolphin")
        })
        .map(|entry| entry.path().join("User"))
        .filter(|path| path.is_dir())
        .collect();

    out.sort();
    out
}

/// Eseguibili di Dolphin trovati nelle radici di ricerca.
pub fn executable_candidates(probe: &PathProbe) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    for root in &probe.search_roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();

            if path.is_file()
                && (executable_names()
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(&name))
                    || is_dolphin_appimage(&name))
            {
                out.push(path);
            } else if path.is_dir() && name.contains("dolphin") {
                if name.ends_with(".app") {
                    out.push(path.clone());
                }
                for candidate in executable_names() {
                    let nested = path.join(candidate);
                    if nested.exists() {
                        out.push(nested);
                    }
                }
            }
        }
    }

    dedupe_paths(out)
}

/// Prima cartella `User` realmente esistente.
pub fn first_existing_user_folder(
    probe: &PathProbe,
    configured_dolphin: Option<&Path>,
) -> Option<PathBuf> {
    user_folder_candidates(probe, configured_dolphin)
        .into_iter()
        .find(|path| path.is_dir())
}

/// `true` se la cartella ha l'aspetto di una User di Dolphin.
pub fn looks_like_user_folder(path: &Path) -> bool {
    path.is_dir() && (path.join("Config").is_dir() || path.join("GameSettings").is_dir())
}

/// Cartella Riivolution dove vivono le modpack: `<User>/Load/Riivolution`.
pub fn riivolution_folder(user_folder: &Path) -> PathBuf {
    user_folder.join("Load").join("Riivolution")
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        let normalized = normalize(&path);
        if normalized.as_os_str().is_empty() {
            continue;
        }
        let duplicate = out.iter().any(|item| {
            item.to_string_lossy()
                .eq_ignore_ascii_case(normalized.to_string_lossy().as_ref())
        });
        if !duplicate {
            out.push(normalized);
        }
    }
    out
}

fn normalize(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let trimmed = text.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        PathBuf::from(text.trim())
    } else {
        PathBuf::from(trimmed)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_kde_file_manager_is_not_the_emulator() {
        // Su Linux `/usr/bin/dolphin` è il gestore di file di KDE.
        let atteso = !cfg!(windows) && !cfg!(target_os = "macos");
        assert_eq!(
            super::is_kde_file_manager(std::path::Path::new("/usr/bin/dolphin")),
            atteso
        );
        assert!(!super::is_kde_file_manager(std::path::Path::new(
            "/usr/bin/dolphin-emu"
        )));

        // E non compare più fra i nomi cercati dall'auto-rilevamento.
        if atteso {
            assert!(!super::executable_names()
                .iter()
                .any(|name| name.eq_ignore_ascii_case("dolphin")));
        }
    }

    #[test]
    fn a_dolphin_appimage_is_recognised_by_its_name() {
        // Vale solo su Linux: altrove un .AppImage non è un eseguibile.
        let expected = cfg!(all(unix, not(target_os = "macos")));

        assert_eq!(
            super::is_dolphin_appimage("dolphin_emulator-2503-x86_64.appimage"),
            expected
        );
        assert_eq!(super::is_dolphin_appimage("dolphin.appimage"), expected);
        assert!(!super::is_dolphin_appimage("krita-5.2.2-x86_64.appimage"));
        assert!(!super::is_dolphin_appimage("dolphin-emu"));
    }

    use super::*;

    #[test]
    fn detects_a_portable_installation() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("Dolphin.exe");
        std::fs::write(&executable, b"").unwrap();
        std::fs::create_dir_all(dir.path().join("User")).unwrap();

        // Senza portable.txt non è portable.
        assert_eq!(portable_user_folder(Some(&executable)), None);

        std::fs::write(dir.path().join("portable.txt"), b"").unwrap();
        assert_eq!(
            portable_user_folder(Some(&executable)),
            Some(dir.path().join("User"))
        );
    }

    #[test]
    fn portable_detection_ignores_missing_inputs() {
        assert_eq!(portable_user_folder(None), None);
        assert_eq!(portable_user_folder(Some(Path::new(""))), None);
    }

    #[test]
    fn the_portable_folder_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("Dolphin.exe");
        std::fs::write(&executable, b"").unwrap();
        std::fs::write(dir.path().join("portable.txt"), b"").unwrap();
        std::fs::create_dir_all(dir.path().join("User")).unwrap();

        let probe = PathProbe {
            home: Some(PathBuf::from("/home/utente")),
            documents: Some(PathBuf::from("/home/utente/Documents")),
            registry_user_path: Some("/da/registro".into()),
            ..Default::default()
        };

        let candidates = user_folder_candidates(&probe, Some(&executable));
        assert_eq!(candidates[0], dir.path().join("User"));
        assert!(candidates.len() > 1);
    }

    #[test]
    fn the_registry_value_precedes_the_standard_paths() {
        let probe = PathProbe {
            home: Some(PathBuf::from("/home/utente")),
            documents: Some(PathBuf::from("/home/utente/Documents")),
            registry_user_path: Some("/da/registro/".into()),
            ..Default::default()
        };

        let candidates = user_folder_candidates(&probe, None);
        assert_eq!(candidates[0], PathBuf::from("/da/registro"));
    }

    #[test]
    fn candidates_are_deduplicated_case_insensitively() {
        let probe = PathProbe {
            documents: Some(PathBuf::from("/Docs")),
            app_data: Some(PathBuf::from("/Docs")),
            registry_user_path: Some("/docs/Dolphin Emulator".into()),
            ..Default::default()
        };
        let candidates = user_folder_candidates(&probe, None);
        let duplicates = candidates
            .iter()
            .filter(|path| {
                path.to_string_lossy()
                    .to_lowercase()
                    .contains("dolphin emulator")
            })
            .count();
        assert!(duplicates <= 2, "candidati: {candidates:?}");
    }

    #[test]
    fn finds_portable_installations_under_a_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Dolphin-5.0/User")).unwrap();
        std::fs::create_dir_all(dir.path().join("dolphin-dev/User")).unwrap();
        std::fs::create_dir_all(dir.path().join("Altro/User")).unwrap();
        std::fs::create_dir_all(dir.path().join("Dolphin-senza-user")).unwrap();

        let found = portable_candidates_under(dir.path());

        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|path| path.ends_with("User")));
        assert!(!found
            .iter()
            .any(|path| path.to_string_lossy().contains("Altro")));
    }

    #[test]
    fn resolves_a_macos_app_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Dolphin.app");
        let macos = bundle.join("Contents").join("MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("Dolphin"), b"").unwrap();

        assert_eq!(resolve_launch_executable(&bundle), macos.join("Dolphin"));
        // Un percorso non-bundle resta invariato.
        let plain = dir.path().join("Dolphin.exe");
        assert_eq!(resolve_launch_executable(&plain), plain);
    }

    #[test]
    fn app_bundle_root_walks_up() {
        let path = Path::new("/Applications/Dolphin.app/Contents/MacOS/Dolphin");
        assert_eq!(
            app_bundle_root(path),
            Some(PathBuf::from("/Applications/Dolphin.app"))
        );
        assert_eq!(app_bundle_root(Path::new("/usr/bin/dolphin-emu")), None);
    }

    #[test]
    fn executable_directory_escapes_the_bundle() {
        assert_eq!(
            executable_directory(Path::new(
                "/Applications/Dolphin.app/Contents/MacOS/Dolphin"
            )),
            Some(PathBuf::from("/Applications"))
        );
        assert_eq!(
            executable_directory(Path::new("/opt/dolphin/Dolphin.exe")),
            Some(PathBuf::from("/opt/dolphin"))
        );
    }

    #[test]
    fn recognises_a_user_folder_by_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!looks_like_user_folder(dir.path()));
        std::fs::create_dir_all(dir.path().join("Config")).unwrap();
        assert!(looks_like_user_folder(dir.path()));
    }

    #[test]
    fn riivolution_folder_matches_the_legacy_layout() {
        assert_eq!(
            riivolution_folder(Path::new("/home/a/Dolphin Emulator")),
            PathBuf::from("/home/a/Dolphin Emulator/Load/Riivolution")
        );
    }

    #[test]
    fn first_existing_user_folder_skips_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(&real).unwrap();

        let probe = PathProbe {
            registry_user_path: Some("/percorso/inesistente".into()),
            documents: Some(dir.path().to_path_buf()),
            app_data: Some(dir.path().to_path_buf()),
            local_app_data: Some(dir.path().to_path_buf()),
            home: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        // Su Linux i percorsi standard usano nomi diversi: il test verifica
        // solo che venga scartato ciò che non esiste.
        let found = first_existing_user_folder(&probe, None);
        assert!(found.is_none() || found.is_some_and(|path| path.is_dir()));
    }
}
