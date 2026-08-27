//! Import non distruttivo dei dati del launcher legacy.
//!
//! Contratto (vedi `docs/migration.md` §2): nessun file legacy viene mai
//! spostato, modificato o cancellato. Prima di tradurre qualunque cosa se ne
//! conserva una copia integrale in `<data-root>/legacy-import/<timestamp>/`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vk_core::Channel;

use crate::error::AppResult;
use crate::storage::install_state::{self, InstallState};
use crate::storage::paths::AppPaths;
use crate::storage::preferences::{self, UserPreferences};
use crate::storage::secrets::{self, Secrets};
use crate::storage::settings::{self, LauncherSettings};

/// Verbale dell'import, scritto accanto alla copia dei file originali.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub performed: bool,
    pub imported_at: String,
    pub source_paths: Vec<String>,
    pub files: Vec<ImportedFile>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub name: String,
    pub status: String,
    pub sha256: String,
}

/// Esegue l'import se non è già stato fatto.
///
/// È idempotente: la presenza di `settings.json` nella nuova radice segnala
/// che l'import è già avvenuto.
///
/// `sources` arriva dal chiamante — in produzione da [`legacy_sources`] — così
/// che i test non possano mai toccare l'installazione reale della macchina su
/// cui girano.
pub async fn run_legacy_import(paths: &AppPaths, sources: &[PathBuf]) -> AppResult<ImportReport> {
    if paths.settings_file().exists() {
        return Ok(ImportReport::default());
    }

    let Some(source) = sources.iter().find(|dir| has_legacy_data(dir)) else {
        return Ok(ImportReport {
            notes: vec!["nessuna installazione legacy trovata".into()],
            ..Default::default()
        });
    };

    let stamp = vk_core::fsx::backup_timestamp();
    let archive = paths.legacy_import_dir().join(&stamp);
    tokio::fs::create_dir_all(&archive)
        .await
        .map_err(|error| crate::error::AppError::io(&archive, error))?;

    let mut report = ImportReport {
        performed: true,
        imported_at: stamp.clone(),
        source_paths: vec![source.to_string_lossy().to_string()],
        ..Default::default()
    };

    // 1. Copia integrale, prima di qualunque traduzione.
    for name in LEGACY_FILES {
        let from = source.join(name);
        if !from.is_file() {
            continue;
        }
        match vk_core::fsx::copy_file(&from, &archive.join(name)).await {
            Ok(_) => report.files.push(ImportedFile {
                name: (*name).to_string(),
                status: "copied".into(),
                sha256: vk_core::hash::sha256_file(&from).await.unwrap_or_default(),
            }),
            Err(error) => report.files.push(ImportedFile {
                name: (*name).to_string(),
                status: format!("skipped: {error}"),
                sha256: String::new(),
            }),
        }
    }

    // 2. Traduzione nei nuovi formati.
    let settings = import_settings(source).await;
    settings::save(paths, &settings).await?;

    let (mut prefs, token) = import_preferences(source).await;
    prefs.schema_version = preferences::SCHEMA_VERSION;
    preferences::save(paths, &prefs).await?;

    if !token.trim().is_empty() {
        secrets::save(
            paths,
            &Secrets {
                beta_access_token: token,
            },
        )
        .await?;
        report
            .notes
            .push("token beta spostato in secrets.json".into());
    }

    let state = import_install_state(source).await;
    install_state::save(paths, &state).await?;

    // 3. Verbale.
    vk_core::fsx::write_json_atomic(&archive.join("IMPORT.json"), &report).await?;

    tracing::info!(
        source = %vk_core::redact::redact(&source.to_string_lossy()),
        files = report.files.len(),
        "import dal launcher legacy completato"
    );

    Ok(report)
}

/// File del launcher legacy da conservare.
const LEGACY_FILES: &[&str] = &[
    "launcher_settings.json",
    "user_preferences.json",
    "mod_install_state.json",
    "mod_version.txt",
    "mod_beta_version.txt",
    "musicpack_version.txt",
    "musicpack_beta_version.txt",
    "VanzaKart_launcher.json",
    "VKBeta_launcher.json",
    "active_mii.txt",
];

/// Directory in cui cercare i dati legacy, in ordine di priorità.
pub fn legacy_sources() -> Vec<PathBuf> {
    let mut sources = Vec::new();

    // Il launcher C# è Windows-only.
    if !cfg!(windows) {
        return sources;
    }

    if let Some(local) = dirs::data_local_dir() {
        sources.push(local.join("VanzaKart").join("Launcher"));
        sources.push(local.join("Programs").join("VanzaKartLauncher"));
    }
    if let Some(registry_path) = crate::platform::legacy_install_dir() {
        sources.push(registry_path);
    }

    sources
}

fn has_legacy_data(directory: &Path) -> bool {
    LEGACY_FILES
        .iter()
        .any(|name| directory.join(name).is_file())
}

async fn import_settings(source: &Path) -> LauncherSettings {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Legacy {
        #[serde(rename = "DolphinPath")]
        dolphin_path: String,
        #[serde(rename = "RomPath")]
        rom_path: String,
        #[serde(rename = "UserFolderPath")]
        user_folder_path: String,
        #[serde(rename = "ControllerConfigurationMode")]
        controller_configuration_mode: String,
    }

    let legacy: Legacy = read_legacy(source, "launcher_settings.json").await;

    LauncherSettings {
        schema_version: settings::SCHEMA_VERSION,
        dolphin_path: legacy.dolphin_path,
        rom_path: legacy.rom_path,
        user_folder_path: legacy.user_folder_path,
        controller_mode: legacy.controller_configuration_mode,
    }
    .normalized()
}

async fn import_preferences(source: &Path) -> (UserPreferences, String) {
    #[derive(Deserialize)]
    #[serde(default)]
    struct Legacy {
        #[serde(rename = "DiscordRpcEnabled")]
        discord_rpc_enabled: bool,
        #[serde(rename = "AutoCheckUpdates")]
        auto_check_updates: bool,
        #[serde(rename = "SeparateSavegame")]
        separate_savegame: bool,
        #[serde(rename = "ModOptionChoice")]
        mod_option_choice: i32,
        #[serde(rename = "WindowWidth")]
        window_width: f64,
        #[serde(rename = "WindowHeight")]
        window_height: f64,
        #[serde(rename = "WindowMaximized")]
        window_maximized: bool,
        #[serde(rename = "LastPlayedUtc")]
        last_played_utc: Option<String>,
        #[serde(rename = "LaunchCount")]
        launch_count: u64,
        #[serde(rename = "TotalPlayTimeMinutes")]
        total_play_time_minutes: f64,
        #[serde(rename = "LastKnownLatestModVersion")]
        last_known_stable: String,
        #[serde(rename = "LastKnownLatestBetaModVersion")]
        last_known_beta: String,
        #[serde(rename = "BetaAccessToken")]
        beta_access_token: String,
        #[serde(rename = "ModReleaseChannel")]
        mod_release_channel: String,
    }

    impl Default for Legacy {
        fn default() -> Self {
            let defaults = UserPreferences::default();
            Self {
                discord_rpc_enabled: defaults.discord_rpc_enabled,
                auto_check_updates: defaults.auto_check_updates,
                separate_savegame: defaults.separate_savegame,
                mod_option_choice: defaults.mod_option_choice,
                window_width: defaults.window.width,
                window_height: defaults.window.height,
                window_maximized: false,
                last_played_utc: None,
                launch_count: 0,
                total_play_time_minutes: 0.0,
                last_known_stable: String::new(),
                last_known_beta: String::new(),
                beta_access_token: String::new(),
                mod_release_channel: "Stable".into(),
            }
        }
    }

    let legacy: Legacy = read_legacy(source, "user_preferences.json").await;

    let preferences = UserPreferences {
        discord_rpc_enabled: legacy.discord_rpc_enabled,
        auto_check_updates: legacy.auto_check_updates,
        separate_savegame: legacy.separate_savegame,
        mod_option_choice: legacy.mod_option_choice,
        channel: legacy
            .mod_release_channel
            .parse::<Channel>()
            .unwrap_or(Channel::Stable),
        window: crate::storage::preferences::WindowPreferences {
            width: legacy.window_width,
            height: legacy.window_height,
            maximized: legacy.window_maximized,
        },
        stats: crate::storage::preferences::PlayStats {
            last_played_utc: legacy.last_played_utc,
            launch_count: legacy.launch_count,
            total_play_time_minutes: legacy.total_play_time_minutes,
        },
        last_known: crate::storage::preferences::LastKnownVersions {
            stable: legacy.last_known_stable,
            beta: legacy.last_known_beta,
        },
        ..UserPreferences::default()
    };

    (preferences, legacy.beta_access_token)
}

async fn import_install_state(source: &Path) -> InstallState {
    let mut state = match vk_core::fsx::read_text_opt(&source.join("mod_install_state.json")).await
    {
        Ok(Some(raw)) => install_state::migrate_from_json(&raw),
        _ => InstallState::default(),
    };

    for (channel, file) in [
        (Channel::Stable, "mod_version.txt"),
        (Channel::Beta, "mod_beta_version.txt"),
    ] {
        if state.get(channel).version.trim().is_empty() {
            if let Ok(Some(text)) = vk_core::fsx::read_text_opt(&source.join(file)).await {
                let version = text.trim().to_string();
                if !version.is_empty() {
                    state.get_mut(channel).version = version;
                }
            }
        }
    }

    state.schema_version = install_state::SCHEMA_VERSION;
    state
}

async fn read_legacy<T: serde::de::DeserializeOwned + Default>(source: &Path, name: &str) -> T {
    match vk_core::fsx::read_text_opt(&source.join(name)).await {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_default(),
        _ => T::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_legacy(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("launcher_settings.json"),
            r#"{"DolphinPath":"C:\\Dolphin\\Dolphin.exe","RomPath":"C:\\Giochi\\rom.wbfs",
                "UserFolderPath":"C:\\Users\\a\\Documents\\Dolphin Emulator\\",
                "ControllerConfigurationMode":"LauncherConfiguration"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("user_preferences.json"),
            r#"{"DiscordRpcEnabled":false,"AutoCheckUpdates":false,"SeparateSavegame":false,
                "ModOptionChoice":0,"WindowWidth":1600,"WindowHeight":900,"WindowMaximized":true,
                "LaunchCount":42,"TotalPlayTimeMinutes":123.5,
                "LastKnownLatestModVersion":"1.5.0","LastKnownLatestBetaModVersion":"1.6.0-beta.1",
                "BetaAccessToken":"segretissimo","ModReleaseChannel":"Beta"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("mod_install_state.json"),
            r#"{"Stable":{"Version":"1.5.0"},"Beta":{"Version":"1.6.0-beta.1"}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("mod_version.txt"), "1.5.0").unwrap();
    }

    #[tokio::test]
    async fn translates_every_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        seed_legacy(&legacy);

        let settings = import_settings(&legacy).await;
        assert_eq!(settings.dolphin_path, r"C:\Dolphin\Dolphin.exe");
        assert_eq!(
            settings.user_folder_path,
            r"C:\Users\a\Documents\Dolphin Emulator"
        );

        let (preferences, token) = import_preferences(&legacy).await;
        assert!(!preferences.discord_rpc_enabled);
        assert!(!preferences.auto_check_updates);
        assert_eq!(preferences.mod_option_choice, 0);
        assert_eq!(preferences.channel, Channel::Beta);
        assert_eq!(preferences.window.width, 1600.0);
        assert!(preferences.window.maximized);
        assert_eq!(preferences.stats.launch_count, 42);
        assert_eq!(preferences.last_known.beta, "1.6.0-beta.1");
        assert_eq!(token, "segretissimo");

        let state = import_install_state(&legacy).await;
        assert_eq!(state.stable.version, "1.5.0");
        assert_eq!(state.beta.version, "1.6.0-beta.1");
    }

    #[tokio::test]
    async fn missing_legacy_files_produce_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = import_settings(dir.path()).await;
        assert!(settings.dolphin_path.is_empty());

        let (preferences, token) = import_preferences(dir.path()).await;
        assert_eq!(preferences, UserPreferences::default());
        assert!(token.is_empty());
    }

    #[tokio::test]
    async fn the_import_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        seed_legacy(&legacy);

        let paths = AppPaths::at(dir.path().join("nuovo"));
        paths.ensure().unwrap();
        std::fs::write(paths.settings_file(), "{}").unwrap();

        let report = run_legacy_import(&paths, &[legacy]).await.unwrap();
        assert!(!report.performed);
    }

    #[tokio::test]
    async fn without_legacy_data_nothing_is_imported() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path().join("nuovo"));
        paths.ensure().unwrap();

        let report = run_legacy_import(&paths, &[]).await.unwrap();
        assert!(!report.performed);
        assert!(!report.notes.is_empty());
        assert!(!paths.settings_file().exists());
    }

    #[tokio::test]
    async fn a_full_import_copies_before_translating() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        seed_legacy(&legacy);

        let paths = AppPaths::at(dir.path().join("nuovo"));
        paths.ensure().unwrap();

        let sources = vec![legacy.clone()];
        let report = run_legacy_import(&paths, &sources).await.unwrap();

        assert!(report.performed);
        assert!(report.files.iter().all(|file| file.status == "copied"));

        // I file legacy sono intatti.
        assert!(legacy.join("launcher_settings.json").is_file());
        assert!(legacy.join("user_preferences.json").is_file());

        // La copia integrale esiste, con il verbale accanto.
        let archive = paths.legacy_import_dir().join(&report.imported_at);
        assert!(archive.join("launcher_settings.json").is_file());
        assert!(archive.join("IMPORT.json").is_file());

        // I nuovi file sono stati scritti e il token è finito nei segreti.
        assert!(paths.settings_file().is_file());
        assert!(paths.secrets_file().is_file());
        let preferences = std::fs::read_to_string(paths.preferences_file()).unwrap();
        assert!(!preferences.contains("segretissimo"));
    }

    #[test]
    fn the_legacy_file_list_covers_the_documented_set() {
        for name in [
            "launcher_settings.json",
            "user_preferences.json",
            "mod_install_state.json",
            "mod_version.txt",
            "mod_beta_version.txt",
        ] {
            assert!(LEGACY_FILES.contains(&name), "manca {name}");
        }
    }
}
