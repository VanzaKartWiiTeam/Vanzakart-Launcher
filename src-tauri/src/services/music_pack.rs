//! Music pack ufficiale.
//!
//! Porta `Launcher/Services/MusicPackService.cs`. Il music pack è un **addon
//! gestito** con un identificatore fisso: si scarica come la modpack — mirror
//! a cascata, verifica SHA-256, aggiornamento differenziale quando è già
//! installato — ma si installa come un addon, quindi resta disattivabile e
//! rimovibile senza lasciare file orfani in `My Stuff`.
//!
//! L'ordine di un aggiornamento differenziale è vincolato dalla regola che
//! vale per tutti i download: **niente viene applicato prima che l'intero
//! aggiornamento sia stato scaricato e verificato.** Solo dopo l'addon viene
//! disattivato, il payload sostituito e l'addon riattivato.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use vk_core::endpoints::MirrorPlan;
use vk_core::manifest::{ModManifest, ModManifestFile};
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
use vk_core::{Channel, CoreError, ModLayout};

use crate::error::{AppError, AppResult};
use crate::services::addons::{self, ImportRequest, OFFICIAL_MUSIC_PACK_ID};
use crate::state::AppState;

/// Nome mostrato nella libreria addon, lo stesso del launcher legacy.
const DISPLAY_NAME: &str = "VanzaKart Music Pack";
const AUTHOR: &str = "VanzaKart Team";
const SOURCE: &str = "Official VanzaKart package";

/// Stato del music pack per il canale selezionato.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPackStatus {
    pub installed: bool,
    pub enabled: bool,
    pub installed_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub file_count: usize,
    pub changelog: Vec<String>,
    /// Vuoto quando il music pack è installabile; altrimenti spiega perché no.
    pub blocker: String,
}

/// Esito di un'installazione o di un aggiornamento.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPackOutcome {
    pub mode: String,
    pub version: String,
    pub files_written: u32,
    pub files_pruned: u32,
    pub summary: String,
}

/// Nome del file di versione legacy, riscritto dopo ogni installazione perché
/// un ritorno al launcher C# trovi lo stato corretto.
const fn legacy_version_file(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "musicpack_version.txt",
        Channel::Beta => "musicpack_beta_version.txt",
    }
}

fn version_file(state: &Arc<AppState>, channel: Channel) -> PathBuf {
    state.paths.root().join(legacy_version_file(channel))
}

async fn read_installed_version(state: &Arc<AppState>, channel: Channel) -> String {
    vk_core::fsx::read_text_opt(&version_file(state, channel))
        .await
        .ok()
        .flatten()
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

async fn write_installed_version(
    state: &Arc<AppState>,
    channel: Channel,
    version: &str,
) -> AppResult<()> {
    if version.trim().is_empty() {
        return Ok(());
    }
    vk_core::fsx::write_atomic(&version_file(state, channel), version.trim().as_bytes()).await?;
    Ok(())
}

/// Stato corrente, senza toccare la rete.
pub async fn status(state: &Arc<AppState>) -> AppResult<MusicPackStatus> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;
    let remote = state.remote.read().await.clone();

    let manifest = addons::read_manifest(&layout, OFFICIAL_MUSIC_PACK_ID)
        .await
        .ok();
    let installed_version = read_installed_version(state, channel).await;
    let latest_version = remote.info.music_pack_version.trim().to_string();

    let blocker = if !layout.is_installed() {
        format!(
            "Installa prima la modpack {}: il music pack va dentro la sua cartella My Stuff.",
            channel.display_name()
        )
    } else {
        String::new()
    };

    Ok(MusicPackStatus {
        installed: manifest.is_some(),
        enabled: manifest
            .as_ref()
            .is_some_and(|manifest| manifest.is_enabled),
        file_count: manifest.as_ref().map_or(0, |manifest| manifest.files.len()),
        update_available: manifest.is_some()
            && !latest_version.is_empty()
            && !installed_version.is_empty()
            && installed_version != latest_version,
        installed_version,
        latest_version,
        changelog: remote.info.music_pack_changelog.clone(),
        blocker,
    })
}

/// Installa il music pack, o lo aggiorna se è già presente.
pub async fn install(state: &Arc<AppState>, progress: ProgressSink) -> AppResult<MusicPackOutcome> {
    let guard = state.mod_operation.try_lock().map_err(|_| AppError::Busy)?;
    let cancel = state.renew_cancel_token().await;

    let result = install_inner(state, &progress, &cancel).await;
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
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<MusicPackOutcome> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;

    if !layout.is_installed() {
        return Err(AppError::Configuration(format!(
            "Installa prima la modpack {}: il music pack va dentro la sua cartella My Stuff.",
            channel.display_name()
        )));
    }

    let endpoints = state.endpoints.read().await.clone();
    let defaults = crate::storage::endpoints::defaults();
    let plan = MirrorPlan::for_music_pack(&endpoints, &defaults);
    let version = state
        .remote
        .read()
        .await
        .info
        .music_pack_version
        .trim()
        .to_string();

    let already_installed = addons::read_manifest(&layout, OFFICIAL_MUSIC_PACK_ID)
        .await
        .is_ok();

    progress(ProgressUpdate::new(
        Phase::Connecting,
        if already_installed {
            "Controllo degli aggiornamenti del music pack..."
        } else {
            "Preparazione del music pack..."
        },
    ));

    // Il percorso differenziale vale solo per un music pack già installato, e
    // solo se il manifest è utilizzabile. In ogni altro caso si scarica
    // l'archivio completo, come nel legacy.
    if already_installed {
        if let Some(manifest) = fetch_manifest(state).await {
            match update_differential(state, &layout, &plan, &manifest, progress, cancel).await {
                Ok(outcome) => {
                    write_installed_version(
                        state,
                        channel,
                        &effective_version(&version, &manifest),
                    )
                    .await?;
                    progress(ProgressUpdate::new(
                        Phase::Completed,
                        outcome.summary.clone(),
                    ));
                    return Ok(outcome);
                }
                Err(AppError::Cancelled) => return Err(AppError::Cancelled),
                Err(error) => {
                    let reason = vk_core::redact::redact(&error.to_string());
                    tracing::warn!(%reason, "aggiornamento differenziale del music pack fallito: archivio completo");
                    progress(ProgressUpdate::new(
                        Phase::Recovery,
                        "Aggiornamento differenziale non riuscito. Download del pacchetto completo...",
                    ));
                }
            }
        }
    }

    let outcome = install_full(state, &layout, &plan, progress, cancel).await?;
    write_installed_version(state, channel, &version).await?;
    progress(ProgressUpdate::new(
        Phase::Completed,
        outcome.summary.clone(),
    ));
    Ok(outcome)
}

/// La versione del manifest ha la precedenza: è quella dei file appena
/// applicati. `versions.json` è solo il valore annunciato.
fn effective_version(announced: &str, manifest: &ModManifest) -> String {
    let from_manifest = manifest.mod_version.trim();
    if from_manifest.is_empty() {
        announced.to_string()
    } else {
        from_manifest.to_string()
    }
}

// ---------------------------------------------------------------------------
// Archivio completo
// ---------------------------------------------------------------------------

async fn install_full(
    state: &Arc<AppState>,
    layout: &ModLayout,
    plan: &MirrorPlan,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<MusicPackOutcome> {
    let candidates = plan.archive_candidates();
    if candidates.is_empty() {
        return Err(AppError::Configuration(
            "Nessun URL configurato per il music pack.".into(),
        ));
    }

    let archive = state.paths.downloads_dir().join("vanzakart_musicpack.zip");
    tokio::fs::create_dir_all(state.paths.downloads_dir())
        .await
        .map_err(|error| AppError::io(state.paths.downloads_dir(), error))?;

    progress(ProgressUpdate::new(
        Phase::Download,
        "Download del music pack ufficiale...",
    ));

    let download = state
        .downloader
        .download_with_mirrors(&candidates, &archive, progress, cancel)
        .await;

    let result = async {
        download.map_err(map_core)?;

        progress(ProgressUpdate::new(
            Phase::Verifying,
            "Verifica dell'integrità dell'archivio...",
        ));

        let expected = state
            .remote
            .read()
            .await
            .info
            .music_pack_sha256
            .trim()
            .to_string();
        if !expected.is_empty() {
            let actual = vk_core::hash::sha256_file(&archive).await?;
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(AppError::Core(CoreError::HashMismatch {
                    path: "vanzakart_musicpack.zip".into(),
                    expected,
                    actual,
                }));
            }
        }

        progress(ProgressUpdate::new(
            Phase::Installing,
            "Estrazione e installazione in My Stuff...",
        ));

        let addon = addons::import_archive_as(
            layout,
            &archive,
            ImportRequest {
                id: OFFICIAL_MUSIC_PACK_ID.to_string(),
                name: DISPLAY_NAME.into(),
                author: AUTHOR.into(),
                source: SOURCE.into(),
                replace_existing: true,
                ..Default::default()
            },
        )
        .await?;

        addons::set_enabled(layout, OFFICIAL_MUSIC_PACK_ID, true).await?;
        Ok(addon)
    }
    .await;

    // L'archivio non serve più, né in caso di successo né in caso di errore.
    let _ = tokio::fs::remove_file(&archive).await;
    let addon = result?;

    tracing::info!(files = addon.file_count, "music pack installato");
    Ok(MusicPackOutcome {
        mode: "full".into(),
        version: String::new(),
        files_written: addon.file_count as u32,
        files_pruned: 0,
        summary: format!("Music pack installato: {} file.", addon.file_count),
    })
}

// ---------------------------------------------------------------------------
// Aggiornamento differenziale
// ---------------------------------------------------------------------------

async fn fetch_manifest(state: &Arc<AppState>) -> Option<ModManifest> {
    let url = state
        .endpoints
        .read()
        .await
        .music_pack_manifest_url
        .trim()
        .to_string();
    if url.is_empty() {
        return None;
    }

    let no_cache = vk_core::endpoints::add_no_cache_query(&url, vk_core::now_millis());
    match state.downloader.get_string(&no_cache).await {
        Ok(raw) => match ModManifest::parse(&raw).and_then(ModManifest::validated) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                tracing::warn!(
                    error = %vk_core::redact::redact(&error.to_string()),
                    "manifest del music pack non valido: si userà l'archivio completo"
                );
                None
            }
        },
        Err(error) => {
            tracing::warn!(
                error = %vk_core::redact::redact(&error.to_string()),
                "manifest del music pack non scaricabile: si userà l'archivio completo"
            );
            None
        }
    }
}

async fn update_differential(
    state: &Arc<AppState>,
    layout: &ModLayout,
    plan: &MirrorPlan,
    manifest: &ModManifest,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<MusicPackOutcome> {
    let payload = addons::payload_dir(layout, OFFICIAL_MUSIC_PACK_ID);

    progress(ProgressUpdate::new(
        Phase::Verifying,
        "Confronto con i file già installati...",
    ));

    let changed = changed_files(&payload, manifest, cancel).await?;
    let obsolete = obsolete_files(&payload, manifest);

    if changed.is_empty() && obsolete.is_empty() {
        return Ok(MusicPackOutcome {
            mode: "up-to-date".into(),
            version: manifest.mod_version.clone(),
            files_written: 0,
            files_pruned: 0,
            summary: "Il music pack è già aggiornato.".into(),
        });
    }

    // ── 1. Scarica tutto in staging e verifica ogni file ────────────────────
    let staging = state.paths.downloads_dir().join(format!(
        "musicpack-staging-{}",
        vk_core::fsx::backup_timestamp()
    ));
    let result = download_changed(state, plan, &changed, &staging, progress, cancel).await;

    // ── 2. Solo se tutto è arrivato e verificato, si applica ────────────────
    let applied = match result {
        Ok(()) => apply_staged(layout, &payload, &staging, manifest, &obsolete, progress).await,
        Err(error) => Err(error),
    };

    let _ = tokio::fs::remove_dir_all(&staging).await;
    applied?;

    tracing::info!(
        written = changed.len(),
        pruned = obsolete.len(),
        "music pack aggiornato"
    );

    Ok(MusicPackOutcome {
        mode: "differential".into(),
        version: manifest.mod_version.clone(),
        files_written: changed.len() as u32,
        files_pruned: obsolete.len() as u32,
        summary: format!(
            "Music pack aggiornato: {} file scaricati, {} rimossi.",
            changed.len(),
            obsolete.len()
        ),
    })
}

/// File del manifest che mancano o non coincidono con quelli installati.
async fn changed_files(
    payload: &Path,
    manifest: &ModManifest,
    cancel: &CancelToken,
) -> AppResult<Vec<ModManifestFile>> {
    let mut changed = Vec::new();

    for file in &manifest.files {
        cancel.check().map_err(map_core)?;

        let local = addons::safe_join(payload, &file.path)?;
        let matches = match tokio::fs::metadata(&local).await {
            Ok(metadata) if metadata.len() as i64 == file.size => {
                vk_core::hash::sha256_file(&local)
                    .await
                    .is_ok_and(|hash| hash.eq_ignore_ascii_case(&file.sha256))
            }
            _ => false,
        };

        if !matches {
            changed.push(file.clone());
        }
    }

    Ok(changed)
}

/// File presenti nel payload che il manifest non dichiara più.
fn obsolete_files(payload: &Path, manifest: &ModManifest) -> Vec<String> {
    vk_core::fsx::list_files_recursive(payload)
        .into_iter()
        .map(|path| vk_core::fsx::relative_slash(payload, &path))
        .filter(|relative| !relative.is_empty() && manifest.find(relative).is_none())
        .collect()
}

async fn download_changed(
    state: &Arc<AppState>,
    plan: &MirrorPlan,
    changed: &[ModManifestFile],
    staging: &Path,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<()> {
    if changed.is_empty() {
        return Ok(());
    }

    tokio::fs::create_dir_all(staging)
        .await
        .map_err(|error| AppError::io(staging, error))?;

    let total_bytes: u64 = changed
        .iter()
        .map(|file| u64::try_from(file.size).unwrap_or(0))
        .sum();
    let mut done_bytes = 0u64;

    for (index, file) in changed.iter().enumerate() {
        cancel.check().map_err(map_core)?;

        progress(
            ProgressUpdate::new(
                Phase::Download,
                format!(
                    "Download {}/{}: {}",
                    index + 1,
                    changed.len(),
                    file_name(&file.path)
                ),
            )
            .with_bytes(done_bytes, total_bytes.max(1))
            .with_files(index as u32, changed.len() as u32),
        );

        let destination = addons::safe_join(staging, &file.path)?;
        let candidates = plan.file_candidates(&file.path, &file.sha256, vk_core::now_millis());

        state
            .downloader
            .download_with_mirrors(
                &candidates,
                &destination,
                &vk_core::progress::noop_sink(),
                cancel,
            )
            .await
            .map_err(map_core)?;

        let actual = vk_core::hash::sha256_file(&destination).await?;
        if !actual.eq_ignore_ascii_case(&file.sha256) {
            return Err(AppError::Core(CoreError::HashMismatch {
                path: file.path.clone(),
                expected: file.sha256.clone(),
                actual,
            }));
        }

        done_bytes += u64::try_from(file.size).unwrap_or(0);
    }

    progress(
        ProgressUpdate::new(Phase::Verifying, "Tutti i file scaricati e verificati.")
            .with_bytes(done_bytes, total_bytes.max(1))
            .with_files(changed.len() as u32, changed.len() as u32),
    );
    Ok(())
}

/// Applica lo staging al payload, con l'addon disattivato.
///
/// Disattivare prima è ciò che tiene `My Stuff` coerente: i file vecchi
/// tornano al loro posto (o spariscono), poi il payload viene aggiornato e
/// l'addon riattivato ricopia quelli nuovi.
async fn apply_staged(
    layout: &ModLayout,
    payload: &Path,
    staging: &Path,
    manifest: &ModManifest,
    obsolete: &[String],
    progress: &ProgressSink,
) -> AppResult<()> {
    progress(ProgressUpdate::new(
        Phase::Installing,
        "Applicazione dell'aggiornamento...",
    ));

    let mut addon = addons::read_manifest(layout, OFFICIAL_MUSIC_PACK_ID).await?;
    let was_enabled = addon.is_enabled;

    if was_enabled {
        addons::set_enabled(layout, OFFICIAL_MUSIC_PACK_ID, false).await?;
    }

    for source in vk_core::fsx::list_files_recursive(staging) {
        let relative = vk_core::fsx::relative_slash(staging, &source);
        if relative.is_empty() {
            continue;
        }
        vk_core::fsx::copy_file(&source, &addons::safe_join(payload, &relative)?).await?;
    }

    for relative in obsolete {
        let _ = tokio::fs::remove_file(addons::safe_join(payload, relative)?).await;
    }

    addon.files = manifest
        .files
        .iter()
        .map(|file| file.path.replace('\\', "/"))
        .collect();
    addon.name = DISPLAY_NAME.into();
    addon.author = AUTHOR.into();
    addon.source = SOURCE.into();
    addon.is_managed = true;
    addon.is_enabled = false;
    addon.displaced_files.clear();
    addons::write_manifest(layout, &addon).await?;

    if was_enabled {
        addons::set_enabled(layout, OFFICIAL_MUSIC_PACK_ID, true).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Attivazione e rimozione
// ---------------------------------------------------------------------------

/// Attiva o disattiva il music pack.
pub async fn set_enabled(state: &Arc<AppState>, enabled: bool) -> AppResult<MusicPackStatus> {
    let layout = state.layout(state.channel().await).await;
    addons::set_enabled(&layout, OFFICIAL_MUSIC_PACK_ID, enabled).await?;
    status(state).await
}

/// Disinstalla il music pack e dimentica la versione installata.
pub async fn uninstall(state: &Arc<AppState>) -> AppResult<MusicPackStatus> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;

    addons::remove(&layout, OFFICIAL_MUSIC_PACK_ID).await?;
    let _ = tokio::fs::remove_file(version_file(state, channel)).await;

    tracing::info!("music pack disinstallato");
    status(state).await
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn map_core(error: CoreError) -> AppError {
    match error {
        CoreError::Cancelled => AppError::Cancelled,
        other => AppError::Core(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use std::io::Write;

    async fn state_at(dir: &Path) -> Arc<AppState> {
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap();
        state.settings.write().await.user_folder_path =
            dir.join("Dolphin Emulator").to_string_lossy().to_string();
        state
    }

    /// Crea una modpack "installata", perché il music pack ci vive dentro.
    async fn install_modpack(state: &Arc<AppState>) -> ModLayout {
        let layout = state.layout(Channel::Stable).await;
        crate::testkit::install_modpack(&layout);
        std::fs::create_dir_all(layout.my_stuff()).unwrap();
        std::fs::create_dir_all(layout.user_data_root()).unwrap();
        assert!(layout.is_installed());
        layout
    }

    fn build_archive(path: &Path, files: &[(&str, &[u8])]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, body) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[tokio::test]
    async fn without_the_modpack_the_music_pack_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let status = status(&state).await.unwrap();
        assert!(!status.installed);
        assert!(status.blocker.contains("modpack"));

        let error = install(&state, vk_core::progress::noop_sink())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "configuration");
    }

    #[tokio::test]
    async fn a_fresh_installation_lands_in_my_stuff_and_is_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let layout = install_modpack(&state).await;

        // L'archivio viene "scaricato" da un file locale: il downloader
        // accetta solo https, quindi qui si scavalca la rete importando
        // direttamente, che è ciò che fa `install_full` dopo il download.
        let archive = dir.path().join("musicpack.zip");
        build_archive(
            &archive,
            &[
                ("My Stuff/Music/song.brstm", b"nota"),
                ("leggimi.txt", b"x"),
            ],
        );

        addons::import_archive_as(
            &layout,
            &archive,
            ImportRequest {
                id: OFFICIAL_MUSIC_PACK_ID.to_string(),
                name: DISPLAY_NAME.into(),
                source: SOURCE.into(),
                replace_existing: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        set_enabled(&state, true).await.unwrap();

        let status = status(&state).await.unwrap();
        assert!(status.installed);
        assert!(status.enabled);
        assert_eq!(status.file_count, 1, "solo il contenuto di My Stuff");
        assert!(layout.my_stuff().join("Music/song.brstm").is_file());
    }

    #[tokio::test]
    async fn disabling_takes_the_songs_back_out_of_my_stuff() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let layout = install_modpack(&state).await;

        let archive = dir.path().join("musicpack.zip");
        build_archive(&archive, &[("Music/song.brstm", b"nota")]);
        addons::import_archive_as(
            &layout,
            &archive,
            ImportRequest {
                id: OFFICIAL_MUSIC_PACK_ID.to_string(),
                name: DISPLAY_NAME.into(),
                replace_existing: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        set_enabled(&state, true).await.unwrap();
        assert!(layout.my_stuff().join("Music/song.brstm").is_file());

        let status = set_enabled(&state, false).await.unwrap();
        assert!(status.installed);
        assert!(!status.enabled);
        assert!(!layout.my_stuff().join("Music/song.brstm").exists());
        assert!(
            addons::payload_dir(&layout, OFFICIAL_MUSIC_PACK_ID)
                .join("Music/song.brstm")
                .is_file(),
            "disattivare non cancella il payload"
        );
    }

    #[tokio::test]
    async fn uninstalling_removes_everything_and_the_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let layout = install_modpack(&state).await;

        let archive = dir.path().join("musicpack.zip");
        build_archive(&archive, &[("Music/song.brstm", b"nota")]);
        addons::import_archive_as(
            &layout,
            &archive,
            ImportRequest {
                id: OFFICIAL_MUSIC_PACK_ID.to_string(),
                name: DISPLAY_NAME.into(),
                replace_existing: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        set_enabled(&state, true).await.unwrap();
        write_installed_version(&state, Channel::Stable, "1.4.0")
            .await
            .unwrap();

        let status = uninstall(&state).await.unwrap();

        assert!(!status.installed);
        assert!(status.installed_version.is_empty());
        assert!(!layout.my_stuff().join("Music/song.brstm").exists());
        assert!(!addons::addon_dir(&layout, OFFICIAL_MUSIC_PACK_ID).exists());
        assert!(!version_file(&state, Channel::Stable).exists());
    }

    #[tokio::test]
    async fn the_version_file_keeps_the_legacy_name() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        write_installed_version(&state, Channel::Stable, "1.4.0 ")
            .await
            .unwrap();
        write_installed_version(&state, Channel::Beta, "1.5.0")
            .await
            .unwrap();

        assert!(state.paths.root().join("musicpack_version.txt").is_file());
        assert!(state
            .paths
            .root()
            .join("musicpack_beta_version.txt")
            .is_file());
        assert_eq!(
            read_installed_version(&state, Channel::Stable).await,
            "1.4.0"
        );
    }

    #[tokio::test]
    async fn an_update_is_announced_only_when_the_versions_differ() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let layout = install_modpack(&state).await;

        let archive = dir.path().join("musicpack.zip");
        build_archive(&archive, &[("Music/song.brstm", b"nota")]);
        addons::import_archive_as(
            &layout,
            &archive,
            ImportRequest {
                id: OFFICIAL_MUSIC_PACK_ID.to_string(),
                name: DISPLAY_NAME.into(),
                replace_existing: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        write_installed_version(&state, Channel::Stable, "1.4.0")
            .await
            .unwrap();

        state.remote.write().await.info.music_pack_version = "1.4.0".into();
        assert!(!status(&state).await.unwrap().update_available);

        state.remote.write().await.info.music_pack_version = "1.5.0".into();
        let status = status(&state).await.unwrap();
        assert!(status.update_available);
        assert_eq!(status.installed_version, "1.4.0");
        assert_eq!(status.latest_version, "1.5.0");
    }

    #[tokio::test]
    async fn obsolete_and_changed_files_are_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("payload");

        std::fs::create_dir_all(payload.join("Music")).unwrap();
        std::fs::write(payload.join("Music/uguale.brstm"), b"identico").unwrap();
        std::fs::write(payload.join("Music/diverso.brstm"), b"vecchio").unwrap();
        std::fs::write(payload.join("Music/obsoleto.brstm"), b"da togliere").unwrap();

        let manifest = ModManifest {
            mod_version: "1.5.0".into(),
            archive_sha256: String::new(),
            files: vec![
                ModManifestFile {
                    path: "Music/uguale.brstm".into(),
                    sha256: sha256_of(b"identico"),
                    size: 8,
                },
                ModManifestFile {
                    path: "Music/diverso.brstm".into(),
                    sha256: sha256_of(b"nuovo"),
                    size: 5,
                },
                ModManifestFile {
                    path: "Music/aggiunto.brstm".into(),
                    sha256: sha256_of(b"nuovissimo"),
                    size: 10,
                },
            ],
        };

        let changed = changed_files(&payload, &manifest, &CancelToken::new())
            .await
            .unwrap();
        let changed_paths: Vec<&str> = changed.iter().map(|file| file.path.as_str()).collect();

        assert_eq!(
            changed_paths,
            vec!["Music/diverso.brstm", "Music/aggiunto.brstm"]
        );
        assert_eq!(
            obsolete_files(&payload, &manifest),
            vec!["Music/obsoleto.brstm".to_string()]
        );
    }

    fn sha256_of(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn the_file_name_is_the_last_segment() {
        assert_eq!(file_name("Music/song.brstm"), "song.brstm");
        assert_eq!(file_name("song.brstm"), "song.brstm");
        assert_eq!(file_name("a\\b\\c.brstm"), "c.brstm");
    }

    #[test]
    fn the_manifest_version_wins_over_the_announced_one() {
        let mut manifest = ModManifest::default();
        assert_eq!(effective_version("1.4.0", &manifest), "1.4.0");

        manifest.mod_version = "1.5.0".into();
        assert_eq!(effective_version("1.4.0", &manifest), "1.5.0");
    }
}
