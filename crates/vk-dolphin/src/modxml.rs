//! Ispezione del descrittore XML Riivolution della modpack.
//!
//! Il descrittore JSON che il launcher passa a Dolphin (`riivolution::
//! GameModDescriptor`) si limita a **nominare** una sezione e le sue opzioni:
//! è l'XML della modpack a dire cosa quelle opzioni fanno davvero. Se l'XML
//! non dichiara quella sezione — o è un `<wiidisc/>` vuoto — Dolphin non
//! applica nessuna patch e avvia il gioco originale senza segnalare nulla.
//!
//! Qui si legge il minimo che serve per accorgersene prima dell'avvio:
//! sezioni, opzioni e numero di patch. Non è un parser XML completo e non
//! vuole esserlo — è uno scanner tollerante che ignora commenti, CDATA e
//! istruzioni di elaborazione.

use std::path::Path;

use crate::error::{DolphinError, DolphinResult};

/// Limite di lettura del descrittore. I file reali stanno sotto i 100 KB;
/// il tetto evita di caricare in memoria un file scambiato per errore.
pub const MAX_XML_BYTES: u64 = 8 * 1024 * 1024;

/// Contenuto significativo del descrittore XML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModXml {
    /// `true` se l'elemento radice `<wiidisc>` è presente.
    pub has_wiidisc: bool,
    /// Nomi delle sezioni dichiarate in `<options>`.
    pub sections: Vec<String>,
    /// Coppie `(sezione, opzione)`, nell'ordine del documento.
    pub options: Vec<(String, String)>,
    /// Numero di elementi `<patch id="...">` fuori da `<options>`, cioè quelli
    /// che definiscono davvero le modifiche al disco.
    pub patches: usize,
}

impl ModXml {
    pub fn has_section(&self, section: &str) -> bool {
        self.sections
            .iter()
            .any(|name| name.eq_ignore_ascii_case(section))
    }

    pub fn has_option(&self, section: &str, option: &str) -> bool {
        self.options.iter().any(|(declared_section, declared)| {
            declared_section.eq_ignore_ascii_case(section) && declared.eq_ignore_ascii_case(option)
        })
    }

    /// Opzioni dichiarate da una sezione.
    pub fn options_of(&self, section: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(declared_section, _)| declared_section.eq_ignore_ascii_case(section))
            .map(|(_, option)| option.as_str())
            .collect()
    }

    /// `true` se il descrittore può patchare qualcosa.
    pub fn is_usable(&self) -> bool {
        self.has_wiidisc && !self.options.is_empty() && self.patches > 0
    }
}

/// Analizza il testo del descrittore.
pub fn parse(raw: &str) -> ModXml {
    let mut result = ModXml::default();
    let mut section_stack: Vec<String> = Vec::new();

    for tag in tags(raw) {
        match tag.name.to_ascii_lowercase().as_str() {
            "wiidisc" if !tag.closing => result.has_wiidisc = true,
            "section" => {
                if tag.closing {
                    section_stack.pop();
                    continue;
                }
                let name = tag.attribute("name").unwrap_or_default();
                if !result.has_section(&name) {
                    result.sections.push(name.clone());
                }
                if !tag.self_closing {
                    section_stack.push(name);
                }
            }
            "option" if !tag.closing => {
                let Some(section) = section_stack.last() else {
                    continue;
                };
                let name = tag.attribute("name").unwrap_or_default();
                if !name.is_empty() {
                    result.options.push((section.clone(), name));
                }
            }
            // Le `<patch>` dentro una `<choice>` sono soli riferimenti: le
            // patch vere stanno fuori dal blocco `<options>`.
            "patch" if !tag.closing && section_stack.is_empty() => result.patches += 1,
            _ => {}
        }
    }

    result
}

/// Legge e analizza il descrittore.
pub fn read(path: &Path) -> DolphinResult<ModXml> {
    let metadata = std::fs::metadata(path).map_err(|error| DolphinError::io(path, error))?;
    if metadata.len() > MAX_XML_BYTES {
        return Err(DolphinError::ModIncomplete(format!(
            "{}: il descrittore Riivolution è troppo grande ({} byte)",
            file_name(path),
            metadata.len()
        )));
    }

    let bytes = std::fs::read(path).map_err(|error| DolphinError::io(path, error))?;
    Ok(parse(&String::from_utf8_lossy(&bytes)))
}

/// Verifica che il descrittore possa davvero applicare la sezione richiesta.
///
/// È il controllo che distingue "modpack installata" da "modpack che si avvia
/// ma lascia partire Mario Kart Wii originale".
pub fn validate(path: &Path, section: &str) -> DolphinResult<()> {
    let xml = read(path)?;

    if !xml.has_wiidisc || xml.patches == 0 {
        return Err(DolphinError::ModIncomplete(format!(
            "{}: il descrittore Riivolution non contiene nessuna patch",
            file_name(path)
        )));
    }

    if !xml.has_section(section) {
        return Err(DolphinError::ModIncomplete(format!(
            "{}: il descrittore Riivolution non dichiara la sezione «{section}»",
            file_name(path)
        )));
    }

    if !xml.has_option(section, crate::riivolution::PACK_OPTION) {
        return Err(DolphinError::ModIncomplete(format!(
            "{}: la sezione «{section}» non dichiara l'opzione «{}»",
            file_name(path),
            crate::riivolution::PACK_OPTION
        )));
    }

    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    name: String,
    closing: bool,
    self_closing: bool,
    attributes: Vec<(String, String)>,
}

impl Tag {
    fn attribute(&self, key: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.clone())
    }
}

/// Elenca i tag del documento, saltando commenti, CDATA e `<?...?>`/`<!...>`.
fn tags(raw: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut rest = raw;

    while let Some(open) = rest.find('<') {
        rest = &rest[open..];

        if let Some(skipped) = skip_non_element(rest) {
            rest = skipped;
            continue;
        }

        let Some(close) = find_tag_end(rest) else {
            // Un `<` isolato: si riparte dal carattere successivo.
            rest = &rest[1..];
            continue;
        };
        let body = &rest[1..close];
        rest = &rest[close + 1..];

        if let Some(tag) = parse_tag(body) {
            out.push(tag);
        }
    }

    out
}

/// Se `rest` inizia con un nodo che non è un elemento, restituisce ciò che
/// resta dopo di esso.
fn skip_non_element(rest: &str) -> Option<&str> {
    for (prefix, terminator) in [
        ("<!--", "-->"),
        ("<![CDATA[", "]]>"),
        ("<?", "?>"),
        ("<!", ">"),
    ] {
        if let Some(after) = rest.strip_prefix(prefix) {
            return Some(match after.find(terminator) {
                Some(end) => &after[end + terminator.len()..],
                None => "",
            });
        }
    }
    None
}

/// Indice del `>` che chiude il tag, ignorando quelli dentro le virgolette.
fn find_tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;

    for (index, character) in rest.char_indices().skip(1) {
        match (quote, character) {
            (Some(open), current) if current == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '>') => return Some(index),
            (None, '<') => return None,
            (None, _) => {}
        }
    }

    None
}

fn parse_tag(body: &str) -> Option<Tag> {
    let body = body.trim();
    let (closing, body) = match body.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let (self_closing, body) = match body.strip_suffix('/') {
        Some(rest) => (true, rest),
        None => (false, body),
    };

    let body = body.trim_start();
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = &body[..name_end];
    if name.is_empty() {
        return None;
    }

    Some(Tag {
        name: name.to_string(),
        closing,
        self_closing,
        attributes: parse_attributes(&body[name_end..]),
    })
}

fn parse_attributes(mut rest: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();

    while let Some(equals) = rest.find('=') {
        let name = rest[..equals]
            .trim_end()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default()
            .trim();
        let after = rest[equals + 1..].trim_start();

        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            // Attributo senza virgolette: non compare nei descrittori reali,
            // si abbandona il tag per non interpretare male i valori.
            break;
        };
        let after = &after[quote.len_utf8()..];
        let Some(end) = after.find(quote) else {
            break;
        };

        if !name.is_empty() {
            out.push((name.to_string(), decode_entities(&after[..end])));
        }
        rest = &after[end + quote.len_utf8()..];
    }

    out
}

/// Decodifica le sole entità predefinite di XML.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Estratto fedele del descrittore reale della modpack.
    const REAL: &str = r#"<wiidisc version="1">
	<id game="RMC"/>
	<options>
		<section name="VanzaKart">
			<option name="Pack">
				<choice name="Enabled">
					<patch id="VanzaKart164LoadPack"/>
				</choice>
			</option>
			<option name="My Stuff">
				<choice name="From CTGP-r"><patch id="CTGPLoad"/></choice>
				<choice name="From Pack"><patch id="VanzaKart164Load"/></choice>
			</option>
			<option name="Seperate Savegame">
				<choice name="Enabled"><patch id="VKSave"/></choice>
			</option>
		</section>
	</options>
	<patch id="VanzaKart164LoadPack">
		<memory offset="0x80242698" value="4BDC1968" original="4e800020" /> <!--RMCP DOL-->
		<folder external="/VanzaKart/Binaries" disc="/Binaries" create="true"/>
	</patch>
	<patch id="VKSave">
		<savegame external="/VanzaKart_UserData/save" clone="true"/>
	</patch>
</wiidisc>"#;

    #[test]
    fn reads_sections_options_and_patches() {
        let xml = parse(REAL);

        assert!(xml.has_wiidisc);
        assert_eq!(xml.sections, vec!["VanzaKart"]);
        assert_eq!(
            xml.options_of("VanzaKart"),
            vec!["Pack", "My Stuff", "Seperate Savegame"]
        );
        assert_eq!(xml.patches, 2);
        assert!(xml.is_usable());
    }

    #[test]
    fn the_launch_options_of_the_descriptor_exist_in_the_xml() {
        let xml = parse(REAL);
        for option in [
            crate::riivolution::PACK_OPTION,
            crate::riivolution::MY_STUFF_OPTION,
            crate::riivolution::SEPARATE_SAVEGAME_OPTION,
        ] {
            assert!(xml.has_option("VanzaKart", option), "{option}");
        }
    }

    #[test]
    fn an_empty_wiidisc_is_not_usable() {
        let xml = parse("<wiidisc/>");
        assert!(xml.has_wiidisc);
        assert!(xml.sections.is_empty());
        assert_eq!(xml.patches, 0);
        assert!(!xml.is_usable());
    }

    #[test]
    fn comments_and_declarations_are_ignored() {
        let xml = parse(
            r#"<?xml version="1.0"?>
            <!-- <section name="Finta"> -->
            <!DOCTYPE wiidisc>
            <wiidisc version="1">
              <options><section name="VKBeta"><option name="Pack"/></section></options>
              <patch id="X"><![CDATA[<option name="Nascosta"/>]]></patch>
            </wiidisc>"#,
        );

        assert_eq!(xml.sections, vec!["VKBeta"]);
        assert_eq!(xml.options_of("VKBeta"), vec!["Pack"]);
        assert_eq!(xml.patches, 1);
    }

    #[test]
    fn attribute_values_may_contain_angle_brackets_and_entities() {
        let xml = parse(
            r#"<wiidisc><options><section name="A &amp; B">
               <option name="&lt;Pack&gt;"/></section></options>
               <patch id="p"/></wiidisc>"#,
        );

        assert!(xml.has_section("A & B"));
        assert!(xml.has_option("A & B", "<Pack>"));
    }

    #[test]
    fn a_self_closing_section_does_not_swallow_the_following_options() {
        let xml = parse(
            r#"<wiidisc><options>
                 <section name="Vuota"/>
                 <section name="Vera"><option name="Pack"/></section>
               </options><patch id="p"/></wiidisc>"#,
        );

        assert!(xml.options_of("Vuota").is_empty());
        assert_eq!(xml.options_of("Vera"), vec!["Pack"]);
    }

    #[test]
    fn section_matching_is_case_insensitive() {
        let xml = parse(REAL);
        assert!(xml.has_section("vanzakart"));
        assert!(xml.has_option("VANZAKART", "pack"));
        assert!(!xml.has_section("VKBeta"));
    }

    #[test]
    fn validate_accepts_the_real_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("VanzaKart.xml");
        std::fs::write(&path, REAL).unwrap();

        validate(&path, "VanzaKart").unwrap();
    }

    #[test]
    fn validate_rejects_an_empty_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("VanzaKart.xml");
        std::fs::write(&path, b"<wiidisc/>").unwrap();

        let error = validate(&path, "VanzaKart").unwrap_err();
        assert!(matches!(error, DolphinError::ModIncomplete(_)));
        assert!(error.to_string().contains("nessuna patch"), "{error}");
    }

    #[test]
    fn validate_rejects_a_descriptor_of_the_other_channel() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("VKBeta.xml");
        std::fs::write(&path, REAL).unwrap();

        let error = validate(&path, "VKBeta").unwrap_err();
        assert!(error.to_string().contains("VKBeta"), "{error}");
    }

    #[test]
    fn validate_rejects_a_section_without_the_pack_option() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("VanzaKart.xml");
        std::fs::write(
            &path,
            r#"<wiidisc><options><section name="VanzaKart">
                 <option name="My Stuff"/></section></options>
               <patch id="p"/></wiidisc>"#,
        )
        .unwrap();

        let error = validate(&path, "VanzaKart").unwrap_err();
        assert!(error.to_string().contains("Pack"), "{error}");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            validate(&dir.path().join("assente.xml"), "VanzaKart"),
            Err(DolphinError::Io { .. })
        ));
    }

    #[test]
    fn binary_content_does_not_panic() {
        let xml = parse(&String::from_utf8_lossy(&[0xff, 0x3c, 0x00, 0x3e, 0xfe]));
        assert!(!xml.is_usable());
    }
}
