//! Registro dell'installazione: cosa è stato messo, e dove.
//!
//! Il disinstallatore legacy indovinava: cercava `*.exe` con "Launcher" nel
//! nome, provava percorsi noti di scorciatoie, cancellava cartelle chiamate
//! `Cache` o `Logs`. Funzionava finché l'installazione era quella prevista.
//!
//! Qui l'installer scrive l'elenco esatto di ciò che ha creato — file del
//! pacchetto, scorciatoie, voci del menu applicazioni, icone, chiavi di
//! registro — e la disinstallazione è la sua lettura al contrario. Le
//! euristiche restano solo come ripiego, per le installazioni fatte prima che
//! questo registro esistesse (vedi [`crate::discovery`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{InstallError, InstallResult};
use crate::paths;

pub const RECORD_SCHEMA_VERSION: u32 = 1;

/// Una cosa creata dall'installer fuori dalla cartella d'installazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    /// Collegamento sul desktop (`.lnk` su Windows, `.desktop` su Linux).
    DesktopShortcut,
    /// Voce nel menu Start / menu applicazioni.
    StartMenuShortcut,
    /// Voce "Disinstalla" nel menu applicazioni.
    UninstallShortcut,
    /// Barra di avvio veloce, solo Windows.
    QuickLaunchShortcut,
    /// Icona installata nel tema (`hicolor`), solo Linux.
    Icon,
    /// Collegamento simbolico in una cartella del `PATH`.
    Symlink,
    /// Chiave di disinstallazione di Windows.
    RegistryKey,
    /// Registro dell'installazione.
    Record,
}

impl ArtifactKind {
    /// Etichetta mostrata nel riepilogo della disinstallazione.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DesktopShortcut => "Collegamento sul desktop",
            Self::StartMenuShortcut => "Voce nel menu applicazioni",
            Self::UninstallShortcut => "Voce di disinstallazione",
            Self::QuickLaunchShortcut => "Avvio veloce",
            Self::Icon => "Icona dell'applicazione",
            Self::Symlink => "Collegamento nel PATH",
            Self::RegistryKey => "Registrazione fra i programmi installati",
            Self::Record => "Registro dell'installazione",
        }
    }
}

/// Una voce del registro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: ArtifactKind,
    /// Percorso, o nome della chiave di registro.
    pub path: String,
}

impl Artifact {
    pub fn new(kind: ArtifactKind, path: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
        }
    }

    pub fn file(kind: ArtifactKind, path: &Path) -> Self {
        Self::new(kind, path.to_string_lossy().to_string())
    }
}

/// Il registro completo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct InstallRecord {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    /// Chiave della piattaforma, per esempio `windows-x86_64`.
    pub target: String,
    /// Data ISO-8601 in UTC.
    pub installed_at: String,
    pub install_dir: PathBuf,
    /// `true` quando la cartella appartiene solo al launcher e può essere
    /// cancellata per intero. Su macOS, dove si installa in una cartella
    /// Applicazioni condivisa, è `false` e si rimuovono i singoli bundle.
    pub owns_install_dir: bool,
    pub executable: PathBuf,
    /// Vuoto quando il disinstallatore non è stato copiato.
    pub uninstaller: PathBuf,
    /// Voci di primo livello create dentro `install_dir`.
    pub payload: Vec<PathBuf>,
    pub artifacts: Vec<Artifact>,
}

impl InstallRecord {
    pub fn new(
        version: impl Into<String>,
        target: impl Into<String>,
        install_dir: PathBuf,
    ) -> Self {
        let owns_install_dir = paths::owns_install_dir(&install_dir);
        Self {
            schema_version: RECORD_SCHEMA_VERSION,
            product: crate::PRODUCT_NAME.to_string(),
            version: version.into(),
            target: target.into(),
            installed_at: now_iso8601(),
            install_dir,
            owns_install_dir,
            ..Default::default()
        }
    }

    /// Il registro scritto dentro la cartella d'installazione.
    pub fn local_path(&self) -> PathBuf {
        self.install_dir.join(paths::RECORD_FILE_NAME)
    }

    /// Percorsi che compongono l'installazione vera e propria.
    ///
    /// Quando la cartella è nostra è la cartella stessa; quando è condivisa
    /// (macOS) sono i singoli elementi che ci abbiamo messo dentro.
    pub fn installed_paths(&self) -> Vec<PathBuf> {
        let paths = if self.owns_install_dir {
            vec![self.install_dir.clone()]
        } else {
            self.payload
                .iter()
                .map(|entry| self.install_dir.join(entry))
                .collect()
        };

        // Un percorso vuoto o relativo non è un'installazione: è un registro
        // rotto. Rimuoverlo non vuol dire niente, e un percorso vuoto è
        // prefisso di qualunque altro — cioè sembra contenere tutto.
        // Si guarda la radice e non `is_absolute`, che su Windows pretende
        // anche la lettera di unità: un registro scritto su un altro sistema
        // resta leggibile.
        paths.into_iter().filter(|path| path.has_root()).collect()
    }

    pub fn add_artifact(&mut self, artifact: Artifact) {
        if !self.artifacts.contains(&artifact) {
            self.artifacts.push(artifact);
        }
    }

    pub fn artifacts_of(&self, kind: ArtifactKind) -> impl Iterator<Item = &Artifact> {
        self.artifacts
            .iter()
            .filter(move |artifact| artifact.kind == kind)
    }

    /// Legge un registro da disco.
    pub fn load(path: &Path) -> InstallResult<Self> {
        let raw = std::fs::read_to_string(path).map_err(|error| InstallError::io(path, error))?;
        let cleaned = vk_core::json::strip_leading_noise(&raw);
        let record: Self = serde_json::from_str(cleaned)
            .map_err(|error| InstallError::InvalidManifest(error.to_string()))?;
        if record.install_dir.as_os_str().is_empty() {
            return Err(InstallError::InvalidManifest(
                "registro senza cartella d'installazione".into(),
            ));
        }
        Ok(record)
    }

    /// Scrive il registro dove serve: nella cartella dati dell'installer e —
    /// se possibile — accanto all'eseguibile installato.
    ///
    /// Le due copie servono a due domande diverse: "cosa c'è installato su
    /// questa macchina?" e "cosa devo rimuovere, io che sono il
    /// disinstallatore dentro questa cartella?".
    pub fn save(&self) -> InstallResult<Vec<PathBuf>> {
        let mut written = Vec::new();

        if let Some(shared) = paths::record_path() {
            self.write_to(&shared)?;
            written.push(shared);
        }

        let local = self.local_path();
        if self.install_dir.is_dir() && self.write_to(&local).is_ok() {
            written.push(local);
        }

        Ok(written)
    }

    fn write_to(&self, path: &Path) -> InstallResult<()> {
        if let Some(parent) = path.parent() {
            crate::fsops::ensure_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| InstallError::InvalidManifest(error.to_string()))?;
        std::fs::write(path, json).map_err(|error| InstallError::io(path, error))
    }

    /// Cancella le copie del registro.
    pub fn forget(&self) {
        if let Some(shared) = paths::record_path() {
            crate::fsops::remove_path_best_effort(&shared);
            if let Some(parent) = shared.parent() {
                if crate::fsops::is_dir_empty(parent) {
                    crate::fsops::remove_path_best_effort(parent);
                }
            }
        }
        crate::fsops::remove_path_best_effort(&self.local_path());
    }
}

/// Data corrente in UTC, formato `2026-08-27T10:15:00Z`.
pub fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(dir: &Path) -> InstallRecord {
        let mut record = InstallRecord::new("2.0.0", "windows-x86_64", dir.to_path_buf());
        record.executable = dir.join("VanzaKart Launcher.exe");
        record.payload = vec![PathBuf::from("VanzaKart Launcher.exe")];
        record.add_artifact(Artifact::new(
            ArtifactKind::DesktopShortcut,
            "C:/Users/tizio/Desktop/VanzaKart Launcher.lnk",
        ));
        record
    }

    #[test]
    fn a_record_survives_a_round_trip() {
        let temp = tempfile::tempdir().expect("temp");
        let record = sample(temp.path());
        let path = temp.path().join("install.json");
        record.write_to(&path).expect("scritto");

        let reloaded = InstallRecord::load(&path).expect("riletto");
        assert_eq!(reloaded, record);
        assert_eq!(reloaded.schema_version, RECORD_SCHEMA_VERSION);
    }

    #[test]
    fn a_record_without_an_install_dir_is_refused() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("rotto.json");
        std::fs::write(&path, r#"{"version":"2.0.0"}"#).expect("scritto");
        assert!(InstallRecord::load(&path).is_err());
    }

    #[test]
    fn an_owned_folder_is_removed_whole() {
        let temp = tempfile::tempdir().expect("temp");
        let record = sample(&temp.path().join("VanzaKart Launcher"));
        assert!(record.owns_install_dir);
        assert_eq!(
            record.installed_paths(),
            vec![temp.path().join("VanzaKart Launcher")]
        );
    }

    #[test]
    fn a_shared_applications_folder_gives_up_only_its_own_bundles() {
        let mut record = InstallRecord::new(
            "2.0.0",
            "darwin-universal",
            PathBuf::from("/Users/tizio/Applications"),
        );
        record.payload = vec![
            PathBuf::from("VanzaKart Launcher.app"),
            PathBuf::from("VanzaKart Uninstaller.app"),
        ];
        assert!(!record.owns_install_dir);
        assert_eq!(
            record.installed_paths(),
            vec![
                PathBuf::from("/Users/tizio/Applications/VanzaKart Launcher.app"),
                PathBuf::from("/Users/tizio/Applications/VanzaKart Uninstaller.app"),
            ]
        );
    }

    #[test]
    fn a_record_without_a_folder_installs_nothing_anywhere() {
        // Il percorso vuoto è prefisso di ogni altro percorso: se arrivasse
        // fino alla rimozione, il disinstallatore crederebbe di trovarsi
        // dentro ciò che sta togliendo (§D-055).
        let record = InstallRecord::default();
        assert!(record.installed_paths().is_empty());
    }

    #[test]
    fn the_same_artifact_is_not_recorded_twice() {
        let temp = tempfile::tempdir().expect("temp");
        let mut record = sample(temp.path());
        let artifact = Artifact::new(
            ArtifactKind::DesktopShortcut,
            "C:/Users/tizio/Desktop/VanzaKart Launcher.lnk",
        );
        record.add_artifact(artifact);
        assert_eq!(record.artifacts.len(), 1);
        assert_eq!(
            record.artifacts_of(ArtifactKind::DesktopShortcut).count(),
            1
        );
    }

    #[test]
    fn a_record_read_with_a_bom_is_still_a_record() {
        let temp = tempfile::tempdir().expect("temp");
        let record = sample(temp.path());
        let path = temp.path().join("bom.json");
        let json = serde_json::to_string_pretty(&record).expect("json");
        std::fs::write(&path, format!("\u{feff}{json}")).expect("scritto");
        assert_eq!(
            InstallRecord::load(&path).expect("riletto").version,
            "2.0.0"
        );
    }

    #[test]
    fn the_timestamp_is_iso8601_in_utc() {
        let stamp = now_iso8601();
        assert!(stamp.ends_with('Z'), "{stamp}");
        assert!(stamp.len() >= 20, "{stamp}");
    }
}
