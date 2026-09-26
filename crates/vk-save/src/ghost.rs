//! Ghost del time trial di Pulsar.
//!
//! Pulsar tiene i ghost nella NAND di Dolphin, una cartella per pista:
//!
//! ```text
//! <User>/Wii/shared2/Pulsar/<Modpack>/Ghosts/<crc32>/150/3m31s006.rkg
//! ```
//!
//! Tre dettagli che contano, tutti verificati su un'installazione vera:
//!
//! - `shared2` è minuscolo e `Ghosts` è al plurale. Su Windows non farebbe
//!   differenza, su Linux e macOS una cartella scritta con le maiuscole
//!   sbagliate è un'altra cartella, che il gioco non guarda mai.
//! - `<Modpack>` non è il nome della cartella Riivolution: è quello scritto
//!   nell'intestazione di `Config.pul` (`/VanzaKart`, `/VKBeta`), e cambia con
//!   il canale.
//! - `<crc32>` è il CRC della variante **principale** della pista, in
//!   esadecimale minuscolo su otto cifre. La modpack spedisce la tabella nome →
//!   CRC in `Ghosts/FolderToTrackName.txt`.
//!
//! I nomi dei file nella NAND non possono superare i 12 caratteri: Pulsar
//! scrive `3m31s006.rkg`, che li occupa tutti, e quando due tempi coincidono
//! sostituisce l'ultima cifra con una lettera (`1m44s85A.rkg`). Qui si fa lo
//! stesso: un nome più lungo il gioco non riuscirebbe ad aprirlo.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{SaveError, SaveResult};

/// Cartella dei ghost dentro la cartella Pulsar della modpack.
pub const GHOSTS_DIR: &str = "Ghosts";

/// Tabella nome → CRC spedita con la modpack, in `<Mod>/<Mod>/Ghosts/`.
pub const TRACK_MAP_FILE: &str = "FolderToTrackName.txt";

/// Intestazione di un ghost.
pub const RKG_MAGIC: &[u8; 4] = b"RKGD";

/// Lunghezza dell'intestazione di un `.rkg`, prima degli input.
pub const RKG_HEADER_LEN: usize = 0x88;

/// Oltre questa dimensione non è un ghost: gli input compressi di una gara
/// intera stanno in pochi KB, e anche non compressi restano sotto i 10 KB.
pub const RKG_MAX_LEN: usize = 64 * 1024;

/// Lunghezza massima di un nome di file nella NAND della Wii.
pub const NAND_NAME_LIMIT: usize = 12;

// ---------------------------------------------------------------------------
// Config.pul
// ---------------------------------------------------------------------------

/// Nome della cartella Pulsar della modpack, dall'intestazione di `Config.pul`.
///
/// L'intestazione è `PULS`, versione, tre offset e poi il nome della cartella
/// preceduto da `/`, lungo al massimo quanto un nome della NAND. `None` se il
/// file non è un `Config.pul` o il nome non è un nome di cartella sicuro: un
/// nome con `..` o con un separatore porterebbe la scrittura fuori da
/// `Pulsar/`.
pub fn pulsar_folder_name(config: &[u8]) -> Option<String> {
    const NAME_OFFSET: usize = 0x14;
    const NAME_FIELD: usize = 16;

    if config.len() < NAME_OFFSET + 1 || &config[..4] != b"PULS" {
        return None;
    }

    let end = (NAME_OFFSET + NAME_FIELD).min(config.len());
    let field = &config[NAME_OFFSET..end];
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    let name = std::str::from_utf8(&field[..length]).ok()?;
    let name = name.trim().trim_start_matches('/').trim();

    let safe = !name.is_empty()
        && name.len() <= NAND_NAME_LIMIT
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_ ".contains(character));
    safe.then(|| name.to_string())
}

// ---------------------------------------------------------------------------
// FolderToTrackName.txt
// ---------------------------------------------------------------------------

/// Id del server della prima pista custom: le piste custom di Pulsar partono
/// da 0x100, dopo le 32 originali e le arene.
pub const FIRST_CUSTOM_COURSE_ID: u32 = 0x100;

/// Lunghezza minima di un nome perché valga come "contenuto" in un altro:
/// sotto, una parola comune troverebbe corrispondenze ovunque.
const MIN_CONTAINED_NAME: usize = 6;

/// Tabella nome della pista → CRC della variante principale.
#[derive(Debug, Clone, Default)]
pub struct TrackCrcMap {
    by_name: HashMap<String, u32>,
    /// Le piste nell'ordine del file, che è quello di `Config.pul`.
    ordered: Vec<(String, u32)>,
}

impl TrackCrcMap {
    /// Legge `FolderToTrackName.txt`: una pista per riga, `Nome = CRC`.
    ///
    /// Le righe che non si leggono si saltano: una riga rovinata non deve
    /// togliere tutte le altre piste. Se due nomi coincidono dopo la
    /// normalizzazione vale il primo, che è quello nell'ordine della modpack.
    pub fn parse(text: &str) -> Self {
        let mut by_name = HashMap::new();
        let mut ordered = Vec::new();

        for line in text.trim_start_matches('\u{feff}').lines() {
            let Some((name, crc)) = line.rsplit_once('=') else {
                continue;
            };
            let crc = crc.trim().trim_start_matches("0x").trim_start_matches("0X");
            let Ok(crc) = u32::from_str_radix(crc, 16) else {
                continue;
            };
            let key = normalize_track_name(name);
            if !key.is_empty() {
                by_name.entry(key.clone()).or_insert(crc);
                ordered.push((key, crc));
            }
        }

        Self { by_name, ordered }
    }

    /// CRC di una pista, dal nome con cui la chiama il server.
    pub fn crc_for(&self, track_name: &str) -> Option<u32> {
        self.by_name.get(&normalize_track_name(track_name)).copied()
    }

    /// CRC di una pista del server, dal nome e dal suo id.
    ///
    /// Prima il nome. Poi, per le poche piste che la modpack chiama con un
    /// nome più corto di quello del server — `Promise of Annalise` contro
    /// `Lilac Chronicles Part 4: Promise of Annalise` — una seconda prova
    /// che vale solo se **due** indizi indipendenti concordano: la pista che
    /// sta alla posizione indicata dall'id del server, e un nome contenuto
    /// nell'altro. Uno solo dei due metterebbe il ghost nella cartella di
    /// un'altra pista, perché in fondo alla lista gli id del server non
    /// seguono più l'ordine della modpack (§D-087).
    pub fn crc_for_track(&self, track_name: &str, course_id: u32) -> Option<u32> {
        if let Some(crc) = self.crc_for(track_name) {
            return Some(crc);
        }

        let wanted = normalize_track_name(track_name);
        let index = usize::try_from(course_id.checked_sub(FIRST_CUSTOM_COURSE_ID)?).ok()?;
        let (local, crc) = self.ordered.get(index)?;

        let shorter = local.chars().count().min(wanted.chars().count());
        let related = wanted.contains(local.as_str()) || local.contains(wanted.as_str());
        (related && shorter >= MIN_CONTAINED_NAME).then_some(*crc)
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// Nome di una pista ridotto alla forma che si può confrontare.
///
/// Il server e la modpack scrivono gli stessi nomi in modo diverso: la
/// modpack con i codici colore del gioco (`\c{yor5}SW2 \c{off}Crown City`) e
/// gli apostrofi tipografici (`µTorrent’s`), il server senza codici e a volte
/// con uno spazio al posto dell'apostrofo (`µTorrent s`). Tolti i codici,
/// resta il confronto delle sole lettere e cifre, senza maiuscole.
pub fn normalize_track_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut characters = name.chars().peekable();

    while let Some(character) = characters.next() {
        // Codice di formattazione BMG: `\` + lettera + `{…}`.
        if character == '\\' {
            let mut lookahead = characters.clone();
            if lookahead.next().is_some_and(char::is_alphabetic) && lookahead.next() == Some('{') {
                characters.next();
                characters.next();
                for skipped in characters.by_ref() {
                    if skipped == '}' {
                        break;
                    }
                }
                continue;
            }
        }

        if character.is_alphanumeric() {
            out.extend(character.to_lowercase());
        }
    }

    out
}

/// Nome della cartella di una pista: il CRC in esadecimale minuscolo, otto
/// cifre, come lo scrive Pulsar.
pub fn crc_folder_name(crc: u32) -> String {
    format!("{crc:08x}")
}

/// Cartella in cui Pulsar cerca i ghost di una pista a una cilindrata.
pub fn track_ghost_dir(user_folder: &Path, pulsar_folder: &str, crc: u32, cc: u16) -> PathBuf {
    pulsar_ghosts_root(user_folder, pulsar_folder)
        .join(crc_folder_name(crc))
        .join(cc.to_string())
}

/// Radice dei ghost della modpack nella NAND di Dolphin.
pub fn pulsar_ghosts_root(user_folder: &Path, pulsar_folder: &str) -> PathBuf {
    user_folder
        .join("Wii")
        .join("shared2")
        .join("Pulsar")
        .join(pulsar_folder)
        .join(GHOSTS_DIR)
}

// ---------------------------------------------------------------------------
// .rkg
// ---------------------------------------------------------------------------

/// Ciò che serve sapere di un ghost prima di scriverlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RkgInfo {
    pub minutes: u8,
    pub seconds: u8,
    pub milliseconds: u16,
    pub lap_count: u8,
}

impl RkgInfo {
    /// Tempo finale in millisecondi.
    pub fn finish_ms(&self) -> u32 {
        u32::from(self.minutes) * 60_000
            + u32::from(self.seconds) * 1_000
            + u32::from(self.milliseconds)
    }

    /// Nome del file come lo scrive Pulsar: `3m31s006.rkg`.
    pub fn file_name(&self) -> String {
        format!(
            "{}m{:02}s{:03}.rkg",
            self.minutes, self.seconds, self.milliseconds
        )
    }
}

/// Controlla che i byte siano un ghost intero e ne legge l'intestazione.
///
/// L'API del time trial non pubblica un'impronta SHA-256 dei ghost: la
/// verifica è quella che il formato porta con sé, cioè l'intestazione `RKGD`
/// e il CRC-32 finale calcolato su tutto il resto del file. Un download
/// troncato o una pagina d'errore del server non la superano (§D-088).
pub fn parse_rkg(bytes: &[u8]) -> SaveResult<RkgInfo> {
    if bytes.len() < RKG_HEADER_LEN + 4 {
        return Err(SaveError::InvalidGhost(format!(
            "{} bytes, too short for a ghost",
            bytes.len()
        )));
    }
    if bytes.len() > RKG_MAX_LEN {
        return Err(SaveError::InvalidGhost(format!(
            "{} bytes, too large for a ghost",
            bytes.len()
        )));
    }
    if &bytes[..4] != RKG_MAGIC {
        return Err(SaveError::InvalidGhost("missing RKGD header".into()));
    }

    let (body, tail) = bytes.split_at(bytes.len() - 4);
    let expected = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
    let actual = crate::crc::crc32(body);
    if expected != actual {
        return Err(SaveError::ChecksumMismatch { expected, actual });
    }

    // 0x04–0x06: 7 bit di minuti, 7 di secondi, 10 di millesimi.
    let (b4, b5, b6) = (bytes[4], bytes[5], bytes[6]);
    let info = RkgInfo {
        minutes: b4 >> 1,
        seconds: ((b4 & 0x01) << 6) | (b5 >> 2),
        milliseconds: (u16::from(b5 & 0x03) << 8) | u16::from(b6),
        lap_count: bytes[0x10],
    };

    if info.seconds >= 60 || info.milliseconds >= 1000 {
        return Err(SaveError::InvalidGhost(format!(
            "impossible finish time {}:{:02}.{:03}",
            info.minutes, info.seconds, info.milliseconds
        )));
    }

    Ok(info)
}

/// Nome libero per un ghost dentro `directory`.
///
/// `Ok(None)` quando lo stesso identico ghost c'è già: scaricarlo di nuovo non
/// deve crearne una copia. Se il nome è preso da un ghost diverso con lo
/// stesso tempo si fa come Pulsar: l'ultima cifra diventa una lettera, così il
/// nome resta di 12 caratteri.
pub fn free_ghost_name(
    directory: &Path,
    info: &RkgInfo,
    bytes: &[u8],
) -> SaveResult<Option<String>> {
    let base = info.file_name();
    let stem = base.trim_end_matches(".rkg");

    let candidates = std::iter::once(base.clone()).chain(('A'..='Z').map(|letter| {
        let mut name: String = stem.chars().take(stem.chars().count() - 1).collect();
        name.push(letter);
        name.push_str(".rkg");
        name
    }));

    for candidate in candidates {
        let path = directory.join(&candidate);
        match std::fs::read(&path) {
            Ok(existing) if existing == bytes => return Ok(None),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(candidate));
            }
            Err(error) => return Err(SaveError::io(&path, error)),
        }
    }

    Err(SaveError::InvalidGhost(format!(
        "too many ghosts with the time {base}"
    )))
}

// ---------------------------------------------------------------------------
// Nomi di personaggi e veicoli
// ---------------------------------------------------------------------------

/// Nome di un personaggio dall'id che il gioco scrive nel ghost.
///
/// Sono i nomi inglesi del gioco, gli stessi in ogni lingua del launcher: è
/// così che i giocatori li cercano e li scrivono nelle classifiche.
pub fn character_name(id: u32) -> &'static str {
    const NAMES: [&str; 24] = [
        "Mario",
        "Baby Peach",
        "Waluigi",
        "Bowser",
        "Baby Daisy",
        "Dry Bones",
        "Baby Mario",
        "Luigi",
        "Toad",
        "Donkey Kong",
        "Yoshi",
        "Wario",
        "Baby Luigi",
        "Toadette",
        "Koopa Troopa",
        "Daisy",
        "Peach",
        "Birdo",
        "Diddy Kong",
        "King Boo",
        "Bowser Jr.",
        "Dry Bowser",
        "Funky Kong",
        "Rosalina",
    ];

    match id {
        0..=23 => NAMES[id as usize],
        // Gli abiti dei Mii: sei per taglia, più le tre taglie "base".
        24..=29 | 43 => "Mii (S)",
        30..=35 | 42 => "Mii (M)",
        36..=41 | 44 => "Mii (L)",
        45 => "Peach (Biker)",
        46 => "Daisy (Biker)",
        47 => "Rosalina (Biker)",
        _ => "",
    }
}

/// Nome di un veicolo dall'id che il gioco scrive nel ghost.
pub fn vehicle_name(id: u32) -> &'static str {
    const NAMES: [&str; 36] = [
        "Standard Kart S",
        "Standard Kart M",
        "Standard Kart L",
        "Booster Seat",
        "Classic Dragster",
        "Offroader",
        "Mini Beast",
        "Wild Wing",
        "Flame Flyer",
        "Cheep Charger",
        "Super Blooper",
        "Piranha Prowler",
        "Tiny Titan",
        "Daytripper",
        "Jetsetter",
        "Blue Falcon",
        "Sprinter",
        "Honeycoupe",
        "Standard Bike S",
        "Standard Bike M",
        "Standard Bike L",
        "Bullet Bike",
        "Mach Bike",
        "Flame Runner",
        "Bit Bike",
        "Sugarscoot",
        "Wario Bike",
        "Quacker",
        "Zip Zip",
        "Shooting Star",
        "Magikruiser",
        "Sneakster",
        "Spear",
        "Jet Bubble",
        "Dolphin Dasher",
        "Phantom",
    ];
    NAMES.get(id as usize).copied().unwrap_or("")
}

/// `true` se nella cartella c'è già un ghost identico a `bytes`.
pub fn contains_ghost(directory: &Path, bytes: &[u8]) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rkg"))
            && std::fs::read(entry.path()).is_ok_and(|existing| existing == bytes)
    })
}

/// Quanti ghost ci sono in una cartella.
pub fn count_ghosts(directory: &Path) -> usize {
    std::fs::read_dir(directory).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("rkg"))
            })
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un ghost sintetico ma valido: intestazione, qualche byte di input e il
    /// CRC finale, come li scrive il gioco.
    fn ghost(minutes: u8, seconds: u8, milliseconds: u16, filler: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; RKG_HEADER_LEN + 64];
        bytes[..4].copy_from_slice(RKG_MAGIC);
        bytes[4] = (minutes << 1) | (seconds >> 6);
        bytes[5] = ((seconds & 0x3F) << 2) | ((milliseconds >> 8) as u8 & 0x03);
        bytes[6] = (milliseconds & 0xFF) as u8;
        bytes[0x10] = 3;
        for byte in &mut bytes[RKG_HEADER_LEN..] {
            *byte = filler;
        }
        let crc = crate::crc::crc32(&bytes);
        bytes.extend_from_slice(&crc.to_be_bytes());
        bytes
    }

    fn config_with_name(name: &[u8]) -> Vec<u8> {
        let mut config = b"PULS\0\0\0\x03\0\0\0\x24\0\0\0\x74\0\0\x0a\x94".to_vec();
        let mut field = [0u8; 16];
        field[..name.len()].copy_from_slice(name);
        config.extend_from_slice(&field);
        config.extend_from_slice(b"INFO");
        config
    }

    #[test]
    fn the_pulsar_folder_comes_from_the_config_header() {
        assert_eq!(
            pulsar_folder_name(&config_with_name(b"/VanzaKart")).as_deref(),
            Some("VanzaKart")
        );
        assert_eq!(
            pulsar_folder_name(&config_with_name(b"/VKBeta")).as_deref(),
            Some("VKBeta")
        );
    }

    #[test]
    fn a_config_that_is_not_pulsar_or_unsafe_gives_no_folder() {
        assert!(pulsar_folder_name(b"NOPE").is_none());
        assert!(pulsar_folder_name(&config_with_name(b"/../evil")).is_none());
        assert!(pulsar_folder_name(&config_with_name(b"/a/b")).is_none());
        assert!(pulsar_folder_name(&config_with_name(b"/")).is_none());
        assert!(pulsar_folder_name(&config_with_name(b"")).is_none());
    }

    #[test]
    fn the_track_map_reads_the_file_shipped_with_the_modpack() {
        // Righe vere di `FolderToTrackName.txt`, compresi i codici colore e
        // l'apostrofo tipografico.
        let map = TrackCrcMap::parse(
            "\u{feff}Alpine Peak = 90538C70\r\n\
             Color Wonderland = 9C230DBE\r\n\
             \\c{yor5}SW2 \\c{off}Crown City = EEEC6803\r\n\
             µTorrent’s Divinity = D66ACE2B\r\n\
             τ-Cryovolcano = 8D97DDCF\r\n\
             Poké Floats = 605EFF4B\r\n\
             Propeller Paradise = 03B4BECD\r\n\
             riga rovinata\r\n\
             Senza CRC = ZZZ\r\n",
        );

        assert_eq!(map.len(), 7);
        assert_eq!(map.crc_for("Alpine Peak"), Some(0x9053_8C70));
        // Come le chiama il server.
        assert_eq!(map.crc_for("SW2 Crown City"), Some(0xEEEC_6803));
        assert_eq!(map.crc_for("µTorrent s Divinity"), Some(0xD66A_CE2B));
        assert_eq!(map.crc_for("τ-Cryovolcano"), Some(0x8D97_DDCF));
        assert_eq!(map.crc_for("poké floats"), Some(0x605E_FF4B));
        assert_eq!(map.crc_for("Pista che non c'è"), None);
    }

    #[test]
    fn a_shortened_name_matches_only_when_the_position_agrees() {
        // Posizioni 0–2 della modpack: id del server 256–258.
        let map = TrackCrcMap::parse(
            "Alpine Peak = 90538C70\n\
             Dreamer = 4ED9332E\n\
             Promise of Annalise = 3D7FE33E\n",
        );

        assert_eq!(
            map.crc_for_track(
                "Lilac Chronicles Paralogue 3: Dreamer (Secrets of the Waterfalls)",
                257
            ),
            Some(0x4ED9_332E)
        );
        assert_eq!(
            map.crc_for_track("Lilac Chronicles Part 4: Promise of Annalise", 258),
            Some(0x3D7F_E33E)
        );

        // Nome giusto ma posizione sbagliata: niente, meglio non installare
        // che installare nella cartella di un'altra pista.
        assert_eq!(
            map.crc_for_track("Lilac Chronicles Part 4: Promise of Annalise", 257),
            None
        );
        // Posizione giusta ma nome che non c'entra.
        assert_eq!(map.crc_for_track("Banished Courtyard", 257), None);
        // Pista che nella modpack non esiste.
        assert_eq!(map.crc_for_track("GBA Ribbon Road", 900), None);
        // Il nome esatto vince comunque, qualunque sia l'id.
        assert_eq!(map.crc_for_track("Alpine Peak", 999), Some(0x9053_8C70));
    }

    #[test]
    fn the_folder_name_is_lowercase_and_zero_padded() {
        assert_eq!(crc_folder_name(0x03B4_BECD), "03b4becd");
        assert_eq!(crc_folder_name(0x9C23_0DBE), "9c230dbe");
    }

    #[test]
    fn the_ghost_folder_uses_the_exact_case_of_the_nand() {
        let dir = track_ghost_dir(Path::new("/home/a/User"), "VanzaKart", 0x9C23_0DBE, 150);
        assert_eq!(
            dir,
            Path::new("/home/a/User/Wii/shared2/Pulsar/VanzaKart/Ghosts/9c230dbe/150")
        );
    }

    #[test]
    fn a_valid_ghost_is_read() {
        let info = parse_rkg(&ghost(3, 31, 6, 7)).unwrap();
        assert_eq!(info.minutes, 3);
        assert_eq!(info.seconds, 31);
        assert_eq!(info.milliseconds, 6);
        assert_eq!(info.finish_ms(), 211_006);
        assert_eq!(info.file_name(), "3m31s006.rkg");
        assert_eq!(info.file_name().len(), NAND_NAME_LIMIT);
    }

    #[test]
    fn a_damaged_ghost_is_refused() {
        let mut truncated = ghost(1, 57, 383, 1);
        truncated.truncate(truncated.len() - 10);
        assert!(parse_rkg(&truncated).is_err());

        let mut flipped = ghost(1, 57, 383, 1);
        flipped[RKG_HEADER_LEN + 3] ^= 0xFF;
        assert!(matches!(
            parse_rkg(&flipped),
            Err(SaveError::ChecksumMismatch { .. })
        ));

        assert!(parse_rkg(b"<html>404</html>").is_err());

        let mut wrong_magic = ghost(1, 0, 0, 0);
        wrong_magic[0] = b'X';
        assert!(parse_rkg(&wrong_magic).is_err());
    }

    #[test]
    fn the_same_ghost_is_never_written_twice() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = ghost(2, 53, 852, 5);
        let info = parse_rkg(&bytes).unwrap();

        let name = free_ghost_name(dir.path(), &info, &bytes).unwrap().unwrap();
        assert_eq!(name, "2m53s852.rkg");
        std::fs::write(dir.path().join(&name), &bytes).unwrap();

        assert_eq!(free_ghost_name(dir.path(), &info, &bytes).unwrap(), None);
        assert!(contains_ghost(dir.path(), &bytes));
        assert_eq!(count_ghosts(dir.path()), 1);
    }

    #[test]
    fn two_ghosts_with_the_same_time_keep_twelve_characters() {
        let dir = tempfile::tempdir().unwrap();
        let first = ghost(1, 44, 855, 1);
        let second = ghost(1, 44, 855, 2);
        let info = parse_rkg(&first).unwrap();

        std::fs::write(dir.path().join("1m44s855.rkg"), &first).unwrap();

        let name = free_ghost_name(dir.path(), &info, &second)
            .unwrap()
            .unwrap();
        assert_eq!(name, "1m44s85A.rkg");
        assert_eq!(name.len(), NAND_NAME_LIMIT);
    }

    #[test]
    fn characters_and_vehicles_have_their_game_names() {
        // La combinazione del ghost dello staff sul server: Bowser su Flame
        // Runner, pesante su moto pesante.
        assert_eq!(character_name(3), "Bowser");
        assert_eq!(vehicle_name(23), "Flame Runner");
        assert_eq!(character_name(23), "Rosalina");
        assert_eq!(vehicle_name(35), "Phantom");
        assert_eq!(character_name(24), "Mii (S)");
        assert_eq!(character_name(999), "");
        assert_eq!(vehicle_name(36), "");
    }

    #[test]
    fn missing_folders_count_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(count_ghosts(&dir.path().join("non-esiste")), 0);
        assert!(!contains_ghost(&dir.path().join("non-esiste"), b"x"));
    }
}
