//! Stato di installazione della modpack, per canale.
//!
//! Equivalente di `Launcher/Services/ModInstallationStateService.cs`, inclusa
//! la migrazione dal formato a canale singolo del launcher più vecchio.

use serde::{Deserialize, Serialize};
use vk_core::Channel;

use crate::error::AppResult;
use crate::storage::paths::AppPaths;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelState {
    pub version: String,
    pub installed_at_utc: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallState {
    pub schema_version: u32,
    pub stable: ChannelState,
    pub beta: ChannelState,
}

impl InstallState {
    pub fn get(&self, channel: Channel) -> &ChannelState {
        match channel {
            Channel::Stable => &self.stable,
            Channel::Beta => &self.beta,
        }
    }

    pub fn get_mut(&mut self, channel: Channel) -> &mut ChannelState {
        match channel {
            Channel::Stable => &mut self.stable,
            Channel::Beta => &mut self.beta,
        }
    }

    pub fn set(&mut self, channel: Channel, version: &str, installed_at_utc: String) {
        let state = self.get_mut(channel);
        state.version = version.trim().to_string();
        state.installed_at_utc = Some(installed_at_utc);
    }
}

/// Legge lo stato, applicando le migrazioni dai formati precedenti.
///
/// `legacy_version` proviene dai file `mod_version.txt` e viene usato quando lo
/// stato strutturato non esiste ancora.
pub async fn load(paths: &AppPaths) -> AppResult<InstallState> {
    let raw = vk_core::fsx::read_text_opt(&paths.install_state_file()).await?;

    let mut state = match raw.as_deref() {
        Some(text) => migrate_from_json(text),
        None => InstallState::default(),
    };
    state.schema_version = SCHEMA_VERSION;

    // Fallback sui file di versione legacy, che il launcher C# continua a
    // scrivere e che noi manteniamo aggiornati.
    for channel in [Channel::Stable, Channel::Beta] {
        if state.get(channel).version.trim().is_empty() {
            if let Some(version) = vk_core::fsx::read_text_opt(&paths.legacy_version_file(channel))
                .await?
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
            {
                state.get_mut(channel).version = version;
            }
        }
    }

    Ok(state)
}

/// Interpreta sia il formato attuale sia quello a canale singolo.
///
/// Il formato legacy aveva `Version`/`Channel`/`InstalledAtUtc` in radice; il
/// launcher C# lo migrava al primo avvio, ma un utente che salta quella
/// versione arriva qui con il formato vecchio.
pub fn migrate_from_json(text: &str) -> InstallState {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return InstallState::default();
    };

    // Il formato C# usa PascalCase; quello nuovo camelCase. Si accettano
    // entrambi.
    let mut state = InstallState {
        schema_version: SCHEMA_VERSION,
        stable: channel_state(&value, &["stable", "Stable"]),
        beta: channel_state(&value, &["beta", "Beta"]),
    };

    if let Some(version) = string_at(&value, &["Version", "version"]) {
        if !version.trim().is_empty() {
            let channel = string_at(&value, &["Channel", "channel"])
                .and_then(|text| text.parse::<Channel>().ok())
                .unwrap_or(Channel::Stable);

            if state.get(channel).version.trim().is_empty() {
                let target = state.get_mut(channel);
                target.version = version;
                target.installed_at_utc = string_at(&value, &["InstalledAtUtc", "installedAtUtc"]);
            }
        }
    }

    state
}

fn channel_state(value: &serde_json::Value, keys: &[&str]) -> ChannelState {
    for key in keys {
        if let Some(node) = value.get(key) {
            return ChannelState {
                version: string_at(node, &["Version", "version"]).unwrap_or_default(),
                installed_at_utc: string_at(node, &["InstalledAtUtc", "installedAtUtc"]),
            };
        }
    }
    ChannelState::default()
}

fn string_at(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// Scrive lo stato e aggiorna i file di versione in formato legacy.
pub async fn save(paths: &AppPaths, state: &InstallState) -> AppResult<()> {
    vk_core::fsx::write_json_atomic(&paths.install_state_file(), state).await?;

    for channel in [Channel::Stable, Channel::Beta] {
        let version = state.get(channel).version.trim();
        if !version.is_empty() {
            vk_core::fsx::write_atomic(&paths.legacy_version_file(channel), version.as_bytes())
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_current_format() {
        let state = migrate_from_json(
            r#"{"stable":{"version":"1.5.0","installedAtUtc":"2026-01-01T00:00:00Z"},
                "beta":{"version":"1.6.0-beta.1"}}"#,
        );

        assert_eq!(state.stable.version, "1.5.0");
        assert_eq!(state.beta.version, "1.6.0-beta.1");
        assert!(state.beta.installed_at_utc.is_none());
    }

    #[test]
    fn reads_the_pascal_case_format_written_by_the_csharp_launcher() {
        let state = migrate_from_json(
            r#"{"Stable":{"Version":"1.5.0","InstalledAtUtc":"2026-01-01T00:00:00Z"},
                "Beta":{"Version":""}}"#,
        );

        assert_eq!(state.stable.version, "1.5.0");
        assert_eq!(
            state.stable.installed_at_utc.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert!(state.beta.version.is_empty());
    }

    #[test]
    fn migrates_the_single_channel_format() {
        let state = migrate_from_json(
            r#"{"Version":"1.4.0-beta.1","Channel":"Beta","InstalledAtUtc":"2025-06-01T10:00:00Z"}"#,
        );

        assert_eq!(state.beta.version, "1.4.0-beta.1");
        assert_eq!(
            state.beta.installed_at_utc.as_deref(),
            Some("2025-06-01T10:00:00Z")
        );
        assert!(state.stable.version.is_empty());
    }

    #[test]
    fn the_single_channel_format_defaults_to_stable() {
        let state = migrate_from_json(r#"{"Version":"1.2.3"}"#);
        assert_eq!(state.stable.version, "1.2.3");
    }

    #[test]
    fn an_existing_channel_value_is_not_overwritten_by_the_migration() {
        let state = migrate_from_json(
            r#"{"stable":{"version":"2.0.0"},"Version":"1.0.0","Channel":"Stable"}"#,
        );
        assert_eq!(state.stable.version, "2.0.0");
    }

    #[test]
    fn corrupt_json_yields_an_empty_state() {
        assert_eq!(migrate_from_json("{ non json"), InstallState::default());
    }

    #[tokio::test]
    async fn saving_also_writes_the_legacy_version_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        let mut state = InstallState::default();
        state.set(Channel::Stable, "1.5.0", "2026-01-01T00:00:00Z".into());
        state.set(Channel::Beta, "1.6.0-beta.1", "2026-02-01T00:00:00Z".into());
        save(&paths, &state).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(paths.legacy_version_file(Channel::Stable)).unwrap(),
            "1.5.0"
        );
        assert_eq!(
            std::fs::read_to_string(paths.legacy_version_file(Channel::Beta)).unwrap(),
            "1.6.0-beta.1"
        );

        let reloaded = load(&paths).await.unwrap();
        assert_eq!(reloaded.stable.version, "1.5.0");
        assert_eq!(reloaded.beta.version, "1.6.0-beta.1");
    }

    #[tokio::test]
    async fn the_legacy_version_file_is_used_as_a_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        std::fs::write(paths.legacy_version_file(Channel::Stable), "1.3.9\n").unwrap();

        let state = load(&paths).await.unwrap();
        assert_eq!(state.stable.version, "1.3.9");
    }
}
