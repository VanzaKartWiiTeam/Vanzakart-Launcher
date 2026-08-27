//! Rilevamento e configurazione dei controller.
//!
//! Sostituisce `Launcher/Services/ControllerDeviceService.cs`, che faceva
//! P/Invoke su XInput e DirectInput, con `gilrs`: XInput su Windows, IOKit su
//! macOS, evdev su Linux (vedi `docs/decisions.md` §D-008).
//!
//! `gilrs` non è thread-safe e vuole essere usato da un solo thread: viene
//! quindi confinato dentro `spawn_blocking`, mai tenuto nello stato condiviso.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gilrs};
use vk_dolphin::controller::{self, ControllerProfile, DeviceKind, DeviceRef};

use crate::domain::ControllerView;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Soglia oltre la quale un asse conta come "premuto" durante l'acquisizione.
const AXIS_THRESHOLD: f32 = 0.6;

/// Durata massima di un'acquisizione di binding.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(8);

/// Elenca i controller collegati, più la tastiera che è sempre disponibile.
pub async fn scan(state: &Arc<AppState>) -> AppResult<Vec<ControllerView>> {
    let configured = {
        let user_folder = state.settings.read().await.user_folder();
        if user_folder.as_os_str().is_empty() {
            String::new()
        } else {
            controller::configured_device(&user_folder)
        }
    };

    let devices = tokio::task::spawn_blocking(enumerate)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;

    let mut views: Vec<ControllerView> = devices
        .iter()
        .map(|device| to_view(device, &configured))
        .collect();

    // La tastiera è sempre configurabile, anche senza gamepad collegati.
    views.push(to_view(&DeviceRef::keyboard(), &configured));

    // Un device configurato in Dolphin ma ora scollegato resta visibile:
    // il suo binding non va perso solo perché il cavo è staccato.
    let known = views
        .iter()
        .any(|view| view.dolphin_device.eq_ignore_ascii_case(&configured));
    if !configured.trim().is_empty() && !known {
        views.push(to_view(
            &controller::disconnected_device(&configured),
            &configured,
        ));
    }

    Ok(views)
}

/// Enumerazione sincrona: gira dentro `spawn_blocking`.
fn enumerate() -> AppResult<Vec<DeviceRef>> {
    let gilrs = Gilrs::new().map_err(|error| AppError::Internal(error.to_string()))?;

    Ok(gilrs
        .gamepads()
        .map(|(id, pad)| {
            let index = usize::from(id) as i32;
            let name = pad.name().to_string();

            DeviceRef {
                dolphin_device: dolphin_device_name(index, &name),
                kind: classify(&name),
                connected: pad.is_connected(),
                xinput_slot: if cfg!(windows) { index } else { -1 },
                supports_rumble: pad.is_ff_supported(),
                display_name: name,
            }
        })
        .collect())
}

/// Identificatore nella sintassi di Dolphin.
///
/// Su Windows i gamepad standard passano da XInput; altrove Dolphin usa i
/// backend nativi (`evdev` su Linux, `Quartz`/`OSX` su macOS).
fn dolphin_device_name(index: i32, name: &str) -> String {
    if cfg!(windows) {
        format!("XInput/{index}/Gamepad")
    } else if cfg!(target_os = "macos") {
        format!("Quartz/{index}/{name}")
    } else {
        format!("evdev/{index}/{name}")
    }
}

/// Famiglia del controller dedotta dal nome riportato dal sistema.
pub fn classify(name: &str) -> DeviceKind {
    let lowered = name.to_ascii_lowercase();
    let matches = |needles: &[&str]| needles.iter().any(|needle| lowered.contains(needle));

    // L'ordine conta: un pad Sony si presenta come "Wireless Controller", ma
    // anche un pad Xbox si chiama "Xbox Wireless Controller". I marchi
    // espliciti vanno quindi valutati prima del nome generico.
    if matches(&["xbox", "xinput", "microsoft"]) {
        DeviceKind::Xbox
    } else if matches(&["nintendo", "switch", "joy-con", "pro controller"]) {
        DeviceKind::Switch
    } else if matches(&[
        "dualsense",
        "dualshock",
        "playstation",
        "sony",
        "ps4",
        "ps5",
    ]) || lowered.trim() == "wireless controller"
    {
        DeviceKind::PlayStation
    } else {
        DeviceKind::Generic
    }
}

fn to_view(device: &DeviceRef, configured: &str) -> ControllerView {
    ControllerView {
        id: device.dolphin_device.clone(),
        name: device.display_name.clone(),
        kind: format!("{:?}", device.kind).to_lowercase(),
        dolphin_device: device.dolphin_device.clone(),
        connected: device.connected,
        supports_rumble: device.supports_rumble,
        is_configured: controller::is_same_dolphin_device(configured, device),
    }
}

/// Attende un input dal controller indicato e ne restituisce il binding
/// nella sintassi di Dolphin.
///
/// Restituisce `None` allo scadere del timeout, così la UI può tornare allo
/// stato normale senza che l'utente resti bloccato.
pub async fn capture_binding(dolphin_device: String) -> AppResult<Option<String>> {
    tokio::task::spawn_blocking(move || capture_blocking(&dolphin_device))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
}

fn capture_blocking(dolphin_device: &str) -> AppResult<Option<String>> {
    let mut gilrs = Gilrs::new().map_err(|error| AppError::Internal(error.to_string()))?;
    let deadline = Instant::now() + CAPTURE_TIMEOUT;

    // Svuota la coda: eventi arrivati prima della richiesta non contano.
    while gilrs.next_event().is_some() {}

    while Instant::now() < deadline {
        while let Some(event) = gilrs.next_event() {
            let index = usize::from(event.id) as i32;
            let name = gilrs
                .connected_gamepad(event.id)
                .map(|pad| pad.name().to_string())
                .unwrap_or_default();

            if !dolphin_device.trim().is_empty()
                && !dolphin_device_name(index, &name).eq_ignore_ascii_case(dolphin_device)
            {
                continue;
            }

            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(binding) = button_binding(button) {
                        return Ok(Some(binding));
                    }
                }
                EventType::AxisChanged(axis, value, _) if is_axis_press(value) => {
                    if let Some(binding) = axis_binding(axis, value) {
                        return Ok(Some(binding));
                    }
                }
                _ => {}
            }
        }

        std::thread::sleep(Duration::from_millis(16));
    }

    Ok(None)
}

/// `true` se lo spostamento dell'asse è deliberato e non drift.
fn is_axis_press(value: f32) -> bool {
    value.abs() >= AXIS_THRESHOLD
}

/// Nome Dolphin di un pulsante.
pub fn button_binding(button: Button) -> Option<String> {
    let name = match button {
        Button::South => "Button A",
        Button::East => "Button B",
        Button::West => "Button X",
        Button::North => "Button Y",
        Button::LeftTrigger => "Shoulder L",
        Button::RightTrigger => "Shoulder R",
        Button::LeftTrigger2 => "Trigger L",
        Button::RightTrigger2 => "Trigger R",
        Button::Select => "Back",
        Button::Start => "Start",
        Button::LeftThumb => "Thumb L",
        Button::RightThumb => "Thumb R",
        Button::DPadUp => "Pad N",
        Button::DPadDown => "Pad S",
        Button::DPadLeft => "Pad W",
        Button::DPadRight => "Pad E",
        _ => return None,
    };
    Some(format!("`{name}`"))
}

/// Nome Dolphin di una direzione d'asse.
pub fn axis_binding(axis: Axis, value: f32) -> Option<String> {
    let positive = value > 0.0;
    let name = match (axis, positive) {
        (Axis::LeftStickX, true) => "Left X+",
        (Axis::LeftStickX, false) => "Left X-",
        (Axis::LeftStickY, true) => "Left Y+",
        (Axis::LeftStickY, false) => "Left Y-",
        (Axis::RightStickX, true) => "Right X+",
        (Axis::RightStickX, false) => "Right X-",
        (Axis::RightStickY, true) => "Right Y+",
        (Axis::RightStickY, false) => "Right Y-",
        _ => return None,
    };
    Some(format!("`{name}`"))
}

/// Fa vibrare il controller per un istante, per farlo identificare.
///
/// Restituisce `false` se il device non supporta la force-feedback: non è un
/// errore, è un'informazione da mostrare nella UI.
pub async fn rumble(dolphin_device: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || rumble_blocking(&dolphin_device))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
}

fn rumble_blocking(dolphin_device: &str) -> AppResult<bool> {
    let gilrs = Gilrs::new().map_err(|error| AppError::Internal(error.to_string()))?;

    let Some((id, pad)) = gilrs.gamepads().find(|(id, pad)| {
        dolphin_device_name(usize::from(*id) as i32, pad.name())
            .eq_ignore_ascii_case(dolphin_device)
    }) else {
        return Ok(false);
    };

    if !pad.is_ff_supported() {
        return Ok(false);
    }

    let effect = gilrs::ff::EffectBuilder::new()
        .add_effect(gilrs::ff::BaseEffect {
            kind: gilrs::ff::BaseEffectType::Strong { magnitude: 40_000 },
            ..Default::default()
        })
        .gamepads(&[id])
        .finish(&mut Gilrs::new().map_err(|error| AppError::Internal(error.to_string()))?);

    match effect {
        Ok(effect) => {
            let _ = effect.play();
            std::thread::sleep(Duration::from_millis(350));
            let _ = effect.stop();
            Ok(true)
        }
        Err(error) => {
            tracing::debug!(%error, "force-feedback non disponibile");
            Ok(false)
        }
    }
}

/// Profilo corrente, letto da `GCPadNew.ini`.
pub async fn load_profile(state: &Arc<AppState>) -> AppResult<ControllerProfile> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    let bindings = controller::read_effective_bindings(&user_folder);
    let configured = controller::configured_device(&user_folder);

    let device = if configured.trim().is_empty() {
        DeviceRef::keyboard()
    } else {
        scan(state)
            .await?
            .into_iter()
            .find(|view| view.dolphin_device.eq_ignore_ascii_case(&configured))
            .map(|view| DeviceRef {
                dolphin_device: view.dolphin_device,
                display_name: view.name,
                kind: classify(&view.kind),
                connected: view.connected,
                xinput_slot: -1,
                supports_rumble: view.supports_rumble,
            })
            .unwrap_or_else(|| controller::disconnected_device(&configured))
    };

    if bindings.is_empty() {
        return Ok(ControllerProfile::default_for(device));
    }
    Ok(controller::profile_from_bindings(device, bindings))
}

/// Scrive il profilo in `GCPadNew.ini` e attiva la modalità launcher.
pub async fn save_profile(state: &Arc<AppState>, profile: &ControllerProfile) -> AppResult<()> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    let conflicts = profile.conflicts();
    if !conflicts.is_empty() {
        return Err(AppError::BadRequest(format!(
            "Alcune azioni condividono lo stesso input: {}. Solo Brake e Drift possono farlo.",
            conflicts
                .iter()
                .map(|group| group.join(" + "))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    controller::save_profile_to_dolphin(&user_folder, profile)?;

    state.settings.write().await.controller_mode = "LauncherConfiguration".into();
    state.persist_settings().await?;

    tracing::info!(device = %profile.device.dolphin_device, "binding del controller salvati");
    Ok(())
}

/// Modalità corrente: launcher o Dolphin.
pub async fn mode(state: &Arc<AppState>) -> controller::ControllerMode {
    let user_folder = state.settings.read().await.user_folder();
    controller::detect_mode(&user_folder)
}

/// Attiva una modalità.
pub async fn set_mode(
    state: &Arc<AppState>,
    mode: controller::ControllerMode,
) -> AppResult<controller::ControllerMode> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    controller::activate_mode(&user_folder, mode)?;

    state.settings.write().await.controller_mode = format!("{mode:?}");
    state.persist_settings().await?;

    Ok(controller::detect_mode(&user_folder))
}

/// Profili nominati disponibili.
pub async fn list_profiles(state: &Arc<AppState>) -> Vec<String> {
    let user_folder = state.settings.read().await.user_folder();
    controller::list_profiles(&user_folder, false)
}

fn require_user_folder(user_folder: &std::path::Path) -> AppResult<()> {
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Err(AppError::Configuration(
            "Seleziona prima la cartella User di Dolphin.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    async fn state_with(dir: &std::path::Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    #[test]
    fn device_families_are_recognised_by_name() {
        assert_eq!(classify("Xbox Wireless Controller"), DeviceKind::Xbox);
        assert_eq!(classify("Wireless Controller"), DeviceKind::PlayStation);
        assert_eq!(
            classify("DualSense Wireless Controller"),
            DeviceKind::PlayStation
        );
        assert_eq!(
            classify("Nintendo Switch Pro Controller"),
            DeviceKind::Switch
        );
        assert_eq!(classify("Generic USB Joystick"), DeviceKind::Generic);
    }

    #[test]
    fn the_device_name_follows_the_platform() {
        let name = dolphin_device_name(0, "Xbox Controller");
        if cfg!(windows) {
            assert_eq!(name, "XInput/0/Gamepad");
        } else {
            assert!(name.contains("/0/"), "{name}");
        }
    }

    #[test]
    fn buttons_map_to_the_dolphin_syntax() {
        assert_eq!(button_binding(Button::South).as_deref(), Some("`Button A`"));
        assert_eq!(button_binding(Button::North).as_deref(), Some("`Button Y`"));
        assert_eq!(
            button_binding(Button::RightTrigger2).as_deref(),
            Some("`Trigger R`")
        );
        assert_eq!(button_binding(Button::DPadUp).as_deref(), Some("`Pad N`"));
        assert_eq!(button_binding(Button::Unknown), None);
    }

    #[test]
    fn axes_map_with_their_sign() {
        assert_eq!(
            axis_binding(Axis::LeftStickY, 0.9).as_deref(),
            Some("`Left Y+`")
        );
        assert_eq!(
            axis_binding(Axis::LeftStickY, -0.9).as_deref(),
            Some("`Left Y-`")
        );
        assert_eq!(
            axis_binding(Axis::LeftStickX, -0.8).as_deref(),
            Some("`Left X-`")
        );
        assert_eq!(axis_binding(Axis::Unknown, 1.0), None);
    }

    #[test]
    fn the_capture_threshold_ignores_stick_drift() {
        // Il drift di uno stick usurato non deve registrare un binding
        // involontario mentre l'utente sta ancora scegliendo l'azione.
        assert!(!is_axis_press(0.05));
        assert!(!is_axis_press(-0.3));
        assert!(!is_axis_press(0.55));

        // Un movimento deliberato sì.
        assert!(is_axis_press(0.8));
        assert!(is_axis_press(-1.0));
    }

    #[tokio::test]
    async fn scanning_always_offers_the_keyboard() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let devices = scan(&state).await.unwrap();
        assert!(devices.iter().any(|device| device.kind == "keyboard"));
    }

    #[tokio::test]
    async fn a_configured_but_disconnected_device_stays_visible() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        std::fs::write(
            user.join("Config/GCPadNew.ini"),
            "[GCPad1]\nDevice = XInput/3/Gamepad\n",
        )
        .unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let devices = scan(&state).await.unwrap();
        let ghost = devices
            .iter()
            .find(|device| device.dolphin_device == "XInput/3/Gamepad")
            .expect("il device configurato deve restare in elenco");

        assert!(!ghost.connected);
        assert!(ghost.is_configured);
    }

    #[tokio::test]
    async fn profiles_require_a_user_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert_eq!(
            load_profile(&state).await.unwrap_err().code(),
            "configuration"
        );
        assert!(list_profiles(&state).await.is_empty());
    }

    #[tokio::test]
    async fn saving_refuses_conflicting_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let mut profile = ControllerProfile::default_for(DeviceRef {
            connected: true,
            ..DeviceRef::keyboard()
        });
        profile.set_binding("drive", "`Button A`");
        profile.set_binding("item", "`Button A`");

        let error = save_profile(&state, &profile).await.unwrap_err();
        assert_eq!(error.code(), "bad-request");
        assert!(error.to_string().contains("stesso input"));
    }

    #[tokio::test]
    async fn brake_and_drift_may_share_and_the_profile_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let mut profile = ControllerProfile::default_for(DeviceRef {
            connected: true,
            ..DeviceRef::keyboard()
        });
        profile.set_binding("brake", "`Trigger R`");
        profile.set_binding("drift", "`Trigger R`");

        save_profile(&state, &profile).await.unwrap();

        let reloaded = load_profile(&state).await.unwrap();
        assert_eq!(reloaded.binding_for("brake"), Some("`Trigger R`"));
        assert_eq!(
            mode(&state).await,
            controller::ControllerMode::LauncherConfiguration
        );
    }

    #[tokio::test]
    async fn switching_mode_requires_a_user_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert_eq!(
            set_mode(&state, controller::ControllerMode::LauncherConfiguration)
                .await
                .unwrap_err()
                .code(),
            "configuration"
        );
    }
}
