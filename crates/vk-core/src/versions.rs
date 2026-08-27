//! Contratto `versions.json` — equivalente di `Launcher/Models/VersionInfo.cs`.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::json::{string_or_array, strip_leading_noise};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionInfo {
    #[serde(default, rename = "mod_version")]
    pub mod_version: String,

    #[serde(default, rename = "mod_sha256")]
    pub mod_sha256: String,

    #[serde(default, deserialize_with = "string_or_array")]
    pub changelog: Vec<String>,

    #[serde(default, rename = "beta_mod_version")]
    pub beta_mod_version: String,

    #[serde(default, rename = "beta_mod_sha256")]
    pub beta_mod_sha256: String,

    #[serde(
        default,
        rename = "beta_changelog",
        deserialize_with = "string_or_array"
    )]
    pub beta_changelog: Vec<String>,

    #[serde(default, rename = "music_pack_version")]
    pub music_pack_version: String,

    #[serde(default, rename = "music_pack_sha256")]
    pub music_pack_sha256: String,

    #[serde(
        default,
        rename = "music_pack_changelog",
        deserialize_with = "string_or_array"
    )]
    pub music_pack_changelog: Vec<String>,

    #[serde(default, rename = "launcher_version")]
    pub launcher_version: String,

    #[serde(
        default,
        rename = "launcher_changelog",
        deserialize_with = "string_or_array"
    )]
    pub launcher_changelog: Vec<String>,
}

impl VersionInfo {
    pub fn parse(raw: &str) -> CoreResult<Self> {
        Ok(serde_json::from_str(strip_leading_noise(raw))?)
    }

    /// Versione della modpack per il canale richiesto.
    pub fn mod_version_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_version,
            Channel::Beta => &self.beta_mod_version,
        }
    }

    /// Hash dell'archivio completo per il canale richiesto.
    pub fn mod_sha256_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_sha256,
            Channel::Beta => &self.beta_mod_sha256,
        }
    }

    pub fn changelog_for(&self, channel: Channel) -> &[String] {
        match channel {
            Channel::Stable => &self.changelog,
            Channel::Beta => &self.beta_changelog,
        }
    }
}

/// `true` se `candidate` è una versione più recente di `current`.
///
/// Confronta le componenti numeriche separate da `.` o `-`, ignorando una `v`
/// iniziale; a parità di numeri vince chi ne ha di più, così `1.2.1` batte
/// `1.2`. Una componente non numerica interrompe il confronto: due versioni
/// che differiscono solo per un suffisso non contano come aggiornamento.
///
/// *Divergenza dal legacy*: il launcher C# confronta le due stringhe con `!=`,
/// e quindi annuncia un "aggiornamento" anche quando il server pubblica una
/// versione **più vecchia** di quella in esecuzione. Con due launcher che
/// convivono è esattamente ciò che succede, e proporre un downgrade a tutti
/// non è un aggiornamento.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let candidate = components(candidate);
    let current = components(current);

    for index in 0..candidate.len().max(current.len()) {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }

    false
}

/// Componenti numeriche di una versione, in ordine.
fn components(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map(str::trim)
        .take_while(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Canale di rilascio della modpack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// Nome della directory della modpack: `VanzaKart` o `VKBeta`.
    ///
    /// Corrisponde a `MainWindow.xaml.cs::GetModDirectoryName`.
    pub const fn mod_directory_name(self) -> &'static str {
        match self {
            Self::Stable => "VanzaKart",
            Self::Beta => "VKBeta",
        }
    }

    /// Nome del file di versione legacy accanto all'eseguibile.
    pub const fn legacy_version_file(self) -> &'static str {
        match self {
            Self::Stable => "mod_version.txt",
            Self::Beta => "mod_beta_version.txt",
        }
    }

    /// Nome del descrittore Riivolution generato all'avvio.
    pub fn launcher_descriptor_file(self) -> String {
        format!("{}_launcher.json", self.mod_directory_name())
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Beta => "Beta",
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "beta" => Ok(Self::Beta),
            "stable" | "" => Ok(Self::Stable),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_versions_payload() {
        let raw = r#"{
            "mod_version": "beta-1.0.6",
            "mod_sha256": "de52fd92c7bb97d8f18bdb3cbaefc974e975442bd22a2c97f4d7676df77e973a",
            "changelog": ["VanzaKart Modpack beta-1.0.6"],
            "beta_mod_version": "1.4.0-beta.1",
            "beta_mod_sha256": "caefb9a08468478e35be69d56465cb4e9a921ba91ca8e99174fb4d189db1e097",
            "beta_changelog": "VanzaKart Beta 1.4.0-beta.1",
            "music_pack_version": "1.0.0",
            "launcher_version": "1.3.0-rc.5",
            "campo_sconosciuto": {"a": 1}
        }"#;

        let info = VersionInfo::parse(raw).unwrap();
        assert_eq!(info.mod_version_for(Channel::Stable), "beta-1.0.6");
        assert_eq!(info.mod_version_for(Channel::Beta), "1.4.0-beta.1");
        assert_eq!(info.beta_changelog, vec!["VanzaKart Beta 1.4.0-beta.1"]);
        assert_eq!(info.launcher_version, "1.3.0-rc.5");
        assert!(info.music_pack_changelog.is_empty());
    }

    #[test]
    fn a_newer_version_is_recognised() {
        assert!(is_newer("1.3.0", "1.2.9"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(
            is_newer("1.2.10", "1.2.9"),
            "confronto numerico, non testuale"
        );
        assert!(is_newer("v1.3", "1.2"), "la v iniziale non conta");
        assert!(
            is_newer("1.2.1", "1.2"),
            "più componenti a parità di numeri"
        );
    }

    #[test]
    fn an_older_or_equal_version_is_not_an_update() {
        assert!(!is_newer("1.2.9", "1.2.9"));
        assert!(
            !is_newer("1.5.1", "2.0.0"),
            "un downgrade non è un aggiornamento"
        );
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(!is_newer("", "1.0.0"));
        assert!(!is_newer("non-una-versione", "1.0.0"));
    }

    #[test]
    fn a_suffix_alone_is_not_an_update() {
        // `1.3.0-rc.5` e `1.3.0` hanno le stesse componenti numeriche: il
        // suffisso non basta a proporre un aggiornamento.
        assert!(!is_newer("1.3.0-rc.5", "1.3.0"));
        assert!(!is_newer("1.3.0", "1.3.0-rc.5"));
    }

    #[test]
    fn channel_names_match_the_legacy_layout() {
        assert_eq!(Channel::Stable.mod_directory_name(), "VanzaKart");
        assert_eq!(Channel::Beta.mod_directory_name(), "VKBeta");
        assert_eq!(Channel::Beta.legacy_version_file(), "mod_beta_version.txt");
        assert_eq!(
            Channel::Beta.launcher_descriptor_file(),
            "VKBeta_launcher.json"
        );
    }

    #[test]
    fn channel_parses_case_insensitively() {
        assert_eq!("Beta".parse::<Channel>().unwrap(), Channel::Beta);
        assert_eq!("  stable ".parse::<Channel>().unwrap(), Channel::Stable);
        assert_eq!("".parse::<Channel>().unwrap(), Channel::Stable);
        assert!("nightly".parse::<Channel>().is_err());
    }
}
