//! Simboli estesi del Wii dentro i nomi.
//!
//! Mario Kart Wii — e il Wii in generale — scrivono nei nomi dei caratteri
//! che stanno nell'area a uso privato di Unicode (`U+E000`…`U+F8FF`): la
//! corona, i bottoni del Wiimote, le stelle del Wii Wheel. Hanno un glifo
//! solo nel font della console, quindi in una webview diventano quadratini
//! vuoti: il nome `nosotros \u{f043}` si legge «nosotros ▯».
//!
//! Qui li traduciamo nel carattere Unicode standard più vicino, così il nome
//! resta leggibile ovunque. È una conversione di sola presentazione: le viste
//! che arrivano al frontend la usano, i byte del salvataggio no — chi
//! riscrive un nome (l'editor dei Mii) parte sempre dal testo grezzo.
//!
//! Tabella: <https://wiki.tockdom.com/wiki/Extended_Symbols>.

/// Inizio dell'area a uso privato del piano base.
const PUA_START: u32 = 0xE000;
/// Fine dell'area a uso privato del piano base.
const PUA_END: u32 = 0xF8FF;

/// Corrispondenze fra codepoint privato e testo equivalente.
///
/// Ordinata per codepoint: la ricerca è binaria. Dove il simbolo non ha un
/// equivalente singolo si usa una stringa («★★» per le due stelle), che
/// dice più di un carattere raro che metà dei font non ha.
const SYMBOLS: &[(u32, &str)] = &[
    // Simboli comuni a tutto il Wii (nascono sul DSi).
    (0xE000, "Ⓐ"),
    (0xE001, "Ⓑ"),
    (0xE002, "Ⓧ"),
    (0xE003, "Ⓨ"),
    (0xE004, "Ⓛ"),
    (0xE005, "Ⓡ"),
    (0xE006, "✜"),
    (0xE007, "⏰"),
    (0xE008, "☺"),
    (0xE009, "😣"),
    (0xE00A, "☹"),
    (0xE00B, "😐"),
    (0xE00C, "☀"),
    (0xE00D, "☁"),
    (0xE00E, "☂"),
    (0xE00F, "☃"),
    (0xE010, "⚠"),
    (0xE011, "?"),
    (0xE012, "✉"),
    (0xE013, "📱"),
    (0xE014, "▣"),
    (0xE015, "♠"),
    (0xE016, "♦"),
    (0xE017, "♥"),
    (0xE018, "♣"),
    (0xE019, "→"),
    (0xE01A, "←"),
    (0xE01B, "↑"),
    (0xE01C, "↓"),
    (0xE028, "╳"),
    // Specifici di Mario Kart Wii.
    (0xE068, "er"),
    (0xE069, "re"),
    (0xE06A, "e"),
    (0xE06B, "?"),
    (0xF000, "?"),
    (0xF030, "②"),
    (0xF031, "②"),
    (0xF034, "Ⓐ"),
    (0xF035, "Ⓐ"),
    (0xF038, "ⓐ"),
    (0xF039, "ⓐ"),
    (0xF03C, "Ⓐ"),
    (0xF03D, "Ⓐ"),
    (0xF041, "Ⓑ"),
    (0xF043, "①"),
    (0xF044, "⊕"),
    (0xF047, "⊕"),
    (0xF050, "ⓑ"),
    (0xF058, "Ⓑ"),
    (0xF05E, "ⓢ"),
    (0xF05F, "ⓢ"),
    (0xF060, " "),
    (0xF061, "★"),
    (0xF062, "★★"),
    (0xF063, "★★★"),
    (0xF064, "◎"),
    (0xF065, "◎★"),
    (0xF066, "◎★★"),
    (0xF067, "◎★★★"),
    (0xF068, "$"),
    (0xF069, "🎈"),
    (0xF06A, "🏆"),
    (0xF06B, "🏆"),
    (0xF06C, "🏆"),
    (0xF06D, "👑"),
    (0xF074, "◎"),
    (0xF075, "◎★"),
    (0xF076, "◎★★"),
    (0xF077, "◎★★★"),
    (0xF078, "A"),
    (0xF079, "B"),
    (0xF07A, "C"),
    (0xF07B, "D"),
    (0xF07C, "E"),
    (0xF103, "⓪"),
    (0xF107, "?"),
    // Numeri dei giocatori: cambia solo il colore della cornice.
    (0xF108, "①"),
    (0xF109, "②"),
    (0xF10A, "③"),
    (0xF10B, "④"),
    (0xF10C, "①"),
    (0xF10D, "②"),
    (0xF10E, "③"),
    (0xF10F, "④"),
    (0xF110, "①"),
    (0xF111, "②"),
    (0xF112, "③"),
    (0xF113, "④"),
    (0xF114, "①"),
    (0xF115, "②"),
    (0xF116, "③"),
    (0xF117, "④"),
    (0xF118, "①"),
    (0xF119, "②"),
    (0xF11A, "③"),
    (0xF11B, "④"),
    (0xF11C, "①"),
    (0xF11D, "②"),
    (0xF11E, "③"),
    (0xF11F, "④"),
    (0xF120, "①"),
    (0xF121, "②"),
    (0xF122, "③"),
    (0xF123, "④"),
    (0xF124, "①"),
    (0xF125, "②"),
    (0xF126, "③"),
    (0xF127, "④"),
    (0xF128, "①"),
    (0xF129, "②"),
    (0xF12A, "③"),
    (0xF12B, "④"),
    (0xF12C, "①"),
    (0xF12D, "②"),
    (0xF12E, "③"),
    (0xF12F, "④"),
];

/// Rende leggibile un nome che arriva da un salvataggio o dal server.
///
/// Sostituisce i simboli del font Wii, butta via gli altri caratteri privati
/// — sarebbero comunque quadratini — e i caratteri di controllo, che nei
/// salvataggi rovinati capitano.
pub fn humanize(text: &str) -> String {
    // La stragrande maggioranza dei nomi non ha niente da tradurre: evitiamo
    // di ricostruire la stringa per nulla.
    if !text.chars().any(needs_work) {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if (PUA_START..=PUA_END).contains(&code) {
            if let Ok(index) = SYMBOLS.binary_search_by_key(&code, |(key, _)| *key) {
                out.push_str(SYMBOLS[index].1);
            }
            continue;
        }
        if is_control(ch) {
            continue;
        }
        out.push(ch);
    }

    out.trim().to_string()
}

/// `true` se il carattere va tradotto o tolto.
fn needs_work(ch: char) -> bool {
    let code = ch as u32;
    (PUA_START..=PUA_END).contains(&code) || is_control(ch)
}

/// Controlli e caratteri invisibili che non hanno senso in un nome.
fn is_control(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{FFFD}' | '\u{200B}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabella_ordinata() {
        assert!(SYMBOLS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn traduce_i_simboli_del_font_wii() {
        // Il nome che ha fatto nascere questa funzione: bottone 1 del Wiimote.
        assert_eq!(humanize("nosotros \u{f043}"), "nosotros ①");
        assert_eq!(humanize("\u{f038}C Sossio\u{f06d}"), "ⓐC Sossio👑");
    }

    #[test]
    fn lascia_intatti_i_nomi_normali() {
        assert_eq!(humanize("lago duria"), "lago duria");
        assert_eq!(humanize("Mr.かっちゃんべ"), "Mr.かっちゃんべ");
        assert_eq!(humanize(""), "");
    }

    #[test]
    fn butta_via_i_privati_senza_glifo_e_i_controlli() {
        assert_eq!(humanize("ciao\u{e900}"), "ciao");
        assert_eq!(humanize("ciao\u{0007}mondo"), "ciaomondo");
        assert_eq!(humanize("  \u{f060}ciao\u{f060}  "), "ciao");
    }
}
