//! Motore di aggiornamento transazionale della modpack.
//!
//! Porta `MainWindow.xaml.cs::PerformModInstallation` e
//! `ModUpdateSafetyService::ApplyZipUpdateAsync`, con la stessa garanzia
//! centrale: **nessun file viene applicato finché tutti i file richiesti non
//! sono stati scaricati e verificati**. Le eliminazioni avvengono solo dopo
//! l'applicazione.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::stream::{self, StreamExt};
use serde::Serialize;

use crate::endpoints::MirrorPlan;
use crate::error::{CoreError, CoreResult};
use crate::fsx;
use crate::hash::{hash_eq, sha256_file};
use crate::manifest::{ModManifest, ModManifestFile};
use crate::net::Downloader;
use crate::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
use crate::protect::{is_protected_relative, ModLayout, ProtectionRules};
use crate::zipx::{self, ExtractOptions};

/// Concorrenza predefinita dei download differenziali (vedi `decisions.md` §D-007).
pub const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateMode {
    /// Solo i file cambiati, presi da `files/` o `_by_sha256/`.
    Differential,
    /// Archivio completo.
    FullArchive,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateReport {
    pub files_written: u32,
    pub files_skipped: u32,
    pub files_pruned: u32,
    pub bytes_downloaded: u64,
    pub errors: Vec<String>,
    pub mode: Option<UpdateMode>,
    pub backup_id: Option<String>,
    /// Motivo del fallback su archivio completo, se avvenuto.
    pub fallback_reason: Option<String>,
}

impl UpdateReport {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Riepilogo con lo stesso fraseggio del launcher legacy.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{} updated, {} skipped (protected), {} removed (obsolete)",
            self.files_written, self.files_skipped, self.files_pruned
        );
        if self.has_errors() {
            text.push_str(&format!(", {} errors", self.errors.len()));
        }
        text
    }
}

/// Piano differenziale: cosa scaricare e cosa eliminare.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DifferentialPlan {
    pub to_download: Vec<ModManifestFile>,
    pub to_delete: Vec<String>,
}

impl DifferentialPlan {
    pub fn download_bytes(&self) -> u64 {
        self.to_download
            .iter()
            .map(|file| file.size.max(0) as u64)
            .sum()
    }

    pub fn is_noop(&self) -> bool {
        self.to_download.is_empty() && self.to_delete.is_empty()
    }
}

/// Confronta il manifest remoto con lo stato locale.
///
/// Un file va scaricato se manca localmente o se l'hash differisce; va
/// eliminato se è presente localmente ma non nel manifest.
pub fn diff(manifest: &ModManifest, local: &[ModManifestFile]) -> DifferentialPlan {
    let local_by_path: HashMap<String, &ModManifestFile> = local
        .iter()
        .map(|file| (file.path.to_lowercase(), file))
        .collect();

    let remote_paths: Vec<String> = manifest
        .files
        .iter()
        .map(|file| file.path.to_lowercase())
        .collect();

    let to_download = manifest
        .files
        .iter()
        .filter(|remote| {
            local_by_path
                .get(&remote.path.to_lowercase())
                .is_none_or(|local| !hash_eq(&local.sha256, &remote.sha256))
        })
        .cloned()
        .collect();

    let to_delete = local
        .iter()
        .filter(|file| !remote_paths.contains(&file.path.to_lowercase()))
        .map(|file| file.path.clone())
        .collect();

    DifferentialPlan {
        to_download,
        to_delete,
    }
}

/// Parametri di un'operazione di installazione o aggiornamento.
#[derive(Debug)]
pub struct UpdateContext {
    pub layout: ModLayout,
    pub mirrors: MirrorPlan,
    pub backup_root: PathBuf,
    pub staging_root: PathBuf,
    pub concurrency: usize,
    /// Hash atteso dell'archivio completo (può essere vuoto).
    pub expected_archive_sha256: String,
    /// `true` quando la modpack risulta già installata.
    pub is_update: bool,
}

impl UpdateContext {
    pub fn new(layout: ModLayout, mirrors: MirrorPlan, backup_root: PathBuf) -> Self {
        let staging_root =
            std::env::temp_dir().join(format!("vanzakart_mod_update_{}", fsx::backup_timestamp()));
        Self {
            layout,
            mirrors,
            backup_root,
            staging_root,
            concurrency: DEFAULT_DOWNLOAD_CONCURRENCY,
            expected_archive_sha256: String::new(),
            is_update: false,
        }
    }

    pub fn mod_root(&self) -> PathBuf {
        self.layout.mod_root()
    }
}

/// Esegue l'aggiornamento differenziale.
///
/// Restituisce errore senza aver toccato l'installazione se anche un solo file
/// non può essere scaricato o verificato: è il chiamante a decidere il fallback
/// sull'archivio completo.
pub async fn apply_differential(
    downloader: &Downloader,
    context: &UpdateContext,
    manifest: &ModManifest,
    now_millis: u128,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<UpdateReport> {
    let mod_root = context.mod_root();

    progress(ProgressUpdate::new(
        Phase::Verifying,
        "Checking the local files...",
    ));
    let local = fsx::scan_managed_files(&mod_root, cancel).await?;
    let plan = diff(manifest, &local);

    let total_bytes = plan.download_bytes().max(1);
    let total_files = plan.to_download.len() as u32;

    tracing::info!(
        manifest_files = manifest.files.len(),
        local_files = local.len(),
        download_files = plan.to_download.len(),
        download_bytes = plan.download_bytes(),
        delete_files = plan.to_delete.len(),
        concurrency = context.concurrency,
        "piano differenziale"
    );

    tokio::fs::create_dir_all(&context.staging_root)
        .await
        .map_err(|e| CoreError::io(&context.staging_root, e))?;

    let outcome = download_all_to_staging(
        downloader,
        context,
        &plan,
        &mod_root,
        total_bytes,
        total_files,
        now_millis,
        progress,
        cancel,
    )
    .await;

    if let Err(error) = outcome {
        let _ = tokio::fs::remove_dir_all(&context.staging_root).await;
        return Err(error);
    }

    // Da qui in poi l'installazione viene modificata: tutti i file sono stati
    // scaricati e verificati.
    progress(ProgressUpdate::new(
        Phase::Installing,
        "Applying the verified files...",
    ));

    let mut report = UpdateReport {
        mode: Some(UpdateMode::Differential),
        bytes_downloaded: plan.download_bytes(),
        ..Default::default()
    };

    for file in &plan.to_download {
        cancel.check()?;
        let relative = file.path.replace('/', std::path::MAIN_SEPARATOR_STR);
        let source = context.staging_root.join(&relative);
        let destination = mod_root.join(&relative);
        zipx::ensure_within(&mod_root, &destination)?;
        fsx::move_file(&source, &destination).await?;
        report.files_written += 1;
    }

    for relative in &plan.to_delete {
        cancel.check()?;
        let target = mod_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        if zipx::ensure_within(&mod_root, &target).is_err() {
            report
                .errors
                .push(format!("obsolete path ignored: {relative}"));
            continue;
        }
        if target.is_file() {
            match tokio::fs::remove_file(&target).await {
                Ok(()) => {
                    report.files_pruned += 1;
                    tracing::info!(path = %relative, "file obsoleto rimosso");
                }
                Err(error) => report.errors.push(format!("removing {relative}: {error}")),
            }
        }
    }

    let rules = ProtectionRules::build(context.layout.clone());
    fsx::remove_empty_directories(&mod_root, &rules);
    let _ = tokio::fs::remove_dir_all(&context.staging_root).await;

    ensure_my_stuff_exists(&context.layout).await?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
async fn download_all_to_staging(
    downloader: &Downloader,
    context: &UpdateContext,
    plan: &DifferentialPlan,
    mod_root: &Path,
    total_bytes: u64,
    total_files: u32,
    now_millis: u128,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<()> {
    let aggregator = Arc::new(ByteAggregator::new(total_bytes));
    let completed_files = Arc::new(AtomicU64::new(0));

    let results = stream::iter(plan.to_download.iter().cloned())
        .map(|file| {
            let aggregator = Arc::clone(&aggregator);
            let completed_files = Arc::clone(&completed_files);
            let progress = Arc::clone(progress);
            async move {
                // Un manifest malevolo non deve poter scrivere fuori dallo
                // staging o, in fase di apply, fuori dalla modpack.
                let relative = file.path.replace('/', std::path::MAIN_SEPARATOR_STR);
                let staged = context.staging_root.join(&relative);
                zipx::ensure_within(&context.staging_root, &staged)?;
                zipx::ensure_within(mod_root, &mod_root.join(&relative))?;

                if staged.exists() {
                    let _ = tokio::fs::remove_file(&staged).await;
                }

                let candidates =
                    context
                        .mirrors
                        .file_candidates(&file.path, &file.sha256, now_millis);
                if candidates.is_empty() {
                    return Err(CoreError::InvalidUrl(format!(
                        "no source for {}",
                        file.path
                    )));
                }

                let file_size = file.size.max(0) as u64;
                let path_key = file.path.clone();
                let per_file_sink: ProgressSink = {
                    let aggregator = Arc::clone(&aggregator);
                    let progress = Arc::clone(&progress);
                    let path_key = path_key.clone();
                    Arc::new(move |update: ProgressUpdate| {
                        let (done, total) =
                            aggregator.report_active(&path_key, update.bytes_done.min(file_size));
                        progress(
                            ProgressUpdate::new(Phase::Download, path_key.clone())
                                .with_bytes(done, total),
                        );
                    })
                };

                downloader
                    .download_with_mirrors(&candidates, &staged, &per_file_sink, cancel)
                    .await?;

                let actual = sha256_file(&staged).await?;
                if !hash_eq(&actual, &file.sha256) {
                    return Err(CoreError::HashMismatch {
                        path: file.path.clone(),
                        expected: file.sha256.to_lowercase(),
                        actual,
                    });
                }

                let (done, total) = aggregator.complete(&path_key, file_size);
                let index = completed_files.fetch_add(1, Ordering::SeqCst) + 1;
                progress(
                    ProgressUpdate::new(Phase::Download, file.path.clone())
                        .with_bytes(done, total)
                        .with_files(index as u32, total_files),
                );

                Ok::<(), CoreError>(())
            }
        })
        .buffer_unordered(context.concurrency.clamp(1, 12))
        .collect::<Vec<_>>()
        .await;

    for result in results {
        result?;
    }

    Ok(())
}

/// Scarica e applica l'archivio completo.
///
/// Porta `ApplyZipUpdateAsync`: estrae preservando i dati utente, poi elimina i
/// file gestiti che l'archivio non contiene più.
pub async fn apply_full_archive(
    downloader: &Downloader,
    context: &UpdateContext,
    archive_path: &Path,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<UpdateReport> {
    let candidates = context.mirrors.archive_candidates();
    if candidates.is_empty() {
        return Err(CoreError::InvalidUrl(
            "no source for the modpack archive".into(),
        ));
    }

    progress(ProgressUpdate::new(
        Phase::Download,
        "Downloading the modpack...",
    ));
    let outcome = downloader
        .download_with_mirrors(&candidates, archive_path, progress, cancel)
        .await?;
    tracing::info!("{}", outcome.summary("archivio completo"));

    progress(ProgressUpdate::new(
        Phase::Verifying,
        "Verifying the archive...",
    ));
    if !context.expected_archive_sha256.trim().is_empty() {
        let actual = sha256_file(archive_path).await?;
        if !hash_eq(&actual, &context.expected_archive_sha256) {
            return Err(CoreError::HashMismatch {
                path: "modpack archive".into(),
                expected: context.expected_archive_sha256.to_lowercase(),
                actual,
            });
        }
    }

    let layout = context.layout.clone();
    let mod_folder = layout.mod_folder.clone();
    let mod_root = layout.mod_root();
    let archive = archive_path.to_path_buf();
    let progress_for_task = Arc::clone(progress);
    let cancel_for_task = cancel.clone();

    let report = tokio::task::spawn_blocking(move || {
        extract_and_prune(
            &archive,
            &mod_folder,
            &mod_root,
            layout,
            &progress_for_task,
            &cancel_for_task,
        )
    })
    .await
    .map_err(|error| CoreError::Network(error.to_string()))??;

    ensure_my_stuff_exists(&context.layout).await?;
    Ok(report)
}

fn extract_and_prune(
    archive: &Path,
    mod_folder: &Path,
    mod_root: &Path,
    layout: ModLayout,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> CoreResult<UpdateReport> {
    let rules = ProtectionRules::build(layout);
    let rules_for_predicate = rules.clone();

    let options = ExtractOptions {
        preserve: Some(Box::new(move |path: &Path| {
            rules_for_predicate.is_protected(path)
        })),
        ..Default::default()
    };

    let extraction = zipx::extract_safe(archive, mod_folder, &options, progress, cancel)?;

    let mut report = UpdateReport {
        files_written: extraction.written,
        files_skipped: extraction.preserved + extraction.skipped_identical,
        mode: Some(UpdateMode::FullArchive),
        ..Default::default()
    };

    // I percorsi dell'archivio sono relativi alla cartella Riivolution; per il
    // prune servono relativi alla root della modpack.
    let mod_root_prefix = format!(
        "{}/",
        mod_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    let archive_paths: Vec<String> = extraction
        .entry_paths
        .iter()
        .filter_map(|path| path.strip_prefix(&mod_root_prefix).map(str::to_lowercase))
        .collect();

    if mod_root.is_dir() {
        for existing in fsx::list_files_recursive(mod_root) {
            cancel.check()?;

            if rules.is_absolute_protected(&existing) {
                continue;
            }
            let relative = fsx::relative_slash(mod_root, &existing);
            if is_protected_relative(&relative) {
                continue;
            }
            if archive_paths.contains(&relative.to_lowercase()) {
                continue;
            }

            match std::fs::remove_file(&existing) {
                Ok(()) => {
                    report.files_pruned += 1;
                    tracing::info!(path = %relative, "file obsoleto rimosso");
                }
                Err(error) => report.errors.push(format!("removing {relative}: {error}")),
            }
        }

        fsx::remove_empty_directories(mod_root, &rules);
    }

    Ok(report)
}

/// La release contiene volutamente una `My Stuff` vuota; ZIP e web server
/// possono ometterla, quindi va ricreata dopo ogni installazione.
async fn ensure_my_stuff_exists(layout: &ModLayout) -> CoreResult<()> {
    let path = layout.my_stuff();
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| CoreError::io(&path, e))
}

/// Aggregatore dei byte scaricati fra i task concorrenti.
struct ByteAggregator {
    total: u64,
    completed: AtomicU64,
    active: Mutex<HashMap<String, u64>>,
}

impl ByteAggregator {
    fn new(total: u64) -> Self {
        Self {
            total,
            completed: AtomicU64::new(0),
            active: Mutex::new(HashMap::new()),
        }
    }

    fn report_active(&self, path: &str, bytes: u64) -> (u64, u64) {
        let active_total = {
            let mut guard = self.active.lock().expect("mutex avvelenato");
            guard.insert(path.to_string(), bytes);
            guard.values().sum::<u64>()
        };
        let done = self.completed.load(Ordering::SeqCst) + active_total;
        (done.min(self.total), self.total)
    }

    fn complete(&self, path: &str, size: u64) -> (u64, u64) {
        let active_total = {
            let mut guard = self.active.lock().expect("mutex avvelenato");
            guard.remove(path);
            guard.values().sum::<u64>()
        };
        let done = self.completed.fetch_add(size, Ordering::SeqCst) + size + active_total;
        (done.min(self.total), self.total)
    }
}

// ---------------------------------------------------------------------------
// Impronta dell'installazione
// ---------------------------------------------------------------------------

/// Byte massimi letti per la verifica a campione dell'impronta.
pub const FINGERPRINT_SAMPLE_BYTES: u64 = 16 * 1024 * 1024;

/// File massimi di cui si verifica l'hash nella verifica a campione.
pub const FINGERPRINT_SAMPLE_FILES: usize = 64;

/// Esito del confronto rapido fra installazione locale e manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fingerprint {
    /// Ogni file gestito del manifest è presente con la dimensione attesa e il
    /// campione di hash verificato corrisponde.
    Matches,
    /// L'installazione non corrisponde: `reason` dice qual è il primo file che
    /// non torna.
    Differs { reason: String },
}

impl Fingerprint {
    pub fn matches(&self) -> bool {
        matches!(self, Self::Matches)
    }

    fn differs(reason: impl Into<String>) -> Self {
        Self::Differs {
            reason: reason.into(),
        }
    }
}

/// Confronto rapido fra l'installazione su disco e un manifest.
///
/// Serve a rispondere a una domanda sola — «i file sul disco sono quelli di
/// *questa* versione?» — senza pagare lo SHA-256 di tutta la modpack, che per
/// VanzaKart significa centinaia di megabyte. Il compromesso è esplicito:
///
/// 1. **dimensione di ogni file gestito**, che costa una `stat` a file ed è
///    già sufficiente a smascherare file troncati, svuotati o sostituiti;
/// 2. **SHA-256 di un campione limitato** dai file più piccoli, entro
///    `sample_bytes`, per non fidarsi della sola dimensione.
///
/// I file protetti (dati utente) sono esclusi: l'aggiornamento non li tocca,
/// quindi possono legittimamente differire dal manifest.
///
/// Non è una verifica di integrità: per quella esiste il confronto completo di
/// [`diff`] su [`crate::fsx::scan_managed_files`].
pub async fn fingerprint(
    mod_root: &Path,
    manifest: &ModManifest,
    sample_bytes: u64,
    cancel: &CancelToken,
) -> CoreResult<Fingerprint> {
    let managed: Vec<&ModManifestFile> = manifest
        .files
        .iter()
        .filter(|file| !is_protected_relative(&file.path))
        .collect();

    if managed.is_empty() {
        return Ok(Fingerprint::differs(
            "the manifest lists no managed files".to_string(),
        ));
    }

    for file in &managed {
        cancel.check()?;
        let path = mod_root.join(&file.path);
        match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.is_file() && metadata.len() == file.size.max(0) as u64 => {}
            Ok(metadata) if metadata.is_file() => {
                return Ok(Fingerprint::differs(format!(
                    "{}: {} bytes instead of {}",
                    file.path,
                    metadata.len(),
                    file.size.max(0)
                )));
            }
            _ => return Ok(Fingerprint::differs(format!("{} is missing", file.path))),
        }
    }

    let mut sample: Vec<&ModManifestFile> = managed;
    sample.sort_by(|a, b| a.size.cmp(&b.size).then_with(|| a.path.cmp(&b.path)));

    let mut budget = sample_bytes;
    for file in sample.into_iter().take(FINGERPRINT_SAMPLE_FILES) {
        cancel.check()?;
        let size = file.size.max(0) as u64;
        if size > budget {
            break;
        }
        budget -= size;

        let actual = sha256_file(mod_root.join(&file.path)).await?;
        if !hash_eq(&actual, &file.sha256) {
            return Ok(Fingerprint::differs(format!(
                "{}: contenuto diverso da quello atteso",
                file.path
            )));
        }
    }

    Ok(Fingerprint::Matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::Channel;

    fn file(path: &str, hash_char: char, size: i64) -> ModManifestFile {
        ModManifestFile {
            path: path.to_string(),
            sha256: hash_char.to_string().repeat(64),
            size,
        }
    }

    fn manifest(files: Vec<ModManifestFile>) -> ModManifest {
        ModManifest {
            mod_version: "1.0.0".into(),
            archive_sha256: String::new(),
            files,
        }
    }

    /// Manifest costruito dai file realmente scritti su disco.
    async fn manifest_of(root: &Path, paths: &[&str]) -> ModManifest {
        let mut files = Vec::new();
        for path in paths {
            let full = root.join(path);
            let size = std::fs::metadata(&full).unwrap().len() as i64;
            files.push(ModManifestFile {
                path: (*path).to_string(),
                sha256: sha256_file(&full).await.unwrap(),
                size,
            });
        }
        manifest(files)
    }

    fn seed(root: &Path, path: &str, content: &[u8]) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    #[tokio::test]
    async fn the_fingerprint_recognises_a_matching_installation() {
        let dir = tempfile::tempdir().unwrap();
        seed(
            dir.path(),
            "Riivolution/VanzaKart.xml",
            b"<wiidisc version=\"1\"/>",
        );
        seed(dir.path(), "VanzaKart/Binaries/Code.pul", b"binario");
        let manifest = manifest_of(
            dir.path(),
            &["Riivolution/VanzaKart.xml", "VanzaKart/Binaries/Code.pul"],
        )
        .await;

        let outcome = fingerprint(
            dir.path(),
            &manifest,
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(outcome, Fingerprint::Matches);
        assert!(outcome.matches());
    }

    #[tokio::test]
    async fn a_truncated_riivolution_xml_breaks_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        seed(
            dir.path(),
            "Riivolution/VanzaKart.xml",
            b"<wiidisc version=\"1\">...</wiidisc>",
        );
        let manifest = manifest_of(dir.path(), &["Riivolution/VanzaKart.xml"]).await;

        // È il caso reale: il descrittore sostituito da un segnaposto vuoto.
        seed(dir.path(), "Riivolution/VanzaKart.xml", b"<wiidisc/>");

        let outcome = fingerprint(
            dir.path(),
            &manifest,
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new(),
        )
        .await
        .unwrap();

        assert!(!outcome.matches());
        let Fingerprint::Differs { reason } = outcome else {
            unreachable!()
        };
        assert!(reason.contains("VanzaKart.xml"), "{reason}");
        assert!(reason.contains("byte"), "{reason}");
    }

    #[tokio::test]
    async fn a_missing_file_breaks_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista");
        let manifest = manifest_of(dir.path(), &["VanzaKart/Tracks/0.szs"]).await;
        std::fs::remove_file(dir.path().join("VanzaKart/Tracks/0.szs")).unwrap();

        let outcome = fingerprint(
            dir.path(),
            &manifest,
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new(),
        )
        .await
        .unwrap();

        assert!(!outcome.matches());
    }

    #[tokio::test]
    async fn same_size_but_different_content_breaks_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista-a");
        let manifest = manifest_of(dir.path(), &["VanzaKart/Tracks/0.szs"]).await;
        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista-b");

        let outcome = fingerprint(
            dir.path(),
            &manifest,
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new(),
        )
        .await
        .unwrap();

        assert!(!outcome.matches());
    }

    #[tokio::test]
    async fn user_data_may_differ_without_breaking_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), "VanzaKart/My Stuff/mio.szs", b"personalizzato");
        seed(dir.path(), "VanzaKart/Binaries/Code.pul", b"binario");
        let manifest = manifest_of(
            dir.path(),
            &["VanzaKart/My Stuff/mio.szs", "VanzaKart/Binaries/Code.pul"],
        )
        .await;

        // L'utente ha sostituito il proprio file: è un dato protetto.
        seed(
            dir.path(),
            "VanzaKart/My Stuff/mio.szs",
            b"completamente diverso e piu lungo",
        );

        assert!(fingerprint(
            dir.path(),
            &manifest,
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new()
        )
        .await
        .unwrap()
        .matches());
    }

    #[tokio::test]
    async fn a_zero_sample_budget_still_checks_every_size() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista-a");
        let manifest = manifest_of(dir.path(), &["VanzaKart/Tracks/0.szs"]).await;

        // Nessun hash verificato: il contenuto alterato a parità di dimensione
        // passa, quello di dimensione diversa no.
        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista-b");
        assert!(fingerprint(dir.path(), &manifest, 0, &CancelToken::new())
            .await
            .unwrap()
            .matches());

        seed(dir.path(), "VanzaKart/Tracks/0.szs", b"pista-lunga");
        assert!(!fingerprint(dir.path(), &manifest, 0, &CancelToken::new())
            .await
            .unwrap()
            .matches());
    }

    #[tokio::test]
    async fn an_empty_manifest_never_matches() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!fingerprint(
            dir.path(),
            &manifest(Vec::new()),
            FINGERPRINT_SAMPLE_BYTES,
            &CancelToken::new()
        )
        .await
        .unwrap()
        .matches());
    }

    #[test]
    fn diff_downloads_new_and_changed_files() {
        let remote = manifest(vec![
            file("a.szs", 'a', 10),
            file("b.szs", 'b', 20),
            file("c.szs", 'c', 30),
        ]);
        let local = vec![
            file("a.szs", 'a', 10), // identico
            file("b.szs", 'x', 20), // cambiato
        ];

        let plan = diff(&remote, &local);

        let paths: Vec<&str> = plan.to_download.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["b.szs", "c.szs"]);
        assert!(plan.to_delete.is_empty());
        assert_eq!(plan.download_bytes(), 50);
    }

    #[test]
    fn diff_deletes_files_the_server_dropped() {
        let remote = manifest(vec![file("a.szs", 'a', 10)]);
        let local = vec![file("a.szs", 'a', 10), file("old.szs", 'o', 5)];

        let plan = diff(&remote, &local);

        assert!(plan.to_download.is_empty());
        assert_eq!(plan.to_delete, vec!["old.szs"]);
        assert!(!plan.is_noop());
    }

    #[test]
    fn diff_is_case_insensitive_on_paths_and_hashes() {
        let remote = manifest(vec![ModManifestFile {
            path: "A/B.szs".into(),
            sha256: "A".repeat(64),
            size: 1,
        }]);
        let local = vec![ModManifestFile {
            path: "a/b.szs".into(),
            sha256: "a".repeat(64),
            size: 1,
        }];

        assert!(diff(&remote, &local).is_noop());
    }

    #[test]
    fn diff_of_an_empty_installation_downloads_everything() {
        let remote = manifest(vec![file("a.szs", 'a', 10), file("b.szs", 'b', 20)]);
        let plan = diff(&remote, &[]);
        assert_eq!(plan.to_download.len(), 2);
        assert_eq!(plan.download_bytes(), 30);
    }

    #[test]
    fn report_summary_keeps_the_legacy_shape() {
        let report = UpdateReport {
            files_written: 12,
            files_skipped: 3,
            files_pruned: 1,
            errors: vec!["x".into()],
            ..Default::default()
        };
        assert_eq!(
            report.summary(),
            "12 updated, 3 skipped (protected), 1 removed (obsolete), 1 errors"
        );
    }

    #[test]
    fn byte_aggregator_never_exceeds_the_total() {
        let aggregator = ByteAggregator::new(100);
        assert_eq!(aggregator.report_active("a", 40), (40, 100));
        assert_eq!(aggregator.report_active("b", 30), (70, 100));
        assert_eq!(aggregator.complete("a", 50), (80, 100));
        assert_eq!(aggregator.complete("b", 60), (100, 100));
    }

    #[tokio::test]
    async fn context_defaults_use_the_documented_concurrency() {
        let context = UpdateContext::new(
            ModLayout::new("/riiv", Channel::Stable),
            MirrorPlan::default(),
            PathBuf::from("/backups"),
        );
        assert_eq!(context.concurrency, DEFAULT_DOWNLOAD_CONCURRENCY);
        assert_eq!(context.mod_root(), PathBuf::from("/riiv/VanzaKart"));
    }

    #[tokio::test]
    async fn differential_fails_before_touching_the_installation() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ModLayout::new(dir.path(), Channel::Stable);
        let mod_root = layout.mod_root();
        std::fs::create_dir_all(mod_root.join("Riivolution")).unwrap();
        std::fs::write(mod_root.join("Riivolution/VanzaKart.xml"), b"<xml/>").unwrap();

        // Manifest che chiede un file da un mirror inesistente.
        let remote = manifest(vec![file("Riivolution/nuovo.xml", 'd', 4)]);
        let mut context = UpdateContext::new(
            layout,
            MirrorPlan {
                files_url: "https://127.0.0.1:9/files/".into(),
                ..Default::default()
            },
            dir.path().join("Backups"),
        );
        context.staging_root = dir.path().join("staging");
        context.concurrency = 2;

        let downloader = Downloader::new("test").unwrap().with_retries(1);
        let error = apply_differential(
            &downloader,
            &context,
            &remote,
            0,
            &crate::progress::noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap_err();

        assert!(matches!(error, CoreError::AllMirrorsFailed(_)));
        // L'installazione esistente è intatta e lo staging è stato ripulito.
        assert!(mod_root.join("Riivolution/VanzaKart.xml").is_file());
        assert!(!context.staging_root.exists());
    }
}
