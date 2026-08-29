//! `rksys.dat`, il salvataggio di Mario Kart Wii.
//!
//! Porta `MkwiiSaveParserService.cs` e `RksysManager.cs`. Il file è big-endian;
//! ogni offset qui sotto proviene dal reverse engineering del launcher legacy
//! ed è documentato perché non esiste una specifica.
//!
//! **Le scritture modificano il buffer, non lo ricostruiscono.** Di questo file
//! si conoscono le licenze, la lista amici e il CRC globale; tutto il resto —
//! record dei tempi, impostazioni, il blocco DWC — è opaco e va lasciato dov'è.
//! È anche ciò che rende possibile il test che le autorizza: rileggere e
//! riscrivere un salvataggio reale senza modifiche restituisce gli stessi byte,
//! bit per bit (vedi `docs/decisions.md` §D-012).

use std::path::{Path, PathBuf};

use crate::crc::{self, Crc32Mode};
use crate::error::{SaveError, SaveResult};
use crate::mii;

/// Firma del file di salvataggio.
pub const RKSYS_MAGIC: &[u8; 8] = b"RKSD0006";
/// Firma di un blocco licenza.
pub const RKPD_MAGIC: &[u8; 4] = b"RKPD";
/// Dimensione di un blocco licenza.
pub const RKPD_SIZE: usize = 0x8CC0;
/// Numero di licenze in un salvataggio.
pub const MAX_LICENSE_SLOTS: usize = 4;
/// Offset del CRC globale.
pub const GLOBAL_CRC_OFFSET: usize = 0x27FFC;
/// Offset e lunghezza del blocco DWC, protetto dal CRC a parole invertite.
pub const DWC_OFFSET: usize = 0x40;
pub const DWC_LENGTH: usize = 0x3C;

/// Lista amici: 30 slot da `0x1C0` byte dentro ogni licenza.
pub const FRIEND_MAIN_OFFSET: usize = 0x56D0;
pub const FRIEND_STRIDE: usize = 0x1C0;
/// Tabella secondaria degli amici, 12 byte per slot.
pub const FRIEND_SECONDARY_OFFSET: usize = 0x8B50;
pub const FRIEND_SECONDARY_STRIDE: usize = 0x0C;
/// Numero di amici che una licenza può contenere.
pub const FRIEND_SLOTS: usize = 30;

// Offset dentro uno slot amico.
const FRIEND_KEY: usize = 0x00;
const FRIEND_PROFILE_ID: usize = 0x04;
const FRIEND_STATE: usize = 0x10;
const FRIEND_LOSSES: usize = 0x12;
const FRIEND_WINS: usize = 0x14;
const FRIEND_RACE_RATING: usize = 0x16;
const FRIEND_BATTLE_RATING: usize = 0x18;
const FRIEND_MII_BLOCK: usize = 0x1A;
const FRIEND_ROSTER_INDEX: usize = 0x66;
const FRIEND_COUNTRY: usize = 0x68;
const FRIEND_REGION: usize = 0x69;

/// Stato "richiesta inviata, non ancora confermata" nella tabella secondaria.
const FRIEND_PENDING_CONTROL: u8 = 0x18;
/// VR e BR di partenza di un amico appena aggiunto, come nel legacy.
const DEFAULT_RATING: u16 = 5000;

// Offset dentro un blocco RKPD.
const LICENSE_NAME_OFFSET: usize = 0x14;
const LICENSE_MII_ID_OFFSET: usize = 0x28;
const LICENSE_PROFILE_ID_OFFSET: usize = 0x5C;
const LICENSE_VR_OFFSET: usize = 0xB0;
const LICENSE_BR_OFFSET: usize = 0xB2;
const LICENSE_RACES_OFFSET: usize = 0xB4;
const LICENSE_WINS_OFFSET: usize = 0xDC;
/// I 4 byte di system id del Mii, copiati dal blocco insieme al Mii id.
const LICENSE_MII_SYSTEM_ID_OFFSET: usize = 0x2C;
/// Copia completa del blocco Mii da 74 byte dentro la licenza.
const LICENSE_MII_BLOCK_OFFSET: usize = 0x5680;
/// Lunghezza in byte del nome di una licenza: 10 caratteri UTF-16BE.
const LICENSE_NAME_BYTES: usize = 20;

/// Una licenza di Mario Kart Wii.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LicenseCard {
    pub slot: usize,
    pub is_empty: bool,
    pub name: String,
    pub mii_id: u32,
    pub profile_id: u32,
    pub friend_code: String,
    pub vr: u16,
    pub br: u16,
    pub races: u32,
    pub wins: u32,
}

impl LicenseCard {
    fn empty(slot: usize) -> Self {
        Self {
            slot,
            is_empty: true,
            name: "Vuota".into(),
            ..Default::default()
        }
    }

    /// Percentuale di vittorie, 0.0–1.0.
    pub fn win_rate(&self) -> f64 {
        if self.races == 0 {
            0.0
        } else {
            f64::from(self.wins) / f64::from(self.races)
        }
    }
}

/// Legge le quattro licenze di un `rksys.dat`.
///
/// Un file senza la firma corretta restituisce una lista vuota, non un errore:
/// il launcher legacy si comporta così e la UI mostra semplicemente "nessuna
/// licenza".
pub fn read_license_cards(data: &[u8]) -> Vec<LicenseCard> {
    read_license_cards_with(data, &|mii_id| mii_id != 0)
}

/// Come [`read_license_cards`], ma sapendo quali Mii esistono davvero in
/// `RFL_DB.dat`.
///
/// Serve a riprodurre la condizione di slot vuoto del legacy
/// (`MkwiiSaveParserService.ReadLicenseCards`): una licenza senza nome e senza
/// profile ID conta come vuota **solo se** il suo Mii non è nel database di
/// Dolphin. Un Mii id rimasto in un salvataggio ripulito non basta a far
/// sembrare occupato uno slot che il gioco mostra libero.
pub fn read_license_cards_with(data: &[u8], mii_known: &dyn Fn(u32) -> bool) -> Vec<LicenseCard> {
    if !has_rksys_magic(data) {
        return Vec::new();
    }

    (0..MAX_LICENSE_SLOTS)
        .map(|slot| read_license_card(data, slot, mii_known))
        .collect()
}

fn read_license_card(data: &[u8], slot: usize, mii_known: &dyn Fn(u32) -> bool) -> LicenseCard {
    let base = RKSYS_MAGIC.len() + slot * RKPD_SIZE;
    if base + RKPD_MAGIC.len() >= data.len() {
        return LicenseCard::empty(slot);
    }
    if &data[base..base + RKPD_MAGIC.len()] != RKPD_MAGIC {
        return LicenseCard::empty(slot);
    }

    let name = read_utf16(data, base + LICENSE_NAME_OFFSET, 20);
    let mii_id = read_u32(data, base + LICENSE_MII_ID_OFFSET);
    let profile_id = read_u32(data, base + LICENSE_PROFILE_ID_OFFSET);

    if name.trim().is_empty() && !mii_known(mii_id) && profile_id == 0 {
        return LicenseCard::empty(slot);
    }

    LicenseCard {
        slot,
        is_empty: false,
        name: if name.trim().is_empty() {
            format!("License {}", slot + 1)
        } else {
            name
        },
        mii_id,
        profile_id,
        friend_code: if profile_id == 0 {
            String::new()
        } else {
            crate::friend_code::format(profile_id)
        },
        vr: read_u16(data, base + LICENSE_VR_OFFSET),
        br: read_u16(data, base + LICENSE_BR_OFFSET),
        races: read_u32(data, base + LICENSE_RACES_OFFSET),
        wins: read_u32(data, base + LICENSE_WINS_OFFSET),
    }
}

/// Un amico salvato dentro una licenza.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveFriend {
    pub slot: usize,
    pub profile_id: u32,
    pub friend_code: String,
    pub mii_name: String,
    /// Payload di render del Mii dell'amico, vuoto se il blocco non è leggibile.
    pub studio_data: String,
    pub wins: u16,
    pub losses: u16,
    pub race_rating: u16,
    pub battle_rating: u16,
    pub country_id: u8,
    pub region_id: u8,
    /// Richiesta inviata dal launcher e non ancora confermata dal server.
    pub is_pending: bool,
}

/// Base di una licenza, se lo slot esiste ed è popolato.
fn license_base(data: &[u8], license: usize) -> Option<usize> {
    if license >= MAX_LICENSE_SLOTS || !has_rksys_magic(data) {
        return None;
    }

    let base = RKSYS_MAGIC.len() + license * RKPD_SIZE;
    (base + RKPD_SIZE <= data.len() && &data[base..base + RKPD_MAGIC.len()] == RKPD_MAGIC)
        .then_some(base)
}

/// Legge la lista amici di una licenza.
///
/// Uno slot conta come vuoto solo se **tutti** i campi noti sono a zero: il
/// legacy usa la stessa condizione, perché un salvataggio può contenere resti
/// in slot il cui profile ID è già stato azzerato.
pub fn read_friends(data: &[u8], license: usize) -> Vec<SaveFriend> {
    let Some(base) = license_base(data, license) else {
        return Vec::new();
    };

    let main_base = base + FRIEND_MAIN_OFFSET;
    let secondary_base = base + FRIEND_SECONDARY_OFFSET;
    let mut out = Vec::new();

    for slot in 0..FRIEND_SLOTS {
        let pointer = main_base + slot * FRIEND_STRIDE;
        let secondary = secondary_base + slot * FRIEND_SECONDARY_STRIDE;
        if pointer + FRIEND_STRIDE > data.len() || secondary + FRIEND_SECONDARY_STRIDE > data.len()
        {
            break;
        }

        let profile_id = read_u32(data, pointer + FRIEND_PROFILE_ID);
        let state = read_u16(data, pointer + FRIEND_STATE);
        let control = data[secondary + 0x02];

        let empty = profile_id == 0
            && state & 0x03 == 0
            && control == 0
            && read_u32(data, pointer + FRIEND_KEY) == 0
            && read_u32(data, pointer + 0x08) == 0
            && read_u32(data, pointer + 0x0C) == 0
            && read_u16(data, pointer + FRIEND_LOSSES) == 0
            && read_u16(data, pointer + FRIEND_WINS) == 0
            && read_u16(data, pointer + FRIEND_RACE_RATING) == 0
            && read_u16(data, pointer + FRIEND_BATTLE_RATING) == 0
            && data[pointer + FRIEND_COUNTRY] == 0
            && data[pointer + FRIEND_REGION] == 0
            && read_u16(data, pointer + 0x6C) == 0
            && read_u16(data, pointer + 0x6E) == 0;

        if empty {
            continue;
        }

        let block = &data[pointer + FRIEND_MII_BLOCK..pointer + FRIEND_MII_BLOCK + mii::BLOCK_SIZE];
        let parsed = mii::parse_block(block).ok();
        let mii_name = parsed
            .as_ref()
            .map(|parsed| parsed.name.clone())
            .unwrap_or_else(|| "Mii".to_string());
        // La faccia dell'amico si disegna dagli stessi 74 byte che il gioco ha
        // salvato: nessuna richiesta al server, nessun dato in più.
        let studio_data = parsed
            .as_ref()
            .map(|parsed| mii::studio_data(&parsed.raw))
            .unwrap_or_default();

        out.push(SaveFriend {
            slot,
            profile_id,
            friend_code: if profile_id == 0 {
                String::new()
            } else {
                crate::friend_code::format(profile_id)
            },
            mii_name,
            studio_data,
            wins: read_u16(data, pointer + FRIEND_WINS),
            losses: read_u16(data, pointer + FRIEND_LOSSES),
            race_rating: read_u16(data, pointer + FRIEND_RACE_RATING),
            battle_rating: read_u16(data, pointer + FRIEND_BATTLE_RATING),
            country_id: data[pointer + FRIEND_COUNTRY],
            region_id: data[pointer + FRIEND_REGION],
            is_pending: control == FRIEND_PENDING_CONTROL || state & 0x03 == 0x01,
        });
    }

    out
}

/// Variante di CRC-32 con cui il salvataggio è stato firmato.
///
/// `None` quando nessuna delle quattro coincide: in quel caso il file **non
/// va riscritto**, perché non si sa con cosa rifirmarlo.
pub fn detect_crc_mode(data: &[u8]) -> Option<Crc32Mode> {
    if data.len() < GLOBAL_CRC_OFFSET + 4 {
        return None;
    }

    let stored = read_u32(data, GLOBAL_CRC_OFFSET);
    crc::CRC32_MODES
        .into_iter()
        .find(|mode| crc::crc32_with(*mode, &data[..GLOBAL_CRC_OFFSET]) == stored)
}

/// Ricalcola e riscrive il CRC globale nella variante indicata.
pub fn write_global_crc(data: &mut [u8], mode: Crc32Mode) -> SaveResult<()> {
    if data.len() < GLOBAL_CRC_OFFSET + 4 {
        return Err(SaveError::InvalidSave(
            "the save file is too short to hold the global CRC".into(),
        ));
    }

    let computed = crc::crc32_with(mode, &data[..GLOBAL_CRC_OFFSET]);
    data[GLOBAL_CRC_OFFSET..GLOBAL_CRC_OFFSET + 4].copy_from_slice(&computed.to_be_bytes());
    Ok(())
}

/// Rifiuta un buffer che non è un salvataggio riscrivibile.
fn writable_license(data: &[u8], license: usize) -> SaveResult<(usize, Crc32Mode)> {
    if !has_rksys_magic(data) {
        return Err(SaveError::InvalidSave(
            "the selected file is not a Mario Kart Wii save".into(),
        ));
    }

    let mode = detect_crc_mode(data).ok_or_else(|| {
        SaveError::InvalidSave(
            "the save checksum cannot be verified: the file was not changed".into(),
        )
    })?;

    let base = license_base(data, license)
        .ok_or_else(|| SaveError::InvalidSave("the selected licence is empty or invalid".into()))?;

    Ok((base, mode))
}

/// Aggiunge un amico alla prima posizione libera, restituendone lo slot.
///
/// L'amico viene scritto come **richiesta in uscita**, non come amicizia
/// confermata: un launcher può solo dichiarare il friend code, sono il gioco e
/// il server a completare la voce. Marcarla subito come confermata lascia il
/// registro DWC in uno stato incoerente e può rompere l'accesso online.
///
/// Divergenza dal legacy: un profile ID già presente viene rifiutato invece di
/// creare una seconda voce con lo stesso codice.
pub fn add_friend(data: &mut [u8], license: usize, profile_id: u32) -> SaveResult<usize> {
    if profile_id == 0 {
        return Err(SaveError::InvalidFriendCode(
            "the friend code is not valid".into(),
        ));
    }

    let (base, mode) = writable_license(data, license)?;

    if read_friends(data, license)
        .iter()
        .any(|friend| friend.profile_id == profile_id)
    {
        return Err(SaveError::InvalidFriendCode(
            "this friend is already on the list".into(),
        ));
    }

    let main_base = base + FRIEND_MAIN_OFFSET;
    let secondary_base = base + FRIEND_SECONDARY_OFFSET;

    let slot = (0..FRIEND_SLOTS)
        .find(|slot| read_u32(data, main_base + slot * FRIEND_STRIDE + FRIEND_PROFILE_ID) == 0)
        .ok_or_else(|| {
            SaveError::InvalidSave("the friend list is full (30 friends at most)".into())
        })?;

    let pointer = main_base + slot * FRIEND_STRIDE;
    let secondary = secondary_base + slot * FRIEND_SECONDARY_STRIDE;

    // Si riparte da uno slot pulito: alcuni salvataggi conservano resti in slot
    // il cui profile ID è già zero, e tenerli produce voci online malformate.
    data[pointer..pointer + FRIEND_STRIDE].fill(0);
    data[secondary..secondary + FRIEND_SECONDARY_STRIDE].fill(0);

    write_u32(
        data,
        pointer + FRIEND_KEY,
        u32::from(crate::friend_code::checksum(profile_id)),
    );
    write_u32(data, pointer + FRIEND_PROFILE_ID, profile_id);
    write_u16(data, pointer + FRIEND_STATE, 0x0001);
    write_u16(data, pointer + FRIEND_LOSSES, 0);
    write_u16(data, pointer + FRIEND_WINS, 0);
    write_u16(data, pointer + FRIEND_RACE_RATING, DEFAULT_RATING);
    write_u16(data, pointer + FRIEND_BATTLE_RATING, DEFAULT_RATING);
    data[pointer + FRIEND_ROSTER_INDEX] = (slot + 1) as u8;

    data[secondary + 0x02] = FRIEND_PENDING_CONTROL;
    write_u32(data, secondary + 0x04, profile_id);
    write_u32(data, secondary + 0x08, profile_id);

    // Il CRC del blocco DWC **non** si riscrive: aggiungere un amico non tocca
    // quel blocco, e rifirmarlo con la variante sbagliata rompe l'online
    // lasciando la licenza apparentemente integra.
    write_global_crc(data, mode)?;
    Ok(slot)
}

/// Rimuove l'amico in uno slot, azzerando entrambe le tabelle.
pub fn remove_friend(data: &mut [u8], license: usize, slot: usize) -> SaveResult<()> {
    if slot >= FRIEND_SLOTS {
        return Err(SaveError::InvalidSave(format!(
            "slot amico fuori intervallo: {slot}"
        )));
    }

    let (base, mode) = writable_license(data, license)?;

    let pointer = base + FRIEND_MAIN_OFFSET + slot * FRIEND_STRIDE;
    let secondary = base + FRIEND_SECONDARY_OFFSET + slot * FRIEND_SECONDARY_STRIDE;

    data[pointer..pointer + FRIEND_STRIDE].fill(0);
    data[secondary..secondary + FRIEND_SECONDARY_STRIDE].fill(0);

    write_global_crc(data, mode)?;
    Ok(())
}

/// Assegna un Mii a una licenza.
///
/// Porta `MkwiiSaveParserService.UpdateLicenseMiiAsync`. Quattro scritture, e
/// nient'altro:
///
/// 1. il **nome** della licenza a `0x14`, 10 caratteri UTF-16BE;
/// 2. il **Mii id** a `0x28`, che è la chiave con cui il gioco cerca il Mii in
///    `RFL_DB.dat`;
/// 3. i 4 byte di **system id** a `0x2C`, presi dal blocco (`0x1C`): identificano
///    la console che ha creato il Mii, e il gioco li confronta con il Mii id;
/// 4. la **copia del blocco** a `0x5680`, che è quella che il gioco disegna
///    quando il database non è raggiungibile.
///
/// Il resto della licenza è opaco e resta dov'è; il CRC globale viene rifirmato
/// con la stessa variante che il salvataggio già usava.
pub fn update_license_mii(
    data: &mut [u8],
    license: usize,
    name: &str,
    mii_id: u32,
    block: &[u8],
) -> SaveResult<()> {
    if block.len() != mii::BLOCK_SIZE {
        return Err(SaveError::InvalidMii(format!(
            "a Wii Mii block must be {} bytes, got {}",
            mii::BLOCK_SIZE,
            block.len()
        )));
    }

    let (base, mode) = writable_license(data, license)?;

    // Il Mii id del profilo ha la precedenza; se manca si usa quello scritto
    // dentro il blocco, come nel legacy.
    let mii_id = if mii_id != 0 {
        mii_id
    } else {
        read_u32(block, 0x18)
    };
    if mii_id == 0 {
        return Err(SaveError::InvalidMii(
            "the selected Mii has no valid identifier".into(),
        ));
    }

    write_license_name(data, base + LICENSE_NAME_OFFSET, name);
    write_u32(data, base + LICENSE_MII_ID_OFFSET, mii_id);

    let system_id = base + LICENSE_MII_SYSTEM_ID_OFFSET;
    data[system_id..system_id + 4].copy_from_slice(&block[0x1C..0x20]);

    let copy = base + LICENSE_MII_BLOCK_OFFSET;
    data[copy..copy + mii::BLOCK_SIZE].copy_from_slice(block);

    write_global_crc(data, mode)?;
    Ok(())
}

/// Scrive il nome di una licenza: 20 byte UTF-16BE, azzerati prima.
///
/// Porta `MkwiiSaveParserService.WriteMiiString`, fallback `"Mii"` compreso.
fn write_license_name(data: &mut [u8], offset: usize, value: &str) {
    if offset + LICENSE_NAME_BYTES > data.len() {
        return;
    }

    data[offset..offset + LICENSE_NAME_BYTES].fill(0);

    let name = mii::normalize_name(value, "Mii");
    let mut written = 0;
    for unit in name.encode_utf16() {
        if written + 2 > LICENSE_NAME_BYTES {
            break;
        }
        data[offset + written..offset + written + 2].copy_from_slice(&unit.to_be_bytes());
        written += 2;
    }
}

/// `true` se il buffer inizia con la firma `RKSD0006`.
pub fn has_rksys_magic(data: &[u8]) -> bool {
    data.len() >= RKSYS_MAGIC.len() && &data[..RKSYS_MAGIC.len()] == RKSYS_MAGIC
}

/// Verifica il CRC globale del salvataggio.
///
/// Restituisce `Ok(())` se coincide, altrimenti descrive lo scarto.
pub fn verify_global_crc(data: &[u8]) -> SaveResult<()> {
    if data.len() < GLOBAL_CRC_OFFSET + 4 {
        return Err(SaveError::InvalidSave(
            "the file is too short to hold the global CRC".into(),
        ));
    }

    let stored = read_u32(data, GLOBAL_CRC_OFFSET);
    let computed = crc::crc32(&data[..GLOBAL_CRC_OFFSET]);

    if stored == computed {
        Ok(())
    } else {
        Err(SaveError::ChecksumMismatch {
            expected: stored,
            actual: computed,
        })
    }
}

/// CRC del blocco DWC, nella variante a parole invertite.
pub fn dwc_crc(data: &[u8]) -> Option<u32> {
    if data.len() < DWC_OFFSET + DWC_LENGTH {
        return None;
    }
    crc::crc32_reversed_words(&data[DWC_OFFSET..DWC_OFFSET + DWC_LENGTH])
}

/// Radice dei salvataggi Wii dentro una cartella User di Dolphin.
pub fn wii_root(user_folder: &Path) -> std::path::PathBuf {
    user_folder.join("Wii")
}

/// Nome del file di salvataggio di Mario Kart Wii.
pub const SAVE_FILE_NAME: &str = "rksys.dat";

/// Profondità massima della ricerca ricorsiva, per non perdersi in una
/// cartella User enorme.
const MAX_SEARCH_DEPTH: usize = 12;

/// Cerca tutti i `rksys.dat` dentro una cartella User di Dolphin.
///
/// Porta `MkwiiSaveParserService.FindMarioKartSaveFiles`, ricerca ricorsiva
/// compresa. Guardare solo nella NAND (`Wii/title/00010004/<game id>/data/`)
/// **non basta**: il patch Riivolution `<savegame external=...>` della modpack
/// reindirizza il salvataggio sotto `Load/Riivolution/<Mod>/...`, ed è quello il
/// file che il gioco usa davvero quando "Seperate Savegame" è attivo. Cercare
/// nel posto sbagliato è ciò che faceva sparire le licenze.
///
/// L'ordine delle radici è quello del legacy; l'intera cartella User viene
/// scandita solo se nessuna delle tre ha dato risultati.
pub fn find_save_files(user_folder: &Path) -> Vec<PathBuf> {
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Vec::new();
    }

    let mut found = Vec::new();
    let roots = [
        wii_root(user_folder).join("title"),
        user_folder.join("Load").join("Riivolution"),
        wii_root(user_folder),
    ];

    for root in roots.iter().filter(|root| root.is_dir()) {
        collect_save_files(root, &mut found);
    }

    if found.is_empty() {
        collect_save_files(user_folder, &mut found);
    }

    // `Wii/title` sta dentro `Wii`: lo stesso file arriva due volte.
    found.sort();
    found.dedup();
    found
}

/// Aggiunge a `out` ogni `rksys.dat` sotto `root`.
fn collect_save_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut pending = vec![(root.to_path_buf(), 0usize)];

    while let Some((directory, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            // `file_type` non segue i link simbolici: una directory che punta
            // a un suo antenato non diventa un ciclo.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();

            if kind.is_dir() {
                if depth < MAX_SEARCH_DEPTH {
                    pending.push((path, depth + 1));
                }
            } else if path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(SAVE_FILE_NAME))
            {
                out.push(path);
            }
        }
    }
}

/// `true` se il salvataggio appartiene alla modpack indicata.
///
/// Porta `MkwiiSaveParserService.IsModpackSavePath`: vale il percorso sotto la
/// radice della modpack, oppure qualunque percorso che attraversi una cartella
/// che si chiama come la modpack.
pub fn is_modpack_save_path(path: &Path, mod_root: &Path, mod_directory_name: &str) -> bool {
    if !mod_root.as_os_str().is_empty() && path.starts_with(mod_root) {
        return true;
    }

    let mut components: Vec<_> = path.components().collect();
    components.pop(); // il nome del file non conta
    components.iter().any(|component| {
        component
            .as_os_str()
            .eq_ignore_ascii_case(mod_directory_name)
    })
}

/// Il salvataggio della modpack: il più recente fra quelli che le appartengono.
///
/// Porta `MkwiiSaveParserService.FindVanzaKartSaveFiles`, che ne tiene **uno
/// solo**. Le licenze mostrate dal launcher sono quelle con cui si gioca, non
/// quelle della NAND vaniglia.
pub fn find_mod_save_files(
    user_folder: &Path,
    mod_root: &Path,
    mod_directory_name: &str,
) -> Vec<PathBuf> {
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = find_save_files(user_folder)
        .into_iter()
        .filter(|path| is_modpack_save_path(path, mod_root, mod_directory_name))
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();

    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    candidates
        .into_iter()
        .take(1)
        .map(|(_, path)| path)
        .collect()
}

/// Game id ricavato dal percorso di un `rksys.dat`, per esempio `RMCP`.
///
/// Porta `MkwiiSaveParserService.GetGameIdFromPath`, che legge il nome della
/// cartella contenitrice: nel layout Riivolution della modpack è già il game
/// id (`.../save/VanzaWFC2/RMCP/rksys.dat`).
///
/// Divergenza dal legacy: se quel nome non è un game id si guarda anche la
/// cartella sopra. Nella NAND il file sta in `<title id>/data/rksys.dat`, e il
/// legacy si ferma su `data` restituendolo come game id — da cui l'etichetta
/// "Region a" al posto della regione vera.
pub fn game_id_from_path(path: &Path) -> Option<String> {
    let parent = path.parent()?;

    game_id_from_directory_name(parent).or_else(|| game_id_from_directory_name(parent.parent()?))
}

/// Game id scritto nel nome di una directory, in chiaro o in esadecimale.
fn game_id_from_directory_name(directory: &Path) -> Option<String> {
    let name = directory.file_name()?.to_string_lossy().to_string();

    // Title id in esadecimale ASCII: `524d4350` -> `RMCP`.
    if name.len() >= 8 && name.chars().all(|character| character.is_ascii_hexdigit()) {
        let bytes: Vec<u8> = (0..name.len() / 2)
            .filter_map(|index| u8::from_str_radix(&name[index * 2..index * 2 + 2], 16).ok())
            .collect();
        if bytes.len() >= 4 {
            if let Ok(decoded) = String::from_utf8(bytes[..4].to_vec()) {
                if looks_like_game_id(&decoded) {
                    return Some(decoded);
                }
            }
        }
    }

    let head: String = name.chars().take(4).collect();
    looks_like_game_id(&head).then_some(head)
}

/// Un game id Wii: quattro caratteri ASCII, lettere maiuscole o cifre.
fn looks_like_game_id(value: &str) -> bool {
    value.len() == 4
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// Regione leggibile a partire dall'ultima lettera del game id.
///
/// Porta `MkwiiSaveParserService.BuildRegionLabel`, etichette comprese.
pub fn region_label(game_id: &str) -> String {
    if game_id.chars().count() < 4 {
        return "Mario Kart Wii".into();
    }

    match game_id.chars().nth(3) {
        Some('P') => "PAL (Europe)".into(),
        Some('E') => "NTSC-U (USA)".into(),
        Some('J') => "NTSC-J (Japan)".into(),
        Some('K') => "NTSC-K (Korea)".into(),
        Some(other) => format!("Region {other}"),
        None => "Mario Kart Wii".into(),
    }
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn write_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn read_utf16(data: &[u8], offset: usize, byte_length: usize) -> String {
    if offset + byte_length > data.len() {
        return String::new();
    }

    let units: Vec<u16> = data[offset..offset + byte_length]
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();

    String::from_utf16_lossy(&units).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Costruisce un `rksys.dat` sintetico con `filled` licenze popolate.
    fn build_save(filled: usize) -> Vec<u8> {
        let mut data = vec![0u8; GLOBAL_CRC_OFFSET + 4];
        data[..8].copy_from_slice(RKSYS_MAGIC);

        for slot in 0..filled {
            let base = RKSYS_MAGIC.len() + slot * RKPD_SIZE;
            data[base..base + 4].copy_from_slice(RKPD_MAGIC);

            let name = format!("Pilota{}", slot + 1);
            for (index, unit) in name.encode_utf16().enumerate() {
                let offset = base + LICENSE_NAME_OFFSET + index * 2;
                data[offset..offset + 2].copy_from_slice(&unit.to_be_bytes());
            }

            let write_u32 = |data: &mut Vec<u8>, offset: usize, value: u32| {
                data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
            };
            let write_u16 = |data: &mut Vec<u8>, offset: usize, value: u16| {
                data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            };

            write_u32(&mut data, base + LICENSE_MII_ID_OFFSET, 1000 + slot as u32);
            write_u32(
                &mut data,
                base + LICENSE_PROFILE_ID_OFFSET,
                100_000 + slot as u32,
            );
            write_u16(&mut data, base + LICENSE_VR_OFFSET, 5000 + slot as u16);
            write_u16(&mut data, base + LICENSE_BR_OFFSET, 3000 + slot as u16);
            write_u32(&mut data, base + LICENSE_RACES_OFFSET, 200);
            write_u32(&mut data, base + LICENSE_WINS_OFFSET, 50);
        }

        let crc = crc::crc32(&data[..GLOBAL_CRC_OFFSET]);
        data[GLOBAL_CRC_OFFSET..GLOBAL_CRC_OFFSET + 4].copy_from_slice(&crc.to_be_bytes());
        data
    }

    #[test]
    fn reads_four_slots_from_a_valid_save() {
        let cards = read_license_cards(&build_save(2));

        assert_eq!(cards.len(), 4);
        assert!(!cards[0].is_empty);
        assert_eq!(cards[0].name, "Pilota1");
        assert_eq!(cards[0].mii_id, 1000);
        assert_eq!(cards[0].profile_id, 100_000);
        assert_eq!(cards[0].vr, 5000);
        assert_eq!(cards[0].br, 3000);
        assert_eq!(cards[0].races, 200);
        assert_eq!(cards[0].wins, 50);
        assert!((cards[0].win_rate() - 0.25).abs() < 1e-9);

        assert!(!cards[1].is_empty);
        assert!(cards[2].is_empty);
        assert!(cards[3].is_empty);
    }

    #[test]
    fn the_friend_code_is_derived_from_the_profile_id() {
        let cards = read_license_cards(&build_save(1));
        assert_eq!(cards[0].friend_code, crate::friend_code::format(100_000));
        assert_eq!(
            crate::friend_code::parse(&cards[0].friend_code).unwrap(),
            100_000
        );
        assert!(cards[1].friend_code.is_empty());
    }

    #[test]
    fn a_file_without_the_magic_yields_nothing() {
        assert!(read_license_cards(&[0u8; 1024]).is_empty());
        assert!(read_license_cards(b"RKSD0005").is_empty());
        assert!(read_license_cards(&[]).is_empty());
        assert!(!has_rksys_magic(b"NOPE"));
    }

    #[test]
    fn a_truncated_file_does_not_panic() {
        let mut data = build_save(4);
        data.truncate(RKSYS_MAGIC.len() + RKPD_SIZE + 100);

        let cards = read_license_cards(&data);
        assert_eq!(cards.len(), 4);
        assert!(!cards[0].is_empty);
        assert!(cards[3].is_empty);
    }

    #[test]
    fn the_global_crc_is_verified() {
        let data = build_save(1);
        verify_global_crc(&data).unwrap();

        let mut corrupted = data.clone();
        corrupted[100] ^= 0xFF;
        assert!(matches!(
            verify_global_crc(&corrupted),
            Err(SaveError::ChecksumMismatch { .. })
        ));

        assert!(verify_global_crc(&[0u8; 10]).is_err());
    }

    #[test]
    fn the_dwc_crc_is_computed_over_the_right_window() {
        let data = build_save(1);
        assert_eq!(
            dwc_crc(&data),
            crc::crc32_reversed_words(&data[DWC_OFFSET..DWC_OFFSET + DWC_LENGTH])
        );
        assert_eq!(dwc_crc(&[0u8; 8]), None);
    }

    #[test]
    fn dolphin_paths_match_the_expected_layout() {
        let user = Path::new("/home/a/Dolphin Emulator");
        assert_eq!(wii_root(user), Path::new("/home/a/Dolphin Emulator/Wii"));
    }

    #[test]
    fn the_game_id_is_read_from_the_path() {
        // Layout Riivolution della modpack: la cartella è già il game id.
        assert_eq!(
            game_id_from_path(Path::new(
                "/W/Load/Riivolution/VanzaKart/riivolution/save/VanzaWFC2/RMCP/rksys.dat"
            ))
            .as_deref(),
            Some("RMCP")
        );
        // NAND: `524d4350` = "RMCP", una cartella sopra `data`.
        assert_eq!(
            game_id_from_path(Path::new("/W/title/00010004/524d4350/data/rksys.dat")).as_deref(),
            Some("RMCP")
        );
        assert_eq!(
            game_id_from_path(Path::new("/W/title/00010004/524d4350/rksys.dat")).as_deref(),
            Some("RMCP")
        );
        assert_eq!(game_id_from_path(Path::new("rksys.dat")), None);
        assert_eq!(
            game_id_from_path(Path::new("/W/save/qualsiasi/rksys.dat")),
            None
        );
    }

    #[test]
    fn regions_are_labelled() {
        assert_eq!(region_label("RMCP"), "PAL (Europe)");
        assert_eq!(region_label("RMCE"), "NTSC-U (USA)");
        assert_eq!(region_label("RMCJ"), "NTSC-J (Japan)");
        assert_eq!(region_label("RMCK"), "NTSC-K (Korea)");
        assert_eq!(region_label("RMCX"), "Region X");
        assert_eq!(region_label("XX"), "Mario Kart Wii");
    }

    #[test]
    fn a_modpack_save_is_told_apart_from_the_nand_one() {
        let mod_root = Path::new("/W/Load/Riivolution/VanzaKart");
        let modpack =
            Path::new("/W/Load/Riivolution/VanzaKart/riivolution/save/VanzaWFC2/RMCP/rksys.dat");
        let nand = Path::new("/W/Wii/title/00010004/524d4350/data/rksys.dat");

        assert!(is_modpack_save_path(modpack, mod_root, "VanzaKart"));
        assert!(!is_modpack_save_path(nand, mod_root, "VanzaKart"));
        assert!(!is_modpack_save_path(
            modpack,
            Path::new("/W/Load/Riivolution/VKBeta"),
            "VKBeta"
        ));

        // Anche fuori dalla radice, se il percorso attraversa la modpack.
        assert!(is_modpack_save_path(
            Path::new("/altrove/VanzaKart/save/RMCP/rksys.dat"),
            Path::new(""),
            "VanzaKart"
        ));
    }

    // -----------------------------------------------------------------
    // Fixture reale
    //
    // `fixtures/rksys.dat` è un salvataggio vero, anonimizzato da
    // `fixtures/anonymize.py`: nomi, friend code e identificativi di console
    // sostituiti, struttura e CRC intatti. È l'unica verifica che conti — un
    // salvataggio sintetico dimostra solo che il codice è coerente con sé
    // stesso.
    // -----------------------------------------------------------------

    const FIXTURE: &[u8] = include_bytes!("../fixtures/rksys.dat");

    /// Licenza con la lista amici piena, e una con posto libero.
    const FULL_LICENSE: usize = 0;
    const SPARE_LICENSE: usize = 1;

    #[test]
    fn the_fixture_is_a_real_save() {
        assert_eq!(FIXTURE.len(), GLOBAL_CRC_OFFSET + 4);
        assert!(has_rksys_magic(FIXTURE));
        assert_eq!(read_license_cards(FIXTURE).len(), MAX_LICENSE_SLOTS);
    }

    #[test]
    fn the_fixture_crc_matches_our_own() {
        // Il CRC della fixture è stato scritto da `zlib`, un'implementazione
        // indipendente: se coincide con la nostra su 160 KB di dati reali,
        // l'algoritmo e la finestra sono quelli giusti.
        verify_global_crc(FIXTURE).unwrap();
        assert_eq!(detect_crc_mode(FIXTURE), Some(Crc32Mode::Reflected));
    }

    #[test]
    fn rewriting_without_changes_is_byte_identical() {
        // Il test che autorizza tutte le scritture (`docs/decisions.md`
        // §D-012): se rifirmare un salvataggio reale senza modificarlo non
        // restituisse gli stessi byte, il formato non sarebbe stato capito.
        let mut copy = FIXTURE.to_vec();
        let mode = detect_crc_mode(&copy).expect("variante di CRC riconosciuta");

        write_global_crc(&mut copy, mode).unwrap();

        assert_eq!(copy, FIXTURE);
    }

    #[test]
    fn the_fixture_licenses_are_readable() {
        let cards = read_license_cards(FIXTURE);
        let populated: Vec<&LicenseCard> = cards.iter().filter(|card| !card.is_empty).collect();

        assert_eq!(populated.len(), 4);
        for card in populated {
            assert!(!card.name.trim().is_empty());
            assert_ne!(card.profile_id, 0);
            assert_eq!(card.friend_code.len(), 14, "0000-0000-0000");
            assert_eq!(
                crate::friend_code::parse(&card.friend_code).unwrap(),
                card.profile_id
            );
        }
    }

    #[test]
    fn the_fixture_friends_are_readable() {
        let friends = read_friends(FIXTURE, FULL_LICENSE);

        assert_eq!(friends.len(), FRIEND_SLOTS, "la licenza ha la lista piena");
        for friend in &friends {
            assert_ne!(friend.profile_id, 0);
            assert_eq!(
                crate::friend_code::parse(&friend.friend_code).unwrap(),
                friend.profile_id
            );
        }

        assert!(read_friends(FIXTURE, SPARE_LICENSE).is_empty());
    }

    #[test]
    fn adding_then_removing_a_friend_restores_the_original_bytes() {
        let mut data = FIXTURE.to_vec();

        let slot = add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).unwrap();
        assert_eq!(slot, 0, "la prima posizione libera");
        assert_ne!(data, FIXTURE);
        verify_global_crc(&data).unwrap();

        remove_friend(&mut data, SPARE_LICENSE, slot).unwrap();

        assert_eq!(data, FIXTURE, "la rimozione non ha ripulito tutto");
    }

    #[test]
    fn an_added_friend_reads_back_as_a_pending_request() {
        let mut data = FIXTURE.to_vec();
        let profile_id = 0x1234_5678;

        add_friend(&mut data, SPARE_LICENSE, profile_id).unwrap();
        let friends = read_friends(&data, SPARE_LICENSE);

        assert_eq!(friends.len(), 1);
        let added = &friends[0];
        assert_eq!(added.profile_id, profile_id);
        assert_eq!(added.friend_code, crate::friend_code::format(profile_id));
        assert_eq!(added.race_rating, DEFAULT_RATING);
        assert_eq!(added.battle_rating, DEFAULT_RATING);
        assert_eq!(added.wins, 0);
        assert!(
            added.is_pending,
            "un amico aggiunto dal launcher resta una richiesta finché il server non conferma"
        );
    }

    #[test]
    fn adding_a_friend_touches_only_that_slot_and_the_crc() {
        let mut data = FIXTURE.to_vec();
        add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).unwrap();

        let base = RKSYS_MAGIC.len() + SPARE_LICENSE * RKPD_SIZE;
        let main = base + FRIEND_MAIN_OFFSET;
        let secondary = base + FRIEND_SECONDARY_OFFSET;

        let changed: Vec<usize> = (0..FIXTURE.len())
            .filter(|index| data[*index] != FIXTURE[*index])
            .collect();

        assert!(!changed.is_empty());
        for index in changed {
            let inside_main = (main..main + FRIEND_STRIDE).contains(&index);
            let inside_secondary =
                (secondary..secondary + FRIEND_SECONDARY_STRIDE).contains(&index);
            let inside_crc = (GLOBAL_CRC_OFFSET..GLOBAL_CRC_OFFSET + 4).contains(&index);

            assert!(
                inside_main || inside_secondary || inside_crc,
                "byte modificato fuori dallo slot amico: {index:#x}"
            );
        }
    }

    #[test]
    fn other_licenses_keep_their_friends() {
        let mut data = FIXTURE.to_vec();
        let before = read_friends(FIXTURE, FULL_LICENSE);

        add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).unwrap();

        assert_eq!(read_friends(&data, FULL_LICENSE), before);
        assert_eq!(read_license_cards(&data), read_license_cards(FIXTURE));
    }

    #[test]
    fn the_same_friend_is_not_added_twice() {
        let mut data = FIXTURE.to_vec();
        add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).unwrap();

        let after_first = data.clone();
        assert!(add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).is_err());
        assert_eq!(data, after_first, "un rifiuto non deve toccare il file");
    }

    #[test]
    fn a_full_license_refuses_new_friends() {
        let mut data = FIXTURE.to_vec();

        assert!(add_friend(&mut data, FULL_LICENSE, 0x1234_5678).is_err());
        assert_eq!(data, FIXTURE);
    }

    #[test]
    fn an_empty_license_cannot_be_written() {
        // La fixture ha quattro licenze popolate: se ne costruisce una vuota
        // azzerando la firma RKPD del terzo slot.
        let mut data = FIXTURE.to_vec();
        let base = RKSYS_MAGIC.len() + 2 * RKPD_SIZE;
        data[base..base + RKPD_MAGIC.len()].fill(0);

        let snapshot = data.clone();
        assert!(add_friend(&mut data, 2, 0x1234_5678).is_err());
        assert!(remove_friend(&mut data, 2, 0).is_err());
        assert_eq!(data, snapshot);
    }

    #[test]
    fn a_save_with_an_unknown_checksum_is_never_rewritten() {
        // Un byte cambiato senza rifirmare: nessuna delle quattro varianti
        // coincide, quindi non si sa con cosa rifirmare il file.
        let mut data = FIXTURE.to_vec();
        data[0x1000] ^= 0xFF;
        let snapshot = data.clone();

        assert_eq!(detect_crc_mode(&data), None);
        assert!(add_friend(&mut data, SPARE_LICENSE, 0x1234_5678).is_err());
        assert!(remove_friend(&mut data, SPARE_LICENSE, 0).is_err());
        assert_eq!(data, snapshot);
    }

    #[test]
    fn a_file_that_is_not_a_save_is_refused() {
        let mut data = vec![0u8; GLOBAL_CRC_OFFSET + 4];
        assert!(add_friend(&mut data, 0, 1).is_err());
        assert!(remove_friend(&mut data, 0, 0).is_err());
        assert!(read_friends(&data, 0).is_empty());
    }

    #[test]
    fn out_of_range_arguments_are_refused() {
        let mut data = FIXTURE.to_vec();

        assert!(add_friend(&mut data, MAX_LICENSE_SLOTS, 1).is_err());
        assert!(add_friend(&mut data, SPARE_LICENSE, 0).is_err());
        assert!(remove_friend(&mut data, SPARE_LICENSE, FRIEND_SLOTS).is_err());
        assert!(read_friends(&data, 99).is_empty());
        assert_eq!(data, FIXTURE);
    }

    #[test]
    fn removing_a_friend_from_a_full_license_frees_one_slot() {
        let mut data = FIXTURE.to_vec();
        let before = read_friends(&data, FULL_LICENSE);

        remove_friend(&mut data, FULL_LICENSE, 5).unwrap();
        let after = read_friends(&data, FULL_LICENSE);

        assert_eq!(after.len(), before.len() - 1);
        assert!(after.iter().all(|friend| friend.slot != 5));
        verify_global_crc(&data).unwrap();

        // Lo slot liberato è quello che il prossimo inserimento riusa.
        assert_eq!(add_friend(&mut data, FULL_LICENSE, 0x1234_5678).unwrap(), 5);
    }

    #[test]
    fn every_license_of_the_fixture_can_take_a_friend_and_give_it_back() {
        for license in 0..MAX_LICENSE_SLOTS {
            let mut data = FIXTURE.to_vec();
            if read_friends(&data, license).len() == FRIEND_SLOTS {
                continue;
            }

            let slot = add_friend(&mut data, license, 0x0BAD_C0DE).unwrap();
            verify_global_crc(&data).unwrap();
            remove_friend(&mut data, license, slot).unwrap();

            assert_eq!(data, FIXTURE, "licenza {license}");
        }
    }

    #[test]
    fn finds_save_files_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let save = dir
            .path()
            .join("Wii/title/00010004/524d4350/data/rksys.dat");
        std::fs::create_dir_all(save.parent().unwrap()).unwrap();
        std::fs::write(&save, build_save(1)).unwrap();

        let found = find_save_files(dir.path());
        assert_eq!(found, vec![save]);
        assert!(find_save_files(Path::new("/percorso/inesistente")).is_empty());
    }

    /// La regressione che faceva sparire le licenze: il salvataggio con cui si
    /// gioca sta sotto `Load/Riivolution`, non nella NAND.
    #[test]
    fn finds_the_riivolution_save_of_the_modpack() {
        let dir = tempfile::tempdir().unwrap();
        let modpack = dir
            .path()
            .join("Load/Riivolution/VanzaKart/riivolution/save/VanzaWFC2/RMCP/rksys.dat");
        let nand = dir
            .path()
            .join("Wii/title/00010004/524d4350/data/rksys.dat");

        for path in [&modpack, &nand] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, build_save(1)).unwrap();
        }

        let found = find_save_files(dir.path());
        assert!(found.contains(&modpack), "trovati: {found:?}");
        assert!(found.contains(&nand));
        assert_eq!(found.len(), 2, "nessun duplicato: {found:?}");

        let mod_root = dir.path().join("Load/Riivolution/VanzaKart");
        assert_eq!(
            find_mod_save_files(dir.path(), &mod_root, "VanzaKart"),
            vec![modpack]
        );
        assert!(find_mod_save_files(
            dir.path(),
            &dir.path().join("Load/Riivolution/VKBeta"),
            "VKBeta"
        )
        .is_empty());
    }

    #[test]
    fn assigning_a_mii_writes_the_four_fields_and_resigns_the_save() {
        let mut data = FIXTURE.to_vec();
        let block = crate::mii::build_block("Vanza", 3, false);
        let mii_id = read_u32(&block, 0x18);

        update_license_mii(&mut data, SPARE_LICENSE, "Vanza", mii_id, &block).unwrap();

        let base = RKSYS_MAGIC.len() + SPARE_LICENSE * RKPD_SIZE;
        assert_eq!(read_utf16(&data, base + LICENSE_NAME_OFFSET, 20), "Vanza");
        assert_eq!(read_u32(&data, base + LICENSE_MII_ID_OFFSET), mii_id);
        assert_eq!(
            &data[base + LICENSE_MII_SYSTEM_ID_OFFSET..base + LICENSE_MII_SYSTEM_ID_OFFSET + 4],
            &block[0x1C..0x20]
        );
        assert_eq!(
            &data[base + LICENSE_MII_BLOCK_OFFSET
                ..base + LICENSE_MII_BLOCK_OFFSET + crate::mii::BLOCK_SIZE],
            &block[..]
        );
        verify_global_crc(&data).unwrap();

        // La licenza rilegge il nome nuovo.
        let cards = read_license_cards(&data);
        assert_eq!(cards[SPARE_LICENSE].name, "Vanza");
        assert_eq!(cards[SPARE_LICENSE].mii_id, mii_id);
    }

    #[test]
    fn assigning_a_mii_leaves_the_other_licenses_alone() {
        let mut data = FIXTURE.to_vec();
        let block = crate::mii::build_block("Vanza", 3, false);
        update_license_mii(&mut data, SPARE_LICENSE, "Vanza", 0, &block).unwrap();

        let untouched = RKSYS_MAGIC.len() + FULL_LICENSE * RKPD_SIZE;
        assert_eq!(
            &data[untouched..untouched + RKPD_SIZE],
            &FIXTURE[untouched..untouched + RKPD_SIZE]
        );
    }

    #[test]
    fn a_malformed_mii_never_reaches_the_save() {
        let mut data = FIXTURE.to_vec();
        assert!(update_license_mii(&mut data, SPARE_LICENSE, "Vanza", 1, &[0u8; 10]).is_err());
        assert!(update_license_mii(
            &mut data,
            9,
            "Vanza",
            1,
            &crate::mii::build_block("A", 0, false)
        )
        .is_err());
        assert_eq!(data, FIXTURE);
    }
}
