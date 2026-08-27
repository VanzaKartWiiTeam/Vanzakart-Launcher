//! Configurazione dei controller per Mario Kart Wii.
//!
//! Porta `Launcher/Services/MarioKartControllerConfigurationService.cs`,
//! `DolphinControllerProfileManager.cs` e `Models/MarioKartControllerModels.cs`.
//! L'enumerazione dei device fisici **non** vive qui: è un adapter di
//! piattaforma basato su `gilrs` (vedi `docs/decisions.md` §D-008). Questo
//! modulo conosce solo la sintassi dei binding di Dolphin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DolphinError, DolphinResult};
use crate::ini::{self, IniUpdates};

/// Chi possiede la configurazione dei controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerMode {
    /// Il launcher scrive `GCPadNew.ini` e attiva il GameCube su porta 1.
    #[default]
    LauncherConfiguration,
    /// Dolphin resta l'unico proprietario dei suoi file: Wiimote emulato.
    ConfigureWithDolphin,
}

/// Famiglia del controller, che determina i binding predefiniti e le etichette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    #[default]
    Xbox,
    PlayStation,
    Switch,
    Keyboard,
    Generic,
}

/// Categoria di un'azione, che ne determina il numero di chiavi Dolphin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingKind {
    Single,
    Trigger,
    Steering,
}

/// Un'azione di gioco e le chiavi di `GCPadNew.ini` che la realizzano.
///
/// Solo `Serialize`: è una tabella statica inviata al frontend, mai ricevuta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarioKartAction {
    pub id: &'static str,
    pub section: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: BindingKind,
    pub dolphin_keys: &'static [&'static str],
}

/// Le 11 azioni configurabili, nello stesso ordine del pannello legacy.
pub const ACTIONS: &[MarioKartAction] = &[
    MarioKartAction {
        id: "drive",
        section: "RACING",
        icon: "⚡",
        title: "Drive",
        description: "Accelera e parte dalla griglia",
        kind: BindingKind::Single,
        dolphin_keys: &["Buttons/A"],
    },
    MarioKartAction {
        id: "brake",
        section: "RACING",
        icon: "◼",
        title: "Brake / Reverse",
        description: "Frena e va in retromarcia · può condividere il tasto con Drift",
        kind: BindingKind::Single,
        dolphin_keys: &["Buttons/B"],
    },
    MarioKartAction {
        id: "drift",
        section: "RACING",
        icon: "↗",
        title: "Drift / Hop",
        description: "Salta e derapa · può condividere il tasto con Brake",
        kind: BindingKind::Trigger,
        dolphin_keys: &["Triggers/R", "Triggers/R-Analog"],
    },
    MarioKartAction {
        id: "item",
        section: "RACING",
        icon: "◆",
        title: "Item",
        description: "Usa o trascina un oggetto",
        kind: BindingKind::Trigger,
        dolphin_keys: &["Triggers/L", "Triggers/L-Analog"],
    },
    MarioKartAction {
        id: "look_back",
        section: "RACING",
        icon: "◉",
        title: "Look Back",
        description: "Guarda dietro il veicolo",
        kind: BindingKind::Single,
        dolphin_keys: &["Buttons/X"],
    },
    MarioKartAction {
        id: "pause",
        section: "RACING",
        icon: "Ⅱ",
        title: "Pause",
        description: "Mette in pausa la gara",
        kind: BindingKind::Single,
        dolphin_keys: &["Buttons/Start"],
    },
    MarioKartAction {
        id: "steering",
        section: "MOVEMENT",
        icon: "↔",
        title: "Steering",
        description: "Sterza e naviga i menu",
        kind: BindingKind::Steering,
        dolphin_keys: &[
            "Main Stick/Up",
            "Main Stick/Down",
            "Main Stick/Left",
            "Main Stick/Right",
        ],
    },
    MarioKartAction {
        id: "trick_up",
        section: "MOVEMENT",
        icon: "↑",
        title: "Trick Up",
        description: "Impennata o trick verso l'alto",
        kind: BindingKind::Single,
        dolphin_keys: &["D-Pad/Up"],
    },
    MarioKartAction {
        id: "trick_down",
        section: "MOVEMENT",
        icon: "↓",
        title: "Trick Down",
        description: "Trick verso il basso",
        kind: BindingKind::Single,
        dolphin_keys: &["D-Pad/Down"],
    },
    MarioKartAction {
        id: "trick_left",
        section: "MOVEMENT",
        icon: "←",
        title: "Trick Left",
        description: "Trick verso sinistra",
        kind: BindingKind::Single,
        dolphin_keys: &["D-Pad/Left"],
    },
    MarioKartAction {
        id: "trick_right",
        section: "MOVEMENT",
        icon: "→",
        title: "Trick Right",
        description: "Trick verso destra",
        kind: BindingKind::Single,
        dolphin_keys: &["D-Pad/Right"],
    },
];

/// Device fisico, nella forma minima che serve alla configurazione.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRef {
    /// Identificatore Dolphin, per esempio `XInput/0/Gamepad`.
    pub dolphin_device: String,
    pub display_name: String,
    pub kind: DeviceKind,
    pub connected: bool,
    /// Slot XInput, `-1` se non pertinente.
    pub xinput_slot: i32,
    pub supports_rumble: bool,
}

impl DeviceRef {
    pub fn keyboard() -> Self {
        Self {
            dolphin_device: "DInput/0/Keyboard Mouse".into(),
            display_name: "Tastiera e mouse".into(),
            kind: DeviceKind::Keyboard,
            connected: true,
            xinput_slot: -1,
            supports_rumble: false,
        }
    }
}

/// Profilo completo: device, binding per azione e parametri analogici.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerProfile {
    pub device: DeviceRef,
    /// Chiave Dolphin → valore, per esempio `Buttons/A` → `` `Button A` ``.
    pub bindings: BTreeMap<String, String>,
    pub deadzone: f64,
    pub sensitivity: f64,
    pub vibration: bool,
    pub loaded_from_dolphin: bool,
    pub configured_dolphin_device: Option<String>,
}

impl ControllerProfile {
    /// Profilo con i binding predefiniti per il tipo di device.
    pub fn default_for(device: DeviceRef) -> Self {
        let bindings = default_bindings(device.kind);
        Self {
            device,
            bindings,
            deadzone: 10.0,
            sensitivity: 100.0,
            vibration: true,
            loaded_from_dolphin: false,
            configured_dolphin_device: None,
        }
    }

    /// Valore assegnato a un'azione (la prima delle sue chiavi non vuota).
    pub fn binding_for(&self, action_id: &str) -> Option<&str> {
        let action = ACTIONS.iter().find(|item| item.id == action_id)?;
        action
            .dolphin_keys
            .iter()
            .find_map(|key| self.bindings.get(*key))
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    /// Assegna lo stesso input a tutte le chiavi di un'azione.
    pub fn set_binding(&mut self, action_id: &str, value: &str) {
        let Some(action) = ACTIONS.iter().find(|item| item.id == action_id) else {
            return;
        };
        for key in action.dolphin_keys {
            self.bindings.insert((*key).to_string(), value.to_string());
        }
    }

    /// Azioni che condividono lo stesso input fisico.
    ///
    /// La condivisione fra `brake` e `drift` è legittima (è il layout classico
    /// di Mario Kart Wii): tutte le altre sono conflitti.
    pub fn conflicts(&self) -> Vec<Vec<String>> {
        let mut by_value: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for action in ACTIONS {
            if action.kind == BindingKind::Steering {
                continue;
            }
            if let Some(value) = self.binding_for(action.id) {
                by_value
                    .entry(value.trim().to_lowercase())
                    .or_default()
                    .push(action.id.to_string());
            }
        }

        by_value
            .into_values()
            .filter(|ids| ids.len() > 1 && !is_allowed_shared_binding(ids))
            .collect()
    }
}

/// Replica di `IsAllowedSharedBinding`: solo `brake` + `drift`.
pub fn is_allowed_shared_binding(action_ids: &[String]) -> bool {
    let mut unique: Vec<String> = action_ids.iter().map(|id| id.to_lowercase()).collect();
    unique.sort();
    unique.dedup();
    unique.len() == 2
        && unique.contains(&"brake".to_string())
        && unique.contains(&"drift".to_string())
}

/// Binding predefiniti per famiglia di device.
pub fn default_bindings(kind: DeviceKind) -> BTreeMap<String, String> {
    let pairs: &[(&str, &str)] = match kind {
        DeviceKind::Keyboard => &[
            ("Buttons/A", "`SPACE`"),
            ("Buttons/B", "`C`"),
            ("Triggers/R", "`SHIFT`"),
            ("Triggers/R-Analog", "`SHIFT`"),
            ("Triggers/L", "`E`"),
            ("Triggers/L-Analog", "`E`"),
            ("Buttons/X", "`Q`"),
            ("Buttons/Start", "`RETURN`"),
            ("Main Stick/Up", "`W`"),
            ("Main Stick/Down", "`S`"),
            ("Main Stick/Left", "`A`"),
            ("Main Stick/Right", "`D`"),
            ("D-Pad/Up", "`UP`"),
            ("D-Pad/Down", "`DOWN`"),
            ("D-Pad/Left", "`LEFT`"),
            ("D-Pad/Right", "`RIGHT`"),
        ],
        DeviceKind::PlayStation => &[
            ("Buttons/A", "`Button S`"),
            ("Buttons/B", "`Button E`"),
            ("Triggers/R", "`Trigger R`"),
            ("Triggers/R-Analog", "`Trigger R`"),
            ("Triggers/L", "`Trigger L`"),
            ("Triggers/L-Analog", "`Trigger L`"),
            ("Buttons/X", "`Button N`"),
            ("Buttons/Start", "`Start`"),
            ("Main Stick/Up", "`Left Y+`"),
            ("Main Stick/Down", "`Left Y-`"),
            ("Main Stick/Left", "`Left X-`"),
            ("Main Stick/Right", "`Left X+`"),
            ("D-Pad/Up", "`Pad N`"),
            ("D-Pad/Down", "`Pad S`"),
            ("D-Pad/Left", "`Pad W`"),
            ("D-Pad/Right", "`Pad E`"),
        ],
        _ => &[
            ("Buttons/A", "`Button A`"),
            ("Buttons/B", "`Button B`"),
            ("Triggers/R", "`Trigger R`"),
            ("Triggers/R-Analog", "`Trigger R`"),
            ("Triggers/L", "`Trigger L`"),
            ("Triggers/L-Analog", "`Trigger L`"),
            ("Buttons/X", "`Button X`"),
            ("Buttons/Start", "`Start`"),
            ("Main Stick/Up", "`Left Y+`"),
            ("Main Stick/Down", "`Left Y-`"),
            ("Main Stick/Left", "`Left X-`"),
            ("Main Stick/Right", "`Left X+`"),
            ("D-Pad/Up", "`Pad N`"),
            ("D-Pad/Down", "`Pad S`"),
            ("D-Pad/Left", "`Pad W`"),
            ("D-Pad/Right", "`Pad E`"),
        ],
    };

    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Modalità
// ---------------------------------------------------------------------------

/// Rileva la modalità corrente da `Dolphin.ini`.
///
/// Wiimote emulato attivo e GameCube spento ⇒ Dolphin gestisce i controller.
pub fn detect_mode(user_folder: &Path) -> ControllerMode {
    if user_folder.as_os_str().is_empty() {
        return ControllerMode::LauncherConfiguration;
    }

    let data = ini::read_ini(&user_folder.join("Config").join("Dolphin.ini"));
    let wiimote_emulated = ini::get(&data, "Core", "WiimoteSource0") == Some("1");
    let gamecube_enabled = ini::get(&data, "Core", "SIDevice0") == Some("6");

    if wiimote_emulated && !gamecube_enabled {
        ControllerMode::ConfigureWithDolphin
    } else {
        ControllerMode::LauncherConfiguration
    }
}

/// Chiavi `[Core]` che realizzano una modalità.
pub fn mode_source_settings(mode: ControllerMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        ControllerMode::ConfigureWithDolphin => vec![("SIDevice0", "0"), ("WiimoteSource0", "1")],
        ControllerMode::LauncherConfiguration => vec![("SIDevice0", "6"), ("WiimoteSource0", "0")],
    }
}

/// Attiva una modalità scrivendo in `Dolphin.ini` e verificando il risultato.
///
/// In `ConfigureWithDolphin` non tocca nulla: Dolphin resta proprietario dei
/// suoi file, esattamente come nel launcher legacy.
pub fn activate_mode(user_folder: &Path, mode: ControllerMode) -> DolphinResult<()> {
    if mode == ControllerMode::ConfigureWithDolphin {
        return Ok(());
    }
    if user_folder.as_os_str().is_empty() {
        return Err(DolphinError::InvalidUserFolder(
            "seleziona prima la cartella User di Dolphin".into(),
        ));
    }

    let config_dir = user_folder.join("Config");
    std::fs::create_dir_all(&config_dir).map_err(|e| DolphinError::io(&config_dir, e))?;
    let dolphin_ini = config_dir.join("Dolphin.ini");

    let mut updates = IniUpdates::new();
    for (key, value) in mode_source_settings(mode) {
        updates = updates.set("Core", key, value);
    }
    ini::update_ini(&dolphin_ini, &updates.into_data())?;

    // Verifica: una scrittura andata a vuoto lascerebbe i controller inattivi.
    let written = ini::read_ini(&dolphin_ini);
    for (key, value) in mode_source_settings(mode) {
        if ini::get(&written, "Core", key) != Some(value) {
            return Err(DolphinError::InvalidConfiguration(format!(
                "Dolphin.ini non contiene Core/{key} = {value} dopo il salvataggio"
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Binding su GCPadNew.ini
// ---------------------------------------------------------------------------

/// Sezione dei binding attivi per una porta GameCube.
pub fn gcpad_section(port: u32) -> String {
    format!("GCPad{}", port.max(1))
}

/// Percorso di `GCPadNew.ini` / `WiimoteNew.ini`.
pub fn pad_ini_path(user_folder: &Path, wiimote: bool) -> PathBuf {
    user_folder.join("Config").join(if wiimote {
        "WiimoteNew.ini"
    } else {
        "GCPadNew.ini"
    })
}

/// Legge i binding effettivi della porta 1, risolvendo il profilo referenziato.
///
/// Equivalente di `ReadEffectiveBindings`: le chiavi della sezione attiva hanno
/// la precedenza su quelle del profilo.
pub fn read_effective_bindings(user_folder: &Path) -> BTreeMap<String, String> {
    let ini_path = pad_ini_path(user_folder, false);
    let data = ini::read_ini(&ini_path);
    let mut active: BTreeMap<String, String> =
        data.get(&gcpad_section(1)).cloned().unwrap_or_default();

    let Some(profile_name) = active.get("Profile").cloned() else {
        return active;
    };
    let cleaned = profile_name.trim().trim_matches(['"', '`']).to_string();
    if cleaned.is_empty() {
        return active;
    }

    for (key, value) in read_profile(user_folder, false, &cleaned) {
        active.entry(key).or_insert(value);
    }
    active
}

/// Dispositivo attualmente configurato in `GCPadNew.ini`.
pub fn configured_device(user_folder: &Path) -> String {
    read_effective_bindings(user_folder)
        .get("Device")
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

/// Scrive il profilo in `GCPadNew.ini` e attiva la modalità launcher.
pub fn save_profile_to_dolphin(
    user_folder: &Path,
    profile: &ControllerProfile,
) -> DolphinResult<()> {
    if user_folder.as_os_str().is_empty() {
        return Err(DolphinError::InvalidUserFolder(
            "seleziona prima la cartella User di Dolphin".into(),
        ));
    }
    if !profile.device.connected {
        return Err(DolphinError::InvalidConfiguration(
            "il controller selezionato non è connesso".into(),
        ));
    }

    let merged = build_pad_section(profile, read_effective_bindings(user_folder));
    let mut updates = IniUpdates::new();
    for (key, value) in &merged {
        updates = updates.set(&gcpad_section(1), key, value.clone());
    }

    ini::update_ini(&pad_ini_path(user_folder, false), &updates.into_data())?;
    activate_mode(user_folder, ControllerMode::LauncherConfiguration)
}

/// Costruisce la sezione `GCPad1` fondendo i binding esistenti col profilo.
///
/// Nucleo puro di [`save_profile_to_dolphin`].
pub fn build_pad_section(
    profile: &ControllerProfile,
    existing: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = existing;

    let device = profile
        .configured_dolphin_device
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| profile.device.dolphin_device.clone());

    merged.insert("Device".into(), device);
    merged.insert(
        "Main Stick/Dead Zone".into(),
        format_number(profile.deadzone),
    );
    merged.insert("C-Stick/Dead Zone".into(), format_number(profile.deadzone));
    merged.insert(
        "VanzaKart/Sensitivity".into(),
        format_number(profile.sensitivity),
    );
    merged.insert(
        "Rumble/Motor".into(),
        if profile.vibration {
            match profile.device.kind {
                DeviceKind::Xbox => "`Motor L` | `Motor R`".to_string(),
                _ => "`Motor`".to_string(),
            }
        } else {
            String::new()
        },
    );

    for action in ACTIONS {
        for key in action.dolphin_keys {
            let value = profile.bindings.get(*key).cloned().unwrap_or_default();
            let value = if is_steering_key(key) {
                add_sensitivity_wrapper(&value, profile.sensitivity)
            } else {
                value
            };
            merged.insert((*key).to_string(), value);
        }
    }

    merged
}

/// Ricostruisce un profilo dai binding letti da Dolphin.
pub fn profile_from_bindings(
    device: DeviceRef,
    bindings: BTreeMap<String, String>,
) -> ControllerProfile {
    let sensitivity = bindings
        .get("VanzaKart/Sensitivity")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(100.0);
    let deadzone = bindings
        .get("Main Stick/Dead Zone")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(10.0);
    let vibration = bindings
        .get("Rumble/Motor")
        .is_none_or(|value| !value.trim().is_empty());

    let mut cleaned: BTreeMap<String, String> = BTreeMap::new();
    for action in ACTIONS {
        for key in action.dolphin_keys {
            if let Some(value) = bindings.get(*key) {
                let value = if is_steering_key(key) {
                    remove_sensitivity_wrapper(value, sensitivity)
                } else {
                    value.clone()
                };
                cleaned.insert((*key).to_string(), value);
            }
        }
    }

    let configured = bindings.get("Device").map(|value| value.trim().to_string());

    ControllerProfile {
        device,
        bindings: if cleaned.values().all(|value| value.trim().is_empty()) {
            BTreeMap::new()
        } else {
            cleaned
        },
        deadzone,
        sensitivity,
        vibration,
        loaded_from_dolphin: true,
        configured_dolphin_device: configured,
    }
}

/// `true` per le quattro direzioni dello stick principale.
pub fn is_steering_key(key: &str) -> bool {
    key.starts_with("Main Stick/")
        && ["Up", "Down", "Left", "Right"]
            .iter()
            .any(|suffix| key.ends_with(suffix))
}

/// Avvolge un binding nel moltiplicatore di sensibilità di Dolphin.
pub fn add_sensitivity_wrapper(binding: &str, sensitivity: f64) -> String {
    if binding.trim().is_empty() || (sensitivity - 100.0).abs() < 0.01 {
        return binding.to_string();
    }
    format!("({binding} * {})", format_factor(sensitivity))
}

/// Rimuove il moltiplicatore di sensibilità, se presente.
pub fn remove_sensitivity_wrapper(binding: &str, sensitivity: f64) -> String {
    let suffix = format!(" * {})", format_factor(sensitivity));
    if binding.starts_with('(') && binding.ends_with(&suffix) {
        binding[1..binding.len() - suffix.len()].to_string()
    } else {
        binding.to_string()
    }
}

fn format_factor(sensitivity: f64) -> String {
    trim_zeros(&format!("{:.3}", sensitivity / 100.0))
}

fn format_number(value: f64) -> String {
    trim_zeros(&format!("{value:.3}"))
}

fn trim_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Confronta il device configurato in Dolphin con uno rilevato.
///
/// Replica di `IsSameDolphinDevice`: confronto esatto, poi per slot XInput,
/// poi per indice e nome normalizzato.
pub fn is_same_dolphin_device(configured: &str, candidate: &DeviceRef) -> bool {
    if configured.trim().is_empty() {
        return false;
    }
    if configured.eq_ignore_ascii_case(&candidate.dolphin_device) {
        return true;
    }

    let (configured_backend, configured_index, configured_name) = parse_device(configured);
    let (_, detected_index, detected_name) = parse_device(&candidate.dolphin_device);

    if configured_backend.eq_ignore_ascii_case("XInput") && candidate.xinput_slot >= 0 {
        return configured_index == candidate.xinput_slot;
    }
    if configured_index != detected_index {
        return false;
    }

    normalize_name(&configured_name) == normalize_name(&detected_name)
}

fn parse_device(value: &str) -> (String, i32, String) {
    let mut parts = value.splitn(3, '/');
    let backend = parts.next().unwrap_or("").trim().to_string();
    let index = parts
        .next()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    let name = parts.next().unwrap_or("").trim().to_string();
    (backend, index, name)
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Device segnaposto per una configurazione che punta a un controller assente.
pub fn disconnected_device(configured: &str) -> DeviceRef {
    let (backend, index, name) = parse_device(configured);
    DeviceRef {
        dolphin_device: configured.trim().to_string(),
        display_name: if name.is_empty() {
            format!("{backend}/{index} (non connesso)")
        } else {
            format!("{name} (non connesso)")
        },
        kind: DeviceKind::Generic,
        connected: false,
        xinput_slot: if backend.eq_ignore_ascii_case("XInput") {
            index
        } else {
            -1
        },
        supports_rumble: false,
    }
}

// ---------------------------------------------------------------------------
// Profili nominati
// ---------------------------------------------------------------------------

fn profiles_dir(user_folder: &Path, wiimote: bool) -> PathBuf {
    user_folder
        .join("Config")
        .join("Profiles")
        .join(if wiimote { "Wiimote" } else { "GCPad" })
}

/// Elenca i profili disponibili.
pub fn list_profiles(user_folder: &Path, wiimote: bool) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(profiles_dir(user_folder, wiimote)) else {
        return Vec::new();
    };

    let mut out: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        })
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .collect();

    out.sort();
    out
}

/// Legge un profilo nominato.
pub fn read_profile(user_folder: &Path, wiimote: bool, name: &str) -> BTreeMap<String, String> {
    let path = profiles_dir(user_folder, wiimote).join(format!("{name}.ini"));
    let data = ini::read_ini(&path);

    if let Some(section) = data.get("Profile") {
        if !section.is_empty() {
            return section.clone();
        }
    }
    data.values()
        .find(|section| !section.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// Scrive un profilo nominato.
///
/// Il legacy scrive sia `[Profile]` sia `[GCPad1]`/`[Wiimote1]`, perché
/// versioni diverse di Dolphin leggono l'una o l'altra.
pub fn write_profile(
    user_folder: &Path,
    wiimote: bool,
    name: &str,
    bindings: &BTreeMap<String, String>,
) -> DolphinResult<()> {
    if name.trim().is_empty() {
        return Err(DolphinError::InvalidConfiguration(
            "il nome del profilo non può essere vuoto".into(),
        ));
    }
    if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(DolphinError::InvalidConfiguration(
            "il nome del profilo contiene caratteri non consentiti".into(),
        ));
    }

    let directory = profiles_dir(user_folder, wiimote);
    std::fs::create_dir_all(&directory).map_err(|e| DolphinError::io(&directory, e))?;

    let section = if wiimote { "Wiimote1" } else { "GCPad1" };
    let mut updates = IniUpdates::new();
    for (key, value) in bindings {
        updates = updates.set("Profile", key, value.clone());
        updates = updates.set(section, key, value.clone());
    }

    ini::update_ini(&directory.join(format!("{name}.ini")), &updates.into_data())
}

/// Elimina un profilo nominato. `false` se non esisteva.
pub fn delete_profile(user_folder: &Path, wiimote: bool, name: &str) -> DolphinResult<bool> {
    let path = profiles_dir(user_folder, wiimote).join(format!("{name}.ini"));
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|e| DolphinError::io(&path, e))?;
    Ok(true)
}

/// Etichetta leggibile di un input Dolphin, adattata alla famiglia di device.
///
/// Replica di `FriendlyInput`.
pub fn friendly_input(raw: &str, kind: DeviceKind) -> String {
    let value = raw.trim().trim_matches('`');
    if value.is_empty() {
        return "Unassigned".to_string();
    }

    let mapped = match kind {
        DeviceKind::PlayStation => match value {
            "Button S" | "Button 1" => Some("✕ Cross"),
            "Button E" | "Button 2" => Some("○ Circle"),
            "Button W" | "Button 0" => Some("□ Square"),
            "Button N" | "Button 3" => Some("△ Triangle"),
            "Button 4" | "Shoulder L" => Some("L1"),
            "Button 5" | "Shoulder R" => Some("R1"),
            "Button 6" | "Trigger L" => Some("L2"),
            "Button 7" | "Trigger R" => Some("R2"),
            "Button 8" | "Back" => Some("Create / Share"),
            "Button 9" | "Start" | "Menu" => Some("Options"),
            "Button 10" => Some("L3"),
            "Button 11" => Some("R3"),
            "Button 12" => Some("PS"),
            "Button 13" => Some("Touch Pad"),
            _ => None,
        },
        DeviceKind::Switch => match value {
            "Button S" => Some("B"),
            "Button E" => Some("A"),
            "Button W" => Some("Y"),
            "Button N" => Some("X"),
            "Start" | "Menu" => Some("+"),
            "Back" => Some("−"),
            "Shoulder L" => Some("L"),
            "Shoulder R" => Some("R"),
            "Trigger L" => Some("ZL"),
            "Trigger R" => Some("ZR"),
            _ => None,
        },
        _ => match value {
            "Button A" => Some("A"),
            "Button B" => Some("B"),
            "Button X" => Some("X"),
            "Button Y" => Some("Y"),
            "Menu" | "Start" => Some("Menu"),
            "Back" => Some("View"),
            "Shoulder L" => Some("LB"),
            "Shoulder R" => Some("RB"),
            "Trigger L" => Some("LT"),
            "Trigger R" => Some("RT"),
            _ => None,
        },
    };

    mapped
        .map(str::to_string)
        .unwrap_or_else(|| friendly_common(value))
}

fn friendly_common(value: &str) -> String {
    match value {
        "Pad N" => "D-Pad ↑".into(),
        "Pad S" => "D-Pad ↓".into(),
        "Pad W" => "D-Pad ←".into(),
        "Pad E" => "D-Pad →".into(),
        "Left Y+" => "Stick sinistro ↑".into(),
        "Left Y-" => "Stick sinistro ↓".into(),
        "Left X-" => "Stick sinistro ←".into(),
        "Left X+" => "Stick sinistro →".into(),
        "Right Y+" => "Stick destro ↑".into(),
        "Right Y-" => "Stick destro ↓".into(),
        "Right X-" => "Stick destro ←".into(),
        "Right X+" => "Stick destro →".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xbox() -> DeviceRef {
        DeviceRef {
            dolphin_device: "XInput/0/Gamepad".into(),
            display_name: "Xbox Controller".into(),
            kind: DeviceKind::Xbox,
            connected: true,
            xinput_slot: 0,
            supports_rumble: true,
        }
    }

    #[test]
    fn the_action_list_matches_the_legacy_panel() {
        assert_eq!(ACTIONS.len(), 11);
        assert_eq!(ACTIONS[0].id, "drive");
        assert_eq!(ACTIONS[0].dolphin_keys, &["Buttons/A"]);
        assert_eq!(
            ACTIONS
                .iter()
                .find(|a| a.id == "steering")
                .unwrap()
                .dolphin_keys
                .len(),
            4
        );
        assert_eq!(
            ACTIONS
                .iter()
                .find(|a| a.id == "drift")
                .unwrap()
                .dolphin_keys,
            &["Triggers/R", "Triggers/R-Analog"]
        );
    }

    #[test]
    fn default_profiles_cover_every_action_key() {
        for kind in [
            DeviceKind::Xbox,
            DeviceKind::PlayStation,
            DeviceKind::Keyboard,
        ] {
            let profile = ControllerProfile::default_for(DeviceRef { kind, ..xbox() });
            for action in ACTIONS {
                assert!(
                    profile.binding_for(action.id).is_some(),
                    "{kind:?} non assegna {}",
                    action.id
                );
            }
        }
    }

    #[test]
    fn setting_a_binding_touches_every_key_of_the_action() {
        let mut profile = ControllerProfile::default_for(xbox());
        profile.set_binding("drift", "`Trigger L`");
        assert_eq!(profile.bindings["Triggers/R"], "`Trigger L`");
        assert_eq!(profile.bindings["Triggers/R-Analog"], "`Trigger L`");
    }

    #[test]
    fn brake_and_drift_may_share_an_input() {
        let mut profile = ControllerProfile::default_for(xbox());
        profile.set_binding("brake", "`Trigger R`");
        profile.set_binding("drift", "`Trigger R`");
        assert!(profile.conflicts().is_empty());
    }

    #[test]
    fn other_shared_inputs_are_conflicts() {
        let mut profile = ControllerProfile::default_for(xbox());
        profile.set_binding("drive", "`Button A`");
        profile.set_binding("item", "`Button A`");

        let conflicts = profile.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains(&"drive".to_string()));
        assert!(conflicts[0].contains(&"item".to_string()));
    }

    #[test]
    fn allowed_shared_binding_requires_exactly_brake_and_drift() {
        assert!(is_allowed_shared_binding(&["brake".into(), "drift".into()]));
        assert!(is_allowed_shared_binding(&["DRIFT".into(), "Brake".into()]));
        assert!(!is_allowed_shared_binding(&["brake".into(), "item".into()]));
        assert!(!is_allowed_shared_binding(&[
            "brake".into(),
            "drift".into(),
            "item".into()
        ]));
    }

    #[test]
    fn detects_the_mode_from_dolphin_ini() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("Config");
        std::fs::create_dir_all(&config).unwrap();

        std::fs::write(
            config.join("Dolphin.ini"),
            "[Core]\nSIDevice0 = 0\nWiimoteSource0 = 1\n",
        )
        .unwrap();
        assert_eq!(
            detect_mode(dir.path()),
            ControllerMode::ConfigureWithDolphin
        );

        std::fs::write(
            config.join("Dolphin.ini"),
            "[Core]\nSIDevice0 = 6\nWiimoteSource0 = 0\n",
        )
        .unwrap();
        assert_eq!(
            detect_mode(dir.path()),
            ControllerMode::LauncherConfiguration
        );
    }

    #[test]
    fn a_missing_ini_means_launcher_configuration() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_mode(dir.path()),
            ControllerMode::LauncherConfiguration
        );
        assert_eq!(
            detect_mode(Path::new("")),
            ControllerMode::LauncherConfiguration
        );
    }

    #[test]
    fn activating_launcher_mode_writes_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        activate_mode(dir.path(), ControllerMode::LauncherConfiguration).unwrap();

        let data = ini::read_ini(&dir.path().join("Config/Dolphin.ini"));
        assert_eq!(ini::get(&data, "Core", "SIDevice0"), Some("6"));
        assert_eq!(ini::get(&data, "Core", "WiimoteSource0"), Some("0"));
        assert_eq!(
            detect_mode(dir.path()),
            ControllerMode::LauncherConfiguration
        );
    }

    #[test]
    fn dolphin_mode_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        activate_mode(dir.path(), ControllerMode::ConfigureWithDolphin).unwrap();
        assert!(!dir.path().join("Config").exists());
    }

    #[test]
    fn the_pad_section_carries_device_deadzone_and_rumble() {
        let profile = ControllerProfile::default_for(xbox());
        let section = build_pad_section(&profile, BTreeMap::new());

        assert_eq!(section["Device"], "XInput/0/Gamepad");
        assert_eq!(section["Main Stick/Dead Zone"], "10");
        assert_eq!(section["C-Stick/Dead Zone"], "10");
        assert_eq!(section["VanzaKart/Sensitivity"], "100");
        assert_eq!(section["Rumble/Motor"], "`Motor L` | `Motor R`");
        assert_eq!(section["Buttons/A"], "`Button A`");
    }

    #[test]
    fn disabling_vibration_clears_the_motor_binding() {
        let mut profile = ControllerProfile::default_for(xbox());
        profile.vibration = false;
        assert_eq!(
            build_pad_section(&profile, BTreeMap::new())["Rumble/Motor"],
            ""
        );
    }

    #[test]
    fn non_xbox_devices_use_a_single_motor() {
        let mut profile = ControllerProfile::default_for(DeviceRef {
            kind: DeviceKind::PlayStation,
            ..xbox()
        });
        profile.vibration = true;
        assert_eq!(
            build_pad_section(&profile, BTreeMap::new())["Rumble/Motor"],
            "`Motor`"
        );
    }

    #[test]
    fn unrelated_existing_keys_survive() {
        let mut existing = BTreeMap::new();
        existing.insert("Chiave/Sconosciuta".to_string(), "42".to_string());

        let section = build_pad_section(&ControllerProfile::default_for(xbox()), existing);
        assert_eq!(section["Chiave/Sconosciuta"], "42");
    }

    #[test]
    fn steering_keys_carry_the_sensitivity_multiplier() {
        let mut profile = ControllerProfile::default_for(xbox());
        profile.sensitivity = 130.0;

        let section = build_pad_section(&profile, BTreeMap::new());
        assert_eq!(section["Main Stick/Up"], "(`Left Y+` * 1.3)");
        // I pulsanti non vengono avvolti.
        assert_eq!(section["Buttons/A"], "`Button A`");
    }

    #[test]
    fn sensitivity_at_100_adds_no_wrapper() {
        assert_eq!(add_sensitivity_wrapper("`Left Y+`", 100.0), "`Left Y+`");
        assert_eq!(add_sensitivity_wrapper("", 130.0), "");
    }

    #[test]
    fn the_sensitivity_wrapper_round_trips() {
        for sensitivity in [50.0, 75.0, 130.0, 150.0] {
            let wrapped = add_sensitivity_wrapper("`Left X+`", sensitivity);
            assert_eq!(
                remove_sensitivity_wrapper(&wrapped, sensitivity),
                "`Left X+`",
                "sensibilità {sensitivity}"
            );
        }
    }

    #[test]
    fn identifies_steering_keys() {
        assert!(is_steering_key("Main Stick/Up"));
        assert!(is_steering_key("Main Stick/Right"));
        assert!(!is_steering_key("Main Stick/Dead Zone"));
        assert!(!is_steering_key("C-Stick/Up"));
    }

    #[test]
    fn a_profile_round_trips_through_the_ini() {
        let dir = tempfile::tempdir().unwrap();
        let mut profile = ControllerProfile::default_for(xbox());
        profile.sensitivity = 120.0;
        profile.deadzone = 15.0;
        profile.set_binding("drive", "`Button B`");

        save_profile_to_dolphin(dir.path(), &profile).unwrap();

        let reloaded = profile_from_bindings(xbox(), read_effective_bindings(dir.path()));
        assert_eq!(reloaded.sensitivity, 120.0);
        assert_eq!(reloaded.deadzone, 15.0);
        assert!(reloaded.vibration);
        assert_eq!(reloaded.binding_for("drive"), Some("`Button B`"));
        assert_eq!(reloaded.binding_for("steering"), Some("`Left Y+`"));
        assert_eq!(configured_device(dir.path()), "XInput/0/Gamepad");
    }

    #[test]
    fn named_profiles_can_be_written_listed_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let bindings = default_bindings(DeviceKind::Xbox);

        write_profile(dir.path(), false, "Corsa", &bindings).unwrap();
        assert_eq!(list_profiles(dir.path(), false), vec!["Corsa"]);

        let read = read_profile(dir.path(), false, "Corsa");
        assert_eq!(read["Buttons/A"], "`Button A`");

        assert!(delete_profile(dir.path(), false, "Corsa").unwrap());
        assert!(!delete_profile(dir.path(), false, "Corsa").unwrap());
        assert!(list_profiles(dir.path(), false).is_empty());
    }

    #[test]
    fn profile_names_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        let bindings = default_bindings(DeviceKind::Xbox);

        assert!(write_profile(dir.path(), false, "  ", &bindings).is_err());
        assert!(write_profile(dir.path(), false, "../fuga", &bindings).is_err());
        assert!(write_profile(dir.path(), false, "a:b", &bindings).is_err());
    }

    #[test]
    fn a_profile_reference_is_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let mut bindings = default_bindings(DeviceKind::Xbox);
        bindings.insert("Device".into(), "XInput/0/Gamepad".into());
        write_profile(dir.path(), false, "Base", &bindings).unwrap();

        std::fs::write(
            dir.path().join("Config/GCPadNew.ini"),
            "[GCPad1]\nProfile = `Base`\nButtons/A = `Button Y`\n",
        )
        .unwrap();

        let effective = read_effective_bindings(dir.path());
        // La sezione attiva vince sul profilo…
        assert_eq!(effective["Buttons/A"], "`Button Y`");
        // …ma le chiavi mancanti arrivano dal profilo.
        assert_eq!(effective["Buttons/B"], "`Button B`");
    }

    #[test]
    fn device_matching_prefers_the_xinput_slot() {
        let candidate = xbox();
        assert!(is_same_dolphin_device("XInput/0/Gamepad", &candidate));
        assert!(is_same_dolphin_device("XInput/0/Controller", &candidate));
        assert!(!is_same_dolphin_device("XInput/1/Gamepad", &candidate));
        assert!(!is_same_dolphin_device("", &candidate));
    }

    #[test]
    fn device_matching_falls_back_to_index_and_name() {
        let candidate = DeviceRef {
            dolphin_device: "DInput/0/Wireless Controller".into(),
            xinput_slot: -1,
            kind: DeviceKind::PlayStation,
            ..xbox()
        };
        assert!(is_same_dolphin_device(
            "DInput/0/Wireless-Controller",
            &candidate
        ));
        assert!(!is_same_dolphin_device(
            "DInput/1/Wireless Controller",
            &candidate
        ));
    }

    #[test]
    fn a_missing_device_becomes_a_disconnected_placeholder() {
        let device = disconnected_device("XInput/2/Gamepad");
        assert!(!device.connected);
        assert_eq!(device.xinput_slot, 2);
        assert!(device.display_name.contains("non connesso"));
    }

    #[test]
    fn friendly_labels_follow_the_device_family() {
        assert_eq!(friendly_input("`Button A`", DeviceKind::Xbox), "A");
        assert_eq!(
            friendly_input("`Button S`", DeviceKind::PlayStation),
            "✕ Cross"
        );
        assert_eq!(friendly_input("`Button S`", DeviceKind::Switch), "B");
        assert_eq!(friendly_input("`Trigger R`", DeviceKind::Xbox), "RT");
        assert_eq!(friendly_input("`Trigger R`", DeviceKind::Switch), "ZR");
        assert_eq!(friendly_input("`Pad N`", DeviceKind::Xbox), "D-Pad ↑");
        assert_eq!(friendly_input("  ", DeviceKind::Xbox), "Unassigned");
        assert_eq!(friendly_input("`SPACE`", DeviceKind::Keyboard), "SPACE");
    }

    #[test]
    fn numbers_are_formatted_without_trailing_zeros() {
        assert_eq!(format_number(10.0), "10");
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_factor(130.0), "1.3");
        assert_eq!(format_factor(100.0), "1");
    }
}
