//! Operazioni su file e cartelle usate da installazione e rimozione.
//!
//! Qui vive l'unica regola che non si negozia: **prima di cancellare, chiedere
//! a [`ensure_safe_target`]**. Il setup legacy si difendeva con
//! `fullPath.Length < 8`, che lasciava passare `C:\Users`; qui la lista dei
//! percorsi intoccabili è esplicita.

use std::path::{Component, Path, PathBuf};

use crate::error::{InstallError, InstallResult};

/// Crea la cartella e tutti i genitori.
pub fn ensure_dir(path: &Path) -> InstallResult<()> {
    std::fs::create_dir_all(path).map_err(|error| InstallError::io(path, error))
}

/// `true` se la cartella non esiste o non contiene nulla.
pub fn is_dir_empty(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

/// Dimensione di un file o di un albero di cartelle. Gli errori valgono zero:
/// serve a mostrare un numero, non a decidere.
pub fn path_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if metadata.is_symlink() {
        return 0;
    }

    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    total
}

/// Spazio libero sul volume che ospita (o ospiterà) il percorso.
///
/// La cartella d'installazione di solito non esiste ancora: si risale al primo
/// antenato esistente, che sta comunque sullo stesso volume.
pub fn available_space(path: &Path) -> Option<u64> {
    let anchor = nearest_existing(path)?;
    let anchor = std::fs::canonicalize(&anchor).unwrap_or(anchor);
    let anchor_text = normalize_for_compare(&anchor);

    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount = normalize_for_compare(disk.mount_point());
            is_under(&anchor_text, &mount).then_some((mount.len(), disk.available_space()))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, available)| available)
}

/// Il primo antenato del percorso che esiste davvero.
pub fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// Copia ricorsiva. Restituisce i byte copiati.
///
/// I collegamenti simbolici vengono ricreati come collegamenti: dentro un
/// bundle `.app` risolverli produrrebbe copie multiple dello stesso framework
/// e un bundle non firmabile.
pub fn copy_tree(source: &Path, destination: &Path) -> InstallResult<u64> {
    let metadata =
        std::fs::symlink_metadata(source).map_err(|error| InstallError::io(source, error))?;

    if metadata.is_symlink() {
        copy_symlink(source, destination)?;
        return Ok(0);
    }

    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            ensure_dir(parent)?;
        }
        let copied = std::fs::copy(source, destination)
            .map_err(|error| InstallError::io(destination, error))?;
        return Ok(copied);
    }

    ensure_dir(destination)?;
    let mut total = 0;
    let entries = std::fs::read_dir(source).map_err(|error| InstallError::io(source, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| InstallError::io(source, error))?;
        total += copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(total)
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> InstallResult<()> {
    let target = std::fs::read_link(source).map_err(|error| InstallError::io(source, error))?;
    if destination.exists() || std::fs::symlink_metadata(destination).is_ok() {
        let _ = std::fs::remove_file(destination);
    }
    if let Some(parent) = destination.parent() {
        ensure_dir(parent)?;
    }
    std::os::unix::fs::symlink(&target, destination)
        .map_err(|error| InstallError::io(destination, error))
}

#[cfg(windows)]
fn copy_symlink(source: &Path, destination: &Path) -> InstallResult<()> {
    // Su Windows i pacchetti non contengono collegamenti simbolici: se
    // capitasse, si copia il contenuto puntato invece di fallire.
    let resolved =
        std::fs::canonicalize(source).map_err(|error| InstallError::io(source, error))?;
    copy_tree(&resolved, destination).map(|_| ())
}

/// Cancella un file o un albero, ripulendo gli attributi che lo bloccano.
pub fn remove_path(path: &Path) -> InstallResult<()> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    match remove_once(path) {
        Ok(()) => Ok(()),
        Err(_) => {
            clear_attributes(path);
            remove_once(path).map_err(|error| InstallError::io(path, error))
        }
    }
}

/// Cancella senza far fallire l'operazione complessiva: la rimozione di una
/// scorciatoia o di una cache non deve fermare una disinstallazione.
pub fn remove_path_best_effort(path: &Path) -> bool {
    remove_path(path).is_ok()
}

fn remove_once(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn clear_attributes(path: &Path) {
    vk_core::zipx::clear_blocking_attributes(path);
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        clear_attributes(&entry.path());
    }
}

/// Rende eseguibile un file. Su Windows non serve: l'estensione basta.
pub fn set_executable(path: &Path) -> InstallResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)
            .map_err(|error| InstallError::io(path, error))?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| InstallError::io(path, error))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Controlla che il percorso possa ospitare — ed eventualmente perdere — una
/// installazione.
///
/// Rifiuta la radice del disco, la cartella utente, le cartelle note del
/// sistema e qualunque percorso con meno di due livelli sotto la radice.
pub fn ensure_safe_target(path: &Path) -> InstallResult<PathBuf> {
    let absolute = absolutize(path)?;

    if absolute.parent().is_none() {
        return Err(InstallError::UnsafePath(format!(
            "{} è la radice del disco",
            absolute.display()
        )));
    }

    let depth = absolute
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    if depth < 2 {
        return Err(InstallError::UnsafePath(format!(
            "{} è troppo vicino alla radice del disco",
            absolute.display()
        )));
    }

    for forbidden in protected_dirs() {
        if same_path(&absolute, &forbidden) {
            return Err(InstallError::UnsafePath(format!(
                "{} è una cartella di sistema",
                absolute.display()
            )));
        }
    }

    Ok(absolute)
}

/// Percorso assoluto e normalizzato, senza richiedere che esista.
pub fn absolutize(path: &Path) -> InstallResult<PathBuf> {
    let trimmed = path.to_string_lossy().trim().to_string();
    if trimmed.is_empty() {
        return Err(InstallError::UnsafePath("percorso vuoto".into()));
    }
    let candidate = PathBuf::from(trimmed);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        std::env::current_dir()
            .map_err(|error| InstallError::io(".", error))?
            .join(candidate)
    };

    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

/// Cartelle che nessuna installazione può occupare o cancellare.
fn protected_dirs() -> Vec<PathBuf> {
    let mut protected: Vec<PathBuf> = [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        dirs::config_dir(),
        dirs::cache_dir(),
        dirs::picture_dir(),
        dirs::video_dir(),
        dirs::audio_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();

    #[cfg(windows)]
    protected.extend(
        [
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
        ]
        .into_iter()
        .filter_map(|variable| std::env::var_os(variable).map(PathBuf::from)),
    );

    #[cfg(unix)]
    protected.extend(
        [
            "/",
            "/usr",
            "/bin",
            "/sbin",
            "/etc",
            "/opt",
            "/var",
            "/tmp",
            "/Applications",
            "/Library",
            "/System",
            "/Users",
            "/home",
        ]
        .into_iter()
        .map(PathBuf::from),
    );

    protected
}

/// `true` se `path` sta dentro `prefix`, confrontando componenti intere:
/// `/home` non deve risultare il volume di `/homework`.
fn is_under(path: &str, prefix: &str) -> bool {
    prefix == "/" || path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_for_compare(left) == normalize_for_compare(right)
}

fn normalize_for_compare(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    // Su Windows `canonicalize` restituisce il prefisso esteso (`\\?\C:\…`),
    // che nei punti di montaggio non compare mai: senza toglierlo nessun
    // volume corrisponderebbe al percorso e lo spazio libero resterebbe
    // sconosciuto.
    let text = match text.strip_prefix("//?/UNC/") {
        Some(rest) => format!("//{rest}"),
        None => text
            .strip_prefix("//?/")
            .map(str::to_string)
            .unwrap_or(text),
    };
    let trimmed = text.trim_end_matches('/');
    let value = if trimmed.is_empty() { "/" } else { trimmed };
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_directory_is_never_an_install_target() {
        let Some(home) = dirs::home_dir() else { return };
        let error = ensure_safe_target(&home).expect_err("home protetta");
        assert_eq!(error.code(), "unsafe-path");
    }

    #[test]
    fn a_shallow_path_is_refused() {
        let root = if cfg!(windows) {
            "C:\\Programmi"
        } else {
            "/opt"
        };
        assert!(ensure_safe_target(Path::new(root)).is_err());
    }

    #[test]
    fn a_normal_install_path_is_accepted() {
        let base = std::env::temp_dir().join("vk-install-test").join("app");
        let resolved = ensure_safe_target(&base).expect("percorso valido");
        assert!(resolved.is_absolute());
    }

    #[test]
    fn parent_components_are_collapsed() {
        let messy = std::env::temp_dir().join("uno").join("..").join("due");
        let clean = absolutize(&messy).expect("normalizzato");
        assert!(clean.ends_with("due"));
        assert!(!clean.to_string_lossy().contains(".."));
    }

    #[test]
    fn an_empty_path_is_refused() {
        assert!(absolutize(Path::new("   ")).is_err());
    }

    #[test]
    fn a_tree_is_copied_with_its_content() {
        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("src");
        std::fs::create_dir_all(source.join("nested")).expect("mkdir");
        std::fs::write(source.join("a.txt"), b"ciao").expect("write");
        std::fs::write(source.join("nested").join("b.txt"), b"mondo").expect("write");

        let destination = temp.path().join("dst");
        let copied = copy_tree(&source, &destination).expect("copia");

        assert_eq!(copied, 9);
        assert_eq!(
            std::fs::read_to_string(destination.join("nested").join("b.txt")).expect("letto"),
            "mondo"
        );
    }

    #[test]
    fn removing_something_that_is_not_there_is_not_an_error() {
        let temp = tempfile::tempdir().expect("temp");
        assert!(remove_path(&temp.path().join("mai-esistito")).is_ok());
    }

    #[test]
    fn a_read_only_file_is_removed_anyway() {
        let temp = tempfile::tempdir().expect("temp");
        let file = temp.path().join("bloccato.txt");
        std::fs::write(&file, b"x").expect("write");
        let mut permissions = std::fs::metadata(&file).expect("meta").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).expect("chmod");

        remove_path(&file).expect("rimosso");
        assert!(!file.exists());
    }

    #[test]
    fn the_size_of_a_tree_is_the_sum_of_its_files() {
        let temp = tempfile::tempdir().expect("temp");
        std::fs::write(temp.path().join("a"), b"12345").expect("write");
        std::fs::create_dir(temp.path().join("d")).expect("mkdir");
        std::fs::write(temp.path().join("d").join("b"), b"123").expect("write");
        assert_eq!(path_size(temp.path()), 8);
    }

    #[test]
    fn free_space_is_reported_for_a_folder_that_does_not_exist_yet() {
        let future = std::env::temp_dir().join("vk-install-mai-creata").join("x");
        assert!(available_space(&future).is_some_and(|bytes| bytes > 0));
    }

    #[test]
    fn an_empty_directory_is_recognised() {
        let temp = tempfile::tempdir().expect("temp");
        assert!(is_dir_empty(temp.path()));
        std::fs::write(temp.path().join("a"), b"x").expect("write");
        assert!(!is_dir_empty(temp.path()));
    }
}
