//! Avvio di Dolphin con la patch Riivolution.
//!
//! Porta `MainWindow.xaml.cs::LaunchButton_OnClick`. Differenza sostanziale:
//! il processo viene avviato con argomenti separati tramite `std::process`,
//! **mai** attraverso una shell.

use std::sync::Arc;

use vk_dolphin::riivolution::{self, GameModDescriptor};

use crate::error::{AppError, AppResult};
use crate::state::{now_iso, AppState, GameSession};

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
pub async fn launch(state: &Arc<AppState>) -> AppResult<LaunchResult> {
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

    // 3. Statistiche e tracciamento della sessione.
    {
        let mut preferences = state.preferences.write().await;
        preferences.record_launch(now_iso());
    }
    state.persist_preferences().await?;

    *state.game_session.write().await = Some(GameSession {
        pid,
        started_at: std::time::Instant::now(),
    });

    tracing::info!(pid, channel = ?channel, "Dolphin avviato");

    Ok(LaunchResult {
        pid,
        descriptor_path: descriptor_path.to_string_lossy().to_string(),
        channel,
        closed_previous,
    })
}

/// Chiude la sessione corrente e somma i minuti giocati.
///
/// Viene chiamata quando il processo non risulta più in esecuzione.
pub async fn finish_session(state: &Arc<AppState>) -> AppResult<f64> {
    let Some(session) = state.game_session.write().await.take() else {
        return Ok(0.0);
    };

    let minutes = session.started_at.elapsed().as_secs_f64() / 60.0;
    {
        let mut preferences = state.preferences.write().await;
        preferences.record_session(minutes);
    }
    state.persist_preferences().await?;

    tracing::info!(
        minutes = format!("{minutes:.1}"),
        "sessione di gioco conclusa"
    );
    Ok(minutes)
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

        let error = launch(&state).await.unwrap_err();
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

        let error = launch(&state).await.unwrap_err();
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
        let _ = launch(&state).await;

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
        *state.game_session.write().await = Some(GameSession {
            pid: 1,
            started_at: std::time::Instant::now(),
        });

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

    #[tokio::test]
    async fn finishing_a_session_accumulates_play_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        *state.game_session.write().await = Some(GameSession {
            pid: 1,
            started_at: std::time::Instant::now(),
        });

        let minutes = finish_session(&state).await.unwrap();
        assert!(minutes >= 0.0);
        assert!(state.game_session.read().await.is_none());
    }
}
