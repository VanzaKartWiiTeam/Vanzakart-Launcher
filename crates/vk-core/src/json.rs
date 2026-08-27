//! Helper di deserializzazione condivisi con i contratti legacy.

use serde::{Deserialize, Deserializer};

/// Rimuove BOM UTF-8 e zero-width space in testa, come
/// `json.TrimStart('﻿', '​')` del launcher legacy.
pub fn strip_leading_noise(raw: &str) -> &str {
    raw.trim_start_matches(['\u{FEFF}', '\u{200B}'])
}

/// Deserializza un campo che il server può inviare come stringa singola o come
/// array di stringhe (equivalente di `StringArrayOrSingleConverter`).
///
/// `null`, valori non stringa e stringhe vuote vengono scartati, esattamente
/// come nel converter C#.
pub fn string_or_array<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Single(String),
        Many(Vec<serde_json::Value>),
        Null,
    }

    Ok(
        match Repr::deserialize(deserializer).unwrap_or(Repr::Null) {
            Repr::Null => Vec::new(),
            Repr::Single(value) if value.trim().is_empty() => Vec::new(),
            Repr::Single(value) => vec![value],
            Repr::Many(values) => values
                .into_iter()
                .filter_map(|value| match value {
                    serde_json::Value::String(text) if !text.trim().is_empty() => Some(text),
                    _ => None,
                })
                .collect(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Probe {
        #[serde(default, deserialize_with = "string_or_array")]
        changelog: Vec<String>,
    }

    fn changelog(raw: &str) -> Vec<String> {
        serde_json::from_str::<Probe>(raw).unwrap().changelog
    }

    #[test]
    fn accepts_a_single_string() {
        assert_eq!(changelog(r#"{"changelog":"una riga"}"#), vec!["una riga"]);
    }

    #[test]
    fn accepts_an_array() {
        assert_eq!(changelog(r#"{"changelog":["a","b"]}"#), vec!["a", "b"]);
    }

    #[test]
    fn drops_null_and_empty_entries() {
        assert!(changelog(r#"{"changelog":null}"#).is_empty());
        assert!(changelog(r#"{"changelog":"   "}"#).is_empty());
        assert_eq!(
            changelog(r#"{"changelog":["a",null,"","b",7]}"#),
            vec!["a", "b"]
        );
    }

    #[test]
    fn missing_field_yields_empty() {
        assert!(changelog("{}").is_empty());
    }

    #[test]
    fn strips_bom_and_zero_width() {
        assert_eq!(strip_leading_noise("\u{FEFF}\u{200B}{}"), "{}");
        assert_eq!(strip_leading_noise("{}"), "{}");
    }
}
