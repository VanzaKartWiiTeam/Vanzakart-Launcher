//! Posizione dei dati del launcher.
//!
//! Radice unica per sistema operativo (vedi `docs/migration.md` §1.2):
//! `%APPDATA%\VanzaKart\Launcher`, `~/Library/Application Support/VanzaKart/Launcher`
//! o `~/.local/share/VanzaKart/Launcher`.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Tutte le directory e i file usati dal launcher.
#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// Radice predefinita per la piattaforma corrente.
    pub fn discover() -> AppResult<Self> {
        let base = dirs::data_dir().ok_or_else(|| {
            AppError::Storage("the user data folder could not be determined".into())
        })?;
        Ok(Self::at(base.join("VanzaKart").join("Launcher")))
    }

    /// Radice esplicita, usata dai test.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn preferences_file(&self) -> PathBuf {
        self.root.join("preferences.json")
    }

    pub fn install_state_file(&self) -> PathBuf {
        self.root.join("install_state.json")
    }

    pub fn secrets_file(&self) -> PathBuf {
        self.root.join("secrets.json")
    }

    pub fn endpoints_cache_file(&self) -> PathBuf {
        self.root.join("endpoints.cache.json")
    }

    /// Modpack locale usata quando la cartella User non è configurata.
    pub fn fallback_mod_folder(&self) -> PathBuf {
        self.root.join("Modpack")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups").join("mod-updates")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// File che esiste solo mentre un avvio è in corso.
    ///
    /// Se c'è ancora quando il launcher parte, l'avvio precedente non è
    /// arrivato alla finestra: è così che ci si accorge di un crash dentro le
    /// librerie grafiche, che nessun gestore di errori può intercettare
    /// (§D-072).
    pub fn startup_marker(&self) -> PathBuf {
        self.root.join("startup.lock")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn rank_images_dir(&self) -> PathBuf {
        self.cache_dir().join("rank-images")
    }

    pub fn mii_avatars_dir(&self) -> PathBuf {
        self.cache_dir().join("mii-avatars")
    }

    pub fn mii_runtime_dir(&self) -> PathBuf {
        self.root.join("mii-runtime")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn legacy_import_dir(&self) -> PathBuf {
        self.root.join("legacy-import")
    }

    /// Descrittore Riivolution generato all'avvio, con il nome legacy.
    pub fn launcher_descriptor(&self, channel: vk_core::Channel) -> PathBuf {
        self.root.join(channel.launcher_descriptor_file())
    }

    /// File di versione in formato legacy, riscritti dopo ogni installazione
    /// per non rompere un eventuale ritorno al launcher C#.
    pub fn legacy_version_file(&self, channel: vk_core::Channel) -> PathBuf {
        self.root.join(channel.legacy_version_file())
    }

    /// Crea la struttura di directory. Idempotente.
    pub fn ensure(&self) -> AppResult<()> {
        for directory in [
            self.root.clone(),
            self.backups_dir(),
            self.logs_dir(),
            self.rank_images_dir(),
            self.mii_avatars_dir(),
            self.downloads_dir(),
        ] {
            std::fs::create_dir_all(&directory).map_err(|error| AppError::io(&directory, error))?;
        }
        Ok(())
    }

    /// Cartelle apribili dalla UI, per identificatore.
    ///
    /// Il frontend non passa mai un percorso: passa una di queste chiavi
    /// (vedi `docs/decisions.md` §D-017).
    pub fn well_known(&self, key: &str) -> Option<PathBuf> {
        Some(match key {
            "data" => self.root.clone(),
            "logs" => self.logs_dir(),
            "backups" => self.backups_dir(),
            "cache" => self.cache_dir(),
            "downloads" => self.downloads_dir(),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_path_lives_under_the_root() {
        let paths = AppPaths::at("/dati/VanzaKart");
        for path in [
            paths.settings_file(),
            paths.preferences_file(),
            paths.install_state_file(),
            paths.secrets_file(),
            paths.backups_dir(),
            paths.logs_dir(),
            paths.mii_avatars_dir(),
            paths.launcher_descriptor(vk_core::Channel::Stable),
        ] {
            assert!(path.starts_with("/dati/VanzaKart"), "{}", path.display());
        }
    }

    #[test]
    fn legacy_file_names_are_preserved() {
        let paths = AppPaths::at("/dati");
        assert!(paths
            .launcher_descriptor(vk_core::Channel::Stable)
            .ends_with("VanzaKart_launcher.json"));
        assert!(paths
            .launcher_descriptor(vk_core::Channel::Beta)
            .ends_with("VKBeta_launcher.json"));
        assert!(paths
            .legacy_version_file(vk_core::Channel::Beta)
            .ends_with("mod_beta_version.txt"));
    }

    #[test]
    fn ensure_creates_the_whole_tree() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path().join("VanzaKart"));

        paths.ensure().unwrap();
        paths.ensure().unwrap();

        assert!(paths.logs_dir().is_dir());
        assert!(paths.backups_dir().is_dir());
        assert!(paths.mii_avatars_dir().is_dir());
    }

    #[test]
    fn well_known_folders_are_an_allowlist() {
        let paths = AppPaths::at("/dati");
        assert_eq!(paths.well_known("logs"), Some(paths.logs_dir()));
        assert_eq!(paths.well_known("../etc"), None);
        assert_eq!(paths.well_known("/etc/passwd"), None);
        assert_eq!(paths.well_known(""), None);
    }
}
