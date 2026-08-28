//! `VKRating.pul`, il file dei punteggi di Pulsar.
//!
//! I VR e BR che la modpack usa **non** stanno in `rksys.dat`: quelli nella
//! licenza sono i valori vanilla, che il motore ripristina prima di riscrivere
//! il salvataggio proprio per non toccarlo. Il punteggio vero vive in
//! `Wii/shared2/Pulsar/<Modpack>/VKRating.pul`, indicizzato per profile ID
//! online e non per slot di licenza (§D-063).
//!
//! Formato, letto da `PulsarEngine/Network/Rating/RatingStorage.cpp` della
//! modpack. Tutto big-endian, come il Wii:
//!
//! ```text
//! intestazione   0x00 u32 magic 'RRRT'
//!                0x04 u16 versione (1, 2 o 3)
//!                0x06 u16 numero di voci
//!
//! voce v1/v2     0x00 i32 profile ID
//! (16 byte)      0x04 f32 VR          0x08 f32 BR
//!                0x0C u32 flag        bit 0 valida, bit 8-11 grado (solo v2)
//!
//! voce v3        …come sopra, e in più:
//! (32 byte)      0x10 u8  grado       0x14 u32 gare      0x18 u32 vittorie
//! ```
//!
//! I punteggi sono float pari al **VR mostrato diviso 100**: `50.0` sono 5000
//! VR. È la scala dichiarata in `RatingConfig.hpp`.

use std::path::{Path, PathBuf};

use crate::friend_code;

/// Nome del file, come in `Config::SAVE_FILE_NAME`.
pub const FILE_NAME: &str = "VKRating.pul";

const MAGIC: u32 = u32::from_be_bytes(*b"RRRT");
const HEADER_SIZE: usize = 8;
const ENTRY_SIZE_LEGACY: usize = 16;
const ENTRY_SIZE_V3: usize = 32;
const MAX_VERSION: u16 = 3;

/// Voci oltre le quali il file è considerato corrotto: `Config::MAX_PROFILES`.
const MAX_PROFILES: usize = 100;

/// Un punteggio dal file di Pulsar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rating {
    /// Profile ID online. È la chiave: due licenze non possono scambiarsi i
    /// punteggi fra loro.
    pub profile_id: u32,
    /// Friend code corrispondente, che è come lo si ritrova nel salvataggio.
    pub friend_code: String,
    /// VR come lo mostra il gioco.
    pub vr: u32,
    /// BR come lo mostra il gioco.
    pub br: u32,
    /// Grado di prestigio, `0` se non rankato.
    pub rank: u8,
    /// Gare e vittorie contate dalla modpack; `0` nei file v1 e v2.
    pub races: u32,
    pub wins: u32,
}

/// Legge tutti i punteggi validi del file.
///
/// Un file troncato, di versione sconosciuta o con un'intestazione diversa non
/// è un errore: significa solo che non ci sono punteggi da mostrare.
pub fn read_ratings(bytes: &[u8]) -> Vec<Rating> {
    if bytes.len() < HEADER_SIZE || read_u32(bytes, 0) != MAGIC {
        return Vec::new();
    }

    let version = read_u16(bytes, 4);
    if version == 0 || version > MAX_VERSION {
        return Vec::new();
    }

    let entry_size = if version >= 3 {
        ENTRY_SIZE_V3
    } else {
        ENTRY_SIZE_LEGACY
    };

    let declared = usize::from(read_u16(bytes, 6));
    let available = (bytes.len() - HEADER_SIZE) / entry_size;
    let count = declared.min(available).min(MAX_PROFILES);

    (0..count)
        .filter_map(|index| entry(bytes, HEADER_SIZE + index * entry_size, version))
        .collect()
}

fn entry(bytes: &[u8], offset: usize, version: u16) -> Option<Rating> {
    let flags = read_u32(bytes, offset + 0x0C);
    if flags & 1 == 0 {
        return None;
    }

    // Il motore accetta come chiave qualunque ID positivo: gli ID del server
    // VanzaKart stanno sopra il miliardo, e un limite superiore li scartava
    // tutti.
    let profile_id = read_i32(bytes, offset);
    if profile_id <= 0 {
        return None;
    }
    let profile_id = profile_id as u32;

    let rank = if version >= 3 {
        bytes[offset + 0x10]
    } else if version == 2 {
        ((flags >> 8) & 0x0F) as u8
    } else {
        0
    };

    Some(Rating {
        friend_code: friend_code::format(profile_id),
        profile_id,
        vr: displayed(read_f32(bytes, offset + 0x04)),
        br: displayed(read_f32(bytes, offset + 0x08)),
        rank,
        races: if version >= 3 {
            read_u32(bytes, offset + 0x14)
        } else {
            0
        },
        wins: if version >= 3 {
            read_u32(bytes, offset + 0x18)
        } else {
            0
        },
    })
}

/// Punteggio interno → punteggio mostrato. `MAX_RATING` è 5000, cioè 500000 VR.
fn displayed(rating: f32) -> u32 {
    if !rating.is_finite() || rating <= 0.0 {
        return 0;
    }
    (f64::from(rating) * 100.0).round().min(500_000.0) as u32
}

/// Cerca i `VKRating.pul` del salvataggio, i più promettenti per primi.
///
/// Pulsar scrive sempre nella NAND di Dolphin — `Wii/shared2/Pulsar/<Modpack>`
/// — anche con "Seperate Savegame" attivo, perché la redirezione di
/// Riivolution copre la cartella di salvataggio del gioco, non `shared2`. La
/// cartella della modpack si guarda comunque, per i layout portabili.
pub fn find_rating_files(
    user_folder: &Path,
    mod_root: &Path,
    mod_directory_name: &str,
) -> Vec<PathBuf> {
    let mut found = Vec::new();

    for root in [
        user_folder.join("Wii").join("shared2"),
        user_folder.join("shared2"),
    ] {
        if root.is_dir() {
            collect(&root, 5, &mut found, false);
        }
    }

    // Dentro la modpack ci sono decine di migliaia di file di gioco e questa
    // funzione gira a ogni apertura delle licenze: si scende solo lungo le
    // cartelle che possono portare a una NAND redirezionata.
    if mod_root.is_dir() {
        // Sette livelli: `riivolution/save/Wii/shared2/Pulsar/<pack>` è il
        // percorso più lungo che Riivolution produce.
        collect(mod_root, 7, &mut found, true);
    }

    found.sort();
    found.dedup();

    // Prima quello della modpack in uso: se un altro pack Pulsar sta nella
    // stessa NAND, i suoi punteggi non sono questi.
    let wanted = mod_directory_name.to_ascii_lowercase();
    found.sort_by_key(|path| {
        let mine = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            == Some(wanted.clone());
        u8::from(!mine)
    });

    found
}

/// Cartelle lungo cui può stare una NAND redirezionata da Riivolution.
const SAVE_PATH_NAMES: &[&str] = &["save", "saves", "riivolution", "wii", "shared2", "pulsar"];

/// Aggiunge a `out` ogni `VKRating.pul` sotto `root`, fino a `depth` livelli.
///
/// Con `only_save_paths` si scende solo nelle cartelle che possono portare a
/// un salvataggio: serve dentro la modpack, che è grande e non va attraversata
/// tutta a ogni apertura delle licenze.
fn collect(root: &Path, depth: usize, out: &mut Vec<PathBuf>, only_save_paths: bool) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    let inside_pulsar = root
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("Pulsar"));

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };

        if kind.is_dir() {
            // Dentro `Pulsar` ogni pack ha la sua cartella, con il proprio
            // nome: lì si scende comunque.
            let worth_it = !only_save_paths
                || inside_pulsar
                || entry
                    .file_name()
                    .to_str()
                    .map(str::to_ascii_lowercase)
                    .is_some_and(|name| SAVE_PATH_NAMES.contains(&name.as_str()));

            if depth > 0 && worth_it {
                collect(&path, depth - 1, out, only_save_paths);
            }
        } else if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(FILE_NAME))
        {
            out.push(path);
        }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    read_u32(bytes, offset) as i32
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32(bytes, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(version: u16, count: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC.to_be_bytes());
        bytes.extend_from_slice(&version.to_be_bytes());
        bytes.extend_from_slice(&count.to_be_bytes());
        bytes
    }

    fn entry_v3(profile_id: i32, vr: f32, br: f32, rank: u8, races: u32, wins: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&profile_id.to_be_bytes());
        bytes.extend_from_slice(&vr.to_be_bytes());
        bytes.extend_from_slice(&br.to_be_bytes());
        bytes.extend_from_slice(&(1u32 | (u32::from(rank) << 8)).to_be_bytes());
        bytes.push(rank);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&races.to_be_bytes());
        bytes.extend_from_slice(&wins.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes
    }

    fn entry_legacy(profile_id: i32, vr: f32, br: f32, flags: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&profile_id.to_be_bytes());
        bytes.extend_from_slice(&vr.to_be_bytes());
        bytes.extend_from_slice(&br.to_be_bytes());
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes
    }

    #[test]
    fn a_v3_file_is_read_with_the_displayed_scale() {
        let mut bytes = header(3, 2);
        bytes.extend(entry_v3(1_000_000_134, 76.5, 50.0, 3, 259, 151));
        bytes.extend(entry_v3(1_000_000_211, 50.0, 61.25, 0, 12, 3));

        let ratings = read_ratings(&bytes);
        assert_eq!(ratings.len(), 2);

        assert_eq!(ratings[0].profile_id, 1_000_000_134);
        assert_eq!(ratings[0].vr, 7650);
        assert_eq!(ratings[0].br, 5000);
        assert_eq!(ratings[0].rank, 3);
        assert_eq!(ratings[0].races, 259);
        assert_eq!(ratings[0].wins, 151);

        assert_eq!(ratings[1].vr, 5000);
        assert_eq!(ratings[1].br, 6125);
    }

    #[test]
    fn the_friend_code_comes_from_the_profile_id() {
        let mut bytes = header(3, 1);
        bytes.extend(entry_v3(1_000_000_134, 50.0, 50.0, 0, 0, 0));

        let rating = &read_ratings(&bytes)[0];
        assert_eq!(rating.friend_code, friend_code::format(1_000_000_134));
        assert_eq!(
            friend_code::parse(&rating.friend_code).unwrap(),
            rating.profile_id
        );
    }

    #[test]
    fn invalid_entries_are_skipped() {
        let mut bytes = header(3, 3);
        bytes.extend(entry_v3(1_000_000_134, 76.5, 50.0, 1, 0, 0));
        // Flag "valida" a zero.
        let mut unused = entry_v3(1_000_000_999, 90.0, 50.0, 0, 0, 0);
        unused[0x0C..0x10].copy_from_slice(&0u32.to_be_bytes());
        bytes.extend(unused);
        // Profile ID non utilizzabile.
        bytes.extend(entry_v3(0, 90.0, 50.0, 0, 0, 0));

        let ratings = read_ratings(&bytes);
        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0].profile_id, 1_000_000_134);
    }

    #[test]
    fn a_v2_file_takes_the_rank_from_the_flags() {
        let mut bytes = header(2, 1);
        bytes.extend(entry_legacy(1_000_000_134, 76.5, 50.0, 1 | (5 << 8)));

        let ratings = read_ratings(&bytes);
        assert_eq!(ratings[0].vr, 7650);
        assert_eq!(ratings[0].rank, 5);
        assert_eq!(ratings[0].races, 0);
    }

    #[test]
    fn a_v1_file_has_no_rank() {
        let mut bytes = header(1, 1);
        bytes.extend(entry_legacy(1_000_000_134, 50.0, 50.0, 1 | (5 << 8)));

        assert_eq!(read_ratings(&bytes)[0].rank, 0);
    }

    #[test]
    fn a_truncated_file_yields_what_it_can() {
        let mut bytes = header(3, 4);
        bytes.extend(entry_v3(1_000_000_134, 76.5, 50.0, 3, 0, 0));
        bytes.extend_from_slice(&[0u8; 10]);

        assert_eq!(read_ratings(&bytes).len(), 1);
    }

    #[test]
    fn a_foreign_file_is_not_read() {
        assert!(read_ratings(b"PULSARSETTINGS").is_empty());
        assert!(read_ratings(&[]).is_empty());

        let mut wrong_version = header(9, 1);
        wrong_version.extend(entry_v3(1_000_000_134, 76.5, 50.0, 0, 0, 0));
        assert!(read_ratings(&wrong_version).is_empty());
    }

    #[test]
    fn a_rating_out_of_scale_is_clamped_not_wrapped() {
        let mut bytes = header(3, 3);
        bytes.extend(entry_v3(1, f32::MAX, 50.0, 0, 0, 0));
        bytes.extend(entry_v3(2, -10.0, 50.0, 0, 0, 0));
        bytes.extend(entry_v3(3, f32::NAN, 50.0, 0, 0, 0));

        let ratings = read_ratings(&bytes);
        assert_eq!(ratings[0].vr, 500_000);
        assert_eq!(ratings[1].vr, 0);
        assert_eq!(ratings[2].vr, 0);
    }

    #[test]
    fn inside_the_modpack_only_save_paths_are_walked() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("User");
        let mod_root = root.path().join("Load/Riivolution/VanzaKart");

        let mut bytes = header(3, 1);
        bytes.extend(entry_v3(1_000_000_134, 50.0, 50.0, 0, 0, 0));

        // NAND redirezionata: si trova.
        let redirected = mod_root.join("save/Wii/shared2/Pulsar/VanzaKart");
        std::fs::create_dir_all(&redirected).unwrap();
        std::fs::write(redirected.join(FILE_NAME), &bytes).unwrap();

        // Dentro i file di gioco: non si scende nemmeno a guardare.
        let assets = mod_root.join("VanzaKart/My Stuff/Course");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join(FILE_NAME), &bytes).unwrap();

        let found = find_rating_files(&user, &mod_root, "VanzaKart");
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with(mod_root.join("save")));
    }

    #[test]
    fn the_file_of_the_running_modpack_comes_first() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path();

        for pack in ["RetroRewind6", "VanzaKart"] {
            let directory = user.join("Wii").join("shared2").join("Pulsar").join(pack);
            std::fs::create_dir_all(&directory).unwrap();

            let mut bytes = header(3, 1);
            bytes.extend(entry_v3(1_000_000_134, 50.0, 50.0, 0, 0, 0));
            std::fs::write(directory.join(FILE_NAME), bytes).unwrap();
        }

        let found = find_rating_files(user, &user.join("mai"), "VanzaKart");
        assert_eq!(found.len(), 2);
        assert!(
            found[0].ends_with("VanzaKart/VKRating.pul")
                || found[0].ends_with("VanzaKart\\VKRating.pul")
        );
    }
}
