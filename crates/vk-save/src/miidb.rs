//! `RFL_DB.dat`, il database Mii della Wii che Dolphin tiene in
//! `Wii/shared2/menu/FaceLib/`.
//!
//! Porta la parte di `MkwiiSaveParserService.cs` che legge e scrive il
//! database. Il formato:
//!
//! | Offset | Contenuto |
//! | --- | --- |
//! | `0x00` | firma `RNOD` |
//! | `0x04` | 100 blocchi Mii da 74 byte, fino a `0x1CEC` |
//! | `0x1CEC` | `0x80`, marcatore di fine elenco |
//! | `0x1D00` | firma `RNHD` seguita da `FF FF FF FF` |
//! | `0x1F1DE` | CRC-16/CCITT su tutto ciò che precede |
//!
//! **Le scritture non ricostruiscono il file**: modificano il buffer letto dal
//! disco e ricalcolano il solo CRC. Tutto quello che sta fra `0x1D00` e il CRC
//! — un'area di cui non si conosce il significato — resta esattamente com'era.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::crc;
use crate::error::{SaveError, SaveResult};
use crate::mii::{self, WiiMii};

/// Dimensione utile del database. Dolphin alloca il file NAND più grande e
/// riempie il resto di zeri.
pub const DB_SIZE: usize = 0x1F1E0;
/// Firma del database.
pub const MAGIC: &[u8; 4] = b"RNOD";
/// Offset del primo blocco Mii.
pub const FIRST_BLOCK_OFFSET: usize = 0x04;
/// Numero di Mii che il database può contenere.
pub const SLOTS: usize = 100;
/// Offset del CRC-16 finale.
pub const CRC_OFFSET: usize = 0x1F1DE;

const END_MARKER_OFFSET: usize = 0x1CEC;
const HEADER_OFFSET: usize = 0x1D00;
const HEADER_MAGIC: &[u8; 4] = b"RNHD";

/// Percorso del database dentro una cartella User di Dolphin.
pub fn database_path(user_folder: &Path) -> PathBuf {
    user_folder
        .join("Wii")
        .join("shared2")
        .join("menu")
        .join("FaceLib")
        .join("RFL_DB.dat")
}

/// Legge il database, indicizzato per Mii id.
///
/// I blocchi non decodificabili vengono saltati: il database contiene slot
/// vuoti e, dopo una migrazione, anche dati residui.
pub fn read(data: &[u8]) -> BTreeMap<u32, WiiMii> {
    let mut out = BTreeMap::new();

    for index in 0..SLOTS {
        let offset = FIRST_BLOCK_OFFSET + index * mii::BLOCK_SIZE;
        if offset + mii::BLOCK_SIZE > data.len() {
            break;
        }

        if let Ok(parsed) = mii::parse_block(&data[offset..offset + mii::BLOCK_SIZE]) {
            if parsed.mii_id != 0 {
                out.entry(parsed.mii_id).or_insert(parsed);
            }
        }
    }

    out
}

/// Verifica il CRC-16 memorizzato in coda.
pub fn verify_crc(data: &[u8]) -> SaveResult<()> {
    if data.len() < CRC_OFFSET + 2 {
        return Err(SaveError::InvalidSave(
            "il database Mii è troppo corto per contenere il CRC".into(),
        ));
    }

    let stored = u16::from_be_bytes([data[CRC_OFFSET], data[CRC_OFFSET + 1]]);
    let computed = crc::crc16_ccitt(&data[..CRC_OFFSET]);

    if stored == computed {
        Ok(())
    } else {
        Err(SaveError::ChecksumMismatch {
            expected: u32::from(stored),
            actual: u32::from(computed),
        })
    }
}

/// Ricalcola e riscrive il CRC-16 in coda.
pub fn write_crc(data: &mut [u8]) -> SaveResult<()> {
    if data.len() < CRC_OFFSET + 2 {
        return Err(SaveError::InvalidSave(
            "il database Mii è troppo corto per contenere il CRC".into(),
        ));
    }

    let computed = crc::crc16_ccitt(&data[..CRC_OFFSET]);
    data[CRC_OFFSET..CRC_OFFSET + 2].copy_from_slice(&computed.to_be_bytes());
    Ok(())
}

/// Database vuoto ma valido, come `CreateEmptyMiiDatabase`.
///
/// Serve quando Dolphin non ha mai aperto il Canale Mii: senza il file, il
/// gioco non trova alcun Mii da associare alla licenza.
pub fn create_empty() -> Vec<u8> {
    let mut data = vec![0u8; DB_SIZE];
    data[..MAGIC.len()].copy_from_slice(MAGIC);
    data[END_MARKER_OFFSET] = 0x80;
    data[HEADER_OFFSET..HEADER_OFFSET + HEADER_MAGIC.len()].copy_from_slice(HEADER_MAGIC);
    data[HEADER_OFFSET + 4..HEADER_OFFSET + 8].fill(0xFF);

    let _ = write_crc(&mut data);
    data
}

/// Offset dello slot che contiene un dato Mii id.
pub fn find_slot(data: &[u8], mii_id: u32) -> Option<usize> {
    if mii_id == 0 {
        return None;
    }

    (0..SLOTS)
        .map(|index| FIRST_BLOCK_OFFSET + index * mii::BLOCK_SIZE)
        .find(|offset| {
            offset + mii::BLOCK_SIZE <= data.len()
                && u32::from_be_bytes([
                    data[offset + 0x18],
                    data[offset + 0x19],
                    data[offset + 0x1A],
                    data[offset + 0x1B],
                ]) == mii_id
        })
}

/// Offset del primo slot libero.
///
/// Uno slot è libero quando è tutto a `0x00` o tutto a `0xFF`, esattamente
/// come in `FindEmptyMiiSlotOffset`.
pub fn find_empty_slot(data: &[u8]) -> Option<usize> {
    (0..SLOTS)
        .map(|index| FIRST_BLOCK_OFFSET + index * mii::BLOCK_SIZE)
        .find(|offset| {
            offset + mii::BLOCK_SIZE <= data.len() && {
                let block = &data[*offset..offset + mii::BLOCK_SIZE];
                block.iter().all(|byte| *byte == 0x00) || block.iter().all(|byte| *byte == 0xFF)
            }
        })
}

/// Inserisce o aggiorna un Mii, restituendo l'offset dello slot usato.
///
/// Se un Mii con lo stesso Mii id esiste già viene sostituito sul posto: è
/// così che una modifica nell'editor si riflette sul Mii che il gioco mostra,
/// invece di crearne un secondo identico.
pub fn upsert(data: &mut [u8], block: &[u8]) -> SaveResult<usize> {
    if block.len() != mii::BLOCK_SIZE {
        return Err(SaveError::InvalidMii(format!(
            "un blocco Mii Wii deve essere di {} byte, ricevuti {}",
            mii::BLOCK_SIZE,
            block.len()
        )));
    }
    if data.len() < DB_SIZE {
        return Err(SaveError::InvalidSave(
            "il database Mii non ha la dimensione attesa".into(),
        ));
    }

    let mii_id = u32::from_be_bytes([block[0x18], block[0x19], block[0x1A], block[0x1B]]);
    if mii_id == 0 {
        return Err(SaveError::InvalidMii(
            "il Mii non ha un identificativo: non può entrare nel database".into(),
        ));
    }

    let offset = find_slot(data, mii_id)
        .or_else(|| find_empty_slot(data))
        .ok_or_else(|| {
            SaveError::InvalidSave("il database Mii di Dolphin è pieno (100 Mii)".into())
        })?;

    data[offset..offset + mii::BLOCK_SIZE].copy_from_slice(block);
    write_crc(data)?;
    Ok(offset)
}

/// Rimuove un Mii dal database. `false` se non c'era.
pub fn remove(data: &mut [u8], mii_id: u32) -> SaveResult<bool> {
    if data.len() < DB_SIZE {
        return Err(SaveError::InvalidSave(
            "il database Mii non ha la dimensione attesa".into(),
        ));
    }

    let Some(offset) = find_slot(data, mii_id) else {
        return Ok(false);
    };

    data[offset..offset + mii::BLOCK_SIZE].fill(0);
    write_crc(data)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Il `RFL_DB.dat` reale e anonimizzato, con 22 Mii.
    const FIXTURE: &[u8] = include_bytes!("../fixtures/RFL_DB.dat");

    #[test]
    fn the_fixture_is_a_real_database() {
        assert_eq!(FIXTURE.len(), DB_SIZE);
        assert_eq!(&FIXTURE[..4], MAGIC);
        assert_eq!(&FIXTURE[HEADER_OFFSET..HEADER_OFFSET + 4], HEADER_MAGIC);
        assert_eq!(FIXTURE[END_MARKER_OFFSET], 0x80);
    }

    #[test]
    fn the_fixture_crc_matches_our_own() {
        // Il CRC della fixture è stato scritto da un'implementazione
        // indipendente (`fixtures/anonymize.py`): se coincide con la nostra su
        // 127 KB di dati reali, l'algoritmo è quello giusto.
        verify_crc(FIXTURE).unwrap();
    }

    #[test]
    fn the_fixture_contains_readable_miis() {
        let database = read(FIXTURE);

        // 22 slot pieni ma 21 Mii id distinti: due slot del file reale
        // condividono lo stesso id. Succede quando un Mii viene copiato da una
        // console all'altra, e l'elenco è indicizzato per id, quindi ne resta
        // uno solo. È il comportamento del launcher legacy, conservato.
        assert_eq!(filled_slots(FIXTURE), 22);
        assert_eq!(database.len(), 21);
        assert!(database.values().all(|mii| !mii.name.trim().is_empty()));
        assert!(database.keys().all(|id| *id != 0));
    }

    fn filled_slots(data: &[u8]) -> usize {
        (0..SLOTS)
            .map(|index| FIRST_BLOCK_OFFSET + index * mii::BLOCK_SIZE)
            .filter(|offset| {
                data[*offset..offset + mii::BLOCK_SIZE]
                    .iter()
                    .any(|b| *b != 0)
            })
            .count()
    }

    #[test]
    fn rewriting_without_changes_is_byte_identical() {
        // Il test di round-trip: se riscrivere un database reale senza
        // modificarlo non restituisse gli stessi byte, il formato non sarebbe
        // stato capito e nessuna scrittura sarebbe sicura.
        let mut copy = FIXTURE.to_vec();
        write_crc(&mut copy).unwrap();
        assert_eq!(copy, FIXTURE);
    }

    #[test]
    fn an_existing_mii_is_replaced_in_place() {
        let mut data = FIXTURE.to_vec();
        let before = read(&data);
        let (id, existing) = before.iter().next().unwrap();

        let offset = find_slot(&data, *id).expect("slot del Mii esistente");
        let mut block = existing.raw.clone();
        mii::set_name(&mut block, "Nuovo").unwrap();

        assert_eq!(upsert(&mut data, &block).unwrap(), offset);

        let after = read(&data);
        assert_eq!(after.len(), before.len(), "nessuno slot in più");
        assert_eq!(after[id].name, "Nuovo");
        verify_crc(&data).unwrap();
    }

    #[test]
    fn a_new_mii_takes_the_first_free_slot() {
        let mut data = FIXTURE.to_vec();
        let before = read(&data);
        let expected = find_empty_slot(&data).expect("uno slot libero");

        let block = mii::build_block("Aggiunto", 3, false);
        let mii_id = u32::from_be_bytes([block[0x18], block[0x19], block[0x1A], block[0x1B]]);

        assert_eq!(upsert(&mut data, &block).unwrap(), expected);

        let after = read(&data);
        assert_eq!(after.len(), before.len() + 1);
        assert_eq!(after[&mii_id].name, "Aggiunto");
        verify_crc(&data).unwrap();
    }

    #[test]
    fn adding_then_removing_restores_the_original_bytes() {
        let mut data = FIXTURE.to_vec();
        let block = mii::build_block("Temporaneo", 1, false);
        let mii_id = u32::from_be_bytes([block[0x18], block[0x19], block[0x1A], block[0x1B]]);

        upsert(&mut data, &block).unwrap();
        assert_ne!(data, FIXTURE);

        assert!(remove(&mut data, mii_id).unwrap());
        assert_eq!(data, FIXTURE, "la rimozione non ha ripulito tutto");
    }

    #[test]
    fn removing_an_absent_mii_changes_nothing() {
        let mut data = FIXTURE.to_vec();
        assert!(!remove(&mut data, 0x7FFF_FFFE).unwrap());
        assert!(!remove(&mut data, 0).unwrap());
        assert_eq!(data, FIXTURE);
    }

    #[test]
    fn a_block_without_an_identity_is_refused() {
        let mut data = FIXTURE.to_vec();
        let mut block = mii::build_block("Anonimo", 0, false);
        block[0x18..0x1C].fill(0);

        assert!(matches!(
            upsert(&mut data, &block),
            Err(SaveError::InvalidMii(_))
        ));
        assert_eq!(data, FIXTURE, "un rifiuto non deve toccare il database");
    }

    #[test]
    fn wrong_sizes_are_refused() {
        let mut data = FIXTURE.to_vec();
        assert!(upsert(&mut data, &[0u8; 10]).is_err());

        let mut short = vec![0u8; 128];
        assert!(upsert(&mut short, &mii::build_block("X", 0, false)).is_err());
        assert!(remove(&mut short, 1).is_err());
        assert!(verify_crc(&short).is_err());
        assert!(write_crc(&mut short).is_err());
    }

    #[test]
    fn a_full_database_refuses_new_miis() {
        let mut data = create_empty();
        for index in 0..SLOTS {
            let block = mii::build_block(&format!("Mii{index:02}"), 0, false);
            upsert(&mut data, &block).unwrap();
        }

        assert!(find_empty_slot(&data).is_none());
        assert!(upsert(&mut data, &mii::build_block("Troppo", 0, false)).is_err());
        assert_eq!(read(&data).len(), SLOTS);
    }

    #[test]
    fn an_empty_database_is_valid_and_readable() {
        let data = create_empty();

        assert_eq!(data.len(), DB_SIZE);
        verify_crc(&data).unwrap();
        assert!(read(&data).is_empty());
        assert_eq!(find_empty_slot(&data), Some(FIRST_BLOCK_OFFSET));
    }

    #[test]
    fn a_corrupted_database_fails_verification() {
        let mut data = FIXTURE.to_vec();
        data[1000] ^= 0xFF;

        assert!(matches!(
            verify_crc(&data),
            Err(SaveError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn dolphin_paths_match_the_expected_layout() {
        assert_eq!(
            database_path(Path::new("/home/a/Dolphin Emulator")),
            Path::new("/home/a/Dolphin Emulator/Wii/shared2/menu/FaceLib/RFL_DB.dat")
        );
    }

    #[test]
    fn a_truncated_buffer_does_not_panic() {
        assert!(read(&[]).is_empty());
        assert!(read(&[0u8; 100]).is_empty());
        assert!(find_slot(&[0u8; 10], 1).is_none());
        assert!(find_empty_slot(&[]).is_none());
    }
}
