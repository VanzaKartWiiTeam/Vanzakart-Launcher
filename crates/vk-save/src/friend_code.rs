//! Friend code di Mario Kart Wii.
//!
//! Porta `RksysManager::{CalculateFriendCodeChecksum, FormatFriendCode,
//! TryParseFriendCode}`.
//!
//! Un friend code è un intero a 39 bit: 32 bit di profile ID più 7 bit di
//! checksum, presentato come 12 cifre decimali in gruppi di 4.

use md5::{Digest, Md5};

use crate::error::{SaveError, SaveResult};

/// Salt del gioco: `RMCJ` (il game id) letto al contrario.
const FC_SALT: [u8; 4] = *b"JCMR";

/// Valore massimo rappresentabile: 2^39 − 1.
pub const MAX_FRIEND_CODE: u64 = (1u64 << 39) - 1;

/// Checksum a 7 bit di un profile ID.
pub fn checksum(profile_id: u32) -> u8 {
    let mut buffer = [0u8; 8];
    buffer[..4].copy_from_slice(&profile_id.to_le_bytes());
    buffer[4..].copy_from_slice(&FC_SALT);

    let digest = Md5::digest(buffer);
    (digest[0] >> 1) & 0x7F
}

/// Formatta un profile ID come `0000-0000-0000`.
pub fn format(profile_id: u32) -> String {
    let value = (u64::from(checksum(profile_id)) << 32) | u64::from(profile_id);
    let digits = format!("{value:012}");
    format!("{}-{}-{}", &digits[..4], &digits[4..8], &digits[8..12])
}

/// Estrae il profile ID da un friend code, verificandone il checksum.
///
/// Accetta qualunque separatore: vengono considerate solo le cifre.
pub fn parse(text: &str) -> SaveResult<u32> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();

    if digits.len() != 12 {
        return Err(SaveError::InvalidFriendCode(
            "a friend code must hold exactly 12 digits".into(),
        ));
    }

    let value: u64 = digits
        .parse()
        .map_err(|_| SaveError::InvalidFriendCode("invalid friend code".into()))?;

    if value > MAX_FRIEND_CODE {
        return Err(SaveError::InvalidFriendCode(
            "the friend code is outside the Mario Kart Wii range".into(),
        ));
    }

    let profile_id = (value & 0xFFFF_FFFF) as u32;
    let provided = ((value >> 32) & 0x7F) as u8;

    if provided != checksum(profile_id) {
        return Err(SaveError::InvalidFriendCode(
            "the friend code checksum is not valid".into(),
        ));
    }

    Ok(profile_id)
}

/// `true` se il testo è un friend code valido.
pub fn is_valid(text: &str) -> bool {
    parse(text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_produces_three_groups_of_four_digits() {
        let code = format(1_234_567);
        assert_eq!(code.len(), 14);
        assert_eq!(code.matches('-').count(), 2);
        assert!(code
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-'));
    }

    #[test]
    fn formatting_and_parsing_round_trip() {
        for profile_id in [0u32, 1, 42, 1_234_567, 0x7FFF_FFFF, u32::MAX] {
            let code = format(profile_id);
            assert_eq!(parse(&code).unwrap(), profile_id, "id {profile_id}");
        }
    }

    #[test]
    fn separators_are_ignored() {
        let code = format(999_999);
        let stripped = code.replace('-', "");
        let spaced = code.replace('-', " ");
        assert_eq!(parse(&stripped).unwrap(), 999_999);
        assert_eq!(parse(&spaced).unwrap(), 999_999);
        assert_eq!(parse(&format!("  {code}  ")).unwrap(), 999_999);
    }

    #[test]
    fn the_checksum_fits_in_seven_bits() {
        for profile_id in [0u32, 1, 12_345, u32::MAX] {
            assert!(checksum(profile_id) <= 0x7F);
        }
    }

    #[test]
    fn a_wrong_checksum_is_rejected() {
        let code = format(1_234_567);
        let digits: String = code.chars().filter(char::is_ascii_digit).collect();
        let value: u64 = digits.parse().unwrap();

        // Cambia solo i bit di checksum.
        let corrupted = value ^ (1u64 << 32);
        let text = std::format!("{corrupted:012}");

        assert!(parse(&text).is_err());
    }

    #[test]
    fn wrong_lengths_are_rejected() {
        assert!(parse("123").is_err());
        assert!(parse("1234567890123").is_err());
        assert!(parse("").is_err());
        assert!(parse("abcd-efgh-ijkl").is_err());
    }

    #[test]
    fn out_of_range_codes_are_rejected() {
        // 999999999999 > 2^39-1 = 549755813887.
        assert!(parse("999999999999").is_err());
    }

    #[test]
    fn is_valid_agrees_with_parse() {
        let code = format(777);
        assert!(is_valid(&code));
        assert!(!is_valid("0000-0000-0001"));
    }

    #[test]
    fn the_checksum_is_deterministic() {
        assert_eq!(checksum(1_234_567), checksum(1_234_567));
        assert_ne!(checksum(1), checksum(2));
    }
}
