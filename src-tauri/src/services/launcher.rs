//! Aggiornamento del launcher stesso.
//!
//! Due domande diverse, e quindi due funzioni.
//!
//! [`status`] è quella che si fa a ogni avvio: `versions.json` pubblica
//! `launcher_version`, il launcher lo confronta con la propria e accende il
//! puntino sulla Home. Non contatta nessuno di suo — legge ciò che
//! `mods::check_updates` ha già scaricato — ed è la stessa cosa che faceva
//! `MainWindow.PerformLauncherUpdateAsync` del launcher legacy.
//!
//! [`check`] e [`install`] sono quelle che si fanno quando l'utente apre la
//! finestra dell'aggiornamento. Leggono `install.json`, cioè lo **stesso
//! manifest che legge l'installer**, e applicano il pacchetto della
//! piattaforma corrente **dentro la cartella in cui il launcher sta girando**
//! (vedi `vk_install::update` e `docs/decisions.md` §D-084).
//!
//! È il punto in cui il launcher legacy apriva una console nera con uno
//! script PowerShell non firmato, e in cui la 2.0.1 faceva peggio: lanciava
//! l'installer NSIS del pacchetto Tauri, che ignorava la cartella scelta a suo
//! tempo, si installava in `%LOCALAPPDATA%` e lasciava sul computer una
//! seconda voce "Disinstalla" accanto a quella vera. Qui non viene eseguito
//! nessun secondo installer: si scarica, si verifica impronta e firma, e si
//! sostituiscono i file dove sono.

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
use vk_install::elevated::{self, ElevatedJob, ElevatedResult};
use vk_install::update::{self, Access, UpdateTarget};
use vk_install::{InstallError, Installer, ReleaseManifest};

use crate::error::{AppError, AppResult};
use crate::state::{AppState, LAUNCHER_VERSION};

/// Indirizzo di ripiego del manifest di rilascio, allineato a
/// `resources/endpoints.default.json`. Serve solo se `endpoints.json` non
/// dichiara la chiave.
const INSTALL_MANIFEST_URL: &str = "https://vanzakart.net:8443/Launcher/install.json";

/// Nome con cui l'operazione firma i progressi e occupa il turno (§D-086).
pub const OPERATION: &str = "launcher";

/// Stato dell'aggiornamento del launcher, per il frontend.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateStatus {
    /// Versione in esecuzione.
    pub current: String,
    /// Versione pubblicata su `versions.json`, vuota se non l'abbiamo letta.
    pub latest: String,
    /// `true` solo se quella pubblicata è **più recente** di quella in uso.
    pub available: bool,
    pub changelog: Vec<String>,
    /// Pagina da cui scaricarla a mano, quando l'aggiornamento da qui non si
    /// può fare.
    pub download_page: String,
    /// `false` quando `versions.json` non è stato letto in questa sessione.
    pub checked: bool,
    pub message: String,
}

/// Ciò che si sa dopo aver letto `install.json`: la versione pubblicata, il
/// pacchetto della piattaforma corrente e dove finirebbe.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateOffer {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub pub_date: String,
    /// Cartella in cui il launcher è installato: quella scelta a suo tempo,
    /// non una nuova.
    pub install_dir: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub enough_space: bool,
    /// Il pacchetto dichiara un'impronta SHA-256.
    pub verifiable: bool,
    /// Il pacchetto è firmato: si può dimostrare chi l'ha pubblicato.
    pub signed: bool,
    /// `true` quando l'installazione ha il registro scritto dall'installer.
    pub managed: bool,
    /// `true` quando il pulsante "Aggiorna" ha senso.
    pub can_install: bool,
    /// `true` quando lo scambio dei file richiede i permessi di
    /// amministratore: Windows chiederà una conferma (§D-093).
    pub needs_elevation: bool,
    pub download_page: String,
    /// Perché da qui non si può aggiornare, quando non si può. Vuoto
    /// altrimenti.
    pub blocked: String,
    /// Codice stabile del motivo, per il frontend.
    pub blocked_code: String,
}

/// Esito dell'aggiornamento.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateOutcome {
    pub version: String,
    pub install_dir: String,
    pub bytes: u64,
    /// `true` quando qualcosa della versione precedente non si è potuto
    /// cancellare subito: sparirà al prossimo avvio.
    pub cleanup_pending: bool,
}

/// Confronta la versione in esecuzione con quella pubblicata.
///
/// Non contatta nessuno: legge ciò che `mods::check_updates` ha già scaricato.
pub async fn status(state: &Arc<AppState>) -> AppResult<LauncherUpdateStatus> {
    let remote = state.remote.read().await.clone();
    let download_page = state.endpoints.read().await.download_page_url.clone();

    let latest = remote.info.launcher_version.trim().to_string();
    let available = !latest.is_empty() && vk_core::is_newer(&latest, LAUNCHER_VERSION);

    let message = if !remote.checked {
        "No check run in this session.".to_string()
    } else if latest.is_empty() {
        "The server publishes no launcher version.".to_string()
    } else if available {
        format!("Version {latest} is available.")
    } else {
        format!("The launcher is up to date (v{LAUNCHER_VERSION}).")
    };

    Ok(LauncherUpdateStatus {
        current: LAUNCHER_VERSION.to_string(),
        latest,
        available,
        changelog: remote.info.launcher_changelog.clone(),
        download_page,
        checked: remote.checked,
        message,
    })
}

/// Legge `install.json` e prepara l'offerta di aggiornamento.
///
/// Un'installazione che non può aggiornarsi da sé — una build dai sorgenti,
/// una cartella in sola lettura, una copia arrivata dal gestore di pacchetti
/// della distribuzione — non è un errore da mostrare come guasto: è
/// un'offerta con il motivo scritto dentro, e il pulsante che porta alla
/// pagina dei download al posto di quello che aggiorna.
pub async fn check(state: &Arc<AppState>) -> AppResult<LauncherUpdateOffer> {
    let download_page = state.endpoints.read().await.download_page_url.clone();

    let target = match UpdateTarget::current() {
        Ok(target) => target,
        Err(error) => return Ok(blocked(&error, download_page)),
    };

    let engine = engine(state)?;
    let manifest = engine.fetch_manifest(&manifest_urls(state).await).await?;
    let plan = match update::plan(&manifest, &target, LAUNCHER_VERSION) {
        Ok(plan) => plan,
        Err(error) => return Ok(blocked(&error, download_page)),
    };

    let can_install = plan.can_install();
    let (blocked_reason, blocked_code) = if plan.enough_space {
        (String::new(), String::new())
    } else {
        (
            format!(
                "Not enough space: {} needed, {} free.",
                vk_core::progress::format_bytes(plan.required_bytes),
                vk_core::progress::format_bytes(plan.available_bytes)
            ),
            "not-enough-space".to_string(),
        )
    };

    Ok(LauncherUpdateOffer {
        current: plan.current_version,
        latest: plan.latest_version,
        available: plan.available,
        notes: plan.notes,
        pub_date: plan.pub_date,
        install_dir: plan.install_dir.to_string_lossy().to_string(),
        size_bytes: plan.size_bytes,
        size_label: vk_core::progress::format_bytes(plan.size_bytes),
        enough_space: plan.enough_space,
        verifiable: plan.verifiable,
        signed: plan.signed,
        managed: plan.managed,
        can_install,
        needs_elevation: plan.needs_elevation,
        download_page,
        blocked: blocked_reason,
        blocked_code,
    })
}

/// Scarica e installa l'aggiornamento nella cartella in cui il launcher gira.
///
/// Prende lo stesso lucchetto della modpack: un launcher che si sostituisce
/// da sé mentre sta scaricando altro si porterebbe dietro un download a metà.
///
/// Se la cartella si scrive solo come amministratore — `Programmi` — il
/// download resta qui e lo scambio lo fa un processo elevato, dopo la
/// conferma della UAC (§D-093). `owner_window` è la finestra davanti a cui
/// Windows mette la richiesta.
pub async fn install(
    state: &Arc<AppState>,
    progress: ProgressSink,
    owner_window: Option<isize>,
) -> AppResult<LauncherUpdateOutcome> {
    let guard = state.begin_operation(OPERATION)?;
    let cancel = state.renew_cancel_token().await;

    let result = async {
        let target = UpdateTarget::current()?;
        let engine = engine(state)?;
        let manifest = engine.fetch_manifest(&manifest_urls(state).await).await?;

        if target.access()? == Access::Elevated {
            return install_elevated(&engine, &manifest, &target, &progress, &cancel, owner_window)
                .await;
        }

        match engine
            .update_in_place(&manifest, &target, &progress, &cancel)
            .await
        {
            Ok(report) => {
                tracing::info!(
                    version = %report.version,
                    directory = %report.install_dir.display(),
                    pending = report.pending_cleanup.len(),
                    "launcher aggiornato in loco"
                );
                Ok(LauncherUpdateOutcome {
                    version: report.version,
                    install_dir: report.install_dir.to_string_lossy().to_string(),
                    bytes: report.bytes,
                    cleanup_pending: !report.pending_cleanup.is_empty(),
                })
            }
            // Un permesso negato a metà scambio: la cartella è già tornata
            // com'era, e la stessa operazione come amministratore può riuscire.
            Err(InstallError::NeedsElevation(reason)) if cfg!(windows) => {
                tracing::info!(%reason, "permesso negato durante lo scambio: si riprova come amministratore");
                install_elevated(&engine, &manifest, &target, &progress, &cancel, owner_window)
                    .await
            }
            Err(error) => Err(error.into()),
        }
    }
    .await;
    drop(guard);

    if let Err(error) = &result {
        progress(ProgressUpdate::new(
            Phase::Error,
            vk_core::redact::redact(&error.to_string()),
        ));
    }
    result
}

/// L'aggiornamento di un'installazione in `Programmi`.
///
/// Il download e la verifica restano in questo processo, con la barra e il
/// pulsante per annullare. Poi il launcher avvia **sé stesso** come
/// amministratore con un job che dice dove sta il pacchetto, aspetta che
/// finisca e ne legge l'esito. Il processo elevato non si fida del job: vedi
/// `vk_install::elevated`.
async fn install_elevated(
    engine: &Installer,
    manifest: &ReleaseManifest,
    target: &UpdateTarget,
    progress: &ProgressSink,
    cancel: &CancelToken,
    owner_window: Option<isize>,
) -> AppResult<LauncherUpdateOutcome> {
    if !cfg!(windows) {
        return Err(InstallError::NotUpdatable(format!(
            "{} cannot be written to",
            target.install_dir.display()
        ))
        .into());
    }

    let downloaded = engine
        .download_update(manifest, target, progress, cancel)
        .await?;
    if let Err(error) = cancel.check() {
        let _ = std::fs::remove_file(&downloaded.archive);
        return Err(error.into());
    }

    let nonce = hex(&crate::platform::random_bytes::<16>());
    let job_dir = std::env::temp_dir().join(format!("vk-launcher-update-{nonce}"));
    let job = ElevatedJob {
        version: downloaded.version.clone(),
        archive: downloaded.archive.clone(),
        nonce,
    };

    let result = run_elevated_job(&job, &job_dir, target, progress, owner_window).await;

    let _ = std::fs::remove_dir_all(&job_dir);
    let _ = std::fs::remove_file(&downloaded.archive);
    result
}

async fn run_elevated_job(
    job: &ElevatedJob,
    job_dir: &Path,
    target: &UpdateTarget,
    progress: &ProgressSink,
    owner_window: Option<isize>,
) -> AppResult<LauncherUpdateOutcome> {
    let job_path = job.write_to(job_dir)?;

    progress(ProgressUpdate::new(
        Phase::Installing,
        "Waiting for the administrator confirmation",
    ));

    let executable = target.executable.clone();
    let args = vec![
        OsString::from(elevated::APPLY_UPDATE_FLAG),
        job_path.into_os_string(),
    ];
    let exit = tokio::task::spawn_blocking(move || {
        vk_elevate::run_elevated(&executable, &args, owner_window)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?;

    let exit = match exit {
        Ok(exit) => exit,
        Err(vk_elevate::ElevationError::Cancelled) => {
            tracing::info!("richiesta di amministratore rifiutata: niente è cambiato");
            return Err(InstallError::ElevationDeclined.into());
        }
        Err(error) => return Err(InstallError::platform(error.to_string()).into()),
    };

    let result = elevated::read_result(&target.install_dir, &job.nonce);
    if exit != elevated::EXIT_OK {
        let error = elevated::error_for_exit(exit, result.as_ref());
        tracing::warn!(
            exit,
            error = %vk_core::redact::redact(&error.to_string()),
            "aggiornamento come amministratore non riuscito"
        );
        return Err(error.into());
    }

    let result = result.unwrap_or_else(|| ElevatedResult {
        ok: true,
        version: job.version.clone(),
        ..ElevatedResult::default()
    });
    tracing::info!(
        version = %result.version,
        directory = %target.install_dir.display(),
        "launcher aggiornato come amministratore"
    );
    progress(ProgressUpdate::new(Phase::Completed, "Update complete").with_percent(100.0));

    Ok(LauncherUpdateOutcome {
        version: result.version,
        install_dir: target.install_dir.to_string_lossy().to_string(),
        bytes: result.bytes,
        cleanup_pending: result.cleanup_pending,
    })
}

/// Se questo processo è quello elevato di un aggiornamento, fa il suo lavoro
/// ed esce.
///
/// Va chiamata per prima cosa: il processo elevato non apre finestre, non
/// avvia WebView2 e non tocca la cartella dati. Quando è un genitore a
/// digitare la password gira con il **suo** profilo, e non deve lasciarci
/// niente (§D-093).
pub fn handle_elevated_update_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(elevated::APPLY_UPDATE_FLAG)) {
        return;
    }

    let exit = match args.next() {
        Some(job) => apply_elevated_job(Path::new(&job)),
        None => elevated::exit_code(&InstallError::platform("no update job")),
    };
    std::process::exit(exit as i32);
}

fn apply_elevated_job(job_path: &Path) -> u32 {
    let job = match ElevatedJob::read(job_path) {
        Ok(job) => job,
        Err(error) => return elevated::exit_code(&error),
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return elevated::exit_code(&InstallError::platform(error.to_string())),
    };

    let applied = runtime.block_on(async {
        let engine = Installer::new(LAUNCHER_VERSION, None)?;
        engine
            .apply_elevated_job(&job, &elevated_manifest_urls())
            .await
    });

    match applied {
        Ok(report) => {
            elevated::write_result(&report.install_dir, &ElevatedResult::success(&job, &report));
            elevated::EXIT_OK
        }
        Err(error) => {
            if let Ok(target) = UpdateTarget::current() {
                elevated::write_result(
                    &target.install_dir,
                    &ElevatedResult::failure(&job.nonce, &error),
                );
            }
            elevated::exit_code(&error)
        }
    }
}

/// Gli indirizzi di `install.json` per il processo elevato: solo quelli
/// **compilati**. La cache degli endpoint sta nella cartella dati
/// dell'utente, che per un processo amministratore è un dato non fidato.
fn elevated_manifest_urls() -> Vec<String> {
    let defaults = crate::storage::endpoints::defaults();
    let mut urls: Vec<String> = std::iter::once(defaults.launcher_install_url)
        .chain(defaults.launcher_install_mirrors)
        .filter(|url| vk_core::endpoints::is_safe_endpoint(url))
        .collect();
    urls.push(INSTALL_MANIFEST_URL.to_string());
    vk_core::net::dedupe_urls(&urls)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Toglie ciò che un aggiornamento precedente non aveva potuto cancellare.
///
/// Si chiama all'avvio, prima di qualunque altra cosa: a quel punto il
/// binario che teneva aperti quei file non esiste più. Non fallisce mai —
/// nel peggiore dei casi non toglie niente.
pub fn sweep_previous_version() {
    let Ok(target) = UpdateTarget::current() else {
        return;
    };
    let removed = update::sweep_leftovers(&target.install_dir);
    if removed > 0 {
        tracing::info!(removed, "residui dell'aggiornamento precedente rimossi");
    }
}

/// Il motore di `vk-install`, con il client HTTP del launcher.
fn engine(state: &Arc<AppState>) -> AppResult<Installer> {
    Ok(Installer::new(LAUNCHER_VERSION, None)?.with_downloader(state.downloader.clone()))
}

/// Indirizzi da cui leggere `install.json`, in ordine di tentativo: quelli
/// dichiarati da `endpoints.json`, poi la ricaduta compilata.
///
/// Come per ogni altro indirizzo, il frontend non ne vede nessuno (§D-005).
async fn manifest_urls(state: &Arc<AppState>) -> Vec<String> {
    let endpoints = state.endpoints.read().await;
    let mut urls: Vec<String> = std::iter::once(endpoints.launcher_install_url.clone())
        .chain(endpoints.launcher_install_mirrors.clone())
        .filter(|url| vk_core::endpoints::is_safe_endpoint(url))
        .collect();
    urls.push(INSTALL_MANIFEST_URL.to_string());

    vk_core::net::dedupe_urls(&urls)
}

/// Un'offerta che dice perché non si può aggiornare da qui.
fn blocked(error: &vk_install::InstallError, download_page: String) -> LauncherUpdateOffer {
    tracing::info!(%error, "aggiornamento in loco non disponibile");

    LauncherUpdateOffer {
        current: LAUNCHER_VERSION.to_string(),
        download_page,
        blocked: vk_core::redact::redact(&error.to_string()),
        blocked_code: error.code().to_string(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use std::path::Path;

    async fn state_at(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    async fn with_published(dir: &Path, version: &str) -> Arc<AppState> {
        let state = state_at(dir).await;
        {
            let mut remote = state.remote.write().await;
            remote.info.launcher_version = version.to_string();
            remote.checked = true;
        }
        state
    }

    #[tokio::test]
    async fn without_a_check_nothing_is_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let status = status(&state).await.unwrap();
        assert_eq!(status.current, LAUNCHER_VERSION);
        assert!(!status.checked);
        assert!(!status.available);
        assert!(status.latest.is_empty());
    }

    #[tokio::test]
    async fn a_newer_published_version_is_offered() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), "99.0.0").await;

        let status = status(&state).await.unwrap();
        assert!(status.available);
        assert_eq!(status.latest, "99.0.0");
        assert!(status.message.contains("99.0.0"));
    }

    /// Il caso reale: `versions.json` pubblica il launcher C# 1.5.1 mentre qui
    /// gira la 2.0.0. Annunciarlo come aggiornamento manderebbe tutti indietro.
    #[tokio::test]
    async fn an_older_published_version_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), "1.5.1").await;

        let status = status(&state).await.unwrap();
        assert!(!status.available, "1.5.1 non è più recente di 2.0.0");
        assert!(status.message.contains("up to date"));
    }

    #[tokio::test]
    async fn the_same_version_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), LAUNCHER_VERSION).await;

        assert!(!status(&state).await.unwrap().available);
    }

    /// Gli indirizzi del manifest li decide il backend, e la ricaduta
    /// compilata è sempre l'ultima carta: senza, un `endpoints.json` senza
    /// quella chiave lascerebbe il launcher senza aggiornamenti.
    #[tokio::test]
    async fn the_manifest_addresses_always_end_with_the_compiled_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let urls = manifest_urls(&state).await;
        assert!(!urls.is_empty());
        assert_eq!(urls.last().map(String::as_str), Some(INSTALL_MANIFEST_URL));
        assert!(
            urls.iter().all(|url| url.starts_with("https://")),
            "{urls:?}"
        );
    }

    /// Un `endpoints.json` manomesso non deve poter dirottare l'aggiornamento
    /// su http (§D-004).
    #[tokio::test]
    async fn an_insecure_address_never_reaches_the_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        {
            let mut endpoints = state.endpoints.write().await;
            endpoints.launcher_install_url = "http://insicuro.example/install.json".into();
            endpoints.launcher_install_mirrors = vec!["ftp://insicuro.example/i.json".into()];
        }

        let urls = manifest_urls(&state).await;
        assert_eq!(urls, vec![INSTALL_MANIFEST_URL.to_string()]);
    }

    /// Il processo elevato non legge la cache degli endpoint, che sta nella
    /// cartella dati dell'utente: solo gli indirizzi compilati, e solo https.
    #[test]
    fn the_elevated_process_only_trusts_the_compiled_addresses() {
        let urls = elevated_manifest_urls();
        assert_eq!(urls.last().map(String::as_str), Some(INSTALL_MANIFEST_URL));
        assert!(
            urls.iter().all(|url| url.starts_with("https://")),
            "{urls:?}"
        );
    }

    #[test]
    fn the_nonce_is_plain_hex() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }

    /// Ciò che non si può aggiornare si racconta, non si nasconde dietro un
    /// errore generico.
    #[test]
    fn a_blocked_update_keeps_the_reason_and_the_download_page() {
        let offer = blocked(
            &vk_install::InstallError::NotUpdatable("build dai sorgenti".into()),
            "https://vwfc.vanzakart.net/".into(),
        );

        assert_eq!(offer.blocked_code, "not-updatable");
        assert!(offer.blocked.contains("build dai sorgenti"));
        assert!(!offer.can_install);
        assert!(!offer.available);
        assert_eq!(offer.download_page, "https://vwfc.vanzakart.net/");
        assert_eq!(offer.current, LAUNCHER_VERSION);
    }
}
