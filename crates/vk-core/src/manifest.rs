//! Contratto `manifest_files.json`.
//!
//! Compatibile byte per byte con `Launcher/Models/ModManifest.cs`. La
//! validazione replica `MainWindow.xaml.cs::ValidateModManifest`.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::hash::is_valid_sha256;
use crate::json;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModManifest {
    #[serde(default, rename = "mod_version")]
    pub mod_version: String,

    #[serde(default, rename = "archive_sha256")]
    pub archive_sha256: String,

    #[serde(default)]
    pub files: Vec<ModManifestFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModManifestFile {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: i64,
}

impl ModManifest {
    /// Deserializza tollerando BOM UTF-8 e zero-width space iniziali, come il
    /// legacy (`json.TrimStart('\uFEFF', '\u200B')`).
    pub fn parse(raw: &str) -> CoreResult<Self> {
        let manifest: Self = serde_json::from_str(json::strip_leading_noise(raw))?;
        Ok(manifest)
    }

    /// Valida e restituisce il manifest normalizzato (path con `/`, hash lowercase).
    pub fn validated(self) -> CoreResult<Self> {
        validate(&self)?;
        Ok(Self {
            mod_version: self.mod_version.trim().to_string(),
            archive_sha256: self.archive_sha256.trim().to_lowercase(),
            files: self
                .files
                .into_iter()
                .map(|file| ModManifestFile {
                    path: file.path.replace('\\', "/"),
                    sha256: file.sha256.trim().to_lowercase(),
                    size: file.size,
                })
                .collect(),
        })
    }

    /// Somma delle dimensioni dichiarate.
    pub fn total_size(&self) -> i64 {
        self.files.iter().map(|file| file.size.max(0)).sum()
    }

    pub fn find(&self, path: &str) -> Option<&ModManifestFile> {
        self.files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(path))
    }
}

/// Replica esatta di `ValidateModManifest` del launcher legacy.
pub fn validate(manifest: &ModManifest) -> CoreResult<()> {
    if manifest.mod_version.trim().is_empty() || manifest.files.is_empty() {
        return Err(CoreError::InvalidManifest(
            "il manifest della modpack è vuoto o non valido".into(),
        ));
    }

    let mut seen: Vec<String> = Vec::with_capacity(manifest.files.len());

    for file in &manifest.files {
        let normalized = file.path.replace('\\', "/");
        let sha256 = file.sha256.trim();

        let invalid = normalized.is_empty()
            || normalized.starts_with('/')
            || normalized
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            || file.size < 0
            || !is_valid_sha256(sha256);

        let lowered = normalized.to_lowercase();
        let duplicate = seen.contains(&lowered);

        if invalid || duplicate {
            return Err(CoreError::InvalidManifest(format!(
                "voce di manifest non valida o duplicata: {}",
                file.path
            )));
        }

        // Un path assoluto Windows (`C:/...`) supera i controlli precedenti.
        if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
            return Err(CoreError::InvalidManifest(format!(
                "voce di manifest con percorso assoluto: {}",
                file.path
            )));
        }

        seen.push(lowered);
    }

    let archive = manifest.archive_sha256.trim();
    if !archive.is_empty() && !is_valid_sha256(archive) {
        return Err(CoreError::InvalidManifest(
            "l'hash dell'archivio nel manifest non è valido".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> ModManifestFile {
        ModManifestFile {
            path: path.to_string(),
            sha256: "a".repeat(64),
            size: 10,
        }
    }

    fn manifest_with(files: Vec<ModManifestFile>) -> ModManifest {
        ModManifest {
            mod_version: "1.0.0".into(),
            archive_sha256: String::new(),
            files,
        }
    }

    #[test]
    fn accepts_a_well_formed_manifest() {
        let manifest = manifest_with(vec![entry("Riivolution/VanzaKart.xml"), entry("a/b/c.szs")]);
        validate(&manifest).unwrap();
    }

    #[test]
    fn rejects_empty_manifest() {
        assert!(validate(&manifest_with(vec![])).is_err());
        assert!(validate(&ModManifest {
            mod_version: "  ".into(),
            archive_sha256: String::new(),
            files: vec![entry("a")],
        })
        .is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        for path in [
            "../escape.txt",
            "a/../../escape.txt",
            "/absolute.txt",
            "a//b.txt",
            "./relative.txt",
            "..\\windows.txt",
            "C:/Windows/System32/evil.dll",
        ] {
            assert!(
                validate(&manifest_with(vec![entry(path)])).is_err(),
                "avrebbe dovuto rifiutare {path}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_paths_case_insensitively() {
        let manifest = manifest_with(vec![entry("A/B.szs"), entry("a/b.szs")]);
        assert!(validate(&manifest).is_err());
    }

    #[test]
    fn rejects_bad_hashes_and_sizes() {
        let mut short = entry("a.szs");
        short.sha256 = "abc".into();
        assert!(validate(&manifest_with(vec![short])).is_err());

        let mut negative = entry("a.szs");
        negative.size = -1;
        assert!(validate(&manifest_with(vec![negative])).is_err());

        let mut bad_archive = manifest_with(vec![entry("a.szs")]);
        bad_archive.archive_sha256 = "nope".into();
        assert!(validate(&bad_archive).is_err());
    }

    #[test]
    fn parses_with_bom_and_normalizes() {
        let raw = "\u{FEFF}{\"mod_version\":\"1.4.0\",\"archive_sha256\":\"BB\",\"files\":[{\"path\":\"a\\\\b.szs\",\"sha256\":\"AA\",\"size\":3}]}";
        let manifest = ModManifest::parse(raw).unwrap();
        assert_eq!(manifest.mod_version, "1.4.0");
        assert_eq!(manifest.files[0].path, "a\\b.szs");

        // La normalizzazione avviene in `validated`, che qui fallisce sull'hash corto.
        assert!(manifest.validated().is_err());
    }

    #[test]
    fn normalizes_separators_and_case() {
        let manifest = ModManifest {
            mod_version: " 1.0 ".into(),
            archive_sha256: "A".repeat(64),
            files: vec![ModManifestFile {
                path: "a\\b.szs".into(),
                sha256: "B".repeat(64),
                size: 3,
            }],
        }
        .validated()
        .unwrap();

        assert_eq!(manifest.mod_version, "1.0");
        assert_eq!(manifest.files[0].path, "a/b.szs");
        assert_eq!(manifest.files[0].sha256, "b".repeat(64));
        assert_eq!(manifest.archive_sha256, "a".repeat(64));
        assert_eq!(manifest.total_size(), 3);
    }

    #[test]
    fn ignores_unknown_fields() {
        let raw = r#"{"mod_version":"1","files":[],"nuovo_campo":42}"#;
        assert_eq!(ModManifest::parse(raw).unwrap().mod_version, "1");
    }
}
