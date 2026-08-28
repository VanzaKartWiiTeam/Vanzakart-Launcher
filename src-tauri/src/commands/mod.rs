//! Comandi IPC.
//!
//! Guscio sottile: ogni comando valida gli argomenti, delega a un servizio e
//! converte l'errore. Nessuna logica di dominio vive qui.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, State};
use vk_core::progress::{ProgressSink, ProgressUpdate};

use crate::domain::{
    ConflictView, DiagnosticEntry, InstallOutcome, LauncherStatus, LeaderboardPage, ModStatus,
    NewsItem, PlayStatsView, RoomsSummary, SettingsView,
};
use crate::error::{AppError, AppResult};
use crate::services;
use crate::state::AppState;

/// Nome dell'evento con cui il backend spinge i progressi verso la UI.
pub const PROGRESS_EVENT: &str = "vk://progress";

type Shared<'a> = State<'a, Arc<AppState>>;

/// Costruisce un sink che inoltra i progressi al frontend, con throttling a
/// 100 ms (vedi `docs/decisions.md` §D-018).
fn progress_sink(app: AppHandle, operation: &str) -> ProgressSink {
    use std::sync::Mutex;

    let operation = operation.to_string();
    let meter = Mutex::new(Meter::new());

    Arc::new(move |update: ProgressUpdate| {
        let terminal = matches!(
            update.phase,
            vk_core::Phase::Completed | vk_core::Phase::Error
        );

        let rate = {
            let mut guard = meter.lock().expect("mutex dei progressi avvelenato");
            if !terminal && guard.since_emit() < Duration::from_millis(100) {
                return;
            }
            guard.tick(update.bytes_done)
        };

        let payload = crate::domain::ProgressEvent {
            operation: operation.clone(),
            phase: update.phase.label().to_string(),
            detail: vk_core::redact::redact(&update.detail),
            percent: update.percent,
            bytes_done: update.bytes_done,
            bytes_total: update.bytes_total,
            files_done: update.files_done,
            files_total: update.files_total,
            bytes_label: if update.bytes_total > 0 {
                format!(
                    "{} / {}",
                    vk_core::progress::format_bytes(update.bytes_done),
                    vk_core::progress::format_bytes(update.bytes_total)
                )
            } else {
                String::new()
            },
            speed_label: match rate {
                Some(bytes_per_second) if !terminal => {
                    format!("{}/s", vk_core::progress::format_bytes(bytes_per_second))
                }
                _ => String::new(),
            },
        };

        let _ = app.emit(PROGRESS_EVENT, payload);
    })
}

/// Misura la velocità di trasferimento fra due invii di progresso.
///
/// Il valore istantaneo su una finestra di 100 ms salta troppo per essere
/// letto, quindi viene lisciato con una media esponenziale. Quando i byte
/// tornano indietro — è un file nuovo che comincia — la misura riparte invece
/// di produrre una velocità negativa.
struct Meter {
    at: Instant,
    bytes: u64,
    rate: Option<f64>,
}

impl Meter {
    /// Peso del campione nuovo. Più basso è, più la cifra sta ferma.
    const SMOOTHING: f64 = 0.3;

    fn new() -> Self {
        Self {
            at: Instant::now() - Duration::from_secs(1),
            bytes: 0,
            rate: None,
        }
    }

    fn since_emit(&self) -> Duration {
        self.at.elapsed()
    }

    /// Aggiorna la misura e restituisce i byte al secondo, se calcolabili.
    fn tick(&mut self, bytes_done: u64) -> Option<u64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.at).as_secs_f64();
        let previous = std::mem::replace(&mut self.bytes, bytes_done);
        self.at = now;

        if bytes_done < previous || elapsed <= 0.0 {
            self.rate = None;
            return None;
        }

        let sample = (bytes_done - previous) as f64 / elapsed;
        let smoothed = match self.rate {
            Some(current) => current + Self::SMOOTHING * (sample - current),
            None => sample,
        };
        self.rate = Some(smoothed);

        (smoothed >= 1.0).then_some(smoothed as u64)
    }
}

// ---------------------------------------------------------------------------
// Stato generale
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn launcher_status(state: Shared<'_>) -> AppResult<LauncherStatus> {
    let state = state.inner().clone();
    let settings = state.settings.read().await.clone();
    let preferences = state.preferences.read().await.clone();
    let secrets = state.secrets.read().await.clone();

    Ok(LauncherStatus {
        launcher_version: crate::state::LAUNCHER_VERSION.to_string(),
        platform: crate::platform::platform_name().to_string(),
        channel: preferences.channel,
        settings_complete: settings.is_complete(),
        missing_settings: settings
            .missing_fields()
            .into_iter()
            .map(str::to_string)
            .collect(),
        mod_state: services::mods::status(&state).await?,
        stats: PlayStatsView {
            last_played_utc: preferences.stats.last_played_utc.clone(),
            launch_count: preferences.stats.launch_count,
            total_play_time_minutes: preferences.stats.total_play_time_minutes,
        },
        has_beta_token: secrets.has_beta_token(),
        beta_token_masked: secrets.masked_beta_token(),
        dolphin_detected: !settings.dolphin_path.is_empty() && settings.dolphin().exists(),
        dolphin_running: services::launch::is_running(&state).await,
        save_writes_enabled: services::saves::SAVE_WRITES_ENABLED,
    })
}

/// Aggiorna endpoint, versioni e token beta all'avvio.
#[tauri::command]
pub async fn bootstrap(state: Shared<'_>) -> AppResult<ModStatus> {
    let state = state.inner().clone();

    let current = state.endpoints.read().await.clone();
    match crate::storage::endpoints::refresh(&state.paths, &state.downloader, &current).await {
        Ok((resolved, rejected)) => {
            *state.endpoints.write().await = resolved;
            if !rejected.is_empty() {
                tracing::warn!(?rejected, "endpoint remoti parzialmente scartati");
            }
        }
        Err(error) => tracing::warn!(
            error = %vk_core::redact::redact(&error.to_string()),
            "endpoints.json non aggiornabile: si usano default e cache"
        ),
    }

    if state.secrets.read().await.has_beta_token() {
        let _ = services::beta::validate_saved(&state).await;
    }

    if state.preferences.read().await.auto_check_updates {
        services::mods::check_updates(&state).await
    } else {
        services::mods::status(&state).await
    }
}

// ---------------------------------------------------------------------------
// Modpack
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn mods_status(state: Shared<'_>) -> AppResult<ModStatus> {
    services::mods::status(&state.inner().clone()).await
}

#[tauri::command]
pub async fn mods_check_updates(state: Shared<'_>) -> AppResult<ModStatus> {
    services::mods::check_updates(&state.inner().clone()).await
}

#[tauri::command]
pub async fn mods_install(app: AppHandle, state: Shared<'_>) -> AppResult<InstallOutcome> {
    let sink = progress_sink(app, "mods");
    services::mods::install(&state.inner().clone(), false, sink).await
}

#[tauri::command]
pub async fn mods_repair(app: AppHandle, state: Shared<'_>) -> AppResult<InstallOutcome> {
    let sink = progress_sink(app, "mods");
    services::mods::install(&state.inner().clone(), true, sink).await
}

#[tauri::command]
pub async fn mods_verify(state: Shared<'_>) -> AppResult<services::mods::IntegrityReport> {
    services::mods::verify_integrity(&state.inner().clone()).await
}

#[tauri::command]
pub async fn mods_set_channel(state: Shared<'_>, channel: String) -> AppResult<ModStatus> {
    let parsed = channel
        .parse::<vk_core::Channel>()
        .map_err(|()| AppError::BadRequest(format!("canale sconosciuto: {channel}")))?;
    services::mods::set_channel(&state.inner().clone(), parsed).await
}

#[tauri::command]
pub async fn operation_cancel(state: Shared<'_>) -> AppResult<()> {
    state.inner().cancel_current().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// GameBanana
// ---------------------------------------------------------------------------

/// Cerca fra le mod di Mario Kart Wii pubblicate su GameBanana.
#[tauri::command]
pub async fn gamebanana_search(
    state: Shared<'_>,
    query: Option<String>,
    sort: Option<String>,
    page: Option<usize>,
) -> AppResult<services::gamebanana::GameBananaSearchResult> {
    services::gamebanana::search(
        &state.inner().clone(),
        query.unwrap_or_default().trim(),
        &sort.unwrap_or_default(),
        page.unwrap_or(1),
    )
    .await
}

/// Scarica un file di una mod e lo installa come addon.
///
/// Il frontend passa solo gli identificativi: l'URL di download viene riletto
/// dall'API e validato qui contro l'allowlist degli host.
#[tauri::command]
pub async fn gamebanana_install(
    app: AppHandle,
    state: Shared<'_>,
    mod_id: i64,
    file_id: i64,
) -> AppResult<crate::domain::AddonView> {
    let sink = progress_sink(app, "gamebanana");
    services::gamebanana::install(&state.inner().clone(), mod_id, file_id, sink).await
}

// ---------------------------------------------------------------------------
// Music pack
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn music_pack_status(
    state: Shared<'_>,
) -> AppResult<services::music_pack::MusicPackStatus> {
    services::music_pack::status(&state.inner().clone()).await
}

/// Installa il music pack, o lo aggiorna se è già presente.
#[tauri::command]
pub async fn music_pack_install(
    app: AppHandle,
    state: Shared<'_>,
) -> AppResult<services::music_pack::MusicPackOutcome> {
    let sink = progress_sink(app, "music-pack");
    services::music_pack::install(&state.inner().clone(), sink).await
}

#[tauri::command]
pub async fn music_pack_set_enabled(
    state: Shared<'_>,
    enabled: bool,
) -> AppResult<services::music_pack::MusicPackStatus> {
    services::music_pack::set_enabled(&state.inner().clone(), enabled).await
}

#[tauri::command]
pub async fn music_pack_uninstall(
    state: Shared<'_>,
) -> AppResult<services::music_pack::MusicPackStatus> {
    services::music_pack::uninstall(&state.inner().clone()).await
}

// ---------------------------------------------------------------------------
// Avvio
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn launch_preflight(
    state: Shared<'_>,
) -> AppResult<Option<services::launch::LaunchBlocker>> {
    services::launch::preflight(&state.inner().clone()).await
}

#[tauri::command]
pub async fn launch_game(state: Shared<'_>) -> AppResult<services::launch::LaunchResult> {
    services::launch::launch(&state.inner().clone()).await
}

#[tauri::command]
pub async fn launch_session_finished(state: Shared<'_>) -> AppResult<f64> {
    services::launch::finish_session(&state.inner().clone()).await
}

// ---------------------------------------------------------------------------
// Impostazioni
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn settings_get(state: Shared<'_>) -> AppResult<SettingsView> {
    services::dolphin::settings_view(&state.inner().clone()).await
}

#[tauri::command]
pub async fn settings_update_paths(
    state: Shared<'_>,
    dolphin_path: Option<String>,
    rom_path: Option<String>,
    user_folder_path: Option<String>,
) -> AppResult<SettingsView> {
    services::dolphin::update_paths(
        &state.inner().clone(),
        dolphin_path,
        rom_path,
        user_folder_path,
    )
    .await
}

#[tauri::command]
pub async fn settings_detect_dolphin(state: Shared<'_>) -> AppResult<SettingsView> {
    let state = state.inner().clone();

    let executables = services::dolphin::detect_executables();
    if let Some(first) = executables.first() {
        if state.settings.read().await.dolphin_path.is_empty() {
            state.settings.write().await.dolphin_path = first.to_string_lossy().to_string();
        }
    }

    let dolphin = state.settings.read().await.dolphin();
    if let Some(folder) = services::dolphin::detect_user_folders(&dolphin)
        .into_iter()
        .next()
    {
        if state.settings.read().await.user_folder_path.is_empty() {
            state.settings.write().await.user_folder_path = folder.to_string_lossy().to_string();
        }
    }

    state.persist_settings().await?;
    services::dolphin::settings_view(&state).await
}

#[tauri::command]
pub async fn preferences_update(
    state: Shared<'_>,
    separate_savegame: Option<bool>,
    my_stuff_enabled: Option<bool>,
    auto_check_updates: Option<bool>,
    download_concurrency: Option<usize>,
) -> AppResult<SettingsView> {
    let state = state.inner().clone();
    {
        let mut preferences = state.preferences.write().await;
        if let Some(value) = separate_savegame {
            preferences.separate_savegame = value;
        }
        if let Some(value) = my_stuff_enabled {
            preferences.mod_option_choice = if value { 2 } else { 0 };
        }
        if let Some(value) = auto_check_updates {
            preferences.auto_check_updates = value;
        }
        if let Some(value) = download_concurrency {
            preferences.download_concurrency = value.clamp(1, 12);
        }
    }
    state.persist_preferences().await?;
    services::dolphin::settings_view(&state).await
}

// ---------------------------------------------------------------------------
// Impostazioni di Dolphin
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dolphin_settings_get(
    state: Shared<'_>,
) -> AppResult<vk_dolphin::settings::DolphinSettings> {
    services::dolphin::load_dolphin_settings(&state.inner().clone()).await
}

#[tauri::command]
pub async fn dolphin_settings_save(
    state: Shared<'_>,
    settings: vk_dolphin::settings::DolphinSettings,
) -> AppResult<()> {
    services::dolphin::save_dolphin_settings(&state.inner().clone(), &settings).await
}

#[tauri::command]
pub async fn dolphin_settings_optimize(
    state: Shared<'_>,
    screen_width: u32,
) -> AppResult<vk_dolphin::settings::DolphinSettings> {
    services::dolphin::optimize(&state.inner().clone(), screen_width).await
}

#[tauri::command]
pub async fn dolphin_settings_reset(
    state: Shared<'_>,
    category: String,
) -> AppResult<vk_dolphin::settings::DolphinSettings> {
    services::dolphin::reset_category(&state.inner().clone(), &category).await
}

#[tauri::command]
pub async fn dolphin_config_backup(state: Shared<'_>) -> AppResult<String> {
    services::dolphin::backup_config(&state.inner().clone()).await
}

#[tauri::command]
pub async fn dolphin_config_restore(state: Shared<'_>, archive: String) -> AppResult<()> {
    services::dolphin::restore_config(&state.inner().clone(), std::path::Path::new(&archive)).await
}

#[tauri::command]
pub async fn dolphin_delete_game_settings(state: Shared<'_>) -> AppResult<Vec<String>> {
    services::dolphin::delete_game_settings(&state.inner().clone()).await
}

// ---------------------------------------------------------------------------
// Community
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn news_fetch(state: Shared<'_>) -> AppResult<Vec<NewsItem>> {
    services::news::fetch(&state.inner().clone()).await
}

#[tauri::command]
pub async fn rooms_fetch(state: Shared<'_>) -> AppResult<RoomsSummary> {
    services::community::rooms(&state.inner().clone()).await
}

#[tauri::command]
pub async fn leaderboard_fetch(
    state: Shared<'_>,
    offset: Option<u32>,
) -> AppResult<LeaderboardPage> {
    services::community::leaderboard(&state.inner().clone(), offset.unwrap_or(0)).await
}

// ---------------------------------------------------------------------------
// Beta
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn beta_status(state: Shared<'_>) -> AppResult<services::beta::BetaStatus> {
    Ok(services::beta::status(&state.inner().clone()).await)
}

#[tauri::command]
pub async fn beta_verify(
    state: Shared<'_>,
    token: String,
) -> AppResult<services::beta::BetaStatus> {
    services::beta::verify_and_store(&state.inner().clone(), &token).await
}

#[tauri::command]
pub async fn beta_clear(state: Shared<'_>) -> AppResult<services::beta::BetaStatus> {
    services::beta::clear(&state.inner().clone()).await
}

// ---------------------------------------------------------------------------
// Diagnostica
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn diagnostics_collect(state: Shared<'_>) -> AppResult<Vec<DiagnosticEntry>> {
    services::diagnostics::collect(&state.inner().clone()).await
}

#[tauri::command]
pub async fn diagnostics_log(state: Shared<'_>) -> AppResult<String> {
    services::diagnostics::tail_log(&state.inner().clone()).await
}

#[tauri::command]
pub async fn diagnostics_backups(
    state: Shared<'_>,
) -> AppResult<Vec<vk_core::backup::BackupSummary>> {
    Ok(services::diagnostics::backups(&state.inner().clone()).await)
}

#[tauri::command]
pub async fn diagnostics_purge(state: Shared<'_>, confirmation: String) -> AppResult<Vec<String>> {
    services::diagnostics::purge_user_data(&state.inner().clone(), &confirmation).await
}

/// Apre una cartella nota nel file manager di sistema.
///
/// Il frontend passa un identificatore, mai un percorso: l'allowlist è in
/// [`crate::storage::paths::AppPaths::well_known`].
#[tauri::command]
pub async fn open_known_folder(
    app: AppHandle,
    state: Shared<'_>,
    key: String,
) -> AppResult<String> {
    use tauri_plugin_opener::OpenerExt;

    let state = state.inner().clone();

    // `mod` e `addons` non stanno sotto la cartella dati: dipendono dal canale
    // e vivono nella cartella User di Dolphin, quindi li risolve il layout.
    let path = match key.as_str() {
        "mod" | "addons" => {
            let layout = state.layout(state.channel().await).await;
            if key == "mod" {
                layout.mod_root()
            } else {
                layout.my_stuff()
            }
        }
        other => state
            .paths
            .well_known(other)
            .ok_or_else(|| AppError::BadRequest(format!("cartella sconosciuta: {key}")))?,
    };

    std::fs::create_dir_all(&path).map_err(|error| AppError::io(&path, error))?;

    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| AppError::Internal(error.to_string()))?;

    Ok(vk_core::redact::redact(&path.to_string_lossy()))
}

/// Apre un URL esterno, previa validazione.
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;

    vk_core::endpoints::require_safe_endpoint(&url)
        .map_err(|_| AppError::BadRequest("URL non consentito".into()))?;

    if crate::platform::open_with_system_handler(&url) {
        return Ok(());
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| AppError::Internal(error.to_string()))
}

// ---------------------------------------------------------------------------
// Controller
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn controllers_scan(state: Shared<'_>) -> AppResult<Vec<crate::domain::ControllerView>> {
    services::controller::scan(&state.inner().clone()).await
}

#[tauri::command]
pub async fn controller_profile_get(state: Shared<'_>) -> AppResult<vk_dolphin::ControllerProfile> {
    services::controller::load_profile(&state.inner().clone()).await
}

#[tauri::command]
pub async fn controller_profile_save(
    state: Shared<'_>,
    profile: vk_dolphin::ControllerProfile,
) -> AppResult<()> {
    services::controller::save_profile(&state.inner().clone(), &profile).await
}

/// Attende un input e restituisce il binding, oppure `null` allo scadere del
/// timeout di 8 secondi.
#[tauri::command]
pub async fn controller_capture(device: String) -> AppResult<Option<String>> {
    services::controller::capture_binding(device).await
}

#[tauri::command]
pub async fn controller_rumble(device: String) -> AppResult<bool> {
    services::controller::rumble(device).await
}

#[tauri::command]
pub async fn controller_mode_get(state: Shared<'_>) -> AppResult<vk_dolphin::ControllerMode> {
    Ok(services::controller::mode(&state.inner().clone()).await)
}

#[tauri::command]
pub async fn controller_mode_set(
    state: Shared<'_>,
    mode: vk_dolphin::ControllerMode,
) -> AppResult<vk_dolphin::ControllerMode> {
    services::controller::set_mode(&state.inner().clone(), mode).await
}

/// Tabella statica delle azioni configurabili.
#[tauri::command]
pub async fn controller_actions() -> AppResult<Vec<&'static vk_dolphin::controller::MarioKartAction>>
{
    Ok(vk_dolphin::controller::ACTIONS.iter().collect())
}

// ---------------------------------------------------------------------------
// Licenze e salvataggi
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn licenses_list(state: Shared<'_>) -> AppResult<Vec<crate::domain::LicenseView>> {
    services::saves::list_licenses(&state.inner().clone()).await
}

/// Assegna un Mii del launcher a una licenza.
///
/// Il Mii entra prima nel database di Dolphin, poi la licenza lo indica. Il
/// salvataggio viene copiato e verificato prima di essere toccato, e la
/// scrittura è rifiutata se Dolphin è aperto.
#[tauri::command]
pub async fn licenses_set_mii(
    state: Shared<'_>,
    save_index: usize,
    license: usize,
    mii_id: String,
) -> AppResult<Vec<crate::domain::LicenseView>> {
    services::saves::set_license_mii(&state.inner().clone(), save_index, license, &mii_id).await
}

/// Stato dell'aggiornamento del launcher, dalla versione pubblicata su
/// versions.json. Non contatta nessuno: legge l ultimo controllo.
#[tauri::command]
pub async fn launcher_update_status(
    state: Shared<'_>,
) -> AppResult<services::launcher::LauncherUpdateStatus> {
    services::launcher::status(&state.inner().clone()).await
}

#[tauri::command]
pub async fn saves_overview(state: Shared<'_>) -> AppResult<services::saves::SaveOverview> {
    services::saves::overview(&state.inner().clone()).await
}

#[tauri::command]
pub async fn saves_backup(state: Shared<'_>) -> AppResult<String> {
    services::saves::backup_save(&state.inner().clone()).await
}

#[tauri::command]
pub async fn saves_backups(state: Shared<'_>) -> AppResult<Vec<String>> {
    Ok(services::saves::save_backups(&state.inner().clone()))
}

/// Sostituisce il salvataggio corrente con un `rksys.dat` scelto dall'utente.
#[tauri::command]
pub async fn saves_import(state: Shared<'_>, source: String) -> AppResult<String> {
    let path = std::path::PathBuf::from(source);
    services::saves::import_save(&state.inner().clone(), &path).await
}

/// Copia il salvataggio corrente dove l'utente ha scelto.
#[tauri::command]
pub async fn saves_export(state: Shared<'_>, destination: String) -> AppResult<String> {
    let path = std::path::PathBuf::from(destination);
    services::saves::export_save(&state.inner().clone(), &path).await
}

/// Rimette in gioco uno dei backup elencati da `saves_backups`.
#[tauri::command]
pub async fn saves_restore(state: Shared<'_>, name: String) -> AppResult<String> {
    services::saves::restore_backup(&state.inner().clone(), &name).await
}

#[tauri::command]
pub async fn friends_list(
    state: Shared<'_>,
    save_index: usize,
    license: usize,
) -> AppResult<Vec<crate::domain::FriendView>> {
    services::saves::list_friends(&state.inner().clone(), save_index, license).await
}

/// Aggiunge un amico a una licenza.
///
/// Il salvataggio viene copiato e verificato prima di essere toccato, e la
/// scrittura è rifiutata se Dolphin è aperto.
#[tauri::command]
pub async fn friends_add(
    state: Shared<'_>,
    save_index: usize,
    license: usize,
    friend_code: String,
) -> AppResult<Vec<crate::domain::FriendView>> {
    services::saves::add_friend(&state.inner().clone(), save_index, license, &friend_code).await
}

#[tauri::command]
pub async fn friends_remove(
    state: Shared<'_>,
    save_index: usize,
    license: usize,
    slot: usize,
) -> AppResult<Vec<crate::domain::FriendView>> {
    services::saves::remove_friend(&state.inner().clone(), save_index, license, slot).await
}

// ---------------------------------------------------------------------------
// Mii
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn mii_list(state: Shared<'_>) -> AppResult<Vec<services::mii::MiiView>> {
    Ok(services::mii::list(&state.inner().clone()).await)
}

#[tauri::command]
pub async fn mii_create(
    state: Shared<'_>,
    name: String,
    favorite_color_index: u8,
    is_female: bool,
) -> AppResult<services::mii::MiiView> {
    services::mii::create(
        &state.inner().clone(),
        &name,
        favorite_color_index,
        is_female,
    )
    .await
}

/// Crea un Mii a partire dallo stato completo dell'editor.
#[tauri::command]
pub async fn mii_create_from_state(
    state: Shared<'_>,
    editor: vk_save::mii::MiiEditorState,
) -> AppResult<services::mii::MiiView> {
    services::mii::create_from_state(&state.inner().clone(), &editor).await
}

#[tauri::command]
pub async fn mii_editor_state(
    state: Shared<'_>,
    id: String,
) -> AppResult<vk_save::mii::MiiEditorState> {
    services::mii::editor_state(&state.inner().clone(), &id).await
}

#[tauri::command]
pub async fn mii_update(
    state: Shared<'_>,
    id: String,
    editor: vk_save::mii::MiiEditorState,
) -> AppResult<services::mii::MiiView> {
    services::mii::update(&state.inner().clone(), &id, &editor).await
}

#[tauri::command]
pub async fn mii_duplicate(state: Shared<'_>, id: String) -> AppResult<services::mii::MiiView> {
    services::mii::duplicate(&state.inner().clone(), &id).await
}

/// Elimina un Mii del launcher.
///
/// `from_dolphin` toglie il Mii anche dal database del gioco: è una scelta
/// esplicita dell'utente, non un effetto collaterale (`decisions.md` §D-027).
#[tauri::command]
pub async fn mii_delete(state: Shared<'_>, id: String) -> AppResult<()> {
    services::mii::delete(&state.inner().clone(), &id).await
}

/// Importa un Mii da un file scelto con il dialogo nativo.
///
/// Il percorso arriva dal frontend ma viene rivalidato: deve esistere, essere
/// un file e avere una delle estensioni riconosciute.
#[tauri::command]
pub async fn mii_import(state: Shared<'_>, source: String) -> AppResult<services::mii::MiiView> {
    let path = std::path::PathBuf::from(source.trim());
    if !services::mii::is_supported_source(&path) {
        return Err(AppError::BadRequest(
            "formato di file Mii non supportato".into(),
        ));
    }
    services::mii::import_file(&state.inner().clone(), &path).await
}

#[tauri::command]
pub async fn mii_export(state: Shared<'_>, id: String, destination: String) -> AppResult<String> {
    let path = std::path::PathBuf::from(destination.trim());
    if path.as_os_str().is_empty() {
        return Err(AppError::BadRequest("percorso di export vuoto".into()));
    }
    services::mii::export(&state.inner().clone(), &id, &path).await
}

/// Stato casuale per il pulsante "Random" dell'editor.
#[tauri::command]
pub async fn mii_random_state(name: String) -> AppResult<vk_save::mii::MiiEditorState> {
    Ok(services::mii::random_state(&name))
}

/// Stato di partenza per un Mii nuovo, con la data di oggi.
#[tauri::command]
pub async fn mii_default_state(
    name: String,
    favorite_color_index: u8,
    is_female: bool,
) -> AppResult<vk_save::mii::MiiEditorState> {
    Ok(services::mii::default_state(
        &name,
        favorite_color_index,
        is_female,
    ))
}

/// I 12 colori preferiti della Wii, per la tavolozza dell'editor.
#[tauri::command]
pub async fn mii_favorite_colors() -> AppResult<Vec<&'static str>> {
    Ok(vk_save::mii::FAVORITE_COLORS.to_vec())
}

// ---------------------------------------------------------------------------
// Render dei Mii
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn mii_renderer_status(
    state: Shared<'_>,
) -> AppResult<services::mii_render::MiiRendererStatus> {
    Ok(services::mii_render::status(&state.inner().clone()).await)
}

/// Scarica il runtime di rendering. Solo su richiesta esplicita (§D-011).
#[tauri::command]
pub async fn mii_renderer_install(
    app: AppHandle,
    state: Shared<'_>,
) -> AppResult<services::mii_render::MiiRendererStatus> {
    let sink = progress_sink(app, "mii-renderer");
    services::mii_render::install_runtime(&state.inner().clone(), sink).await
}

#[tauri::command]
pub async fn mii_renderer_remove(
    state: Shared<'_>,
) -> AppResult<services::mii_render::MiiRendererStatus> {
    services::mii_render::remove_runtime(&state.inner().clone()).await
}

/// Render di una "studio data" già nota: il Mii di una licenza, di un amico o
/// di un profilo. `null` quando il servizio non risponde.
#[tauri::command]
pub async fn mii_render_studio(
    state: Shared<'_>,
    studio_data: String,
    kind: Option<String>,
    rotation: Option<i32>,
) -> AppResult<Option<String>> {
    services::mii_render::render_studio(
        &state.inner().clone(),
        &studio_data,
        kind.as_deref().unwrap_or("face"),
        rotation.unwrap_or(0),
    )
    .await
}

/// Render di uno stato dell'editor, senza salvare nulla.
///
/// È l'anteprima dal vivo e la miniatura di ogni opzione: il Mii viene
/// costruito in memoria, renderizzato e messo in cache come qualunque altro.
#[tauri::command]
pub async fn mii_render_state(
    state: Shared<'_>,
    editor: vk_save::mii::MiiEditorState,
    kind: Option<String>,
    rotation: Option<i32>,
) -> AppResult<Option<String>> {
    services::mii_render::render_editor_state(
        &state.inner().clone(),
        &editor,
        kind.as_deref().unwrap_or("face"),
        rotation.unwrap_or(0),
    )
    .await
}

#[tauri::command]
pub async fn mii_avatars_clear(state: Shared<'_>) -> AppResult<usize> {
    services::mii_render::clear_cache(&state.inner().clone()).await
}

// ---------------------------------------------------------------------------
// Addon
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn addons_list(state: Shared<'_>) -> AppResult<Vec<crate::domain::AddonView>> {
    let state = state.inner().clone();
    let layout = state.layout(state.channel().await).await;
    Ok(services::addons::list(&layout).await)
}

/// Importa un archivio scelto dall'utente con il dialogo nativo.
///
/// Il percorso arriva dal frontend ma viene rivalidato: deve esistere, essere
/// un file e avere estensione `.zip`.
#[tauri::command]
pub async fn addons_import(
    state: Shared<'_>,
    archive: String,
    name: String,
) -> AppResult<crate::domain::AddonView> {
    let path = std::path::PathBuf::from(archive.trim());
    if !path
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
    {
        return Err(AppError::BadRequest(
            "sono supportati solo archivi .zip".into(),
        ));
    }

    let state = state.inner().clone();
    let layout = state.layout(state.channel().await).await;
    services::addons::import_archive(&layout, &path, &name).await
}

#[tauri::command]
pub async fn addons_set_enabled(
    state: Shared<'_>,
    id: String,
    enabled: bool,
) -> AppResult<crate::domain::AddonView> {
    let state = state.inner().clone();
    let layout = state.layout(state.channel().await).await;
    services::addons::set_enabled(&layout, &id, enabled).await
}

#[tauri::command]
pub async fn addons_remove(state: Shared<'_>, id: String) -> AppResult<()> {
    let state = state.inner().clone();
    let layout = state.layout(state.channel().await).await;
    services::addons::remove(&layout, &id).await
}

/// Elenca i conflitti fra i file degli addon installati.
#[tauri::command]
pub async fn addons_conflicts(state: Shared<'_>) -> AppResult<Vec<ConflictView>> {
    let state = state.inner().clone();
    let layout = state.layout(state.channel().await).await;
    Ok(crate::services::addons::scan_conflicts(&layout.my_stuff()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_progress_event_name_is_namespaced() {
        assert!(PROGRESS_EVENT.starts_with("vk://"));
    }

    /// Un `Meter` con l'ultimo istante spostato indietro di `seconds`, così da
    /// misurare una finestra nota senza far dormire il test.
    fn meter_after(seconds: f64) -> Meter {
        let mut meter = Meter::new();
        meter.at = Instant::now() - Duration::from_secs_f64(seconds);
        meter
    }

    #[test]
    fn the_speed_is_bytes_over_the_elapsed_window() {
        let mut meter = meter_after(1.0);
        let rate = meter.tick(1_000_000).expect("velocità misurabile");

        // Un secondo, un megabyte: la tolleranza copre il tempo del test.
        assert!((900_000..=1_100_000).contains(&rate), "{rate}");
    }

    #[test]
    fn the_speed_is_smoothed_between_samples() {
        let mut meter = meter_after(1.0);
        meter.tick(1_000_000).unwrap();

        // Campione successivo a velocità doppia: la cifra sale, ma non salta
        // subito al valore nuovo.
        meter.at = Instant::now() - Duration::from_secs_f64(1.0);
        let rate = meter.tick(3_000_000).unwrap();

        assert!(rate > 1_100_000, "{rate}");
        assert!(rate < 2_000_000, "{rate}");
    }

    #[test]
    fn a_new_file_restarts_the_measure_instead_of_going_negative() {
        let mut meter = meter_after(1.0);
        meter.tick(5_000_000).unwrap();

        meter.at = Instant::now() - Duration::from_secs_f64(1.0);
        assert!(meter.tick(120).is_none(), "i byte sono tornati indietro");

        meter.at = Instant::now() - Duration::from_secs_f64(1.0);
        let rate = meter.tick(500_120).expect("la misura riparte");
        assert!((450_000..=550_000).contains(&rate), "{rate}");
    }

    #[test]
    fn a_transfer_too_slow_to_measure_reports_nothing() {
        let mut meter = meter_after(1.0);
        assert!(meter.tick(0).is_none());
    }
}
