//! VanzaKart Setup — installer e disinstallatore.
//!
//! Un solo binario in due vesti (§D-053): avviato normalmente installa,
//! avviato come `VanzaKart Uninstaller` o con `--uninstall` rimuove. La
//! logica sta tutta in `vk-install`; qui c'è la finestra, il guscio IPC e la
//! scelta della modalità.
//!
//! Con `--uninstall --quiet` non apre nessuna finestra: è la forma che
//! Windows invoca da "App e funzionalità" quando l'utente sceglie la
//! rimozione silenziosa.

#![forbid(unsafe_code)]

pub mod commands;
pub mod error;
pub mod state;

use std::sync::Arc;

use tauri::Manager;

pub use error::{SetupError, SetupResult};
use state::{Mode, SetupState};

/// Punto d'ingresso condiviso fra il binario e i test.
pub fn run() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let executable_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_default();
    let mode = Mode::detect(&arguments, &executable_name);

    let _guard = init_tracing();
    tracing::info!(
        version = state::SETUP_VERSION,
        mode = mode.as_str(),
        target = %vk_install::Target::current(),
        "avvio dell'installer"
    );

    if mode == Mode::Uninstall && state::wants_quiet(&arguments) {
        std::process::exit(run_quiet_uninstall());
    }

    build(mode)
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!(%error, "errore fatale di Tauri");
            std::process::exit(1);
        });
}

/// Costruisce l'applicazione. Separata da [`run`] per essere testabile.
pub fn build(mode: Mode) -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            // L'icona serve su Linux, dove la voce del menu applicazioni la
            // cita per nome e il file va installato nel tema.
            let icon = app
                .path()
                .resolve("resources/icon.png", tauri::path::BaseDirectory::Resource)
                .ok()
                .filter(|path| path.exists());

            let state = SetupState::new(mode, icon)?;
            app.manage(Arc::new(state));

            commands::show_main_window(&app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::setup_bootstrap,
            commands::setup_refresh,
            commands::setup_preflight,
            commands::setup_install,
            commands::setup_cancel,
            commands::setup_launch,
            commands::setup_uninstall_plan,
            commands::setup_uninstall_run,
            commands::setup_open_download_page,
            commands::setup_data_root,
        ])
}

/// Disinstallazione senza interfaccia.
///
/// Toglie il programma e lascia stare i dati dell'utente: una rimozione
/// silenziosa non è il posto in cui decidere di cancellare salvataggi e
/// modpack senza che nessuno l'abbia chiesto.
///
/// Pretende un'installazione **con registro**: senza, ciò che verrebbe
/// rimosso è una ricostruzione a partire da percorsi noti, cioè un'ipotesi, e
/// un'ipotesi non cancella file senza che nessuno stia guardando (§D-055).
/// Con la finestra aperta l'ipotesi si può mostrare, ed è quello che si fa.
fn run_quiet_uninstall() -> i32 {
    let Some(existing) = vk_install::discovery::find_for_uninstall() else {
        tracing::error!("nessuna installazione da rimuovere");
        return 1;
    };

    if !existing.managed {
        tracing::error!(
            directory = %existing.install_dir.display(),
            "installazione senza registro: la rimozione silenziosa si ferma, serve la finestra"
        );
        return 2;
    }

    let record = existing.record_or_reconstructed();
    match vk_install::uninstall::run(
        &record,
        &vk_install::UninstallOptions::default(),
        &vk_core::progress::noop_sink(),
    ) {
        Ok(report) => {
            tracing::info!(
                removed = report.removed.len(),
                failed = report.failed.len(),
                "disinstallazione silenziosa completata"
            );
            i32::from(!report.failed.is_empty())
        }
        Err(error) => {
            tracing::error!(%error, "disinstallazione silenziosa non riuscita");
            1
        }
    }
}

/// Log su file accanto al registro dell'installazione, più stderr in debug.
///
/// Quando un'installazione fallisce sul computer di qualcun altro, il file di
/// log è l'unica cosa che si può chiedere.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("VK_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,vanzakart_setup=debug,vk_install=debug"));

    let directory = vk_install::paths::installer_data_root()?.join("logs");
    std::fs::create_dir_all(&directory).ok()?;
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never(&directory, "setup.log"));

    let registry = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false),
    );

    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    registry.try_init().ok()?;
    Some(guard)
}
