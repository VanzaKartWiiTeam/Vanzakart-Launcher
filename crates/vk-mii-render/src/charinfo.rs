//! Dalla "studio data" ai parametri di disegno.
//!
//! La studio data è la stringa di 47 byte cifrati che il launcher già calcola
//! per ogni Mii (§D-035): i 46 byte in chiaro sono il Mii nel formato di Mii
//! Studio, e da lì si ricava `CharInfo`, la struttura che la libreria FFL usa
//! per scegliere forme, texture e colori.

use crate::error::{RenderError, RenderResult};

/// Byte in chiaro della studio data.
pub const STUDIO_SIZE: usize = 46;

/// Bit che segnala un colore della tavolozza comune invece di quella del tratto.
pub const COMMON_COLOR: i32 = i32::MIN; // 0x8000_0000

/// Decodifica la stringa esadecimale nei 46 byte in chiaro.
///
/// La cifratura è un XOR progressivo: ogni byte dipende dal precedente già
/// cifrato, e il primo fa da seme.
pub fn decode_studio_hex(hex: &str) -> RenderResult<[u8; STUDIO_SIZE]> {
    let hex = hex.trim();
    if hex.len() != (STUDIO_SIZE + 1) * 2 {
        return Err(RenderError::InvalidStudioData(format!(
            "expected {} hex characters, got {}",
            (STUDIO_SIZE + 1) * 2,
            hex.len()
        )));
    }
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RenderError::InvalidStudioData("not valid hex".into()));
    }

    let mut encoded = [0u8; STUDIO_SIZE + 1];
    for (index, byte) in encoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| RenderError::InvalidStudioData("not valid hex".into()))?;
    }

    let mut decoded = [0u8; STUDIO_SIZE];
    let mut previous = encoded[0];
    for (index, byte) in encoded.iter().enumerate().skip(1) {
        decoded[index - 1] = byte.wrapping_sub(7) ^ previous;
        previous = *byte;
    }

    Ok(decoded)
}

/// I parametri di disegno di un Mii, come `FFLiCharInfo`.
///
/// I colori con [`COMMON_COLOR`] acceso sono indici della tavolozza comune
/// del formato Mii Studio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharInfo {
    pub face_type: i32,
    pub faceline_color: i32,
    pub face_line: i32,
    pub face_makeup: i32,
    pub hair_type: i32,
    pub hair_color: i32,
    pub hair_dir: i32,
    pub eye_type: i32,
    pub eye_color: i32,
    pub eye_scale: i32,
    pub eye_scale_y: i32,
    pub eye_rotate: i32,
    pub eye_spacing_x: i32,
    pub eye_position_y: i32,
    pub eyebrow_type: i32,
    pub eyebrow_color: i32,
    pub eyebrow_scale: i32,
    pub eyebrow_scale_y: i32,
    pub eyebrow_rotate: i32,
    pub eyebrow_spacing_x: i32,
    pub eyebrow_position_y: i32,
    pub nose_type: i32,
    pub nose_scale: i32,
    pub nose_position_y: i32,
    pub mouth_type: i32,
    pub mouth_color: i32,
    pub mouth_scale: i32,
    pub mouth_scale_y: i32,
    pub mouth_position_y: i32,
    pub mustache_type: i32,
    pub beard_type: i32,
    pub beard_color: i32,
    pub mustache_scale: i32,
    pub mustache_position_y: i32,
    pub glass_type: i32,
    pub glass_color: i32,
    pub glass_scale: i32,
    pub glass_position_y: i32,
    pub mole_type: i32,
    pub mole_scale: i32,
    pub mole_position_x: i32,
    pub mole_position_y: i32,
    pub height: i32,
    pub build: i32,
    pub gender: i32,
    pub favorite_color: i32,
}

impl CharInfo {
    /// Mappa i 46 byte di Mii Studio nei campi di FFL.
    pub fn from_studio(studio: &[u8; STUDIO_SIZE]) -> Self {
        let byte = |index: usize| i32::from(studio[index]);
        let common = |index: usize| COMMON_COLOR | byte(index);

        Self {
            beard_color: common(0),
            beard_type: byte(1),
            build: byte(2),
            eye_scale_y: byte(3),
            eye_color: common(4),
            eye_rotate: byte(5),
            eye_scale: byte(6),
            eye_type: byte(7),
            eye_spacing_x: byte(8),
            eye_position_y: byte(9),
            eyebrow_scale_y: byte(10),
            eyebrow_color: common(11),
            eyebrow_rotate: byte(12),
            eyebrow_scale: byte(13),
            eyebrow_type: byte(14),
            eyebrow_spacing_x: byte(15),
            eyebrow_position_y: byte(16),
            faceline_color: byte(17),
            face_makeup: byte(18),
            face_type: byte(19),
            face_line: byte(20),
            favorite_color: byte(21),
            gender: byte(22),
            glass_color: common(23),
            glass_scale: byte(24),
            glass_type: byte(25),
            glass_position_y: byte(26),
            hair_color: common(27),
            hair_dir: byte(28),
            hair_type: byte(29),
            height: byte(30),
            mole_scale: byte(31),
            mole_type: byte(32),
            mole_position_x: byte(33),
            mole_position_y: byte(34),
            mouth_scale_y: byte(35),
            mouth_color: common(36),
            mouth_scale: byte(37),
            mouth_type: byte(38),
            mouth_position_y: byte(39),
            mustache_scale: byte(40),
            mustache_type: byte(41),
            mustache_position_y: byte(42),
            nose_scale: byte(43),
            nose_type: byte(44),
            nose_position_y: byte(45),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cifra come `vk_save::mii::studio_data`, per i test di andata e ritorno.
    fn encode(studio: &[u8; STUDIO_SIZE]) -> String {
        let mut out = String::from("00");
        let mut rolling = 0u8;
        for value in studio {
            let encoded = 7u8.wrapping_add(value ^ rolling);
            rolling = encoded;
            out.push_str(&format!("{encoded:02x}"));
        }
        out
    }

    #[test]
    fn decoding_inverts_the_encoding() {
        let mut studio = [0u8; STUDIO_SIZE];
        for (index, byte) in studio.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37);
        }
        assert_eq!(decode_studio_hex(&encode(&studio)).unwrap(), studio);
    }

    #[test]
    fn malformed_studio_data_is_refused() {
        assert!(decode_studio_hex("").is_err());
        assert!(decode_studio_hex("00ff").is_err());
        assert!(decode_studio_hex(&"zz".repeat(STUDIO_SIZE + 1)).is_err());
    }

    #[test]
    fn colours_come_from_the_common_palette() {
        let mut studio = [0u8; STUDIO_SIZE];
        studio[27] = 8;
        let info = CharInfo::from_studio(&studio);
        assert_eq!(info.hair_color & 0xFF, 8);
        assert_ne!(info.hair_color & COMMON_COLOR, 0);
        assert_eq!(info.faceline_color & COMMON_COLOR, 0);
    }
}
