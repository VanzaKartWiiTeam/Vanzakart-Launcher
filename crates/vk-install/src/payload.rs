//! Srotolamento del pacchetto scaricato dentro la cartella d'installazione.
//!
//! Tre formati, uno per piattaforma: ZIP su Windows, `tar.gz` per i bundle
//! `.app` di macOS, AppImage su Linux. Le protezioni sono le stesse che il
//! launcher applica alle modpack (`vk_core::zipx`, §D-006): niente voci con
//! `..`, niente percorsi assoluti, niente collegamenti simbolici che escono
//! dalla destinazione.

use std::path::{Path, PathBuf};

use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

use crate::error::{InstallError, InstallResult};
use crate::release::PackageFormat;

/// Esito dell'installazione del pacchetto.
#[derive(Debug, Clone)]
pub struct InstalledPayload {
    /// Eseguibile da avviare (o bundle `.app` da aprire).
    pub executable: PathBuf,
    /// Voci di primo livello create dentro la cartella d'installazione.
    pub entries: Vec<PathBuf>,
    /// Byte occupati.
    pub bytes: u64,
}

/// Estrae — o copia — il pacchetto in `destination`.
pub fn install_payload(
    archive: &Path,
    format: PackageFormat,
    destination: &Path,
    declared_executable: &str,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> InstallResult<InstalledPayload> {
    crate::fsops::ensure_dir(destination)?;

    let entries = match format {
        PackageFormat::Zip => extract_zip(archive, destination, progress, cancel)?,
        PackageFormat::TarGz => extract_tar_gz(archive, destination, progress, cancel)?,
        PackageFormat::AppImage => copy_appimage(archive, destination, declared_executable)?,
    };

    let executable = resolve_executable(destination, declared_executable, &entries)?;
    let bytes = entries
        .iter()
        .map(|entry| crate::fsops::path_size(&destination.join(entry)))
        .sum();

    Ok(InstalledPayload {
        executable,
        entries,
        bytes,
    })
}

fn extract_zip(
    archive: &Path,
    destination: &Path,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> InstallResult<Vec<PathBuf>> {
    progress(ProgressUpdate::new(
        Phase::Installing,
        "Extracting the package",
    ));

    let options = vk_core::zipx::ExtractOptions {
        // Un'installazione riscrive tutto: saltare i file identici serve
        // all'aggiornamento differenziale della modpack, non qui.
        skip_identical: false,
        ..Default::default()
    };
    let report = vk_core::zipx::extract_safe(archive, destination, &options, progress, cancel)?;

    let entries = top_level_entries(&report.entry_paths);
    flatten_single_root(destination, &entries)
}

fn extract_tar_gz(
    archive: &Path,
    destination: &Path,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> InstallResult<Vec<PathBuf>> {
    progress(ProgressUpdate::new(
        Phase::Installing,
        "Extracting the bundle",
    ));

    let file = std::fs::File::open(archive).map_err(|error| InstallError::io(archive, error))?;
    let decoder = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut tar = tar::Archive::new(decoder);
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);

    let mut names = Vec::new();
    let entries = tar
        .entries()
        .map_err(|error| InstallError::io(archive, error))?;

    for entry in entries {
        cancel.check()?;
        let mut entry = entry.map_err(|error| InstallError::io(archive, error))?;

        let raw = entry
            .path()
            .map_err(|error| InstallError::io(archive, error))?
            .to_string_lossy()
            .to_string();
        // Stessa validazione delle voci ZIP: niente `..`, niente percorsi
        // assoluti, niente lettere di unità.
        let relative = vk_core::zipx::sanitize_entry_path(&raw)?;
        vk_core::zipx::ensure_within(destination, &destination.join(&relative))?;

        // `unpack_in` rifiuta da sé le voci che uscirebbero dalla cartella e
        // restituisce `false` senza scrivere nulla.
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|error| InstallError::io(destination, error))?;
        if !unpacked {
            return Err(InstallError::Core(vk_core::CoreError::UnsafeArchiveEntry(
                raw,
            )));
        }

        names.push(relative.to_string_lossy().replace('\\', "/"));
    }

    let entries = top_level_entries(&names);
    flatten_single_root(destination, &entries)
}

fn copy_appimage(
    archive: &Path,
    destination: &Path,
    declared_executable: &str,
) -> InstallResult<Vec<PathBuf>> {
    let name = if declared_executable.trim().is_empty() {
        crate::paths::launcher_executable_name().to_string()
    } else {
        vk_core::zipx::sanitize_entry_path(declared_executable)?
            .to_string_lossy()
            .to_string()
    };

    let target = destination.join(&name);
    vk_core::zipx::ensure_within(destination, &target)?;
    if let Some(parent) = target.parent() {
        crate::fsops::ensure_dir(parent)?;
    }
    // Un AppImage in esecuzione non si può sovrascrivere: si toglie prima.
    crate::fsops::remove_path_best_effort(&target);
    std::fs::copy(archive, &target).map_err(|error| InstallError::io(&target, error))?;
    crate::fsops::set_executable(&target)?;

    Ok(vec![PathBuf::from(name)])
}

/// Nomi di primo livello, nell'ordine in cui compaiono.
fn top_level_entries(paths: &[String]) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = Vec::new();
    for path in paths {
        let Some(first) = path.split('/').find(|part| !part.is_empty()) else {
            continue;
        };
        let candidate = PathBuf::from(first);
        if !entries.contains(&candidate) {
            entries.push(candidate);
        }
    }
    entries
}

/// Se l'archivio incarta tutto in una sola cartella, la si toglie di mezzo.
///
/// È la stessa scelta fatta per gli addon (§D-028): l'utente ha scelto una
/// cartella d'installazione, non vuole trovarci dentro un'altra cartella con
/// lo stesso nome.
fn flatten_single_root(destination: &Path, entries: &[PathBuf]) -> InstallResult<Vec<PathBuf>> {
    let [only] = entries else {
        return Ok(entries.to_vec());
    };

    let root = destination.join(only);
    if !root.is_dir() {
        return Ok(entries.to_vec());
    }
    // Un bundle `.app` è una cartella, ma è anche l'applicazione: non si apre.
    if only.extension().is_some_and(|ext| ext == "app") {
        return Ok(entries.to_vec());
    }

    let mut moved = Vec::new();
    let children = std::fs::read_dir(&root).map_err(|error| InstallError::io(&root, error))?;
    for child in children {
        let child = child.map_err(|error| InstallError::io(&root, error))?;
        let target = destination.join(child.file_name());
        crate::fsops::remove_path_best_effort(&target);
        std::fs::rename(child.path(), &target).map_err(|error| InstallError::io(&target, error))?;
        moved.push(PathBuf::from(child.file_name()));
    }

    crate::fsops::remove_path_best_effort(&root);
    Ok(moved)
}

/// Trova l'eseguibile installato.
fn resolve_executable(
    destination: &Path,
    declared: &str,
    entries: &[PathBuf],
) -> InstallResult<PathBuf> {
    if !declared.trim().is_empty() {
        let relative = vk_core::zipx::sanitize_entry_path(declared)?;
        let candidate = destination.join(&relative);
        vk_core::zipx::ensure_within(destination, &candidate)?;
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Ripiego: si cerca fra le voci di primo livello con le regole del setup
    // legacy — "Launcher" nel nome, mai "Setup" né "Uninstaller".
    let mut candidates: Vec<PathBuf> = entries
        .iter()
        .map(|entry| destination.join(entry))
        .filter(|path| looks_like_the_launcher(path))
        .collect();
    candidates.sort();

    candidates
        .into_iter()
        .next()
        .ok_or_else(|| InstallError::ExecutableNotFound(destination.to_path_buf()))
}

fn looks_like_the_launcher(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.contains("uninstall") || lower.contains("setup") {
        return false;
    }

    let is_candidate_kind = if cfg!(windows) {
        lower.ends_with(".exe")
    } else if cfg!(target_os = "macos") {
        lower.ends_with(".app")
    } else {
        lower.ends_with(".appimage") || path.is_file()
    };

    is_candidate_kind && (lower.contains("launcher") || lower.contains("vanzakart"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sink() -> ProgressSink {
        vk_core::progress::noop_sink()
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("zip");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in files {
            writer.start_file(*name, options).expect("voce");
            writer.write_all(content).expect("contenuto");
        }
        writer.finish().expect("chiuso");
    }

    fn write_tar_gz(path: &Path, files: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("tar");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, name, *content)
                .expect("voce");
        }
        builder.into_inner().expect("tar").finish().expect("gz");
    }

    #[test]
    fn a_zip_lands_in_the_install_folder() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("payload.zip");
        write_zip(
            &archive,
            &[
                ("VanzaKart Launcher.exe", b"MZ"),
                ("resources/endpoints.default.json", b"{}"),
            ],
        );

        let destination = temp.path().join("install");
        let payload = install_payload(
            &archive,
            PackageFormat::Zip,
            &destination,
            "VanzaKart Launcher.exe",
            &sink(),
            &CancelToken::new(),
        )
        .expect("installato");

        assert_eq!(
            payload.executable,
            destination.join("VanzaKart Launcher.exe")
        );
        assert!(destination
            .join("resources/endpoints.default.json")
            .exists());
        assert!(payload.bytes > 0);
    }

    #[test]
    fn a_single_wrapping_folder_is_unwrapped() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("payload.zip");
        write_zip(
            &archive,
            &[
                ("VanzaKart Launcher/VanzaKart Launcher.exe", b"MZ"),
                ("VanzaKart Launcher/resources/a.json", b"{}"),
            ],
        );

        let destination = temp.path().join("install");
        let payload = install_payload(
            &archive,
            PackageFormat::Zip,
            &destination,
            "",
            &sink(),
            &CancelToken::new(),
        )
        .expect("installato");

        assert!(destination.join("VanzaKart Launcher.exe").is_file());
        assert!(!destination
            .join("VanzaKart Launcher/VanzaKart Launcher.exe")
            .exists());
        assert!(payload
            .entries
            .contains(&PathBuf::from("VanzaKart Launcher.exe")));
    }

    #[test]
    fn a_tar_gz_keeps_the_app_bundle_folder() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("payload.tar.gz");
        write_tar_gz(
            &archive,
            &[
                ("VanzaKart Launcher.app/Contents/Info.plist", b"<plist/>"),
                ("VanzaKart Launcher.app/Contents/MacOS/launcher", b"bin"),
            ],
        );

        let destination = temp.path().join("Applications");
        let payload = install_payload(
            &archive,
            PackageFormat::TarGz,
            &destination,
            "VanzaKart Launcher.app",
            &sink(),
            &CancelToken::new(),
        )
        .expect("installato");

        assert_eq!(
            payload.entries,
            vec![PathBuf::from("VanzaKart Launcher.app")]
        );
        assert!(destination
            .join("VanzaKart Launcher.app/Contents/Info.plist")
            .is_file());
    }

    /// Un archivio con una voce che risale la gerarchia. `tar::Builder`
    /// rifiuta di scriverla dall'API normale, quindi il nome finisce
    /// direttamente nell'intestazione: è esattamente ciò che farebbe un
    /// archivio costruito per attaccare chi lo estrae.
    fn write_hostile_tar_gz(path: &Path, name: &str, content: &[u8]) {
        let file = std::fs::File::create(path).expect("tar");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        let raw = header.as_mut_bytes();
        raw[..name.len()].copy_from_slice(name.as_bytes());
        let mut header = tar::Header::from_byte_slice(raw).clone();
        header.set_cksum();

        builder.append(&header, content).expect("voce");
        builder.into_inner().expect("tar").finish().expect("gz");
    }

    #[test]
    fn an_entry_that_escapes_the_destination_is_refused() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("cattivo.tar.gz");
        write_hostile_tar_gz(&archive, "../fuori.txt", b"x");

        let destination = temp.path().join("install");
        let error = install_payload(
            &archive,
            PackageFormat::TarGz,
            &destination,
            "",
            &sink(),
            &CancelToken::new(),
        )
        .expect_err("rifiutato");

        assert!(!temp.path().join("fuori.txt").exists());
        assert_eq!(error.code(), "core");
    }

    #[test]
    fn an_appimage_is_copied_and_made_executable() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("launcher.AppImage");
        std::fs::write(&archive, b"ELF").expect("scritto");

        let destination = temp.path().join("opt");
        let payload = install_payload(
            &archive,
            PackageFormat::AppImage,
            &destination,
            "vanzakart-launcher.AppImage",
            &sink(),
            &CancelToken::new(),
        )
        .expect("installato");

        assert_eq!(
            payload.executable,
            destination.join("vanzakart-launcher.AppImage")
        );
        assert!(payload.executable.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&payload.executable)
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
    }

    #[test]
    fn the_uninstaller_is_never_mistaken_for_the_launcher() {
        assert!(!looks_like_the_launcher(Path::new(
            "/opt/VanzaKart Uninstaller.exe"
        )));
        assert!(!looks_like_the_launcher(Path::new(
            "/opt/VanzaKart Setup.exe"
        )));
    }

    #[test]
    fn a_package_without_an_executable_says_so() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = temp.path().join("payload.zip");
        write_zip(&archive, &[("leggimi.txt", b"ciao")]);

        let error = install_payload(
            &archive,
            PackageFormat::Zip,
            &temp.path().join("install"),
            "",
            &sink(),
            &CancelToken::new(),
        )
        .expect_err("niente eseguibile");

        assert_eq!(error.code(), "executable-not-found");
    }
}
