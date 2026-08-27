//! Installazione e aggiornamento della modpack.
//!
//! Porta `MainWindow.xaml.cs::{CheckForUpdatesCoreAsync, PerformModInstallation}`
//! mantenendo l'ordine delle fasi: backup → manifest → differenziale →
//! (fallback archivio completo) → scrittura della versione, con rollback
//! automatico su errore.

use std::path::PathBuf;
use std::sync::Arc;

use vk_core::backup::BackupSet;
use vk_core::endpoints::MirrorPlan;
use vk_core::manifest::ModManifest;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
use vk_core::update::{self, Fingerprint, UpdateContext, UpdateMode};
use vk_core::{Channel, CoreError, ModLayout};

use crate::domain::{InstallOutcome, ModStatus};
use crate::error::{AppError, AppResult};
use crate::state::{now_iso, AppState};

/// Scarica `versions.json` e aggiorna lo stato remoto.
pub async fn check_updates(state: &Arc<AppState>) -> AppResult<ModStatus> {
    let endpoints = state.endpoints.read().await.clone();
    let url = crate::storage::endpoints::versions_url(&endpoints)?;
    let no_cache = vk_core::endpoints::add_no_cache_query(&url, vk_core::now_millis());

    match state.downloader.get_string(&no_cache).await {
        Ok(raw) => {
            let info = vk_core::VersionInfo::parse(&raw)?;
            let mut remote = state.remote.write().await;
            remote.info = info;
            remote.checked = true;
            remote.message = format!("Ultimo controllo: {}", short_time());
        }
        Err(error) => {
            let mut remote = state.remote.write().await;
            remote.checked = false;
            remote.message = "Controllo aggiornamenti non riuscito.".into();
            tracing::warn!(error = %vk_core::redact::redact(&error.to_string()), "versions.json non raggiungibile");
        }
    }

    // Un'installazione fatta dal launcher legacy — o da una copia del launcher
    // i cui dati non sono stati importati — sta sul disco senza che noi ne
    // conosciamo la versione. È il momento buono per dedurla: la rete c'è già.
    for channel in [Channel::Stable, Channel::Beta] {
        if let Err(error) = reconcile_installed_version(state, channel).await {
            tracing::warn!(
                ?channel,
                error = %vk_core::redact::redact(&error.to_string()),
                "versione installata non riconciliabile"
            );
        }
    }

    status(state).await
}

/// Nome del file che registra la versione installata accanto ai dati utente
/// della modpack.
///
/// Vive in `<mod_folder>/<Mod>_UserData/`, che è protetto dagli aggiornamenti
/// e non viene mai potato: sopravvive quindi a un aggiornamento, alla
/// reinstallazione del launcher e alla cancellazione dei suoi dati.
const INSTALLED_VERSION_STAMP: &str = "installed_version.txt";

fn version_stamp(layout: &ModLayout) -> PathBuf {
    layout.user_data_root().join(INSTALLED_VERSION_STAMP)
}

async fn read_version_stamp(layout: &ModLayout) -> Option<String> {
    vk_core::fsx::read_text_opt(&version_stamp(layout))
        .await
        .ok()
        .flatten()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

async fn write_version_stamp(layout: &ModLayout, version: &str) {
    if let Err(error) = vk_core::fsx::write_atomic(&version_stamp(layout), version.as_bytes()).await
    {
        // Non è fatale: lo stato strutturato resta la fonte primaria.
        tracing::warn!(
            error = %vk_core::redact::redact(&error.to_string()),
            "timbro della versione non scritto"
        );
    }
}

/// Registra la versione installata di un canale in tutti i posti che la usano.
async fn adopt_installed_version(
    state: &Arc<AppState>,
    channel: Channel,
    version: &str,
) -> AppResult<()> {
    state
        .install_state
        .write()
        .await
        .set(channel, version, now_iso());
    state.persist_install_state().await?;

    write_version_stamp(&state.layout(channel).await, version).await;
    Ok(())
}

/// Deduce la versione di un'installazione che questo launcher non ha eseguito.
///
/// Il launcher legacy tiene `mod_version.txt` accanto al proprio eseguibile: se
/// l'utente lo ha disinstallato, o se ha installato la modpack da un'altra
/// macchina, quel file non c'è e lo stato strutturato resta vuoto. La UI
/// mostrava allora una modpack "installata" con versione ignota e un
/// aggiornamento perennemente disponibile.
///
/// Si prova, in ordine:
/// 1. il timbro locale lasciato da una nostra installazione precedente;
/// 2. il confronto dell'installazione con il manifest remoto: se i file sul
///    disco sono quelli della versione pubblicata, la versione installata è
///    quella.
///
/// Restituisce la versione adottata, oppure `None` se non è deducibile — nel
/// qual caso non si inventa nulla.
pub async fn reconcile_installed_version(
    state: &Arc<AppState>,
    channel: Channel,
) -> AppResult<Option<String>> {
    let layout = state.layout(channel).await;
    if !layout.is_installed() {
        return Ok(None);
    }
    if !state
        .install_state
        .read()
        .await
        .get(channel)
        .version
        .trim()
        .is_empty()
    {
        return Ok(None);
    }

    if let Some(version) = read_version_stamp(&layout).await {
        tracing::info!(?channel, %version, "versione installata letta dal timbro locale");
        adopt_installed_version(state, channel, &version).await?;
        return Ok(Some(version));
    }

    let Some(manifest) = fetch_manifest(state, channel).await else {
        return Ok(None);
    };
    if manifest.mod_version.trim().is_empty() {
        return Ok(None);
    }

    match update::fingerprint(
        &layout.mod_root(),
        &manifest,
        update::FINGERPRINT_SAMPLE_BYTES,
        &CancelToken::new(),
    )
    .await?
    {
        Fingerprint::Matches => {
            let version = manifest.mod_version.clone();
            tracing::info!(
                ?channel,
                %version,
                "versione installata dedotta dal confronto con il manifest"
            );
            adopt_installed_version(state, channel, &version).await?;
            Ok(Some(version))
        }
        Fingerprint::Differs { reason } => {
            tracing::info!(
                ?channel,
                %reason,
                "l'installazione non corrisponde al manifest: versione ignota"
            );
            Ok(None)
        }
    }
}

/// Controlla che il descrittore Riivolution dell'installazione presente possa
/// davvero applicare la modpack.
///
/// Costa la lettura di un file da qualche decina di KB: si può fare a ogni
/// aggiornamento di stato. Restituisce il motivo, vuoto se va tutto bene.
fn repair_reason(layout: &ModLayout) -> String {
    if !layout.is_installed() {
        return String::new();
    }
    match vk_dolphin::modxml::validate(&layout.riivolution_xml(), layout.directory_name()) {
        Ok(()) => String::new(),
        Err(error) => vk_core::redact::redact(&error.to_string()),
    }
}

/// Stato corrente della modpack per il canale selezionato.
pub async fn status(state: &Arc<AppState>) -> AppResult<ModStatus> {
    let channel = state.channel().await;
    let other = match channel {
        Channel::Stable => Channel::Beta,
        Channel::Beta => Channel::Stable,
    };

    let layout = state.layout(channel).await;
    let other_layout = state.layout(other).await;
    let install_state = state.install_state.read().await.clone();
    let remote = state.remote.read().await.clone();

    let installed = layout.is_installed();
    let installed_version = install_state.get(channel).version.clone();
    let latest_version = remote.info.mod_version_for(channel).to_string();
    let repair_reason = repair_reason(&layout);

    Ok(ModStatus {
        channel,
        installed,
        update_available: installed
            && !latest_version.trim().is_empty()
            && !latest_version.eq_ignore_ascii_case(installed_version.trim()),
        installed_version,
        latest_version,
        checked: remote.checked,
        check_message: remote.message.clone(),
        mod_folder: layout.mod_root().to_string_lossy().to_string(),
        other_channel_installed: other_layout.is_installed(),
        other_channel_version: install_state.get(other).version.clone(),
        changelog: remote.info.changelog_for(channel).to_vec(),
        needs_repair: !repair_reason.is_empty(),
        repair_reason,
    })
}

/// Installa o aggiorna la modpack del canale selezionato.
///
/// `force_full` salta il percorso differenziale: è la funzione "Repair".
pub async fn install(
    state: &Arc<AppState>,
    force_full: bool,
    progress: ProgressSink,
) -> AppResult<InstallOutcome> {
    let guard = state.mod_operation.try_lock().map_err(|_| AppError::Busy)?;
    let cancel = state.renew_cancel_token().await;

    let result = install_inner(state, force_full, &progress, &cancel).await;
    drop(guard);

    if let Err(error) = &result {
        progress(ProgressUpdate::new(
            Phase::Error,
            vk_core::redact::redact(&error.to_string()),
        ));
    }
    result
}

async fn install_inner(
    state: &Arc<AppState>,
    force_full: bool,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<InstallOutcome> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;
    let endpoints = state.endpoints.read().await.clone();
    let defaults = crate::storage::endpoints::defaults();
    let concurrency = state.preferences.read().await.effective_concurrency();

    let is_update = layout.is_installed();
    let remote = state.remote.read().await.clone();

    progress(ProgressUpdate::new(
        Phase::Connecting,
        format!("Preparazione del canale {}...", channel.display_name()),
    ));

    let mut context = UpdateContext::new(
        layout.clone(),
        MirrorPlan::for_channel(&endpoints, &defaults, channel),
        state.paths.backups_dir(),
    );
    context.concurrency = concurrency;
    context.is_update = is_update;
    context.expected_archive_sha256 = remote.info.mod_sha256_for(channel).to_string();
    context.staging_root = state
        .paths
        .downloads_dir()
        .join(format!("staging-{}", vk_core::fsx::backup_timestamp()));

    // ── 1. Backup dei dati utente, solo se c'è già un'installazione ──────────
    let backup: Option<BackupSet> = if is_update {
        progress(ProgressUpdate::new(
            Phase::Backup,
            "Salvataggio di licenze, Mii e profili...",
        ));
        let set =
            vk_core::backup::create_backup(&layout, &state.paths.backups_dir(), progress, cancel)
                .await?;
        if !set.is_empty() {
            tracing::info!(
                backup = %set.backup_id,
                files = set.files.len(),
                "backup dei dati utente creato"
            );
        }
        Some(set)
    } else {
        None
    };

    // ── 2. Manifest e piano differenziale ───────────────────────────────────
    let manifest = if is_update && !force_full {
        fetch_manifest(state, channel).await
    } else {
        None
    };

    let mut warnings: Vec<String> = Vec::new();
    let mut fallback_reason: Option<String> = None;

    let attempt = match manifest.as_ref() {
        Some(manifest) => {
            progress(ProgressUpdate::new(
                Phase::Verifying,
                "Confronto con l'installazione locale...",
            ));
            match update::apply_differential(
                &state.downloader,
                &context,
                manifest,
                vk_core::now_millis(),
                progress,
                cancel,
            )
            .await
            {
                Ok(report) => Ok(report),
                Err(CoreError::Cancelled) => Err(AppError::Cancelled),
                Err(error) => {
                    let reason = vk_core::redact::redact(&error.to_string());
                    tracing::warn!(%reason, "aggiornamento differenziale fallito: si passa all'archivio completo");
                    fallback_reason = Some(reason.clone());
                    warnings.push(format!(
                        "Aggiornamento differenziale non riuscito: {reason}"
                    ));
                    progress(ProgressUpdate::new(
                        Phase::Recovery,
                        "Aggiornamento differenziale non riuscito. Download del pacchetto completo...",
                    ));
                    full_archive(state, &context, progress, cancel).await
                }
            }
        }
        None => full_archive(state, &context, progress, cancel).await,
    };

    // ── 3. Rollback automatico se qualcosa è andato storto ──────────────────
    let report = match attempt {
        Ok(report) => report,
        Err(error) => {
            if let Some(set) = backup.as_ref().filter(|set| !set.is_empty()) {
                progress(ProgressUpdate::new(
                    Phase::Rollback,
                    "Errore: ripristino dei dati utente...",
                ));
                match vk_core::backup::restore_backup(set, progress, &CancelToken::new()).await {
                    Ok(count) => tracing::info!(
                        backup = %set.backup_id,
                        files = count,
                        "rollback completato"
                    ),
                    Err(rollback_error) => {
                        tracing::error!(
                            backup = %set.backup_id,
                            error = %vk_core::redact::redact(&rollback_error.to_string()),
                            "rollback fallito: i dati restano nella cartella di backup"
                        );
                        return Err(AppError::Storage(format!(
                            "L'aggiornamento è fallito e il ripristino automatico non è riuscito. \
                             I tuoi dati sono al sicuro nel backup {}. Copiali manualmente prima di riprovare.",
                            set.backup_id
                        )));
                    }
                }
            }
            let _ = tokio::fs::remove_dir_all(&context.staging_root).await;
            return Err(error);
        }
    };

    // ── 4. Registrazione della versione installata ──────────────────────────
    let version = manifest
        .as_ref()
        .map(|manifest| manifest.mod_version.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| remote.info.mod_version_for(channel).to_string());

    if !version.trim().is_empty() {
        adopt_installed_version(state, channel, &version).await?;

        let mut preferences = state.preferences.write().await;
        preferences.set_last_known(channel, &version);
        drop(preferences);
        state.persist_preferences().await?;
    }

    warnings.extend(report.errors.iter().cloned());

    let outcome = InstallOutcome {
        channel,
        was_update: is_update,
        version,
        mode: match report.mode {
            Some(UpdateMode::Differential) => "differential".into(),
            Some(UpdateMode::FullArchive) => "full-archive".into(),
            None => "unknown".into(),
        },
        files_written: report.files_written,
        files_skipped: report.files_skipped,
        files_pruned: report.files_pruned,
        summary: report.summary(),
        warnings,
        backup_id: backup.map(|set| set.backup_id),
    };

    progress(ProgressUpdate::new(Phase::Completed, outcome.summary.clone()).with_percent(100.0));

    tracing::info!(
        channel = ?channel,
        mode = %outcome.mode,
        written = outcome.files_written,
        pruned = outcome.files_pruned,
        fallback = ?fallback_reason,
        "installazione completata"
    );

    Ok(outcome)
}

async fn full_archive(
    state: &Arc<AppState>,
    context: &UpdateContext,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<update::UpdateReport> {
    let archive = state.paths.downloads_dir().join(format!(
        "{}.zip",
        context.layout.channel.mod_directory_name()
    ));

    let result =
        update::apply_full_archive(&state.downloader, context, &archive, progress, cancel).await;

    // L'archivio non serve più in nessun caso.
    let _ = tokio::fs::remove_file(&archive).await;

    match result {
        Ok(report) => Ok(report),
        Err(CoreError::Cancelled) => Err(AppError::Cancelled),
        Err(error) => Err(AppError::Core(error)),
    }
}

/// Scarica e valida il manifest del canale. `None` se non è utilizzabile:
/// in quel caso si procede con l'archivio completo, come nel legacy.
async fn fetch_manifest(state: &Arc<AppState>, channel: Channel) -> Option<ModManifest> {
    let endpoints = state.endpoints.read().await.clone();
    let url = endpoints.manifest_url_for(channel);
    if url.trim().is_empty() {
        return None;
    }

    let no_cache = vk_core::endpoints::add_no_cache_query(url, vk_core::now_millis());
    match state.downloader.get_string(&no_cache).await {
        Ok(raw) => match ModManifest::parse(&raw).and_then(ModManifest::validated) {
            Ok(manifest) => {
                tracing::info!(
                    version = %manifest.mod_version,
                    files = manifest.files.len(),
                    "manifest caricato"
                );
                Some(manifest)
            }
            Err(error) => {
                tracing::warn!(
                    error = %vk_core::redact::redact(&error.to_string()),
                    "manifest non valido: si userà l'archivio completo"
                );
                None
            }
        },
        Err(error) => {
            tracing::warn!(
                error = %vk_core::redact::redact(&error.to_string()),
                "manifest non scaricabile: si userà l'archivio completo"
            );
            None
        }
    }
}

/// Cambia canale, richiedendo il token beta quando serve.
pub async fn set_channel(state: &Arc<AppState>, channel: Channel) -> AppResult<ModStatus> {
    if channel == Channel::Beta && !state.secrets.read().await.has_beta_token() {
        return Err(AppError::Configuration(
            "Il canale Beta richiede un token di accesso valido.".into(),
        ));
    }

    state.preferences.write().await.channel = channel;
    state.persist_preferences().await?;
    status(state).await
}

/// Verifica l'integrità dell'installazione confrontandola con il manifest.
///
/// Non modifica nulla: restituisce l'elenco dei file diversi o mancanti.
pub async fn verify_integrity(state: &Arc<AppState>) -> AppResult<IntegrityReport> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;

    if !layout.is_installed() {
        return Ok(IntegrityReport {
            checked: false,
            message: "La modpack non è installata.".into(),
            ..Default::default()
        });
    }

    let Some(manifest) = fetch_manifest(state, channel).await else {
        return Ok(IntegrityReport {
            checked: false,
            message: "Manifest non disponibile: impossibile verificare l'integrità.".into(),
            ..Default::default()
        });
    };

    let local = vk_core::fsx::scan_managed_files(&layout.mod_root(), &CancelToken::new()).await?;
    let plan = update::diff(&manifest, &local);

    Ok(IntegrityReport {
        checked: true,
        total_files: manifest.files.len(),
        mismatched: plan.to_download.iter().map(|f| f.path.clone()).collect(),
        obsolete: plan.to_delete.clone(),
        message: if plan.is_noop() {
            "L'installazione corrisponde al manifest.".into()
        } else {
            format!(
                "{} file da ripristinare, {} obsoleti.",
                plan.to_download.len(),
                plan.to_delete.len()
            )
        },
    })
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub checked: bool,
    pub total_files: usize,
    pub mismatched: Vec<String>,
    pub obsolete: Vec<String>,
    pub message: String,
}

fn short_time() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let format = time::macros::format_description!("[hour]:[minute]");
    now.format(&format).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use crate::testkit;

    async fn state_with(dir: &std::path::Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn status_reports_a_missing_installation() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let status = status(&state).await.unwrap();
        assert!(!status.installed);
        assert!(!status.update_available);
        assert_eq!(status.channel, Channel::Stable);
        assert_eq!(status.badge(), "Idle");
    }

    #[tokio::test]
    async fn status_detects_an_available_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        // Installazione presente alla 1.4.0.
        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);
        state
            .install_state
            .write()
            .await
            .set(Channel::Stable, "1.4.0", now_iso());

        // Il server annuncia la 1.5.0.
        {
            let mut remote = state.remote.write().await;
            remote.info.mod_version = "1.5.0".into();
            remote.checked = true;
        }

        let status = status(&state).await.unwrap();
        assert!(status.installed);
        assert!(status.update_available);
        assert_eq!(status.installed_version, "1.4.0");
        assert_eq!(status.latest_version, "1.5.0");
        assert_eq!(status.badge(), "Update");
    }

    #[tokio::test]
    async fn the_same_version_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);
        state
            .install_state
            .write()
            .await
            .set(Channel::Stable, "1.5.0", now_iso());
        {
            let mut remote = state.remote.write().await;
            remote.info.mod_version = "1.5.0".into();
            remote.checked = true;
        }

        let status = status(&state).await.unwrap();
        assert!(!status.update_available);
        assert_eq!(status.badge(), "Up to date");
    }

    #[tokio::test]
    async fn a_gutted_descriptor_is_reported_as_needing_repair() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);
        state
            .install_state
            .write()
            .await
            .set(Channel::Stable, "1.2.9", now_iso());
        {
            let mut remote = state.remote.write().await;
            remote.info.mod_version = "1.2.9".into();
            remote.checked = true;
        }

        let healthy = status(&state).await.unwrap();
        assert!(!healthy.needs_repair);
        assert!(healthy.repair_reason.is_empty());
        assert_eq!(healthy.badge(), "Up to date");

        // Il descrittore diventa un `<wiidisc/>` inerte: il file c'è ancora,
        // ma Dolphin avvierebbe Mario Kart Wii originale.
        testkit::break_modpack(&layout);

        let broken = status(&state).await.unwrap();
        assert!(broken.installed);
        assert!(broken.needs_repair);
        assert!(
            broken.repair_reason.contains("nessuna patch"),
            "{}",
            broken.repair_reason
        );
        assert_eq!(broken.badge(), "Repair");
    }

    #[tokio::test]
    async fn a_descriptor_of_the_wrong_channel_needs_repair() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        // Descrittore della Stable messo nella cartella della Beta.
        let beta = state.layout(Channel::Beta).await;
        std::fs::create_dir_all(beta.riivolution_xml().parent().unwrap()).unwrap();
        std::fs::write(
            beta.riivolution_xml(),
            testkit::riivolution_xml("VanzaKart"),
        )
        .unwrap();

        state.secrets.write().await.beta_access_token = "token".into();
        let status = set_channel(&state, Channel::Beta).await.unwrap();

        assert!(status.installed);
        assert!(status.needs_repair);
        assert!(
            status.repair_reason.contains("VKBeta"),
            "{}",
            status.repair_reason
        );
    }

    #[tokio::test]
    async fn an_unknown_version_is_recovered_from_the_local_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        // Installazione presente, stato del launcher vuoto: è la situazione di
        // chi ha installato la modpack col launcher legacy.
        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);
        assert!(status(&state).await.unwrap().installed_version.is_empty());

        std::fs::create_dir_all(layout.user_data_root()).unwrap();
        std::fs::write(version_stamp(&layout), "1.2.9\n").unwrap();

        assert_eq!(
            reconcile_installed_version(&state, Channel::Stable)
                .await
                .unwrap()
                .as_deref(),
            Some("1.2.9")
        );
        assert_eq!(status(&state).await.unwrap().installed_version, "1.2.9");

        // Il valore è persistito: un riavvio del launcher lo ritrova.
        let reloaded = crate::storage::install_state::load(&state.paths)
            .await
            .unwrap();
        assert_eq!(reloaded.stable.version, "1.2.9");
    }

    #[tokio::test]
    async fn reconciliation_never_overwrites_a_known_version() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);
        std::fs::create_dir_all(layout.user_data_root()).unwrap();
        std::fs::write(version_stamp(&layout), "1.0.0").unwrap();

        state
            .install_state
            .write()
            .await
            .set(Channel::Stable, "1.2.9", now_iso());

        assert_eq!(
            reconcile_installed_version(&state, Channel::Stable)
                .await
                .unwrap(),
            None
        );
        assert_eq!(status(&state).await.unwrap().installed_version, "1.2.9");
    }

    #[tokio::test]
    async fn reconciliation_ignores_a_channel_that_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert_eq!(
            reconcile_installed_version(&state, Channel::Stable)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn adopting_a_version_leaves_a_stamp_next_to_the_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let layout = state.layout(Channel::Stable).await;
        testkit::install_modpack(&layout);

        adopt_installed_version(&state, Channel::Stable, "1.2.9")
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(version_stamp(&layout)).unwrap(),
            "1.2.9"
        );
        // Il timbro sta fuori dalla root della modpack, che l'aggiornamento
        // differenziale pota.
        assert!(!version_stamp(&layout).starts_with(layout.mod_root()));
    }

    #[tokio::test]
    async fn status_reports_the_other_channel() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let beta = state.layout(Channel::Beta).await;
        testkit::install_modpack(&beta);
        state
            .install_state
            .write()
            .await
            .set(Channel::Beta, "1.6.0-beta.1", now_iso());

        let status = status(&state).await.unwrap();
        assert!(!status.installed);
        assert!(status.other_channel_installed);
        assert_eq!(status.other_channel_version, "1.6.0-beta.1");
    }

    #[tokio::test]
    async fn switching_to_beta_requires_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let error = set_channel(&state, Channel::Beta).await.unwrap_err();
        assert_eq!(error.code(), "configuration");
        assert_eq!(state.channel().await, Channel::Stable);

        state.secrets.write().await.beta_access_token = "token".into();
        let status = set_channel(&state, Channel::Beta).await.unwrap();
        assert_eq!(status.channel, Channel::Beta);
        assert_eq!(state.channel().await, Channel::Beta);
    }

    #[tokio::test]
    async fn switching_back_to_stable_never_needs_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        state.preferences.write().await.channel = Channel::Beta;

        assert_eq!(
            set_channel(&state, Channel::Stable).await.unwrap().channel,
            Channel::Stable
        );
    }

    #[tokio::test]
    async fn integrity_check_reports_a_missing_installation() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let report = verify_integrity(&state).await.unwrap();
        assert!(!report.checked);
        assert!(report.message.contains("non è installata"));
    }

    #[tokio::test]
    async fn two_installations_cannot_run_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let held = state.mod_operation.lock().await;
        let error = install(&state, false, vk_core::progress::noop_sink())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "busy");
        drop(held);
    }
}
