//! Lettura e scrittura format-preserving dei file INI di Dolphin.
//!
//! Porta `Launcher/Services/DolphinIniService.cs`. Le righe non toccate
//! (commenti, spaziatura, ordine) sopravvivono a un aggiornamento: Dolphin
//! riscrive i suoi INI e sovrascriverebbe modifiche non conservative.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{DolphinError, DolphinResult};

/// Sezione predefinita per le coppie chiave/valore prima di ogni `[Sezione]`.
pub const GLOBAL_SECTION: &str = "Global";

/// Contenuto di un INI: sezione → chiavi.
pub type IniData = BTreeMap<String, BTreeMap<String, String>>;

/// Legge un INI. Un file assente restituisce una mappa vuota, non un errore.
///
/// In caso di chiave duplicata dentro una sezione vince la **prima**
/// occorrenza, come nel launcher legacy.
pub fn read_ini(path: &Path) -> IniData {
    let Ok(content) = std::fs::read_to_string(path) else {
        return IniData::new();
    };
    parse_ini(&content)
}

/// Come [`read_ini`] ma su una stringa già in memoria.
pub fn parse_ini(content: &str) -> IniData {
    let mut data = IniData::new();
    let mut section = GLOBAL_SECTION.to_string();
    data.entry(section.clone()).or_default();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            data.entry(section.clone()).or_default();
            continue;
        }

        if let Some(index) = line.find('=') {
            if index == 0 {
                continue;
            }
            let key = line[..index].trim().to_string();
            let value = line[index + 1..].trim().to_string();
            data.entry(section.clone())
                .or_default()
                .entry(key)
                .or_insert(value);
        }
    }

    data
}

/// Applica gli aggiornamenti a un INI preservandone il formato.
///
/// - una chiave esistente viene riscritta al suo posto;
/// - una chiave nuova viene aggiunta in coda alla sua sezione;
/// - una sezione nuova viene aggiunta in fondo al file.
///
/// La scrittura è atomica (file temporaneo + rename) e in UTF-8 senza BOM.
pub fn update_ini(path: &Path, updates: &IniData) -> DolphinResult<()> {
    if updates.is_empty() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DolphinError::io(parent, e))?;
    }

    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let rendered = apply_updates(&existing, updates);

    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "config.ini".to_string());
    let temp = path.with_file_name(format!(".{file_name}.vk.tmp"));

    let result = std::fs::write(&temp, rendered.as_bytes())
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| DolphinError::io(path, e));

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Nucleo puro di [`update_ini`]: prende il testo originale e restituisce
/// quello aggiornato. Separato per essere testabile senza filesystem.
pub fn apply_updates(original: &str, updates: &IniData) -> String {
    let uses_crlf = original.contains("\r\n");
    let newline = if uses_crlf { "\r\n" } else { "\n" };
    let ends_with_newline = original.is_empty() || original.ends_with('\n');

    let mut out: Vec<String> = Vec::new();
    let mut written: BTreeMap<String, Vec<String>> = updates
        .keys()
        .map(|section| (section.clone(), Vec::new()))
        .collect();

    let mut current = GLOBAL_SECTION.to_string();

    let flush_section =
        |section: &str, out: &mut Vec<String>, written: &mut BTreeMap<String, Vec<String>>| {
            let Some(pending) = updates.get(section) else {
                return;
            };
            let done = written.entry(section.to_string()).or_default();
            for (key, value) in pending {
                if !done.iter().any(|item| item.eq_ignore_ascii_case(key)) {
                    out.push(format!("{key} = {value}"));
                    done.push(key.clone());
                }
            }
        };

    for raw in original.lines() {
        let trimmed = raw.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Prima di cambiare sezione, aggiunge le chiavi mancanti a quella corrente.
            flush_section(&current, &mut out, &mut written);
            current = trimmed[1..trimmed.len() - 1].trim().to_string();
            out.push(raw.to_string());
            continue;
        }

        if let Some(index) = trimmed.find('=') {
            if index > 0 && !trimmed.starts_with(';') && !trimmed.starts_with('#') {
                let key = trimmed[..index].trim();
                if let Some(section_updates) = updates.get(&current) {
                    if let Some(value) = lookup_ignore_case(section_updates, key) {
                        out.push(format!("{key} = {value}"));
                        written
                            .entry(current.clone())
                            .or_default()
                            .push(key.to_string());
                        continue;
                    }
                }
            }
        }

        out.push(raw.to_string());
    }

    flush_section(&current, &mut out, &mut written);

    // Sezioni interamente nuove.
    for (section, pending) in updates {
        let done = written.get(section).cloned().unwrap_or_default();
        let missing: Vec<(&String, &String)> = pending
            .iter()
            .filter(|(key, _)| !done.iter().any(|item| item.eq_ignore_ascii_case(key)))
            .collect();

        if missing.is_empty() {
            continue;
        }

        if out.last().is_some_and(|line| !line.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(format!("[{section}]"));
        for (key, value) in missing {
            out.push(format!("{key} = {value}"));
        }
    }

    let mut rendered = out.join(newline);
    if ends_with_newline && !rendered.is_empty() {
        rendered.push_str(newline);
    }
    rendered
}

fn lookup_ignore_case<'a>(map: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    map.iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

/// Legge un valore da una sezione, senza distinzione fra maiuscole e minuscole.
pub fn get<'a>(data: &'a IniData, section: &str, key: &str) -> Option<&'a str> {
    data.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(section))
        .and_then(|(_, values)| lookup_ignore_case(values, key))
        .map(String::as_str)
}

/// Legge un booleano nel formato di Dolphin (`True`/`False`, tollerante).
pub fn get_bool(data: &IniData, section: &str, key: &str) -> Option<bool> {
    get(data, section, key).and_then(parse_bool)
}

/// Legge un intero.
pub fn get_int(data: &IniData, section: &str, key: &str) -> Option<i64> {
    get(data, section, key).and_then(|value| value.trim().parse().ok())
}

/// Legge un `f32` con punto decimale (invariante di cultura).
pub fn get_float(data: &IniData, section: &str, key: &str) -> Option<f32> {
    get(data, section, key).and_then(|value| value.trim().parse().ok())
}

/// Interpreta un booleano scritto da Dolphin o da un utente.
pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Serializza un booleano nel formato usato da Dolphin.
pub fn format_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

/// Costruttore fluido di aggiornamenti INI.
#[derive(Debug, Default, Clone)]
pub struct IniUpdates(IniData);

impl IniUpdates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, section: &str, key: &str, value: impl Into<String>) -> Self {
        self.0
            .entry(section.to_string())
            .or_default()
            .insert(key.to_string(), value.into());
        self
    }

    pub fn set_bool(self, section: &str, key: &str, value: bool) -> Self {
        self.set(section, key, format_bool(value))
    }

    pub fn set_int(self, section: &str, key: &str, value: i64) -> Self {
        self.set(section, key, value.to_string())
    }

    pub fn into_data(self) -> IniData {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(|section| section.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; Configurazione di Dolphin
[Core]
CPUThread = True
EnableCheats = True

[Display]
Fullscreen = False
";

    #[test]
    fn parses_sections_and_values() {
        let data = parse_ini(SAMPLE);
        assert_eq!(get(&data, "Core", "CPUThread"), Some("True"));
        assert_eq!(get(&data, "Display", "Fullscreen"), Some("False"));
        assert_eq!(get(&data, "core", "cputhread"), Some("True"));
        assert_eq!(get(&data, "Core", "Inesistente"), None);
    }

    #[test]
    fn first_duplicate_key_wins() {
        let data = parse_ini("[Core]\nA = 1\nA = 2\n");
        assert_eq!(get(&data, "Core", "A"), Some("1"));
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let data = parse_ini("; commento\n# altro\n\n[Core]\nA = 1\n");
        assert_eq!(data["Core"].len(), 1);
    }

    #[test]
    fn values_before_any_section_land_in_global() {
        let data = parse_ini("Chiave = valore\n[Core]\nA = 1\n");
        assert_eq!(get(&data, GLOBAL_SECTION, "Chiave"), Some("valore"));
    }

    #[test]
    fn updating_an_existing_key_preserves_layout() {
        let updates = IniUpdates::new()
            .set_bool("Core", "EnableCheats", false)
            .into_data();
        let result = apply_updates(SAMPLE, &updates);

        assert!(result.contains("; Configurazione di Dolphin"));
        assert!(result.contains("EnableCheats = False"));
        assert!(result.contains("CPUThread = True"));
        assert!(!result.contains("EnableCheats = True"));
        // Nessuna sezione duplicata.
        assert_eq!(result.matches("[Core]").count(), 1);
    }

    #[test]
    fn adding_a_key_appends_to_its_section() {
        let updates = IniUpdates::new()
            .set("Core", "EnableRiivolution", "True")
            .into_data();
        let result = apply_updates(SAMPLE, &updates);
        let core_block = result.split("[Display]").next().unwrap();

        assert!(core_block.contains("EnableRiivolution = True"));
        assert!(result.contains("[Display]"));
    }

    #[test]
    fn adding_a_new_section_appends_at_the_end() {
        let updates = IniUpdates::new()
            .set("VanzaKartLauncher", "PerformancePreset", "Balanced")
            .into_data();
        let result = apply_updates(SAMPLE, &updates);

        assert!(result.trim_end().ends_with("PerformancePreset = Balanced"));
        assert!(result.contains("[VanzaKartLauncher]"));
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let original = "[Core]\r\nA = 1\r\n";
        let updates = IniUpdates::new().set("Core", "A", "2").into_data();
        let result = apply_updates(original, &updates);
        assert_eq!(result, "[Core]\r\nA = 2\r\n");
    }

    #[test]
    fn writes_into_an_empty_file() {
        let updates = IniUpdates::new().set("Core", "A", "1").into_data();
        assert_eq!(apply_updates("", &updates), "[Core]\nA = 1\n");
    }

    #[test]
    fn writes_atomically_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config").join("Dolphin.ini");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, SAMPLE).unwrap();

        update_ini(
            &path,
            &IniUpdates::new()
                .set_int("DSP", "Volume", 80)
                .set_bool("Core", "EnableCheats", false)
                .into_data(),
        )
        .unwrap();

        let data = read_ini(&path);
        assert_eq!(get_int(&data, "DSP", "Volume"), Some(80));
        assert_eq!(get_bool(&data, "Core", "EnableCheats"), Some(false));

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn reading_a_missing_file_is_not_an_error() {
        assert!(read_ini(Path::new("/percorso/inesistente.ini")).is_empty());
    }

    #[test]
    fn parses_booleans_in_every_dolphin_spelling() {
        for value in ["True", "true", "1", "yes", "ON"] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }
        for value in ["False", "false", "0", "no", "off"] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }
        assert_eq!(parse_bool("forse"), None);
        assert_eq!(format_bool(true), "True");
    }

    #[test]
    fn reads_numeric_values() {
        let data = parse_ini("[Core]\nOverclock = 1.5\nVolume = 80\n");
        assert_eq!(get_float(&data, "Core", "Overclock"), Some(1.5));
        assert_eq!(get_int(&data, "Core", "Volume"), Some(80));
        assert_eq!(get_int(&data, "Core", "Overclock"), None);
    }
}
