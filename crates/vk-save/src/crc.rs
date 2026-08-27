//! Checksum dei formati di salvataggio.
//!
//! Porta `RksysManager::{ComputeCrc32, ComputeReversedWordsCrc32}` e
//! `MkwiiSaveParserService.ComputeCrc16Ccitt`.

/// Polinomio CRC-32 riflesso (IEEE 802.3), lo stesso di zlib.
const POLYNOMIAL: u32 = 0xEDB8_8320;

/// Tabella CRC-32 costruita a tempo di compilazione.
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;

    while index < 256 {
        let mut entry = index as u32;
        let mut bit = 0;
        while bit < 8 {
            entry = if entry & 1 == 1 {
                (entry >> 1) ^ POLYNOMIAL
            } else {
                entry >> 1
            };
            bit += 1;
        }
        table[index] = entry;
        index += 1;
    }

    table
}

/// CRC-32 standard sull'intervallo indicato.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = (crc >> 8) ^ TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize];
    }
    crc ^ 0xFFFF_FFFF
}

/// CRC-32 calcolato dopo aver invertito i byte di ogni parola da 32 bit.
///
/// È la variante usata dal blocco DWC di `rksys.dat`, che il gioco scrive in
/// little-endian dentro un file altrimenti big-endian.
///
/// Restituisce `None` se la lunghezza non è multipla di 4.
pub fn crc32_reversed_words(data: &[u8]) -> Option<u32> {
    if data.len() % 4 != 0 {
        return None;
    }

    let mut swapped = Vec::with_capacity(data.len());
    for word in data.chunks_exact(4) {
        swapped.extend_from_slice(&[word[3], word[2], word[1], word[0]]);
    }

    Some(crc32(&swapped))
}

/// Variante di CRC-32 con cui un `rksys.dat` è stato firmato.
///
/// Il launcher legacy prova tutte e quattro e usa quella che coincide con il
/// valore già memorizzato, invece di darne per scontata una: un salvataggio
/// prodotto da uno strumento di terze parti può usarne un'altra, e riscriverlo
/// con la variante sbagliata lo renderebbe illeggibile al gioco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Crc32Mode {
    /// Riflesso, init `0xFFFFFFFF`, xor finale `0xFFFFFFFF`: lo standard.
    #[default]
    Reflected,
    /// Riflesso, init e xor finale a zero.
    ReflectedPlain,
    /// Non riflesso (polinomio `0x04C11DB7`), init e xor a `0xFFFFFFFF`.
    Normal,
    /// Non riflesso, init e xor a zero.
    NormalPlain,
}

/// Le quattro varianti, nell'ordine in cui il legacy le prova.
pub const CRC32_MODES: [Crc32Mode; 4] = [
    Crc32Mode::Reflected,
    Crc32Mode::ReflectedPlain,
    Crc32Mode::Normal,
    Crc32Mode::NormalPlain,
];

/// CRC-32 nella variante indicata.
pub fn crc32_with(mode: Crc32Mode, data: &[u8]) -> u32 {
    match mode {
        Crc32Mode::Reflected => reflected(data, 0xFFFF_FFFF, 0xFFFF_FFFF),
        Crc32Mode::ReflectedPlain => reflected(data, 0, 0),
        Crc32Mode::Normal => normal(data, 0xFFFF_FFFF, 0xFFFF_FFFF),
        Crc32Mode::NormalPlain => normal(data, 0, 0),
    }
}

fn reflected(data: &[u8], initial: u32, xor_out: u32) -> u32 {
    let mut crc = initial;
    for byte in data {
        crc = (crc >> 8) ^ TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize];
    }
    crc ^ xor_out
}

fn normal(data: &[u8], initial: u32, xor_out: u32) -> u32 {
    let mut crc = initial;
    for byte in data {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc ^ xor_out
}

/// CRC-16/CCITT-FALSE con init a zero, il checksum di `RFL_DB.dat`.
///
/// Porta `MkwiiSaveParserService.ComputeCrc16Ccitt`: polinomio `0x1021`,
/// nessuna riflessione, nessuno xor finale.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_known_check_value() {
        // Vettore di riferimento CRC-32/ISO-HDLC.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn is_stable_for_the_same_input() {
        let data = vec![0xABu8; 1024];
        assert_eq!(crc32(&data), crc32(&data));
    }

    #[test]
    fn detects_a_single_flipped_bit() {
        let mut data = vec![0u8; 64];
        let before = crc32(&data);
        data[13] ^= 0x01;
        assert_ne!(before, crc32(&data));
    }

    #[test]
    fn reversed_words_swaps_each_group_of_four() {
        let data = [0x01u8, 0x02, 0x03, 0x04];
        assert_eq!(
            crc32_reversed_words(&data),
            Some(crc32(&[0x04, 0x03, 0x02, 0x01]))
        );
    }

    #[test]
    fn reversed_words_requires_a_multiple_of_four() {
        assert_eq!(crc32_reversed_words(&[1, 2, 3]), None);
        assert!(crc32_reversed_words(&[1, 2, 3, 4, 5, 6, 7, 8]).is_some());
    }

    #[test]
    fn the_reflected_mode_matches_the_plain_function() {
        let data = vec![0x5Au8; 512];
        assert_eq!(crc32_with(Crc32Mode::Reflected, &data), crc32(&data));
    }

    #[test]
    fn the_four_modes_disagree() {
        let data = b"123456789";
        let values: Vec<u32> = CRC32_MODES
            .iter()
            .map(|mode| crc32_with(*mode, data))
            .collect();

        // Riflessa con init e xor a uno è il CRC-32 standard; non riflessa con
        // gli stessi parametri è il CRC-32/BZIP2.
        assert_eq!(values[0], 0xCBF4_3926);
        assert_eq!(values[2], 0xFC89_1918);

        let unique: std::collections::BTreeSet<u32> = values.iter().copied().collect();
        assert_eq!(unique.len(), 4, "due varianti coincidono: {values:?}");
    }

    #[test]
    fn crc16_matches_the_known_check_value() {
        // CRC-16/XMODEM (init 0, poly 0x1021) su "123456789".
        assert_eq!(crc16_ccitt(b"123456789"), 0x31C3);
        assert_eq!(crc16_ccitt(b""), 0);
        assert_ne!(crc16_ccitt(b"a"), crc16_ccitt(b"b"));
    }

    #[test]
    fn reversed_words_differs_from_the_plain_variant() {
        let data = [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
        assert_ne!(crc32_reversed_words(&data).unwrap(), crc32(&data));
    }
}
