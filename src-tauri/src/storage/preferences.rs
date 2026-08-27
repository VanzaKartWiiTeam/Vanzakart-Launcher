//! Preferenze utente e statistiche di gioco.
//!
//! Equivalente di `Launcher/Models/UserPreferences.cs`, **senza** il token
//! beta: quello vive in `secrets.json` (vedi `docs/migration.md` §3).

use serde::{Deserialize, Serialize};
use vk_core::Channel;

use crate::error::AppResult;
use crate::storage::paths::AppPaths;
use crate::storage::settings::read_json;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserPreferences {
    pub schema_version: u32,
    pub discord_rpc_enabled: bool,
    pub auto_check_updates: bool,
    pub separate_savegame: bool,
    /// 2 = "My Stuff" attivo, 0 = disattivo. Stessi valori del legacy.
    pub mod_option_choice: i32,
    pub channel: Channel,
    pub download_concurrency: usize,
    pub window: WindowPreferences,
    pub stats: PlayStats,
    pub last_known: LastKnownVersions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowPreferences {
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayStats {
    pub last_played_utc: Option<String>,
    pub launch_count: u64,
    pub total_play_time_minutes: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LastKnownVersions {
    pub stable: String,
    pub beta: String,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            discord_rpc_enabled: true,
            auto_check_updates: true,
            separate_savegame: true,
            mod_option_choice: 2,
            channel: Channel::Stable,
            download_concurrency: vk_core::update::DEFAULT_DOWNLOAD_CONCURRENCY,
            window: WindowPreferences::default(),
            stats: PlayStats::default(),
            last_known: LastKnownVersions::default(),
        }
    }
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            width: 1320.0,
            height: 840.0,
            maximized: false,
        }
    }
}

impl UserPreferences {
    /// Opzioni Riivolution derivate dalle preferenze.
    pub fn launch_options(&self) -> vk_dolphin::LaunchOptions {
        vk_dolphin::LaunchOptions {
            my_stuff_enabled: self.mod_option_choice == 2,
            separate_savegame: self.separate_savegame,
        }
    }

    /// Concorrenza effettiva, sempre dentro l'intervallo supportato.
    pub fn effective_concurrency(&self) -> usize {
        self.download_concurrency.clamp(1, 12)
    }

    pub fn last_known_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.last_known.stable,
            Channel::Beta => &self.last_known.beta,
        }
    }

    pub fn set_last_known(&mut self, channel: Channel, version: &str) {
        match channel {
            Channel::Stable => self.last_known.stable = version.to_string(),
            Channel::Beta => self.last_known.beta = version.to_string(),
        }
    }

    /// Registra un avvio del gioco.
    pub fn record_launch(&mut self, now_iso: String) {
        self.stats.launch_count += 1;
        self.stats.last_played_utc = Some(now_iso);
    }

    /// Somma i minuti di una sessione conclusa.
    pub fn record_session(&mut self, minutes: f64) {
        if minutes.is_finite() && minutes > 0.0 {
            self.stats.total_play_time_minutes += minutes;
        }
    }
}

pub async fn load(paths: &AppPaths) -> AppResult<UserPreferences> {
    Ok(read_json(&paths.preferences_file())
        .await
        .unwrap_or_default())
}

pub async fn save(paths: &AppPaths, preferences: &UserPreferences) -> AppResult<()> {
    vk_core::fsx::write_json_atomic(&paths.preferences_file(), preferences).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_legacy_preferences() {
        let preferences = UserPreferences::default();
        assert!(preferences.auto_check_updates);
        assert!(preferences.separate_savegame);
        assert_eq!(preferences.mod_option_choice, 2);
        assert_eq!(preferences.channel, Channel::Stable);
        assert_eq!(preferences.window.width, 1320.0);
    }

    #[test]
    fn launch_options_follow_the_preferences() {
        let mut preferences = UserPreferences::default();
        assert_eq!(
            preferences.launch_options(),
            vk_dolphin::LaunchOptions {
                my_stuff_enabled: true,
                separate_savegame: true
            }
        );

        preferences.mod_option_choice = 0;
        preferences.separate_savegame = false;
        assert_eq!(
            preferences.launch_options(),
            vk_dolphin::LaunchOptions {
                my_stuff_enabled: false,
                separate_savegame: false
            }
        );
    }

    #[test]
    fn concurrency_is_clamped() {
        let mut preferences = UserPreferences {
            download_concurrency: 0,
            ..Default::default()
        };
        assert_eq!(preferences.effective_concurrency(), 1);

        preferences.download_concurrency = 99;
        assert_eq!(preferences.effective_concurrency(), 12);
    }

    #[test]
    fn last_known_versions_are_per_channel() {
        let mut preferences = UserPreferences::default();
        preferences.set_last_known(Channel::Beta, "1.4.0-beta.1");
        assert_eq!(preferences.last_known_for(Channel::Beta), "1.4.0-beta.1");
        assert!(preferences.last_known_for(Channel::Stable).is_empty());
    }

    #[test]
    fn stats_accumulate() {
        let mut preferences = UserPreferences::default();
        preferences.record_launch("2026-08-25T10:00:00Z".into());
        preferences.record_launch("2026-08-25T12:00:00Z".into());
        preferences.record_session(30.5);
        preferences.record_session(-5.0);
        preferences.record_session(f64::NAN);

        assert_eq!(preferences.stats.launch_count, 2);
        assert_eq!(preferences.stats.total_play_time_minutes, 30.5);
        assert_eq!(
            preferences.stats.last_played_utc.as_deref(),
            Some("2026-08-25T12:00:00Z")
        );
    }

    #[tokio::test]
    async fn preferences_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        let preferences = UserPreferences {
            channel: Channel::Beta,
            stats: PlayStats {
                launch_count: 7,
                ..Default::default()
            },
            ..Default::default()
        };
        save(&paths, &preferences).await.unwrap();

        assert_eq!(load(&paths).await.unwrap(), preferences);
    }

    #[tokio::test]
    async fn the_serialized_form_never_contains_a_token() {
        let preferences = UserPreferences::default();
        let json = serde_json::to_string(&preferences).unwrap();
        assert!(!json.to_lowercase().contains("token"), "{json}");
    }
}
