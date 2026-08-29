//! Guscio IPC.
//!
//! I comandi non contengono logica: validano ciò che arriva dal frontend,
//! chiamano `vk-install` e traducono l'esito. Nessun percorso arriva dalla UI
//! senza passare da `fsops::ensure_safe_target`, e nessun URL viene mai dal
//! frontend (§D-005, §D-017).

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use vk_core::progress::{ProgressSink, ProgressUpdate};
use vk_install::discovery::{self, ExistingInstall};
use vk_install::{
    fsops, paths, platform, uninstall, InstallError, InstallMode, InstallOptions, InstallReport,
    RemovalItem, Target, UninstallOptions, UninstallReport,
};

use crate::error::{SetupError, SetupResult};
use crate::state::{offline_error, SetupState, SETUP_VERSION};

/// Evento con cui il backend spinge i progressi, come nel launcher.
pub const PROGRESS_EVENT: &str = "vk://progress";

/// Ciò che il frontend riceve all'avvio.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapView {
    pub mode: &'static str,
    pub platform: &'static str,
    pub target: String,
    pub setup_version: String,
    pub default_install_dir: PathBuf,
    pub suggested_install_dirs: Vec<PathBuf>,
    pub default_backup_dir: PathBuf,
    pub supports_quick_launch: bool,
    pub supports_path_symlink: bool,
    pub existing: Option<ExistingView>,
    /// Cartella del launcher legacy in C#, se è ancora installato. È solo
    /// un'informazione: resta dov'è (§D-055).
    pub legacy_install_dir: Option<PathBuf>,
    pub release: Option<ReleaseView>,
    /// Perché il manifest non è disponibile, quando non lo è.
    pub release_error: Option<String>,
    pub download_page_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingView {
    pub install_dir: PathBuf,
    pub version: String,
    pub managed: bool,
    pub executable: Option<PathBuf>,
    pub bytes: u64,
}

impl From<&ExistingInstall> for ExistingView {
    fn from(existing: &ExistingInstall) -> Self {
        Self {
            install_dir: existing.install_dir.clone(),
            version: existing.version.clone(),
            managed: existing.managed,
            executable: existing.executable.clone(),
            bytes: fsops::path_size(&existing.install_dir),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub package_key: String,
    pub size_bytes: u64,
    pub verifiable: bool,
}

/// Scelte della procedura guidata, così come arrivano dalla UI.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptionsInput {
    pub install_dir: String,
    pub mode: InstallMode,
    pub backup_data: bool,
    pub backup_dir: String,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    pub quick_launch_shortcut: bool,
    pub uninstall_entry: bool,
    pub path_symlink: bool,
}

impl InstallOptionsInput {
    fn into_options(self) -> Result<InstallOptions, InstallError> {
        let install_dir = fsops::ensure_safe_target(&PathBuf::from(&self.install_dir))?;
        let backup_dir = if self.backup_dir.trim().is_empty() {
            paths::default_backup_dir()
        } else {
            fsops::absolutize(&PathBuf::from(&self.backup_dir))?
        };

        Ok(InstallOptions {
            install_dir,
            mode: self.mode,
            backup_data: self.backup_data,
            backup_dir,
            desktop_shortcut: self.desktop_shortcut,
            start_menu_shortcut: self.start_menu_shortcut,
            // Le opzioni che una piattaforma non ha restano spente anche se
            // il frontend le manda accese.
            quick_launch_shortcut: self.quick_launch_shortcut && cfg!(windows),
            uninstall_entry: self.uninstall_entry,
            path_symlink: self.path_symlink && cfg!(all(unix, not(target_os = "macos"))),
            copy_uninstaller: true,
            register_system: true,
        })
    }
}

/// Elenco di ciò che verrà rimosso.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalPlanView {
    pub items: Vec<RemovalItem>,
    pub total_bytes: u64,
    pub install_dir: PathBuf,
    pub version: String,
    pub managed: bool,
    /// `true` quando la cartella dell'utente contiene una modpack rimovibile.
    pub has_modpacks: bool,
}

#[tauri::command]
pub async fn setup_bootstrap(state: State<'_, Arc<SetupState>>) -> SetupResult<BootstrapView> {
    let target = Target::current();
    let existing = if state.mode == crate::state::Mode::Uninstall {
        discovery::find_for_uninstall()
    } else {
        discovery::find()
    };

    let (release, release_error) = match state.require_manifest().await {
        Ok(manifest) => match manifest.select(target) {
            Ok((key, package)) => (
                Some(ReleaseView {
                    version: manifest.version.clone(),
                    notes: manifest.notes.clone(),
                    pub_date: manifest.pub_date.clone(),
                    package_key: key,
                    size_bytes: package.size,
                    verifiable: !package.sha256.is_empty(),
                }),
                None,
            ),
            Err(error) => (None, Some(error.to_string())),
        },
        Err(error) => (None, Some(error.to_string())),
    };

    Ok(BootstrapView {
        mode: state.mode.as_str(),
        platform: target.display_name(),
        target: target.key(),
        setup_version: SETUP_VERSION.to_string(),
        default_install_dir: existing
            .as_ref()
            .map(|found| found.install_dir.clone())
            .unwrap_or_else(paths::default_install_dir),
        suggested_install_dirs: paths::suggested_install_dirs(),
        default_backup_dir: paths::default_backup_dir(),
        supports_quick_launch: cfg!(windows),
        supports_path_symlink: cfg!(all(unix, not(target_os = "macos"))),
        existing: existing.as_ref().map(ExistingView::from),
        legacy_install_dir: discovery::legacy_install(),
        release,
        release_error,
        download_page_url: state.download_page_url().await,
    })
}

/// Rilegge il manifest dal server, per il pulsante "Riprova".
#[tauri::command]
pub async fn setup_refresh(state: State<'_, Arc<SetupState>>) -> SetupResult<ReleaseView> {
    let urls = state.manifest_urls().await;
    let manifest = state.installer.fetch_manifest(&urls).await?;
    let (key, package) = manifest.select(Target::current())?;
    let view = ReleaseView {
        version: manifest.version.clone(),
        notes: manifest.notes.clone(),
        pub_date: manifest.pub_date.clone(),
        package_key: key,
        size_bytes: package.size,
        verifiable: !package.sha256.is_empty(),
    };
    state.store_manifest(manifest);
    Ok(view)
}

/// Controlli su spazio, permessi e launcher aperto.
#[tauri::command]
pub async fn setup_preflight(
    state: State<'_, Arc<SetupState>>,
    install_dir: String,
) -> SetupResult<vk_install::install::Preflight> {
    let manifest = state.manifest().ok_or_else(offline_error)?;
    let directory = fsops::ensure_safe_target(&PathBuf::from(install_dir))?;
    Ok(state.installer.preflight(&manifest, &directory)?)
}

#[tauri::command]
pub async fn setup_install(
    app: AppHandle,
    state: State<'_, Arc<SetupState>>,
    options: InstallOptionsInput,
) -> SetupResult<InstallReport> {
    let manifest = match state.manifest() {
        Some(manifest) => manifest,
        None => state.require_manifest().await?,
    };
    let options = options.into_options()?;

    let (cancel, _guard) = state.begin().ok_or_else(SetupError::busy)?;
    let progress = progress_sink(&app);

    let report = state
        .installer
        .install(&manifest, &options, &progress, &cancel)
        .await?;

    tracing::info!(
        version = %report.version,
        directory = %report.install_dir.display(),
        "installazione completata"
    );
    Ok(report)
}

#[tauri::command]
pub fn setup_cancel(state: State<'_, Arc<SetupState>>) {
    state.cancel();
}

/// Avvia il launcher appena installato.
#[tauri::command]
pub fn setup_launch(executable: String) -> SetupResult<()> {
    let path = fsops::absolutize(&PathBuf::from(executable))?;
    if !path.exists() {
        return Err(SetupError::new(
            "executable-not-found",
            format!("{} does not exist", path.display()),
        ));
    }
    platform::launch_detached(&path)?;
    Ok(())
}

/// Elenco di ciò che la disinstallazione porterà via.
#[tauri::command]
pub fn setup_uninstall_plan(options: UninstallOptions) -> SetupResult<RemovalPlanView> {
    let existing = discovery::find_for_uninstall().ok_or(InstallError::NotInstalled)?;
    let record = existing.record_or_reconstructed();
    let items = uninstall::plan(&record, &options);
    let total_bytes = items.iter().map(|item| item.bytes).sum();

    Ok(RemovalPlanView {
        items,
        total_bytes,
        install_dir: record.install_dir.clone(),
        version: record.version.clone(),
        managed: existing.managed,
        has_modpacks: uninstall::modpack_paths(false)
            .iter()
            .any(|(_, path)| path.is_dir()),
    })
}

#[tauri::command]
pub async fn setup_uninstall_run(
    app: AppHandle,
    state: State<'_, Arc<SetupState>>,
    options: UninstallOptions,
) -> SetupResult<UninstallReport> {
    let existing = discovery::find_for_uninstall().ok_or(InstallError::NotInstalled)?;
    let record = existing.record_or_reconstructed();

    let (_cancel, _guard) = state.begin().ok_or_else(SetupError::busy)?;
    let progress = progress_sink(&app);
    let report = uninstall::run(&record, &options, &progress)?;

    tracing::info!(
        removed = report.removed.len(),
        failed = report.failed.len(),
        deferred = report.deferred,
        "disinstallazione completata"
    );
    Ok(report)
}

/// Apre la pagina dei download nel browser di sistema.
///
/// L'indirizzo lo decide il backend leggendolo da `endpoints.json`: il
/// frontend chiede "apri la pagina", non "apri questo URL" (§D-005).
#[tauri::command]
pub async fn setup_open_download_page(
    app: AppHandle,
    state: State<'_, Arc<SetupState>>,
) -> SetupResult<()> {
    use tauri_plugin_opener::OpenerExt;

    let pagina = state.download_page_url().await;
    app.opener()
        .open_url(pagina, None::<&str>)
        .map_err(|error| SetupError::new("opener", error.to_string()))
}

/// Cartella dei dati del launcher, mostrata nella pagina di disinstallazione.
#[tauri::command]
pub fn setup_data_root() -> Option<PathBuf> {
    paths::launcher_data_root()
}

/// Aggiornamento di stato verso la UI.
///
/// Stessa forma di quello del launcher, con in più il tempo rimanente: il
/// setup legacy lo mostrava, ed è l'unica informazione che dice all'utente se
/// può andare a prendere un caffè (§D-018 per il throttling).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub phase: String,
    pub detail: String,
    pub percent: Option<f64>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_label: String,
    pub speed_label: String,
    pub eta_label: String,
}

fn progress_sink(app: &AppHandle) -> ProgressSink {
    use std::sync::Mutex;

    let app = app.clone();
    let meter = Mutex::new(Meter::new());

    Arc::new(move |update: ProgressUpdate| {
        let terminal = matches!(
            update.phase,
            vk_core::Phase::Completed | vk_core::Phase::Error
        );

        let rate = {
            let mut guard = meter.lock().expect("mutex dei progressi avvelenato");
            if !terminal && guard.since_emit() < std::time::Duration::from_millis(100) {
                return;
            }
            guard.tick(update.bytes_done)
        };

        let remaining = update.bytes_total.saturating_sub(update.bytes_done);
        let payload = ProgressEvent {
            phase: update.phase.label().to_string(),
            detail: vk_core::redact::redact(&update.detail),
            percent: update.percent,
            bytes_done: update.bytes_done,
            bytes_total: update.bytes_total,
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
            eta_label: match rate {
                Some(bytes_per_second) if !terminal && bytes_per_second > 0 && remaining > 0 => {
                    format_duration(remaining / bytes_per_second)
                }
                _ => String::new(),
            },
        };

        if let Err(error) = app.emit(PROGRESS_EVENT, payload) {
            tracing::debug!(%error, "evento di progresso non consegnato");
        }
    })
}

/// "45 s", "3 min", "1,2 h": la stessa scala del setup legacy.
fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds} s")
    } else if seconds < 3600 {
        format!("{} min", seconds / 60)
    } else {
        format!("{:.1} h", seconds as f64 / 3600.0)
    }
}

/// Misura la velocità di trasferimento fra due invii di progresso.
///
/// Copiata dal launcher (`commands::Meter`): il valore istantaneo su una
/// finestra di 100 ms salta troppo per essere letto, quindi viene lisciato con
/// una media esponenziale.
struct Meter {
    at: std::time::Instant,
    bytes: u64,
    rate: Option<f64>,
}

impl Meter {
    const SMOOTHING: f64 = 0.3;

    fn new() -> Self {
        Self {
            at: std::time::Instant::now() - std::time::Duration::from_secs(1),
            bytes: 0,
            rate: None,
        }
    }

    fn since_emit(&self) -> std::time::Duration {
        self.at.elapsed()
    }

    fn tick(&mut self, bytes_done: u64) -> Option<u64> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.at).as_secs_f64();
        let previous = std::mem::replace(&mut self.bytes, bytes_done);
        self.at = now;

        if elapsed <= 0.0 || bytes_done < previous {
            // I byte tornati indietro sono un file nuovo che comincia: la
            // misura riparte invece di mostrare una velocità negativa.
            self.rate = None;
            return None;
        }

        let sample = (bytes_done - previous) as f64 / elapsed;
        let smoothed = match self.rate {
            Some(previous) => previous * (1.0 - Self::SMOOTHING) + sample * Self::SMOOTHING,
            None => sample,
        };
        self.rate = Some(smoothed);
        (smoothed > 0.0).then_some(smoothed as u64)
    }
}

/// Mostra la finestra a layout pronto, come fa il launcher.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(directory: &str) -> InstallOptionsInput {
        InstallOptionsInput {
            install_dir: directory.to_string(),
            mode: InstallMode::Fresh,
            backup_data: false,
            backup_dir: String::new(),
            desktop_shortcut: true,
            start_menu_shortcut: true,
            quick_launch_shortcut: true,
            uninstall_entry: true,
            path_symlink: true,
        }
    }

    #[test]
    fn a_dangerous_install_folder_never_reaches_the_engine() {
        let home = dirs::home_dir().expect("home");
        let error = input(&home.to_string_lossy())
            .into_options()
            .expect_err("rifiutata");
        assert_eq!(error.code(), "unsafe-path");
    }

    #[test]
    fn an_empty_backup_folder_falls_back_to_the_default() {
        let directory = std::env::temp_dir().join("vk-setup-test").join("app");
        let options = input(&directory.to_string_lossy())
            .into_options()
            .expect("opzioni");
        assert_eq!(options.backup_dir, paths::default_backup_dir());
    }

    #[test]
    fn options_that_the_platform_does_not_have_stay_off() {
        let directory = std::env::temp_dir().join("vk-setup-test").join("app");
        let options = input(&directory.to_string_lossy())
            .into_options()
            .expect("opzioni");
        assert_eq!(options.quick_launch_shortcut, cfg!(windows));
        assert_eq!(
            options.path_symlink,
            cfg!(all(unix, not(target_os = "macos")))
        );
    }

    #[test]
    fn a_duration_is_shown_in_the_right_unit() {
        assert_eq!(format_duration(45), "45 s");
        assert_eq!(format_duration(180), "3 min");
        assert_eq!(format_duration(5400), "1.5 h");
    }

    #[test]
    fn the_speed_meter_restarts_when_bytes_go_backwards() {
        let mut meter = Meter::new();
        assert!(meter.tick(1_000_000).is_some());
        // Un secondo file che ricomincia da zero non deve produrre una
        // velocità negativa, né un ETA fantasioso.
        assert!(meter.tick(0).is_none());
    }

    #[test]
    fn the_uninstaller_is_always_copied() {
        let directory = std::env::temp_dir().join("vk-setup-test").join("app");
        assert!(
            input(&directory.to_string_lossy())
                .into_options()
                .expect("opzioni")
                .copy_uninstaller
        );
    }
}
