//! Avvio di Dolphin con la patch Riivolution.
//!
//! Porta `MainWindow.xaml.cs::LaunchButton_OnClick`. Differenza sostanziale:
//! il processo viene avviato con argomenti separati tramite `std::process`,
//! **mai** attraverso una shell.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vk_dolphin::riivolution::{self, GameModDescriptor};

use crate::error::{AppError, AppResult};
use crate::state::{now_iso, unix_now, AppState, GameSession};
use crate::storage::preferences::OpenSession;

/// Esito della richiesta di avvio.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: u32,
    pub descriptor_path: String,
    pub channel: vk_core::Channel,
    /// `true` se prima di avviare è stato chiuso un Dolphin già aperto.
    pub closed_previous: bool,
}

/// Motivo per cui l'avvio non è possibile, con la pagina dove risolverlo.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchBlocker {
    pub code: String,
    pub message: String,
    pub navigate_to: String,
}

/// Controlla i prerequisiti senza avviare nulla.
///
/// La UI la chiama per decidere se il pulsante PLAY deve essere attivo.
pub async fn preflight(state: &Arc<AppState>) -> AppResult<Option<LaunchBlocker>> {
    let channel = state.channel().await;
    let layout = state.layout(channel).await;
    let settings = state.settings.read().await.clone();

    if !layout.is_installed() {
        return Ok(Some(LaunchBlocker {
            code: "mod-not-installed".into(),
            message: format!(
                "Install the {} modpack before starting. The other channel is left alone.",
                channel.display_name()
            ),
            navigate_to: "mods".into(),
        }));
    }

    if let Err(error) = riivolution::validate_preconditions(
        &settings.dolphin(),
        &settings.rom(),
        &settings.user_folder(),
        &layout.riivolution_xml(),
        layout.directory_name(),
    ) {
        let error = AppError::from(error);
        let code = error.code().to_string();
        return Ok(Some(LaunchBlocker {
            message: if code == "mod-incomplete" {
                // Il caso peggiore da diagnosticare: Dolphin accetterebbe il
                // descrittore, non applicherebbe nessuna patch e partirebbe
                // Mario Kart Wii originale.
                format!(
                    "{error}. Riscarica i file della modpack dalla pagina Mod — «Installa / \
                     aggiorna» ripristina solo quelli danneggiati, «Ripara file» riscarica \
                     tutto: finché non lo fai, Dolphin avvia Mario Kart Wii originale."
                )
            } else {
                error.to_string()
            },
            navigate_to: if code == "mod-not-installed" || code == "mod-incomplete" {
                "mods".into()
            } else {
                "settings".into()
            },
            code,
        }));
    }

    // Su Linux e macOS un binario o un AppImage appena scaricato arriva senza
    // il bit di esecuzione: senza questo controllo l'unico segnale sarebbe un
    // "Permission denied" all'avvio (§D-068).
    let executable = vk_dolphin::paths::resolve_launch_executable(&settings.dolphin());

    // `dolphin` e `dolphin-emu` si chiamano quasi uguale, e su Linux il primo
    // è il gestore di file di KDE: avviandolo si ottiene un errore su
    // `libKF6Archive` che non dice niente a nessuno (§D-073).
    if vk_dolphin::paths::is_kde_file_manager(&executable) {
        return Ok(Some(LaunchBlocker {
            code: "dolphin-is-file-manager".into(),
            message: format!(
                "{} is the KDE file manager, not the emulator. The one you need                  is called `dolphin-emu`: install it with your distribution's package                  manager, or download the AppImage from dolphin-emu.org and point                  the launcher at it.",
                executable.display()
            ),
            navigate_to: "settings".into(),
        }));
    }
    if !crate::platform::is_executable_file(&executable) {
        return Ok(Some(LaunchBlocker {
            code: "dolphin-not-executable".into(),
            message: format!(
                "Il file di Dolphin non ha il permesso di esecuzione. Dallo con:

                     chmod +x {}",
                executable.display()
            ),
            navigate_to: "settings".into(),
        }));
    }

    // Con l'opzione attiva un Dolphin già aperto non è un ostacolo: `launch`
    // lo chiude da sé prima di avviare (§D-089).
    let close_running = state.preferences.read().await.close_running_dolphin;
    if !close_running && crate::platform::is_executable_running(&settings.dolphin()) {
        return Ok(Some(LaunchBlocker {
            code: "dolphin-running".into(),
            message: "Dolphin is already open. Close it before starting: it has to re-read the \
                      mode and the bindings saved by the launcher. You can let the launcher close \
                      it for you from Settings → Launch options."
                .into(),
            navigate_to: "home".into(),
        }));
    }

    Ok(None)
}

/// Quanto aspettare che Dolphin sparisca dopo averlo chiuso.
const CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Chiude il Dolphin configurato se è aperto. `true` se c'era.
///
/// Prima chiude la sessione di gioco registrata, così il tempo giocato fino a
/// qui non va perso; poi chiude il processo e i suoi figli e aspetta che
/// spariscano davvero. Se qualcosa resta in piedi — un Dolphin avviato come
/// amministratore, che l'utente normale non può chiudere — si ferma con un
/// errore invece di avviarne un secondo accanto.
pub async fn close_running_dolphin(state: &Arc<AppState>) -> AppResult<bool> {
    let dolphin = state.settings.read().await.dolphin();
    if !crate::platform::is_executable_running(&dolphin) {
        return Ok(false);
    }

    let _ = finish_session(state).await;

    let target = dolphin.clone();
    let report = tokio::task::spawn_blocking(move || {
        crate::platform::terminate_executable(&target, CLOSE_TIMEOUT)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?;

    if report.remaining > 0 || crate::platform::is_executable_running(&dolphin) {
        return Err(AppError::Dolphin(vk_dolphin::DolphinError::LaunchFailed(
            "Dolphin is open and could not be closed. Close it yourself and try again \
             (if it was started as administrator, only an administrator can close it)."
                .into(),
        )));
    }

    // Su Windows un processo appena terminato può tenere ancora per un
    // istante i file che aveva aperti: la NAND, i salvataggi, gli INI.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    tracing::info!(
        processes = report.found,
        "Dolphin già aperto chiuso prima dell'avvio"
    );
    Ok(true)
}

/// Genera il descrittore e avvia Dolphin.
///
/// `on_session_end` riceve i minuti della sessione quando Dolphin si chiude:
/// il guscio IPC lo usa per dirlo alla UI, che rilegge le statistiche.
pub async fn launch(
    state: &Arc<AppState>,
    on_session_end: impl FnOnce(f64) + Send + 'static,
) -> AppResult<LaunchResult> {
    if let Some(blocker) = preflight(state).await? {
        return Err(AppError::Configuration(blocker.message));
    }

    let channel = state.channel().await;
    let layout = state.layout(channel).await;
    let settings = state.settings.read().await.clone();
    let (options, close_running) = {
        let preferences = state.preferences.read().await;
        (
            preferences.launch_options(),
            preferences.close_running_dolphin,
        )
    };

    // 0. Un Dolphin già aperto non rileggerebbe binding e modalità scritti
    //    dal launcher: con l'opzione attiva lo si chiude (§D-089).
    let closed_previous = if close_running {
        close_running_dolphin(state).await?
    } else {
        false
    };

    // 1. Descrittore Riivolution.
    let descriptor = GameModDescriptor::build(
        &settings.rom(),
        layout.directory_name(),
        &layout.mod_root(),
        &layout.riivolution_xml(),
        options,
    );
    let descriptor_path = state.paths.launcher_descriptor(channel);
    descriptor.write_to(&descriptor_path)?;

    // 2. Avvio, con argomenti separati e senza shell.
    let executable = vk_dolphin::paths::resolve_launch_executable(&settings.dolphin());
    let arguments = riivolution::launch_arguments(&settings.user_folder(), &descriptor_path);

    let mut command = std::process::Command::new(&executable);
    command.args(&arguments);
    if let Some(directory) = vk_dolphin::paths::executable_directory(&executable) {
        command.current_dir(directory);
    }

    // Dentro un AppImage il launcher gira con le librerie impacchettate: se le
    // eredita anche Dolphin, che è di sistema, non parte (§D-074).
    crate::platform::clean_child_environment(&mut command);

    let mut child = command.spawn().map_err(|error| {
        AppError::Dolphin(vk_dolphin::DolphinError::LaunchFailed(error.to_string()))
    })?;
    let pid = child.id();

    // Qualcuno deve raccogliere Dolphin quando esce: su Linux e macOS un
    // figlio mai atteso resta zombie, e agli occhi del sistema risulta ancora
    // "aperto" finché il launcher non si chiude.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    // 3. Statistiche e tracciamento della sessione: da qui un watcher guarda
    //    Dolphin e somma i minuti mentre si gioca e quando si chiude (§D-106).
    let process_started = crate::platform::process_start_time(pid);
    {
        let mut preferences = state.preferences.write().await;
        preferences.record_launch(now_iso());
        preferences.stats.open_session = Some(OpenSession {
            pid,
            process_started: process_started.unwrap_or(0),
            counted_until: unix_now(),
        });
    }
    *state.game_session.write().await = Some(GameSession::starting_now(pid, process_started));
    spawn_watcher(state, pid, on_session_end);

    state.persist_preferences().await?;

    tracing::info!(pid, channel = ?channel, "Dolphin avviato");

    Ok(LaunchResult {
        pid,
        descriptor_path: descriptor_path.to_string_lossy().to_string(),
        channel,
        closed_previous,
    })
}

/// Ogni quanto il watcher guarda se Dolphin è ancora aperto, e ogni quanto
/// salva i minuti giocati fin lì.
#[derive(Debug, Clone, Copy)]
struct WatchTiming {
    poll: Duration,
    checkpoint: Duration,
}

impl WatchTiming {
    /// Un launcher chiuso a metà partita — o ucciso — perde al massimo un
    /// minuto: il resto è già su disco.
    const DEFAULT: Self = Self {
        poll: Duration::from_secs(5),
        checkpoint: Duration::from_secs(60),
    };
}

/// Il processo di una sessione: PID e, quando si conosce, l'ora di avvio.
#[derive(Debug, Clone, Copy)]
struct SessionProcess {
    pid: u32,
    started: Option<u64>,
}

impl SessionProcess {
    /// `true` se quel processo è ancora vivo.
    ///
    /// Con l'ora di avvio un PID riciclato non inganna: un altro programma con
    /// lo stesso numero è partito in un altro momento. Un secondo di tolleranza
    /// copre gli arrotondamenti del sistema.
    fn is_alive(self) -> bool {
        match crate::platform::process_start_time(self.pid) {
            None => false,
            Some(started) => self
                .started
                .is_none_or(|expected| started.abs_diff(expected) <= 1),
        }
    }
}

fn spawn_watcher(state: &Arc<AppState>, pid: u32, on_end: impl FnOnce(f64) + Send + 'static) {
    tokio::spawn(watch_session(
        state.clone(),
        pid,
        WatchTiming::DEFAULT,
        on_end,
    ));
}

/// Segue la sessione di `pid` finché Dolphin resta aperto.
///
/// Mentre si gioca salva i minuti a intervalli; quando il processo sparisce
/// chiude la sessione e chiama `on_end` con la durata. Se la sessione è già
/// stata chiusa da qualcun altro — un nuovo avvio che ha chiuso Dolphin prima
/// di ripartire — esce senza contare niente: l'ha già fatto chi l'ha chiusa.
async fn watch_session(
    state: Arc<AppState>,
    pid: u32,
    timing: WatchTiming,
    on_end: impl FnOnce(f64) + Send + 'static,
) {
    let mut last_checkpoint = Instant::now();
    loop {
        tokio::time::sleep(timing.poll).await;

        let process = match state.game_session.read().await.as_ref() {
            Some(session) if session.pid == pid => SessionProcess {
                pid,
                started: session.process_started,
            },
            _ => return,
        };

        if !process.is_alive() {
            match finish_session_of(&state, pid).await {
                Ok(Some(minutes)) => on_end(minutes),
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "tempo di gioco non salvato"),
            }
            return;
        }

        if last_checkpoint.elapsed() >= timing.checkpoint {
            last_checkpoint = Instant::now();
            if let Err(error) = checkpoint_session(&state, pid).await {
                tracing::warn!(%error, "tempo di gioco parziale non salvato");
            }
        }
    }
}

/// Somma alle statistiche il tempo giocato dall'ultimo conteggio e lo salva.
async fn checkpoint_session(state: &Arc<AppState>, pid: u32) -> AppResult<()> {
    let minutes = {
        let mut slot = state.game_session.write().await;
        let Some(session) = slot.as_mut().filter(|session| session.pid == pid) else {
            return Ok(());
        };
        let now = Instant::now();
        let minutes = now.duration_since(session.counted_until).as_secs_f64() / 60.0;
        session.counted_until = now;
        minutes
    };

    {
        let mut preferences = state.preferences.write().await;
        preferences.record_session(minutes);
        if let Some(open) = preferences
            .stats
            .open_session
            .as_mut()
            .filter(|open| open.pid == pid)
        {
            open.counted_until = unix_now();
        }
    }
    state.persist_preferences().await
}

/// Chiude la sessione corrente e somma i minuti giocati.
///
/// La chiama il watcher quando Dolphin si chiude, e [`close_running_dolphin`]
/// prima di chiuderlo a forza. Restituisce la durata dell'intera sessione.
pub async fn finish_session(state: &Arc<AppState>) -> AppResult<f64> {
    let session = state.game_session.write().await.take();
    Ok(close_session(state, session).await?.unwrap_or(0.0))
}

/// Come [`finish_session`], ma solo se la sessione in corso è quella di `pid`.
async fn finish_session_of(state: &Arc<AppState>, pid: u32) -> AppResult<Option<f64>> {
    let session = {
        let mut slot = state.game_session.write().await;
        if slot.as_ref().is_some_and(|session| session.pid == pid) {
            slot.take()
        } else {
            None
        }
    };
    close_session(state, session).await
}

async fn close_session(
    state: &Arc<AppState>,
    session: Option<GameSession>,
) -> AppResult<Option<f64>> {
    let Some(session) = session else {
        return Ok(None);
    };

    // Il grosso è già nel totale, sommato a intervalli: resta la coda
    // dall'ultimo conteggio.
    let tail = session.counted_until.elapsed().as_secs_f64() / 60.0;
    {
        let mut preferences = state.preferences.write().await;
        preferences.record_session(tail);
        if preferences
            .stats
            .open_session
            .as_ref()
            .is_some_and(|open| open.pid == session.pid)
        {
            preferences.stats.open_session = None;
        }
    }
    state.persist_preferences().await?;

    let minutes = session.started_at.elapsed().as_secs_f64() / 60.0;
    tracing::info!(
        minutes = format!("{minutes:.1}"),
        "sessione di gioco conclusa"
    );
    Ok(Some(minutes))
}

/// Riprende la sessione rimasta aperta quando il launcher si è chiuso prima
/// di Dolphin (§D-106).
///
/// Se è ancora aperto *lo stesso* Dolphin — stesso PID, stessa ora di avvio —
/// è rimasto acceso per tutto il tempo: si somma la pausa dall'ultimo
/// salvataggio e si riprende a seguirlo. Se non c'è più si è chiuso a launcher
/// spento, e quando non si può sapere: resta contato fino all'ultimo
/// salvataggio, che è al più un minuto prima della chiusura del launcher.
pub async fn resume_session(
    state: &Arc<AppState>,
    on_end: impl FnOnce(f64) + Send + 'static,
) -> AppResult<()> {
    let Some(open) = state.preferences.read().await.stats.open_session.clone() else {
        return Ok(());
    };

    // Senza l'ora di avvio un PID vivo non prova niente: dopo un riavvio
    // potrebbe essere un altro programma, e la pausa conterebbe giorni.
    let process = SessionProcess {
        pid: open.pid,
        started: Some(open.process_started),
    };
    if open.process_started == 0 || !process.is_alive() {
        state.preferences.write().await.stats.open_session = None;
        state.persist_preferences().await?;
        tracing::info!(
            pid = open.pid,
            "sessione chiusa mentre il launcher era spento"
        );
        return Ok(());
    }

    let now = unix_now();
    let gap = now.saturating_sub(open.counted_until) as f64 / 60.0;
    {
        let mut preferences = state.preferences.write().await;
        preferences.record_session(gap);
        if let Some(open) = preferences.stats.open_session.as_mut() {
            open.counted_until = now;
        }
    }
    *state.game_session.write().await = Some(GameSession::starting_now(open.pid, process.started));
    spawn_watcher(state, open.pid, on_end);

    tracing::info!(
        pid = open.pid,
        gap = format!("{gap:.1}"),
        "sessione di gioco ripresa"
    );
    state.persist_preferences().await
}

/// `true` se il gioco risulta ancora in esecuzione.
pub async fn is_running(state: &Arc<AppState>) -> bool {
    if state.game_session.read().await.is_none() {
        return false;
    }
    let dolphin = state.settings.read().await.dolphin();
    crate::platform::is_executable_running(&dolphin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use crate::testkit;
    use vk_core::Channel;

    async fn state_with(dir: &std::path::Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    /// Prepara un'installazione completa e valida.
    async fn seed_ready_to_play(dir: &std::path::Path, state: &Arc<AppState>) {
        let user = dir.join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();

        let dolphin = dir.join("QuestoNonEUnProcessoReale.exe");
        std::fs::write(&dolphin, b"").unwrap();

        // Su Unix un file appena scritto non è eseguibile, e il preflight lo
        // blocca (§D-068): un Dolphin vero il permesso ce l'ha, e la finzione
        // deve somigliargli almeno in questo.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&dolphin).unwrap().permissions();
            permissions.set_mode(permissions.mode() | 0o755);
            std::fs::set_permissions(&dolphin, permissions).unwrap();
        }

        let rom = dir.join("rom.wbfs");
        std::fs::write(&rom, b"").unwrap();

        {
            let mut settings = state.settings.write().await;
            settings.dolphin_path = dolphin.to_string_lossy().to_string();
            settings.rom_path = rom.to_string_lossy().to_string();
            settings.user_folder_path = user.to_string_lossy().to_string();
        }

        testkit::install_modpack(&state.layout(Channel::Stable).await);
    }

    #[tokio::test]
    async fn preflight_blocks_a_missing_installation() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "mod-not-installed");
        assert_eq!(blocker.navigate_to, "mods");
    }

    #[tokio::test]
    async fn preflight_blocks_missing_paths_and_points_to_settings() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        // Modpack installata ma percorsi non configurati.
        testkit::install_modpack(&state.layout(Channel::Stable).await);

        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "dolphin-path");
        assert_eq!(blocker.navigate_to, "settings");
    }

    #[tokio::test]
    async fn preflight_passes_when_everything_is_ready() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        assert!(preflight(&state).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn preflight_blocks_a_gutted_descriptor_instead_of_booting_the_base_game() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        testkit::break_modpack(&state.layout(Channel::Stable).await);

        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "mod-incomplete");
        assert_eq!(blocker.navigate_to, "mods");
        assert!(blocker.message.contains("Ripara"), "{}", blocker.message);
    }

    #[tokio::test]
    async fn a_gutted_descriptor_stops_the_launch_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;
        testkit::break_modpack(&state.layout(Channel::Stable).await);

        let error = launch(&state, |_| {}).await.unwrap_err();
        assert_eq!(error.code(), "configuration");
        assert!(state.game_session.read().await.is_none());
        assert!(!state.paths.launcher_descriptor(Channel::Stable).is_file());
    }

    #[tokio::test]
    async fn preflight_blocks_a_descriptor_that_belongs_to_the_other_channel() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        let layout = state.layout(Channel::Stable).await;
        std::fs::write(layout.riivolution_xml(), testkit::riivolution_xml("VKBeta")).unwrap();

        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "mod-incomplete");
    }

    #[tokio::test]
    async fn preflight_rejects_a_rom_with_a_wrong_extension() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        let bad_rom = dir.path().join("appunti.txt");
        std::fs::write(&bad_rom, b"").unwrap();
        state.settings.write().await.rom_path = bad_rom.to_string_lossy().to_string();

        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "rom-path");
    }

    #[tokio::test]
    async fn launching_without_prerequisites_fails_without_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let error = launch(&state, |_| {}).await.unwrap_err();
        assert_eq!(error.code(), "configuration");
        assert!(state.game_session.read().await.is_none());
    }

    #[tokio::test]
    async fn the_descriptor_is_written_next_to_the_launcher_data() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        // L'avvio fallisce — il file ha il permesso di esecuzione ma dentro
        // non c'è un programma — e il descrittore dev'essere già stato
        // scritto e valido.
        let _ = launch(&state, |_| {}).await;

        let descriptor_path = state.paths.launcher_descriptor(Channel::Stable);
        assert!(descriptor_path.is_file());

        let raw = std::fs::read_to_string(&descriptor_path).unwrap();
        let parsed: GameModDescriptor = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.kind, "dolphin-game-mod-descriptor");
        assert_eq!(parsed.display_name, "VanzaKart Modpack");
        assert!(parsed.riivolution.patches[0]
            .options
            .iter()
            .any(|option| option.option_name == "Seperate Savegame"));
    }

    /// Il caso della richiesta: Dolphin già aperto e Gioca premuto. Con
    /// l'opzione spenta resta l'avviso di sempre; accesa, il launcher lo chiude
    /// da sé.
    #[tokio::test]
    async fn an_open_dolphin_is_closed_only_when_the_option_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_ready_to_play(dir.path(), &state).await;

        let (fake, mut child) = crate::testkit::spawn_fake_dolphin(dir.path());
        state.settings.write().await.dolphin_path = fake.to_string_lossy().to_string();
        let reaper = std::thread::spawn(move || child.wait());

        state.preferences.write().await.close_running_dolphin = false;
        let blocker = preflight(&state).await.unwrap().expect("atteso un blocco");
        assert_eq!(blocker.code, "dolphin-running");
        assert!(blocker.message.contains("Settings"), "{}", blocker.message);

        state.preferences.write().await.close_running_dolphin = true;
        assert!(preflight(&state).await.unwrap().is_none());

        // La sessione aperta viene chiusa e il tempo contato.
        *state.game_session.write().await = Some(GameSession::starting_now(1, None));

        assert!(close_running_dolphin(&state).await.unwrap());
        assert!(!crate::platform::is_executable_running(&fake));
        assert!(state.game_session.read().await.is_none());
        assert!(!reaper.join().unwrap().unwrap().success());

        // Già chiuso: non c'è più niente da fare.
        assert!(!close_running_dolphin(&state).await.unwrap());
    }

    #[test]
    fn closing_dolphin_is_the_default() {
        assert!(crate::storage::preferences::UserPreferences::default().close_running_dolphin);
    }

    #[tokio::test]
    async fn a_session_without_a_start_yields_zero_minutes() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert_eq!(finish_session(&state).await.unwrap(), 0.0);
        assert!(!is_running(&state).await);
    }

    /// Una sessione cominciata `minutes` minuti fa.
    fn session_started_ago(pid: u32, minutes: u64) -> GameSession {
        let mut session = GameSession::starting_now(pid, None);
        let past = Instant::now()
            .checked_sub(Duration::from_secs(minutes * 60))
            .expect("orologio monotono troppo giovane");
        session.started_at = past;
        session.counted_until = past;
        session
    }

    async fn saved_minutes(state: &Arc<AppState>) -> f64 {
        crate::storage::preferences::load(&state.paths)
            .await
            .unwrap()
            .stats
            .total_play_time_minutes
    }

    #[tokio::test]
    async fn finishing_a_session_accumulates_play_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        *state.game_session.write().await = Some(session_started_ago(1, 90));

        let minutes = finish_session(&state).await.unwrap();
        assert!((90.0..91.0).contains(&minutes), "{minutes}");
        assert!(state.game_session.read().await.is_none());

        // Sommati e scritti su disco, non solo in memoria.
        let saved = saved_minutes(&state).await;
        assert!((90.0..91.0).contains(&saved), "{saved}");
    }

    #[tokio::test]
    async fn a_checkpoint_counts_only_the_time_since_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        *state.game_session.write().await = Some(session_started_ago(7, 30));

        checkpoint_session(&state, 7).await.unwrap();
        let after_checkpoint = saved_minutes(&state).await;
        assert!(
            (30.0..31.0).contains(&after_checkpoint),
            "{after_checkpoint}"
        );

        // La chiusura aggiunge solo la coda: niente contato due volte.
        let session = finish_session(&state).await.unwrap();
        assert!((30.0..31.0).contains(&session), "{session}");
        let total = saved_minutes(&state).await;
        assert!(total - after_checkpoint < 0.1, "{total}");

        // Il checkpoint di una sessione che non c'è più non tocca niente.
        checkpoint_session(&state, 7).await.unwrap();
        assert_eq!(saved_minutes(&state).await, total);
    }

    /// Il caso della segnalazione: si gioca, si chiude Dolphin, e il tempo
    /// deve finire nelle statistiche senza che la UI faccia niente.
    #[tokio::test]
    async fn closing_dolphin_ends_the_session_and_saves_the_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let (_fake, mut child) = crate::testkit::spawn_fake_dolphin(dir.path());
        let pid = child.id();
        let started = crate::platform::process_start_time(pid);
        assert!(started.is_some());

        let mut session = session_started_ago(pid, 12);
        session.process_started = started;
        *state.game_session.write().await = Some(session);
        state.preferences.write().await.stats.open_session = Some(OpenSession {
            pid,
            process_started: started.unwrap(),
            counted_until: unix_now() - 12 * 60,
        });

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let timing = WatchTiming {
            poll: Duration::from_millis(50),
            checkpoint: Duration::ZERO,
        };
        let watcher = tokio::spawn(watch_session(state.clone(), pid, timing, move |minutes| {
            let _ = sender.send(minutes);
        }));

        // Mentre Dolphin è aperto il tempo viene già salvato.
        let deadline = Instant::now() + Duration::from_secs(10);
        while saved_minutes(&state).await < 12.0 {
            assert!(Instant::now() < deadline, "nessun salvataggio intermedio");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(state.game_session.read().await.is_some());

        child.kill().unwrap();
        child.wait().unwrap();

        let minutes = tokio::time::timeout(Duration::from_secs(10), receiver)
            .await
            .expect("il watcher non si è accorto della chiusura")
            .unwrap();
        watcher.await.unwrap();

        assert!((12.0..13.0).contains(&minutes), "{minutes}");
        assert!(state.game_session.read().await.is_none());

        let saved = crate::storage::preferences::load(&state.paths)
            .await
            .unwrap();
        assert!(
            (12.0..13.0).contains(&saved.stats.total_play_time_minutes),
            "{}",
            saved.stats.total_play_time_minutes
        );
        assert_eq!(saved.stats.open_session, None);
    }

    #[tokio::test]
    async fn a_watcher_leaves_alone_a_session_it_does_not_own() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        // La sessione è di un altro processo: quella del watcher è già stata
        // chiusa da un nuovo avvio.
        *state.game_session.write().await = Some(GameSession::starting_now(42, None));

        let timing = WatchTiming {
            poll: Duration::from_millis(10),
            checkpoint: Duration::ZERO,
        };
        watch_session(state.clone(), 7, timing, |_| {
            panic!("non è la sua sessione")
        })
        .await;

        assert_eq!(state.game_session.read().await.as_ref().unwrap().pid, 42);
        assert_eq!(saved_minutes(&state).await, 0.0);
    }

    /// Launcher chiuso a metà partita e riaperto con Dolphin ancora acceso.
    #[tokio::test]
    async fn a_session_left_open_is_resumed_with_the_pause_counted() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let (_fake, mut child) = crate::testkit::spawn_fake_dolphin(dir.path());
        let pid = child.id();
        let started = crate::platform::process_start_time(pid).unwrap();

        state.preferences.write().await.stats.open_session = Some(OpenSession {
            pid,
            process_started: started,
            counted_until: unix_now() - 20 * 60,
        });

        resume_session(&state, |_| {}).await.unwrap();

        let saved = saved_minutes(&state).await;
        assert!((20.0..21.0).contains(&saved), "{saved}");
        assert_eq!(state.game_session.read().await.as_ref().unwrap().pid, pid);

        child.kill().unwrap();
        child.wait().unwrap();
        finish_session(&state).await.unwrap();
    }

    #[tokio::test]
    async fn a_session_whose_dolphin_is_gone_is_just_closed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let own = std::process::id();
        let own_start = crate::platform::process_start_time(own).unwrap();

        // Il PID è vivo — è la suite stessa — ma non è quel Dolphin: con
        // un'ora di avvio diversa, o senza, non si riprende niente.
        for process_started in [own_start - 3600, 0] {
            state.preferences.write().await.stats.open_session = Some(OpenSession {
                pid: own,
                process_started,
                counted_until: unix_now() - 20 * 60,
            });
            resume_session(&state, |_| {}).await.unwrap();
            assert!(state.game_session.read().await.is_none());
            assert_eq!(saved_minutes(&state).await, 0.0);
            assert_eq!(state.preferences.read().await.stats.open_session, None);
        }
    }
}
