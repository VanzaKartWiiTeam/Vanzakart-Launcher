//! VanzaKart Launcher — applicazione Tauri.
//!
//! Struttura:
//! - `domain`: tipi scambiati con il frontend;
//! - `storage`: persistenza e migrazioni;
//! - `platform`: unico punto con API specifiche del sistema operativo;
//! - `services`: casi d'uso, indipendenti da Tauri;
//! - `commands`: guscio IPC.

#![forbid(unsafe_code)]

pub mod commands;
pub mod domain;
pub mod error;
pub mod platform;
pub mod services;
pub mod state;
pub mod storage;
#[cfg(test)]
pub mod testkit;

use std::sync::Arc;

use tauri::Manager;

pub use error::{AppError, AppResult};

/// Punto d'ingresso condiviso fra il binario e i test.
pub fn run() {
    let paths = match storage::paths::AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("Impossibile determinare la cartella dati: {error}");
            std::process::exit(1);
        }
    };

    let _guard = init_tracing(&paths);

    let runtime = tokio::runtime::Runtime::new().expect("runtime tokio");
    let state = runtime
        .block_on(state::AppState::bootstrap(paths))
        .unwrap_or_else(|error| {
            eprintln!("Avvio non riuscito: {error}");
            std::process::exit(1);
        });

    tracing::info!(
        version = state::LAUNCHER_VERSION,
        platform = platform::platform_name(),
        "avvio del launcher"
    );

    build(state)
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!(%error, "errore fatale di Tauri");
            std::process::exit(1);
        });
}

/// Costruisce l'applicazione. Separata da [`run`] per essere testabile.
pub fn build(state: Arc<state::AppState>) -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .manage(state)
        .setup(|app| {
            // La finestra nasce nascosta e viene mostrata a layout pronto,
            // così l'utente non vede un lampo bianco.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::launcher_status,
            commands::bootstrap,
            commands::mods_status,
            commands::mods_check_updates,
            commands::mods_install,
            commands::mods_repair,
            commands::mods_verify,
            commands::mods_set_channel,
            commands::operation_cancel,
            commands::music_pack_status,
            commands::music_pack_install,
            commands::music_pack_set_enabled,
            commands::music_pack_uninstall,
            commands::gamebanana_search,
            commands::gamebanana_install,
            commands::launch_preflight,
            commands::launch_game,
            commands::launch_session_finished,
            commands::settings_get,
            commands::settings_update_paths,
            commands::settings_detect_dolphin,
            commands::preferences_update,
            commands::dolphin_settings_get,
            commands::dolphin_settings_save,
            commands::dolphin_settings_optimize,
            commands::dolphin_settings_reset,
            commands::dolphin_config_backup,
            commands::dolphin_config_restore,
            commands::dolphin_delete_game_settings,
            commands::news_fetch,
            commands::rooms_fetch,
            commands::leaderboard_fetch,
            commands::beta_status,
            commands::beta_verify,
            commands::beta_clear,
            commands::diagnostics_collect,
            commands::diagnostics_log,
            commands::diagnostics_backups,
            commands::diagnostics_purge,
            commands::open_known_folder,
            commands::open_external,
            commands::controllers_scan,
            commands::controller_profile_get,
            commands::controller_profile_save,
            commands::controller_capture,
            commands::controller_rumble,
            commands::controller_mode_get,
            commands::controller_mode_set,
            commands::controller_actions,
            commands::launcher_update_status,
            commands::licenses_list,
            commands::licenses_set_mii,
            commands::saves_overview,
            commands::saves_backup,
            commands::saves_backups,
            commands::saves_import,
            commands::saves_export,
            commands::saves_restore,
            commands::friends_list,
            commands::friends_add,
            commands::friends_remove,
            commands::mii_list,
            commands::mii_create,
            commands::mii_create_from_state,
            commands::mii_editor_state,
            commands::mii_update,
            commands::mii_duplicate,
            commands::mii_delete,
            commands::mii_renderer_status,
            commands::mii_renderer_install,
            commands::mii_renderer_remove,
            commands::mii_render_studio,
            commands::mii_render_state,
            commands::mii_avatars_clear,
            commands::mii_import,
            commands::mii_export,
            commands::mii_random_state,
            commands::mii_default_state,
            commands::mii_favorite_colors,
            commands::addons_list,
            commands::addons_import,
            commands::addons_set_enabled,
            commands::addons_remove,
            commands::addons_conflicts,
        ])
}

/// Inizializza `tracing` con rotazione giornaliera.
///
/// Restituisce la guardia del writer non bloccante: va tenuta viva per tutta
/// la durata del processo.
fn init_tracing(
    paths: &storage::paths::AppPaths,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("VK_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,vanzakart_launcher=debug,vk_core=debug"));

    let _ = std::fs::create_dir_all(paths.logs_dir());
    let appender = tracing_appender::rolling::daily(paths.logs_dir(), "vanzakart-launcher.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let registry = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true),
    );

    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    if registry.try_init().is_err() {
        return None;
    }

    Some(guard)
}
