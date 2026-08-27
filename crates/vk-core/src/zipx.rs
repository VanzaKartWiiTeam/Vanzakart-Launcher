//! Lettura ed estrazione sicura di archivi ZIP.
//!
//! Sostituisce `Launcher/Services/ArchiveService.cs` irrobustendo i controlli:
//! oltre al confronto sul prefisso della destinazione (unica difesa del legacy)
//! qui si rifiutano percorsi assoluti, componenti `..`, prefissi di device
//! Windows, entry symlink e archivi che superano un limite di espansione.
//!
//! Le funzioni sono sincrone: vanno invocate da `spawn_blocking`.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, CoreResult};
use crate::hash::sha256_file_sync;
use crate::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

/// Limite di espansione predefinito: 8 GiB. La modpack completa sta ampiamente
/// sotto; oltre questa soglia si tratta quasi certamente di una zip-bomb.
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Descrizione di una voce dell'archivio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    /// Percorso relativo normalizzato con `/`.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Riepilogo dell'estrazione, con gli stessi contatori del legacy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractReport {
    pub written: u32,
    pub skipped_identical: u32,
    pub preserved: u32,
    pub directories: u32,
    /// Percorsi relativi (normalizzati) contenuti nell'archivio.
    pub entry_paths: Vec<String>,
}

/// Predicato che decide se un file già presente va lasciato intatto.
pub type PreservePredicate = Box<dyn Fn(&Path) -> bool + Send + Sync>;

pub struct ExtractOptions {
    /// Limite di byte scritti complessivi.
    pub max_total_bytes: u64,
    /// Se `true`, non riscrive i file il cui contenuto coincide già (hash).
    pub skip_identical: bool,
    /// File esistenti da preservare (dati utente).
    pub preserve: Option<PreservePredicate>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            skip_identical: true,
            preserve: None,
        }
    }
}

impl std::fmt::Debug for ExtractOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractOptions")
            .field("max_total_bytes", &self.max_total_bytes)
            .field("skip_identical", &self.skip_identical)
            .field("preserve", &self.preserve.is_some())
            .finish()
    }
}

/// Normalizza e valida il percorso di una voce di archivio.
///
/// Restituisce un percorso relativo composto solo da componenti normali.
pub fn sanitize_entry_path(raw: &str) -> CoreResult<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let trimmed = normalized.trim();

    if trimmed.is_empty() {
        return Err(CoreError::UnsafeArchiveEntry("percorso vuoto".into()));
    }
    if trimmed.starts_with('/') {
        return Err(CoreError::UnsafeArchiveEntry(format!(
            "percorso assoluto: {raw}"
        )));
    }
    // Prefisso di device o unità Windows (`C:/…`, `\\?\…` già normalizzato in `//?/…`).
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        return Err(CoreError::UnsafeArchiveEntry(format!(
            "percorso con unità: {raw}"
        )));
    }

    let mut out = PathBuf::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                return Err(CoreError::UnsafeArchiveEntry(format!(
                    "risalita di directory: {raw}"
                )))
            }
            other => out.push(other),
        }
    }

    if out.as_os_str().is_empty() {
        return Err(CoreError::UnsafeArchiveEntry(format!(
            "percorso vuoto dopo la normalizzazione: {raw}"
        )));
    }

    // Difesa in profondità: nessuna componente non-normale deve sopravvivere.
    if out
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CoreError::UnsafeArchiveEntry(format!(
            "componente di percorso non consentita: {raw}"
        )));
    }

    Ok(out)
}

/// Verifica che `candidate` resti dentro `root` una volta risolto.
///
/// Non richiede che il file esista: canonicalizza l'antenato esistente più
/// vicino e ricompone il resto.
pub fn ensure_within(root: &Path, candidate: &Path) -> CoreResult<PathBuf> {
    let root_real = canonical_or_self(root);
    let candidate_real = canonical_or_self(candidate);

    if candidate_real == root_real || candidate_real.starts_with(&root_real) {
        Ok(candidate_real)
    } else {
        Err(CoreError::UnsafePath(format!(
            "{} è fuori da {}",
            candidate.display(),
            root.display()
        )))
    }
}

fn canonical_or_self(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();

    loop {
        if let Ok(resolved) = dunce_canonicalize(existing) {
            let mut out = resolved;
            for segment in tail.iter().rev() {
                out.push(segment);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// `canonicalize` senza il prefisso UNC `\\?\` di Windows, che romperebbe i
/// confronti con i percorsi costruiti a mano.
fn dunce_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let resolved = path.canonicalize()?;
    #[cfg(windows)]
    {
        let text = resolved.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            if !stripped.starts_with("UNC\\") {
                return Ok(PathBuf::from(stripped));
            }
        }
    }
    Ok(resolved)
}

/// Elenca le voci di un archivio, rifiutando quelle non sicure.
pub fn list_entries(zip_path: &Path) -> CoreResult<Vec<EntryInfo>> {
    let file = File::open(zip_path).map_err(|e| CoreError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

    let mut out = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;
        let is_dir = entry.is_dir();
        let sanitized = sanitize_entry_path(entry.name())?;
        out.push(EntryInfo {
            path: sanitized.to_string_lossy().replace('\\', "/"),
            is_dir,
            size: entry.size(),
        });
    }

    Ok(out)
}

/// Legge integralmente ogni voce per verificare che l'archivio non sia
/// troncato o corrotto (equivalente di `ValidateZipAsync`).
pub fn validate_archive(zip_path: &Path, cancel: &CancelToken) -> CoreResult<u64> {
    let file = File::open(zip_path).map_err(|e| CoreError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

    if archive.is_empty() {
        return Err(CoreError::InvalidArchive(
            "l'archivio scaricato è vuoto".into(),
        ));
    }

    let mut total = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];

    for index in 0..archive.len() {
        cancel.check()?;
        let mut entry = archive
            .by_index(index)
            .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

        if entry.is_dir() {
            continue;
        }
        sanitize_entry_path(entry.name())?;

        loop {
            let read = entry
                .read(&mut buffer)
                .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;
            if read == 0 {
                break;
            }
            total += read as u64;
        }
    }

    Ok(total)
}

/// Estrae l'archivio in `destination` applicando tutte le protezioni.
pub fn extract_safe(
    zip_path: &Path,
    destination: &Path,
    options: &ExtractOptions,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<ExtractReport> {
    std::fs::create_dir_all(destination).map_err(|e| CoreError::io(destination, e))?;

    let file = File::open(zip_path).map_err(|e| CoreError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

    let total_entries = archive.len().max(1);
    let mut report = ExtractReport::default();
    let mut written_bytes = 0u64;
    let mut seen: HashSet<String> = HashSet::new();

    for index in 0..archive.len() {
        cancel.check()?;

        let mut entry = archive
            .by_index(index)
            .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

        // Le entry symlink possono puntare fuori dalla destinazione anche con
        // un nome innocuo: vengono rifiutate del tutto.
        if let Some(mode) = entry.unix_mode() {
            if mode & 0xF000 == 0xA000 {
                return Err(CoreError::UnsafeArchiveEntry(format!(
                    "collegamento simbolico: {}",
                    entry.name()
                )));
            }
        }

        let relative = sanitize_entry_path(entry.name())?;
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let target = destination.join(&relative);
        ensure_within(destination, &target)?;

        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| CoreError::io(&target, e))?;
            report.directories += 1;
        } else {
            if seen.insert(relative_text.to_lowercase()) {
                report.entry_paths.push(relative_text.clone());
            }

            written_bytes += entry.size();
            if written_bytes > options.max_total_bytes {
                return Err(CoreError::InvalidArchive(format!(
                    "l'archivio supera il limite di espansione di {} byte",
                    options.max_total_bytes
                )));
            }

            let preserved = options
                .preserve
                .as_ref()
                .is_some_and(|predicate| target.exists() && predicate(&target));

            if preserved {
                report.preserved += 1;
            } else if options.skip_identical && same_size_as_existing(&target, entry.size()) {
                // Il contenuto di una voce ZIP non è rileggibile: si estrae in un
                // file temporaneo accanto alla destinazione e si confrontano gli
                // hash, così un file identico non viene riscritto ma un file
                // diverso non resta mai troncato.
                let temp = temporary_sibling(&target);
                extract_entry_to(&mut entry, &temp)?;

                let extracted = sha256_file_sync(&temp)?;
                let existing = sha256_file_sync(&target)?;

                if extracted == existing {
                    let _ = std::fs::remove_file(&temp);
                    report.skipped_identical += 1;
                } else {
                    clear_blocking_attributes(&target);
                    std::fs::rename(&temp, &target).map_err(|e| CoreError::io(&target, e))?;
                    report.written += 1;
                }
            } else {
                extract_entry_to(&mut entry, &target)?;
                report.written += 1;
            }
        }

        let done = index + 1;
        progress(
            ProgressUpdate::new(Phase::Installing, relative_text)
                .with_percent(done as f64 / total_entries as f64 * 100.0)
                .with_files(done as u32, total_entries as u32),
        );
    }

    Ok(report)
}

/// Estrae una singola voce in memoria. Usato per leggere metadati dentro un
/// addon senza scriverlo su disco.
pub fn read_entry(zip_path: &Path, entry_path: &str) -> CoreResult<Vec<u8>> {
    let file = File::open(zip_path).map_err(|e| CoreError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;
    let mut entry = archive
        .by_name(entry_path)
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;

    let mut buffer = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut buffer)
        .map_err(|e| CoreError::InvalidArchive(e.to_string()))?;
    Ok(buffer)
}

fn same_size_as_existing(target: &Path, size: u64) -> bool {
    std::fs::metadata(target).is_ok_and(|metadata| metadata.len() == size)
}

fn temporary_sibling(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "entry".to_string());
    target.with_file_name(format!(".{name}.vk-extract.tmp"))
}

fn extract_entry_to(entry: &mut zip::read::ZipFile<'_>, target: &Path) -> CoreResult<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    clear_blocking_attributes(target);

    let mut out = File::create(target).map_err(|e| CoreError::io(target, e))?;
    std::io::copy(entry, &mut out).map_err(|e| CoreError::io(target, e))?;
    out.flush().map_err(|e| CoreError::io(target, e))?;
    Ok(())
}

/// Rimuove gli attributi read-only/hidden/system che impedirebbero la
/// sovrascrittura (equivalente di `ClearBlockingAttributes` del legacy).
pub fn clear_blocking_attributes(path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn build_zip(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("archive.zip");
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        for (name, content) in entries {
            if name.ends_with('/') {
                writer.add_directory(*name, options).unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(content).unwrap();
            }
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn sanitizes_ordinary_paths() {
        assert_eq!(
            sanitize_entry_path("a/b/c.szs").unwrap(),
            PathBuf::from("a").join("b").join("c.szs")
        );
        assert_eq!(
            sanitize_entry_path("a\\b\\c.szs").unwrap(),
            PathBuf::from("a").join("b").join("c.szs")
        );
        assert_eq!(
            sanitize_entry_path("./a//b.szs").unwrap(),
            PathBuf::from("a").join("b.szs")
        );
    }

    #[test]
    fn rejects_zip_slip_variants() {
        for name in [
            "../evil.txt",
            "a/../../evil.txt",
            "/etc/passwd",
            "..\\..\\evil.txt",
            "C:/Windows/System32/evil.dll",
            "c:evil.txt",
            "",
            "   ",
            "./",
        ] {
            assert!(
                sanitize_entry_path(name).is_err(),
                "avrebbe dovuto rifiutare {name:?}"
            );
        }
    }

    #[test]
    fn extraction_rejects_a_traversal_entry() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("../escaped.txt", b"boom")]);
        let dest = dir.path().join("out");

        let error = extract_safe(
            &zip,
            &dest,
            &ExtractOptions::default(),
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .unwrap_err();

        assert!(matches!(error, CoreError::UnsafeArchiveEntry(_)));
        assert!(!dir.path().join("escaped.txt").exists());
    }

    #[test]
    fn extracts_files_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(
            dir.path(),
            &[
                ("VanzaKart/", b""),
                ("VanzaKart/Riivolution/VanzaKart.xml", b"<xml/>"),
                ("VanzaKart/VanzaKart/My Stuff/", b""),
            ],
        );
        let dest = dir.path().join("out");

        let report = extract_safe(
            &zip,
            &dest,
            &ExtractOptions::default(),
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .unwrap();

        assert_eq!(report.written, 1);
        assert_eq!(report.directories, 2);
        assert_eq!(
            report.entry_paths,
            vec!["VanzaKart/Riivolution/VanzaKart.xml"]
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("VanzaKart/Riivolution/VanzaKart.xml")).unwrap(),
            "<xml/>"
        );
        assert!(dest.join("VanzaKart/VanzaKart/My Stuff").is_dir());
    }

    #[test]
    fn skips_files_whose_content_already_matches() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("a.txt", b"same"), ("b.txt", b"new")]);
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("a.txt"), b"same").unwrap();

        let report = extract_safe(
            &zip,
            &dest,
            &ExtractOptions::default(),
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .unwrap();

        assert_eq!(report.skipped_identical, 1);
        assert_eq!(report.written, 1);
    }

    #[test]
    fn preserves_user_files() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("My Stuff/custom.szs", b"from-server")]);
        let dest = dir.path().join("out");
        std::fs::create_dir_all(dest.join("My Stuff")).unwrap();
        std::fs::write(dest.join("My Stuff/custom.szs"), b"mine").unwrap();

        let options = ExtractOptions {
            preserve: Some(Box::new(|path: &Path| {
                path.to_string_lossy().contains("My Stuff")
            })),
            ..Default::default()
        };

        let report = extract_safe(
            &zip,
            &dest,
            &options,
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .unwrap();

        assert_eq!(report.preserved, 1);
        assert_eq!(report.written, 0);
        assert_eq!(
            std::fs::read_to_string(dest.join("My Stuff/custom.szs")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn enforces_the_expansion_limit() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("big.bin", &vec![0u8; 4096])]);
        let options = ExtractOptions {
            max_total_bytes: 100,
            ..Default::default()
        };

        let error = extract_safe(
            &zip,
            &dir.path().join("out"),
            &options,
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .unwrap_err();

        assert!(matches!(error, CoreError::InvalidArchive(_)));
    }

    #[test]
    fn validates_and_lists_entries() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("a/", b""), ("a/b.txt", b"1234")]);

        assert_eq!(validate_archive(&zip, &CancelToken::new()).unwrap(), 4);

        let entries = list_entries(&zip).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].path, "a/b.txt");
        assert_eq!(entries[1].size, 4);

        assert_eq!(read_entry(&zip, "a/b.txt").unwrap(), b"1234");
    }

    #[test]
    fn rejects_an_empty_archive() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[]);
        assert!(matches!(
            validate_archive(&zip, &CancelToken::new()),
            Err(CoreError::InvalidArchive(_))
        ));
    }

    #[test]
    fn ensure_within_detects_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();

        ensure_within(&root, &root.join("a/b.txt")).unwrap();
        assert!(ensure_within(&root, &dir.path().join("outside.txt")).is_err());
    }

    #[test]
    fn cancellation_stops_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let zip = build_zip(dir.path(), &[("a.txt", b"1")]);
        let cancel = CancelToken::new();
        cancel.cancel();

        assert!(matches!(
            extract_safe(
                &zip,
                &dir.path().join("out"),
                &ExtractOptions::default(),
                &crate::progress::noop_sink(),
                &cancel
            ),
            Err(CoreError::Cancelled)
        ));
    }
}
