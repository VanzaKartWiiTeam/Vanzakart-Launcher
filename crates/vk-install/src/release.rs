//! Contratto `install.json`: il manifest di rilascio che l'installer legge dal
//! server per sapere cosa scaricare.
//!
//! Sostituisce la coppia `versions.json` + `endpoints.json` che leggeva il
//! setup legacy, che conosceva un solo pacchetto (`vanzakart_launcher.zip`)
//! perché esisteva un solo sistema operativo. Qui i pacchetti sono uno per
//! piattaforma, ognuno con la propria impronta SHA-256.
//!
//! ```json
//! {
//!   "version": "2.0.0",
//!   "notes": "Prima versione Tauri.",
//!   "pub_date": "2026-08-27T10:00:00Z",
//!   "platforms": {
//!     "windows-x86_64": {
//!       "url": "https://…/VanzaKart-Launcher_2.0.0_windows-x86_64.zip",
//!       "sha256": "…",
//!       "size": 24117248,
//!       "executable": "VanzaKart Launcher.exe"
//!     }
//!   }
//! }
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{InstallError, InstallResult};
use crate::target::Target;

/// Formato del pacchetto scaricato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageFormat {
    /// Archivio ZIP, estratto con le protezioni di `vk_core::zipx`.
    Zip,
    /// `tar.gz`: è il formato dei bundle `.app`, perché preserva permessi e
    /// collegamenti simbolici interni al bundle.
    TarGz,
    /// AppImage: un solo file eseguibile, da copiare e rendere eseguibile.
    AppImage,
}

impl PackageFormat {
    /// Dedotto dall'estensione dell'URL quando il manifest non lo dichiara.
    pub fn infer(url: &str) -> Option<Self> {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".zip") {
            Some(Self::Zip)
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if lower.ends_with(".appimage") {
            Some(Self::AppImage)
        } else {
            None
        }
    }

    /// Estensione del file temporaneo in cui salvare il download.
    pub const fn temp_extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
            Self::AppImage => "AppImage",
        }
    }
}

/// Un pacchetto, per una piattaforma.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleasePackage {
    pub url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    /// Impronta SHA-256 in esadecimale. Vuota solo per i rilasci di prova.
    #[serde(default)]
    pub sha256: String,
    /// Dimensione dichiarata, per la stima del tempo e dello spazio.
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub format: Option<PackageFormat>,
    /// Percorso dell'eseguibile dentro il pacchetto, relativo alla cartella
    /// d'installazione. Vuoto: lo cerca [`crate::payload`].
    #[serde(default)]
    pub executable: String,
}

impl ReleasePackage {
    /// URL principale e mirror, senza duplicati.
    pub fn urls(&self) -> Vec<String> {
        let mut all = Vec::with_capacity(1 + self.mirrors.len());
        all.push(self.url.clone());
        all.extend(self.mirrors.iter().cloned());
        vk_core::net::dedupe_urls(&all)
    }

    /// Formato dichiarato, o dedotto dall'URL.
    pub fn format(&self) -> InstallResult<PackageFormat> {
        self.format
            .or_else(|| PackageFormat::infer(&self.url))
            .ok_or_else(|| {
                InstallError::InvalidManifest(format!(
                    "package format not recognisable from {}",
                    vk_core::redact::redact_url(&self.url)
                ))
            })
    }

    fn validate(&self, key: &str) -> InstallResult<()> {
        if self.url.trim().is_empty() {
            return Err(InstallError::InvalidManifest(format!("{key}: url missing")));
        }
        for url in self.urls() {
            if !is_acceptable_source(&url) {
                return Err(InstallError::InvalidManifest(format!(
                    "{key}: URLs must be https, found {}",
                    vk_core::redact::redact_url(&url)
                )));
            }
        }
        if !self.sha256.is_empty() && !vk_core::hash::is_valid_sha256(&self.sha256) {
            return Err(InstallError::InvalidManifest(format!(
                "{key}: invalid sha256"
            )));
        }
        self.format()?;
        Ok(())
    }
}

/// Dove scaricare **l'installer stesso**, per piattaforma.
///
/// Non serve all'installer, che è già in esecuzione quando legge il manifest:
/// serve al **sito**, che così ha una fonte sola da leggere per i suoi
/// pulsanti di download, invece di tre link scritti a mano che invecchiano a
/// ogni versione.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupDownload {
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

/// Il manifest completo.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: String,
    /// I pacchetti del launcher, quelli che scarica l'installer.
    #[serde(default)]
    pub platforms: BTreeMap<String, ReleasePackage>,
    /// Gli installer, quelli che scaricano le persone dal sito.
    #[serde(default)]
    pub setup: BTreeMap<String, SetupDownload>,
}

impl ReleaseManifest {
    /// Legge il manifest, tollerando BOM e spazi iniziali come fa il launcher.
    pub fn parse(raw: &str) -> InstallResult<Self> {
        let cleaned = vk_core::json::strip_leading_noise(raw);
        let manifest: Self = serde_json::from_str(cleaned)
            .map_err(|error| InstallError::InvalidManifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Controlla versione e pacchetti prima di mostrare qualsiasi cosa.
    pub fn validate(&self) -> InstallResult<()> {
        if !is_valid_version(&self.version) {
            return Err(InstallError::InvalidManifest(format!(
                "invalid version: {}",
                self.version
            )));
        }
        if self.platforms.is_empty() {
            return Err(InstallError::InvalidManifest("no platform declared".into()));
        }
        for (key, package) in &self.platforms {
            package.validate(key)?;
        }
        for (key, download) in &self.setup {
            if !is_acceptable_source(&download.url) {
                return Err(InstallError::InvalidManifest(format!(
                    "setup/{key}: URLs must be https"
                )));
            }
        }
        Ok(())
    }

    /// Il pacchetto adatto alla piattaforma, con la chiave scelta.
    pub fn select(&self, target: Target) -> InstallResult<(String, &ReleasePackage)> {
        target
            .candidates()
            .into_iter()
            .find_map(|key| self.platforms.get(&key).map(|package| (key, package)))
            .ok_or_else(|| InstallError::UnsupportedTarget(target.key()))
    }
}

fn is_https(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
}

/// Sorgenti che il manifest può dichiarare.
///
/// Solo `https`, con un'eccezione per il loopback: è ciò che serve ai test di
/// integrazione, che scaricano da un server locale. Non è un buco — chi
/// scarica è `vk_core::net::Downloader`, che l'http su loopback lo accetta
/// solo se glielo si chiede esplicitamente (`with_loopback_http`), e
/// l'installer non glielo chiede mai (§D-004).
fn is_acceptable_source(url: &str) -> bool {
    if is_https(url) {
        return true;
    }
    let lower = url.trim().to_ascii_lowercase();
    ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Stessa regola di `VanzaKartSetup.MainWindow.IsValidLauncherVersion`.
pub fn is_valid_version(version: &str) -> bool {
    let value = version.trim();
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "version": "2.0.0",
        "notes": "Prima versione Tauri.",
        "pub_date": "2026-08-27T10:00:00Z",
        "platforms": {
            "windows-x86_64": {
                "url": "https://example.test/VanzaKart_2.0.0_windows-x86_64.zip",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "size": 24117248,
                "executable": "VanzaKart Launcher.exe"
            },
            "darwin-universal": {
                "url": "https://example.test/VanzaKart_2.0.0_universal.app.tar.gz",
                "executable": "VanzaKart Launcher.app"
            },
            "linux-x86_64": {
                "url": "https://example.test/vanzakart-launcher_2.0.0_amd64.AppImage"
            }
        }
    }"#;

    #[test]
    fn a_manifest_with_a_bom_is_read() {
        let manifest = ReleaseManifest::parse(&format!("\u{feff}{SAMPLE}")).expect("manifest");
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.platforms.len(), 3);
    }

    #[test]
    fn the_format_is_inferred_from_the_extension() {
        let manifest = ReleaseManifest::parse(SAMPLE).expect("manifest");
        assert_eq!(
            manifest.platforms["windows-x86_64"].format().expect("zip"),
            PackageFormat::Zip
        );
        assert_eq!(
            manifest.platforms["darwin-universal"]
                .format()
                .expect("tar"),
            PackageFormat::TarGz
        );
        assert_eq!(
            manifest.platforms["linux-x86_64"].format().expect("app"),
            PackageFormat::AppImage
        );
    }

    #[test]
    fn an_apple_silicon_machine_accepts_the_universal_package() {
        let manifest = ReleaseManifest::parse(SAMPLE).expect("manifest");
        let (key, _) = manifest
            .select(Target {
                os: "darwin",
                arch: "aarch64",
            })
            .expect("pacchetto");
        assert_eq!(key, "darwin-universal");
    }

    #[test]
    fn an_unlisted_platform_is_rejected_with_its_name() {
        let manifest = ReleaseManifest::parse(SAMPLE).expect("manifest");
        let error = manifest
            .select(Target {
                os: "linux",
                arch: "armv7",
            })
            .expect_err("nessun pacchetto");
        assert_eq!(error.code(), "unsupported-target");
        assert!(error.to_string().contains("linux-armv7"));
    }

    #[test]
    fn only_loopback_may_speak_plain_http() {
        assert!(is_acceptable_source("https://esempio.test/a.zip"));
        assert!(is_acceptable_source("http://127.0.0.1:8080/a.zip"));
        assert!(!is_acceptable_source("http://esempio.test/a.zip"));
        assert!(!is_acceptable_source("ftp://esempio.test/a.zip"));
    }

    #[test]
    fn plain_http_is_refused() {
        let raw = SAMPLE.replace(
            "https://example.test/VanzaKart_2",
            "http://example.test/VanzaKart_2",
        );
        let error = ReleaseManifest::parse(&raw).expect_err("http");
        assert!(error.to_string().contains("https"));
    }

    #[test]
    fn a_broken_hash_is_refused() {
        let raw = SAMPLE.replace(
            "0000000000000000000000000000000000000000000000000000000000000000",
            "non-esadecimale",
        );
        let error = ReleaseManifest::parse(&raw).expect_err("hash");
        assert!(error.to_string().contains("sha256"));
    }

    #[test]
    fn an_unknown_extension_is_refused() {
        let raw = SAMPLE.replace(".zip", ".bin");
        let error = ReleaseManifest::parse(&raw).expect_err("formato");
        assert!(error.to_string().contains("format"));
    }

    #[test]
    fn an_empty_manifest_is_refused() {
        let error = ReleaseManifest::parse(r#"{"version":"2.0.0","platforms":{}}"#)
            .expect_err("piattaforme");
        assert!(error.to_string().contains("no platform"));
    }

    #[test]
    fn a_version_with_a_path_in_it_is_refused() {
        assert!(!is_valid_version("../../etc/passwd"));
        assert!(!is_valid_version(""));
        assert!(is_valid_version("2.0.0-beta.1"));
    }

    #[test]
    fn the_installers_of_the_site_travel_with_the_manifest() {
        let raw = SAMPLE.replace(
            r#""platforms": {"#,
            r#""setup": {
                "windows-x86_64": {
                    "url": "https://example.test/VanzaKart-Setup_2.0.0_windows-x86_64.exe",
                    "size": 8283136
                }
            },
            "platforms": {"#,
        );

        let manifest = ReleaseManifest::parse(&raw).expect("manifest");
        assert_eq!(manifest.setup.len(), 1);
        assert!(manifest.setup["windows-x86_64"].url.ends_with(".exe"));

        // E restano fuori gli indirizzi non sicuri.
        let insicuro = raw.replace(
            "https://example.test/VanzaKart-Setup",
            "http://example.test/VanzaKart-Setup",
        );
        assert!(ReleaseManifest::parse(&insicuro).is_err());
    }

    #[test]
    fn a_manifest_without_the_setup_section_is_still_valid() {
        // I manifest già pubblicati non hanno quella sezione: devono
        // continuare a funzionare.
        let manifest = ReleaseManifest::parse(SAMPLE).expect("manifest");
        assert!(manifest.setup.is_empty());
    }

    #[test]
    fn mirrors_join_the_url_without_duplicates() {
        let package = ReleasePackage {
            url: "https://a.test/p.zip".into(),
            mirrors: vec!["https://a.test/p.zip".into(), "https://b.test/p.zip".into()],
            ..Default::default()
        };
        assert_eq!(
            package.urls(),
            vec![
                "https://a.test/p.zip".to_string(),
                "https://b.test/p.zip".to_string()
            ]
        );
    }
}
