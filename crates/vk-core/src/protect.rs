//! Protezione dei dati utente dentro la cartella della modpack.
//!
//! Porta 1:1 le regole di `Launcher/Services/ModUpdateSafetyService.cs`.
//! L'ordine dei controlli è significativo: `AlwaysManagedDirectoryNames`
//! neutralizza il filtro per sottostringa che altrimenti classificherebbe come
//! "dato utente" asset ufficiali con "mii" nel nome.

use std::path::{Path, PathBuf};

use crate::versions::Channel;

/// Directory il cui contenuto appartiene all'utente.
pub const PROTECTED_DIRECTORY_NAMES: &[&str] = &[
    "My Stuff", "UserData", "userdata", "Saves", "Save", "Licenses", "License", "Patenti",
    "Profiles", "Miis", "Mii", "private", "Patches", "patches",
];

/// File sempre protetti, ovunque si trovino.
pub const PROTECTED_FILE_NAMES: &[&str] = &[
    "rksys.dat",
    "RFL_DB.dat",
    "active_mii.txt",
    "mii_profile.json",
];

/// Estensioni sempre protette.
pub const PROTECTED_EXTENSIONS: &[&str] = &[".mii", ".miigx", ".mae", ".vk-mii"];

/// Directory di asset ufficiali: il loro contenuto è gestito dagli aggiornamenti
/// anche se il nome contiene parole come "mii".
pub const ALWAYS_MANAGED_DIRECTORY_NAMES: &[&str] = &["CTBRSTM", "MiiOutfitC", "Race"];

/// File di sistema ignorati sia dallo scan sia dal ripristino.
pub const IGNORED_SYSTEM_FILE_NAMES: &[&str] = &["desktop.ini", "Thumbs.db", ".DS_Store"];

/// Sottostringhe che marcano un percorso come dato utente.
const PROTECTED_SUBSTRINGS: &[&str] = &["save", "license", "patent", "mii", "profile"];

/// Percorsi della modpack di un canale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModLayout {
    /// `<UserFolder>/Load/Riivolution` oppure la cartella Modpack locale.
    pub mod_folder: PathBuf,
    pub channel: Channel,
}

impl ModLayout {
    pub fn new(mod_folder: impl Into<PathBuf>, channel: Channel) -> Self {
        Self {
            mod_folder: mod_folder.into(),
            channel,
        }
    }

    pub fn directory_name(&self) -> &'static str {
        self.channel.mod_directory_name()
    }

    /// `<mod_folder>/VanzaKart` (o `VKBeta`).
    pub fn mod_root(&self) -> PathBuf {
        self.mod_folder.join(self.directory_name())
    }

    /// `<mod_folder>/VanzaKart_UserData`.
    pub fn user_data_root(&self) -> PathBuf {
        self.mod_folder
            .join(format!("{}_UserData", self.directory_name()))
    }

    /// `<mod_folder>/VanzaKart/VanzaKart/My Stuff` — il doppio livello è la
    /// struttura reale della release Riivolution.
    pub fn my_stuff(&self) -> PathBuf {
        self.mod_root().join(self.directory_name()).join("My Stuff")
    }

    /// Il file XML Riivolution atteso all'avvio.
    pub fn riivolution_xml(&self) -> PathBuf {
        self.mod_root()
            .join("Riivolution")
            .join(format!("{}.xml", self.directory_name()))
    }

    /// `true` se la modpack risulta installata (XML Riivolution presente).
    pub fn is_installed(&self) -> bool {
        self.riivolution_xml().is_file()
    }
}

/// Insieme delle regole di protezione risolte per un layout.
#[derive(Debug, Clone)]
pub struct ProtectionRules {
    layout: ModLayout,
    protected_roots: Vec<PathBuf>,
}

impl ProtectionRules {
    /// Costruisce le regole, includendo le directory protette di primo livello
    /// realmente presenti sotto la root della modpack.
    ///
    /// Equivalente di `BuildProtectedAbsolutePaths`.
    pub fn build(layout: ModLayout) -> Self {
        let mod_root = layout.mod_root();
        let mut roots = vec![layout.my_stuff(), layout.user_data_root()];

        if let Ok(entries) = std::fs::read_dir(&mod_root) {
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if PROTECTED_DIRECTORY_NAMES
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(&name))
                {
                    roots.push(entry.path());
                }
            }
        }

        let mut protected_roots: Vec<PathBuf> = Vec::new();
        for root in roots {
            let normalized = normalize(&root);
            if !protected_roots
                .iter()
                .any(|item| path_eq(item, &normalized))
            {
                protected_roots.push(normalized);
            }
        }

        Self {
            layout,
            protected_roots,
        }
    }

    pub fn layout(&self) -> &ModLayout {
        &self.layout
    }

    pub fn protected_roots(&self) -> &[PathBuf] {
        &self.protected_roots
    }

    /// `true` se il percorso assoluto ricade sotto una radice protetta.
    pub fn is_absolute_protected(&self, path: &Path) -> bool {
        let normalized = normalize(path);
        self.protected_roots
            .iter()
            .any(|root| path_eq(root, &normalized) || normalized.starts_with(root))
    }

    /// `true` se il percorso — assoluto o relativo alla root della modpack — è
    /// un dato utente da preservare.
    pub fn is_protected(&self, path: &Path) -> bool {
        if self.is_absolute_protected(path) {
            return true;
        }
        match path.strip_prefix(self.layout.mod_root()) {
            Ok(relative) => is_protected_relative(&relative.to_string_lossy()),
            Err(_) => is_protected_relative(&path.to_string_lossy()),
        }
    }
}

/// Replica esatta di `IsProtectedRelativePath`.
pub fn is_protected_relative(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    let trimmed = normalized.trim();

    if trimmed.is_empty() || trimmed.starts_with("..") {
        return false;
    }

    let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();

    if segments.iter().any(|segment| {
        PROTECTED_DIRECTORY_NAMES
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(segment))
    }) {
        return true;
    }

    // Gli asset ufficiali non vengono classificati come dati utente dal filtro
    // per sottostringa che segue.
    if is_always_managed(&segments) {
        return false;
    }

    let file_name = segments.last().copied().unwrap_or("");
    if PROTECTED_FILE_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(file_name))
    {
        return true;
    }

    if let Some(dot) = file_name.rfind('.') {
        let extension = &file_name[dot..];
        if PROTECTED_EXTENSIONS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            return true;
        }
    }

    let lowered = trimmed.to_lowercase();
    PROTECTED_SUBSTRINGS
        .iter()
        .any(|needle| lowered.contains(needle))
}

/// Replica di `IsAlwaysManagedRelativePath`.
pub fn is_always_managed(segments: &[&str]) -> bool {
    if segments.iter().any(|segment| {
        ALWAYS_MANAGED_DIRECTORY_NAMES
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(segment))
    }) {
        return true;
    }

    segments
        .windows(2)
        .any(|pair| pair[0].eq_ignore_ascii_case("Scene") && pair[1].eq_ignore_ascii_case("Model"))
}

/// Replica di `IsIgnoredSystemFile`.
pub fn is_ignored_system_file(relative: &str) -> bool {
    let file_name = relative
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    IGNORED_SYSTEM_FILE_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&file_name))
}

fn normalize(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    PathBuf::from(if trimmed.is_empty() {
        text.as_ref()
    } else {
        trimmed
    })
}

fn path_eq(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(b.to_string_lossy().as_ref())
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_legacy_folder_names() {
        let layout = ModLayout::new("/riiv", Channel::Stable);
        assert_eq!(layout.mod_root(), PathBuf::from("/riiv/VanzaKart"));
        assert_eq!(
            layout.user_data_root(),
            PathBuf::from("/riiv/VanzaKart_UserData")
        );
        assert_eq!(
            layout.my_stuff(),
            PathBuf::from("/riiv/VanzaKart/VanzaKart/My Stuff")
        );
        assert_eq!(
            layout.riivolution_xml(),
            PathBuf::from("/riiv/VanzaKart/Riivolution/VanzaKart.xml")
        );

        let beta = ModLayout::new("/riiv", Channel::Beta);
        assert_eq!(beta.mod_root(), PathBuf::from("/riiv/VKBeta"));
        assert_eq!(
            beta.riivolution_xml(),
            PathBuf::from("/riiv/VKBeta/Riivolution/VKBeta.xml")
        );
    }

    #[test]
    fn protects_user_directories() {
        for path in [
            "VanzaKart/My Stuff/custom.szs",
            "Saves/rksys.dat",
            "licenses/a.bin",
            "Patenti/x",
            "private/secret",
            "profiles/p.json",
        ] {
            assert!(is_protected_relative(path), "non protetto: {path}");
        }
    }

    #[test]
    fn protects_files_and_extensions() {
        assert!(is_protected_relative("anywhere/rksys.dat"));
        assert!(is_protected_relative("anywhere/RFL_DB.dat"));
        assert!(is_protected_relative("a/b/custom.vk-mii"));
        assert!(is_protected_relative("a/b/c.MAE"));
    }

    #[test]
    fn protects_by_substring_like_the_legacy() {
        assert!(is_protected_relative("data/my_save_backup.bin"));
        assert!(is_protected_relative("stuff/PROFILE_data.txt"));
    }

    #[test]
    fn official_assets_stay_managed() {
        // Contengono "mii"/"race" ma sono asset ufficiali della modpack.
        assert!(!is_protected_relative("CTBRSTM/track01.brstm"));
        assert!(!is_protected_relative("MiiOutfitC/outfit.szs"));
        assert!(!is_protected_relative("Race/Course/beginner.szs"));
        assert!(!is_protected_relative("Scene/Model/mii_body.brres"));
    }

    #[test]
    fn ordinary_assets_are_not_protected() {
        assert!(!is_protected_relative("Riivolution/VanzaKart.xml"));
        assert!(!is_protected_relative("VanzaKart/Stage/track.szs"));
        assert!(!is_protected_relative(""));
        assert!(!is_protected_relative("../outside"));
    }

    #[test]
    fn detects_system_files() {
        assert!(is_ignored_system_file("a/b/desktop.ini"));
        assert!(is_ignored_system_file("Thumbs.db"));
        assert!(is_ignored_system_file("a/.DS_Store"));
        assert!(!is_ignored_system_file("a/real.szs"));
    }

    #[test]
    fn absolute_protection_covers_my_stuff_and_userdata() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ModLayout::new(dir.path(), Channel::Stable);
        std::fs::create_dir_all(layout.my_stuff()).unwrap();
        std::fs::create_dir_all(layout.mod_root().join("Saves")).unwrap();
        std::fs::create_dir_all(layout.mod_root().join("Riivolution")).unwrap();

        let rules = ProtectionRules::build(layout.clone());

        assert!(rules.is_absolute_protected(&layout.my_stuff().join("a.szs")));
        assert!(rules.is_absolute_protected(&layout.user_data_root().join("b.bin")));
        assert!(rules.is_absolute_protected(&layout.mod_root().join("Saves").join("rksys.dat")));
        assert!(!rules
            .is_absolute_protected(&layout.mod_root().join("Riivolution").join("VanzaKart.xml")));
    }

    #[test]
    fn is_protected_accepts_absolute_and_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ModLayout::new(dir.path(), Channel::Stable);
        std::fs::create_dir_all(layout.mod_root()).unwrap();
        let rules = ProtectionRules::build(layout.clone());

        assert!(rules.is_protected(&layout.mod_root().join("Saves/rksys.dat")));
        assert!(rules.is_protected(Path::new("Saves/rksys.dat")));
        assert!(!rules.is_protected(&layout.mod_root().join("Riivolution/VanzaKart.xml")));
    }
}
