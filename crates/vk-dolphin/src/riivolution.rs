//! Descrittore `dolphin-game-mod-descriptor` e argomenti di avvio.
//!
//! Porta la generazione JSON di `MainWindow.xaml.cs::LaunchButton_OnClick`.
//! Il legacy costruiva il JSON per interpolazione di stringhe con un escaping
//! manuale; qui si usa `serde_json`, che elimina l'intera classe di bug da
//! percorsi con virgolette o backslash.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DolphinError, DolphinResult};

/// Nome dell'opzione Riivolution per il salvataggio separato.
///
/// Il refuso `Seperate` è parte del contratto XML della modpack: correggerlo
/// romperebbe i salvataggi esistenti (vedi `docs/decisions.md` §D-009).
pub const SEPARATE_SAVEGAME_OPTION: &str = "Seperate Savegame";
pub const PACK_OPTION: &str = "Pack";
pub const MY_STUFF_OPTION: &str = "My Stuff";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiivolutionOption {
    pub choice: u32,
    #[serde(rename = "option-name")]
    pub option_name: String,
    #[serde(rename = "section-name")]
    pub section_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiivolutionPatch {
    pub options: Vec<RiivolutionOption>,
    pub root: String,
    pub xml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiivolutionBlock {
    pub patches: Vec<RiivolutionPatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameModDescriptor {
    #[serde(rename = "base-file")]
    pub base_file: String,
    #[serde(rename = "display-name")]
    pub display_name: String,
    pub riivolution: RiivolutionBlock,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u32,
}

/// Scelte dell'utente che finiscono nel descrittore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchOptions {
    /// `true` → l'opzione "My Stuff" viene attivata con `choice = 2`.
    pub my_stuff_enabled: bool,
    /// `true` → "Seperate Savegame" con `choice = 1`, altrimenti `0`.
    pub separate_savegame: bool,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        // Stessi default del launcher legacy (`UserPreferences`).
        Self {
            my_stuff_enabled: true,
            separate_savegame: true,
        }
    }
}

impl GameModDescriptor {
    /// Costruisce il descrittore per un canale.
    ///
    /// `mod_root` è `<Riivolution>/VanzaKart`; `xml_path` è
    /// `<mod_root>/Riivolution/VanzaKart.xml`.
    pub fn build(
        rom_path: &Path,
        mod_directory_name: &str,
        mod_root: &Path,
        xml_path: &Path,
        options: LaunchOptions,
    ) -> Self {
        let mut riivolution_options = vec![RiivolutionOption {
            choice: 1,
            option_name: PACK_OPTION.to_string(),
            section_name: mod_directory_name.to_string(),
        }];

        if options.my_stuff_enabled {
            riivolution_options.push(RiivolutionOption {
                choice: 2,
                option_name: MY_STUFF_OPTION.to_string(),
                section_name: mod_directory_name.to_string(),
            });
        }

        riivolution_options.push(RiivolutionOption {
            choice: u32::from(options.separate_savegame),
            option_name: SEPARATE_SAVEGAME_OPTION.to_string(),
            section_name: mod_directory_name.to_string(),
        });

        Self {
            base_file: rom_path.to_string_lossy().to_string(),
            display_name: format!("{mod_directory_name} Modpack"),
            riivolution: RiivolutionBlock {
                patches: vec![RiivolutionPatch {
                    options: riivolution_options,
                    root: mod_root.to_string_lossy().to_string(),
                    xml: xml_path.to_string_lossy().to_string(),
                }],
            },
            kind: "dolphin-game-mod-descriptor".to_string(),
            version: 1,
        }
    }

    pub fn to_json(&self) -> DolphinResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Scrive il descrittore. La scrittura è atomica.
    pub fn write_to(&self, path: &Path) -> DolphinResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DolphinError::io(parent, e))?;
        }
        let temp = path.with_extension("json.vk.tmp");
        let json = self.to_json()?;

        std::fs::write(&temp, json.as_bytes())
            .and_then(|()| std::fs::rename(&temp, path))
            .map_err(|e| DolphinError::io(path, e))
    }
}

/// Argomenti di avvio di Dolphin, uno per elemento — nessuna shell coinvolta.
///
/// Equivale a `-b -u "<user>" -e "<descrittore>"` del launcher legacy.
pub fn launch_arguments(user_folder: &Path, descriptor_path: &Path) -> Vec<String> {
    vec![
        "-b".to_string(),
        "-u".to_string(),
        normalize_user_folder(user_folder),
        "-e".to_string(),
        descriptor_path.to_string_lossy().to_string(),
    ]
}

/// Rimuove i separatori finali dalla cartella User, come faceva il legacy
/// (`TrimEnd('\\', '/')`): Dolphin non accetta un percorso con slash finale.
pub fn normalize_user_folder(user_folder: &Path) -> String {
    let text = user_folder.to_string_lossy();
    let trimmed = text.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        text.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Prerequisiti verificati prima dell'avvio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPreconditions {
    pub dolphin_path: PathBuf,
    pub rom_path: PathBuf,
    pub user_folder: PathBuf,
    pub xml_path: PathBuf,
}

/// Valida i prerequisiti d'avvio con messaggi specifici, così che la UI possa
/// indirizzare l'utente alla pagina giusta.
///
/// `section_name` è la sezione Riivolution che il descrittore attiverà: viene
/// cercata dentro l'XML, perché un descrittore che nomina una sezione assente
/// fa partire Dolphin sul disco originale senza alcun errore visibile.
pub fn validate_preconditions(
    dolphin_path: &Path,
    rom_path: &Path,
    user_folder: &Path,
    xml_path: &Path,
    section_name: &str,
) -> DolphinResult<LaunchPreconditions> {
    if dolphin_path.as_os_str().is_empty() {
        return Err(DolphinError::InvalidDolphinPath(
            "percorso di Dolphin non configurato".into(),
        ));
    }
    if !dolphin_path.exists() {
        return Err(DolphinError::InvalidDolphinPath(
            "l'eseguibile di Dolphin non esiste".into(),
        ));
    }

    if rom_path.as_os_str().is_empty() {
        return Err(DolphinError::InvalidRom("ROM non configurata".into()));
    }
    if !rom_path.is_file() {
        return Err(DolphinError::InvalidRom(
            "il file della ROM non esiste".into(),
        ));
    }
    if !has_rom_extension(rom_path) {
        return Err(DolphinError::InvalidRom(
            "estensione non riconosciuta: sono supportati .iso, .wbfs, .rvz, .ciso, .gcm, .wia"
                .into(),
        ));
    }

    if user_folder.as_os_str().is_empty() {
        return Err(DolphinError::InvalidUserFolder(
            "cartella User non configurata".into(),
        ));
    }
    if !user_folder.is_dir() {
        return Err(DolphinError::InvalidUserFolder(
            "la cartella User di Dolphin non esiste".into(),
        ));
    }

    if !xml_path.is_file() {
        return Err(DolphinError::ModNotInstalled(
            xml_path.to_string_lossy().to_string(),
        ));
    }
    crate::modxml::validate(xml_path, section_name)?;

    Ok(LaunchPreconditions {
        dolphin_path: dolphin_path.to_path_buf(),
        rom_path: rom_path.to_path_buf(),
        user_folder: user_folder.to_path_buf(),
        xml_path: xml_path.to_path_buf(),
    })
}

/// Estensioni di immagine disco accettate da Dolphin.
pub const ROM_EXTENSIONS: &[&str] = &["iso", "wbfs", "rvz", "ciso", "gcm", "wia", "gcz", "nkit"];

pub fn has_rom_extension(path: &Path) -> bool {
    path.extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| ROM_EXTENSIONS.contains(&extension.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Descrittore XML minimo ma valido per la sezione `VanzaKart`.
    const REAL_XML: &str = r#"<wiidisc version="1">
        <options><section name="VanzaKart">
            <option name="Pack"><choice name="Enabled"><patch id="p"/></choice></option>
        </section></options>
        <patch id="p"><folder external="/VanzaKart/Binaries" disc="/Binaries"/></patch>
    </wiidisc>"#;

    fn descriptor(options: LaunchOptions) -> GameModDescriptor {
        GameModDescriptor::build(
            Path::new("/games/RMCP01.wbfs"),
            "VanzaKart",
            Path::new("/riiv/VanzaKart"),
            Path::new("/riiv/VanzaKart/Riivolution/VanzaKart.xml"),
            options,
        )
    }

    #[test]
    fn descriptor_matches_the_legacy_shape() {
        let value = descriptor(LaunchOptions::default());

        assert_eq!(value.kind, "dolphin-game-mod-descriptor");
        assert_eq!(value.version, 1);
        assert_eq!(value.display_name, "VanzaKart Modpack");
        assert_eq!(value.base_file, "/games/RMCP01.wbfs");

        let patch = &value.riivolution.patches[0];
        assert_eq!(patch.root, "/riiv/VanzaKart");
        assert_eq!(patch.xml, "/riiv/VanzaKart/Riivolution/VanzaKart.xml");
        assert_eq!(patch.options.len(), 3);
        assert_eq!(patch.options[0].option_name, "Pack");
        assert_eq!(patch.options[0].choice, 1);
        assert_eq!(patch.options[1].option_name, "My Stuff");
        assert_eq!(patch.options[1].choice, 2);
        assert_eq!(patch.options[2].option_name, "Seperate Savegame");
        assert_eq!(patch.options[2].choice, 1);
    }

    #[test]
    fn my_stuff_can_be_disabled() {
        let value = descriptor(LaunchOptions {
            my_stuff_enabled: false,
            separate_savegame: false,
        });
        let options = &value.riivolution.patches[0].options;

        assert_eq!(options.len(), 2);
        assert!(!options.iter().any(|o| o.option_name == "My Stuff"));
        assert_eq!(options[1].choice, 0);
    }

    #[test]
    fn every_option_targets_the_channel_section() {
        let value = GameModDescriptor::build(
            Path::new("/games/rom.iso"),
            "VKBeta",
            Path::new("/riiv/VKBeta"),
            Path::new("/riiv/VKBeta/Riivolution/VKBeta.xml"),
            LaunchOptions::default(),
        );

        assert_eq!(value.display_name, "VKBeta Modpack");
        assert!(value.riivolution.patches[0]
            .options
            .iter()
            .all(|option| option.section_name == "VKBeta"));
    }

    #[test]
    fn json_round_trips() {
        let value = descriptor(LaunchOptions::default());
        let json = value.to_json().unwrap();
        assert_eq!(
            serde_json::from_str::<GameModDescriptor>(&json).unwrap(),
            value
        );
        assert!(json.contains("\"dolphin-game-mod-descriptor\""));
        assert!(json.contains("\"Seperate Savegame\""));
    }

    #[test]
    fn json_escapes_windows_paths_correctly() {
        let value = GameModDescriptor::build(
            Path::new(r#"C:\Giochi\Mario "Kart".wbfs"#),
            "VanzaKart",
            Path::new(r"C:\riiv\VanzaKart"),
            Path::new(r"C:\riiv\VanzaKart\Riivolution\VanzaKart.xml"),
            LaunchOptions::default(),
        );

        let json = value.to_json().unwrap();
        let parsed: GameModDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_file, r#"C:\Giochi\Mario "Kart".wbfs"#);
    }

    #[test]
    fn writes_the_descriptor_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("VanzaKart_launcher.json");

        descriptor(LaunchOptions::default())
            .write_to(&path)
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(serde_json::from_str::<GameModDescriptor>(&raw).is_ok());
        assert!(!dir.path().join("VanzaKart_launcher.json.vk.tmp").exists());
    }

    #[test]
    fn launch_arguments_are_separate_tokens() {
        let arguments = launch_arguments(
            Path::new(r"C:\Users\a\Documents\Dolphin Emulator\"),
            Path::new(r"C:\data\VanzaKart_launcher.json"),
        );

        assert_eq!(
            arguments,
            vec![
                "-b",
                "-u",
                r"C:\Users\a\Documents\Dolphin Emulator",
                "-e",
                r"C:\data\VanzaKart_launcher.json",
            ]
        );
    }

    #[test]
    fn normalizes_trailing_separators() {
        assert_eq!(normalize_user_folder(Path::new("/a/b/")), "/a/b");
        assert_eq!(normalize_user_folder(Path::new(r"C:\a\b\\")), r"C:\a\b");
        assert_eq!(normalize_user_folder(Path::new("/")), "/");
    }

    #[test]
    fn accepts_known_rom_extensions() {
        for extension in ["iso", "WBFS", "rvz", "ciso", "gcm", "wia"] {
            assert!(
                has_rom_extension(Path::new(&format!("rom.{extension}"))),
                "{extension}"
            );
        }
        assert!(!has_rom_extension(Path::new("rom.txt")));
        assert!(!has_rom_extension(Path::new("rom")));
    }

    #[test]
    fn preconditions_report_the_first_missing_piece() {
        let dir = tempfile::tempdir().unwrap();
        let dolphin = dir.path().join("Dolphin.exe");
        let rom = dir.path().join("rom.wbfs");
        let user = dir.path().join("User");
        let xml = dir.path().join("VanzaKart.xml");

        assert!(matches!(
            validate_preconditions(Path::new(""), &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::InvalidDolphinPath(_))
        ));

        std::fs::write(&dolphin, b"").unwrap();
        assert!(matches!(
            validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::InvalidRom(_))
        ));

        std::fs::write(&rom, b"").unwrap();
        assert!(matches!(
            validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::InvalidUserFolder(_))
        ));

        std::fs::create_dir_all(&user).unwrap();
        assert!(matches!(
            validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::ModNotInstalled(_))
        ));

        // Un descrittore vuoto non basta: Dolphin lo accetterebbe e avvierebbe
        // il gioco originale.
        std::fs::write(&xml, b"<wiidisc/>").unwrap();
        assert!(matches!(
            validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::ModIncomplete(_))
        ));

        std::fs::write(&xml, REAL_XML).unwrap();
        let ok = validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart").unwrap();
        assert_eq!(ok.rom_path, rom);
    }

    #[test]
    fn rejects_a_rom_with_a_wrong_extension() {
        let dir = tempfile::tempdir().unwrap();
        let dolphin = dir.path().join("Dolphin.exe");
        let rom = dir.path().join("appunti.txt");
        let user = dir.path().join("User");
        let xml = dir.path().join("VanzaKart.xml");

        std::fs::write(&dolphin, b"").unwrap();
        std::fs::write(&rom, b"").unwrap();
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(&xml, b"").unwrap();

        assert!(matches!(
            validate_preconditions(&dolphin, &rom, &user, &xml, "VanzaKart"),
            Err(DolphinError::InvalidRom(_))
        ));
    }
}
