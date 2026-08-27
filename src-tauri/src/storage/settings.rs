//! Impostazioni del launcher: percorsi di Dolphin, ROM e cartella User.
//!
//! Equivalente di `Launcher/Models/LauncherSettings.cs` e
//! `Launcher/Services/SettingsService.cs`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::paths::AppPaths;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct LauncherSettings {
    pub schema_version: u32,
    pub dolphin_path: String,
    pub rom_path: String,
    pub user_folder_path: String,
    pub controller_mode: String,
}

impl LauncherSettings {
    /// Normalizza i campi come faceva il setter C#: trim ovunque e rimozione
    /// del separatore finale dalla cartella User.
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        self.dolphin_path = self.dolphin_path.trim().to_string();
        self.rom_path = self.rom_path.trim().to_string();
        self.user_folder_path = self
            .user_folder_path
            .trim()
            .trim_end_matches(['/', '\\'])
            .to_string();
        self.controller_mode = self.controller_mode.trim().to_string();
        self
    }

    pub fn dolphin(&self) -> PathBuf {
        PathBuf::from(&self.dolphin_path)
    }

    pub fn rom(&self) -> PathBuf {
        PathBuf::from(&self.rom_path)
    }

    pub fn user_folder(&self) -> PathBuf {
        PathBuf::from(&self.user_folder_path)
    }

    /// Cartella che contiene le modpack.
    ///
    /// Come `LauncherSettings.GetModFolder()`: `<User>/Load/Riivolution`, oppure
    /// la Modpack locale quando la cartella User non è configurata. La
    /// posizione del fallback cambia rispetto al legacy (vedi
    /// `docs/decisions.md` §D-020).
    pub fn mod_folder(&self, paths: &AppPaths) -> PathBuf {
        if self.user_folder_path.trim().is_empty() {
            paths.fallback_mod_folder()
        } else {
            vk_dolphin::paths::riivolution_folder(&self.user_folder())
        }
    }

    /// `true` quando i tre percorsi obbligatori sono valorizzati.
    pub fn is_complete(&self) -> bool {
        !self.dolphin_path.trim().is_empty()
            && !self.rom_path.trim().is_empty()
            && !self.user_folder_path.trim().is_empty()
    }

    /// Elenca ciò che manca, per il messaggio della UI.
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.dolphin_path.trim().is_empty() {
            missing.push("dolphinPath");
        }
        if self.rom_path.trim().is_empty() {
            missing.push("romPath");
        }
        if self.user_folder_path.trim().is_empty() {
            missing.push("userFolderPath");
        }
        missing
    }
}

/// Legge le impostazioni; un file assente o corrotto produce i default.
pub async fn load(paths: &AppPaths) -> AppResult<LauncherSettings> {
    let stored: LauncherSettings = read_json(&paths.settings_file()).await.unwrap_or_default();
    Ok(stored.normalized())
}

/// Scrive le impostazioni in modo atomico.
pub async fn save(paths: &AppPaths, settings: &LauncherSettings) -> AppResult<()> {
    vk_core::fsx::write_json_atomic(&paths.settings_file(), settings).await?;
    Ok(())
}

pub(crate) async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = vk_core::fsx::read_text_opt(path).await.ok().flatten()?;
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(
                file = %path.file_name().unwrap_or_default().to_string_lossy(),
                %error,
                "file di configurazione illeggibile: si usano i default"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_trims_and_drops_trailing_separators() {
        let settings = LauncherSettings {
            dolphin_path: "  /opt/dolphin/Dolphin  ".into(),
            rom_path: " /giochi/rom.wbfs ".into(),
            user_folder_path: "  /home/a/Dolphin Emulator/  ".into(),
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.dolphin_path, "/opt/dolphin/Dolphin");
        assert_eq!(settings.rom_path, "/giochi/rom.wbfs");
        assert_eq!(settings.user_folder_path, "/home/a/Dolphin Emulator");
        assert_eq!(settings.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn the_mod_folder_follows_the_user_folder() {
        let paths = AppPaths::at("/dati");
        let settings = LauncherSettings {
            user_folder_path: "/home/a/Dolphin Emulator".into(),
            ..Default::default()
        };

        assert_eq!(
            settings.mod_folder(&paths),
            PathBuf::from("/home/a/Dolphin Emulator/Load/Riivolution")
        );
    }

    #[test]
    fn without_a_user_folder_the_local_modpack_is_used() {
        let paths = AppPaths::at("/dati");
        assert_eq!(
            LauncherSettings::default().mod_folder(&paths),
            paths.fallback_mod_folder()
        );
    }

    #[test]
    fn completeness_lists_what_is_missing() {
        let mut settings = LauncherSettings::default();
        assert!(!settings.is_complete());
        assert_eq!(
            settings.missing_fields(),
            vec!["dolphinPath", "romPath", "userFolderPath"]
        );

        settings.dolphin_path = "/opt/dolphin".into();
        assert_eq!(settings.missing_fields(), vec!["romPath", "userFolderPath"]);

        settings.rom_path = "/rom.wbfs".into();
        settings.user_folder_path = "/user".into();
        assert!(settings.is_complete());
        assert!(settings.missing_fields().is_empty());
    }

    #[tokio::test]
    async fn settings_round_trip_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        // Un file assente produce i default.
        assert_eq!(
            load(&paths).await.unwrap(),
            LauncherSettings::default().normalized()
        );

        let settings = LauncherSettings {
            dolphin_path: "/opt/dolphin/Dolphin".into(),
            rom_path: "/giochi/rom.wbfs".into(),
            user_folder_path: "/home/a/User".into(),
            controller_mode: "LauncherConfiguration".into(),
            schema_version: SCHEMA_VERSION,
        };
        save(&paths, &settings).await.unwrap();

        assert_eq!(load(&paths).await.unwrap(), settings);
    }

    #[tokio::test]
    async fn a_corrupt_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();
        std::fs::write(paths.settings_file(), "{ non json").unwrap();

        assert!(load(&paths).await.unwrap().dolphin_path.is_empty());
    }

    #[tokio::test]
    async fn unknown_fields_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();
        std::fs::write(
            paths.settings_file(),
            r#"{"dolphin_path":"/a","campo_futuro":42}"#,
        )
        .unwrap();

        assert_eq!(load(&paths).await.unwrap().dolphin_path, "/a");
    }
}
