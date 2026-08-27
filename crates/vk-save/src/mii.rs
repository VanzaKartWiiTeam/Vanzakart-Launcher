//! Blocco Mii Wii da 74 byte.
//!
//! Porta `Launcher/Services/MiiFileParserService.cs` e
//! `Launcher/Models/MiiEditorState.cs`. Il formato è big-endian con bit-field
//! impacchettati; ogni offset qui sotto è documentato perché è l'unico
//! riferimento disponibile (il formato non è pubblicato da Nintendo).
//!
//! Layout completo:
//!
//! | Offset | Dim. | Contenuto |
//! | --- | --- | --- |
//! | `0x00` | 2 | header: preferito (bit 0), colore preferito (bit 1–4), giorno (bit 5–9), mese (bit 10–13), femmina (bit 14) |
//! | `0x02` | 20 | nome, UTF-16BE, terminato da zeri |
//! | `0x16` | 1 | altezza (0–127) |
//! | `0x17` | 1 | peso (0–127) |
//! | `0x18` | 4 | Mii id |
//! | `0x1C` | 4 | id della console che ha creato il Mii |
//! | `0x20` | 2 | faccia: forma (bit 13–15), incarnato (10–12), tratti (6–9) |
//! | `0x22` | 2 | capelli: tipo (bit 9–15), colore (6–8), specchiati (5) |
//! | `0x24` | 4 | sopracciglia: tipo (27–31), rotazione (22–25), colore (13–15), dimensione (9–12), altezza (4–8), distanza (0–3) |
//! | `0x28` | 4 | occhi: tipo (26–31), rotazione (21–23), altezza (16–20), colore (13–15), dimensione (9–11), distanza (5–8) |
//! | `0x2C` | 2 | naso: tipo (12–15), dimensione (8–11), altezza (3–7) |
//! | `0x2E` | 2 | bocca: tipo (11–15), colore (9–10), dimensione (5–8), altezza (0–4) |
//! | `0x30` | 2 | occhiali: tipo (12–15), colore (9–11), dimensione (5–7), altezza (0–4) |
//! | `0x32` | 2 | barba: baffi (14–15), barba (12–13), colore (9–11), dimensione (5–8), altezza (0–4) |
//! | `0x34` | 2 | neo: presente (15), dimensione (11–14), altezza (6–10), posizione (1–5) |
//! | `0x36` | 20 | nome del creatore, UTF-16BE |

use serde::{Deserialize, Serialize};

use crate::error::{SaveError, SaveResult};

/// Dimensione esatta di un blocco Mii Wii.
pub const BLOCK_SIZE: usize = 74;

const NAME_OFFSET: usize = 0x02;
const CREATOR_OFFSET: usize = 0x36;
const NAME_BYTES: usize = 20;
/// Un nome Mii è al massimo di 10 unità UTF-16, come nel menu della Wii.
const NAME_UNITS: usize = NAME_BYTES / 2;
const HEIGHT_OFFSET: usize = 0x16;
const WEIGHT_OFFSET: usize = 0x17;
const MII_ID_OFFSET: usize = 0x18;
const SYSTEM_ID_OFFSET: usize = 0x1C;
const FACE_OFFSET: usize = 0x20;
const HAIR_OFFSET: usize = 0x22;
const BROW_OFFSET: usize = 0x24;
const EYE_OFFSET: usize = 0x28;
const NOSE_OFFSET: usize = 0x2C;
const MOUTH_OFFSET: usize = 0x2E;
const GLASSES_OFFSET: usize = 0x30;
const FACIAL_HAIR_OFFSET: usize = 0x32;
const MOLE_OFFSET: usize = 0x34;

/// Estensioni riconosciute come contenitori di Mii.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[".mii", ".miigx", ".mae", ".rcd", ".rsd"];

/// I 12 colori preferiti della Wii, negli stessi hex del launcher legacy.
pub const FAVORITE_COLORS: [&str; 12] = [
    "#FF3B3B", "#FF8A2A", "#FFD166", "#9CFF5E", "#317a11", "#3B82F6", "#8EE7FF", "#FF5CAB",
    "#A855F7", "#3d260c", "#F7FAFF", "#03010a",
];

/// Tabelle di conversione verso il formato "Mii Studio", portate 1:1 dal
/// launcher legacy: il tratto del viso della Wii diventa trucco **oppure**
/// rughe, e i due valori non coincidono.
const MAKEUP_MAP: [u8; 16] = [0, 1, 6, 9, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0];
const WRINKLES_MAP: [u8; 16] = [0, 0, 0, 0, 5, 2, 3, 7, 8, 0, 9, 11, 0, 0, 0, 0];

/// Un Mii Wii decodificato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiiMii {
    pub name: String,
    pub creator_name: String,
    pub mii_id: u32,
    pub favorite_color_index: usize,
    pub height: u8,
    pub weight: u8,
    pub is_female: bool,
    pub is_favorite: bool,
    /// Mese di creazione, 1–12.
    pub birth_month: u8,
    /// Giorno di creazione, 1–31.
    pub birth_day: u8,
    /// I 74 byte originali, così come vanno riscritti.
    pub raw: Vec<u8>,
}

impl WiiMii {
    /// Colore preferito in esadecimale.
    pub fn favorite_color(&self) -> &'static str {
        favorite_color(self.favorite_color_index)
    }

    /// Blocco codificato in base64, come lo persiste il launcher.
    pub fn to_base64(&self) -> String {
        base64_encode(&self.raw)
    }

    /// Stringa "Mii Studio", il formato che il renderer di avatar accetta.
    pub fn studio_data(&self) -> String {
        studio_data(&self.raw)
    }
}

/// Colore preferito per indice, con fallback sul primo.
pub fn favorite_color(index: usize) -> &'static str {
    FAVORITE_COLORS.get(index).copied().unwrap_or("#FF3B3B")
}

/// Indice del colore preferito a partire dal suo esadecimale.
///
/// Il confronto è case-insensitive; un colore sconosciuto restituisce `None`.
pub fn favorite_color_index(hex: &str) -> Option<usize> {
    let wanted = hex.trim();
    FAVORITE_COLORS
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(wanted))
}

// ---------------------------------------------------------------------------
// Stato dell'editor
// ---------------------------------------------------------------------------

/// Lo stato completo dell'editor Mii: tutto ciò che i 74 byte descrivono.
///
/// Porta `Launcher/Models/MiiEditorState.cs` campo per campo. I valori di
/// default sono quelli del launcher legacy, con una sola divergenza: la data
/// di creazione è `1/1` invece di "oggi", perché questo crate è puro e non
/// legge l'orologio di sistema (vedi `docs/decisions.md` §D-025). È il livello
/// servizio a impostare la data corrente quando crea un Mii nuovo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiiEditorState {
    pub name: String,
    pub creator_name: String,
    pub is_female: bool,
    pub is_favorite: bool,
    pub favorite_color_index: u8,
    pub birth_month: u8,
    pub birth_day: u8,
    pub height: u8,
    pub weight: u8,
    pub mii_id: u32,
    /// Id della console che ha creato il Mii, nei 4 byte originali.
    pub system_id: [u8; 4],

    pub face_shape: u8,
    pub skin_color: u8,
    pub facial_feature: u8,

    pub hair_type: u8,
    pub hair_color: u8,
    pub hair_flipped: bool,

    pub eyebrow_type: u8,
    pub eyebrow_rotation: u8,
    pub eyebrow_color: u8,
    pub eyebrow_size: u8,
    pub eyebrow_vertical: u8,
    pub eyebrow_spacing: u8,

    pub eye_type: u8,
    pub eye_rotation: u8,
    pub eye_vertical: u8,
    pub eye_color: u8,
    pub eye_size: u8,
    pub eye_spacing: u8,

    pub nose_type: u8,
    pub nose_size: u8,
    pub nose_vertical: u8,

    pub mouth_type: u8,
    pub mouth_color: u8,
    pub mouth_size: u8,
    pub mouth_vertical: u8,

    pub glasses_type: u8,
    pub glasses_color: u8,
    pub glasses_size: u8,
    pub glasses_vertical: u8,

    pub mustache_type: u8,
    pub beard_type: u8,
    pub facial_hair_color: u8,
    pub mustache_size: u8,
    pub mustache_vertical: u8,

    pub mole_enabled: bool,
    pub mole_size: u8,
    pub mole_vertical: u8,
    pub mole_horizontal: u8,
}

impl Default for MiiEditorState {
    fn default() -> Self {
        Self {
            name: "Vanza Mii".into(),
            creator_name: "VanzaKart".into(),
            is_female: false,
            is_favorite: true,
            favorite_color_index: 4,
            birth_month: 1,
            birth_day: 1,
            height: 64,
            weight: 64,
            mii_id: 0,
            system_id: [0; 4],

            face_shape: 0,
            skin_color: 1,
            facial_feature: 0,

            hair_type: 33,
            hair_color: 0,
            hair_flipped: false,

            eyebrow_type: 6,
            eyebrow_rotation: 6,
            eyebrow_color: 0,
            eyebrow_size: 4,
            eyebrow_vertical: 10,
            eyebrow_spacing: 2,

            eye_type: 2,
            eye_rotation: 4,
            eye_vertical: 12,
            eye_color: 0,
            eye_size: 4,
            eye_spacing: 2,

            nose_type: 1,
            nose_size: 4,
            nose_vertical: 9,

            mouth_type: 23,
            mouth_color: 0,
            mouth_size: 4,
            mouth_vertical: 13,

            glasses_type: 0,
            glasses_color: 0,
            glasses_size: 4,
            glasses_vertical: 10,

            mustache_type: 0,
            beard_type: 0,
            facial_hair_color: 0,
            mustache_size: 4,
            mustache_vertical: 10,

            mole_enabled: false,
            mole_size: 4,
            mole_vertical: 10,
            mole_horizontal: 10,
        }
    }
}

impl MiiEditorState {
    /// Copia con ogni campo riportato dentro l'intervallo valido.
    ///
    /// Gli estremi sono quelli di `NormalizeEditorState`: un valore fuori
    /// scala non viene rifiutato, viene riportato dentro l'intervallo. Il
    /// gioco non tollera indici che non esistono nel suo atlante di texture.
    #[must_use]
    pub fn normalized(&self) -> Self {
        Self {
            name: normalize_name(&self.name, "Vanza Mii"),
            creator_name: normalize_name(&self.creator_name, "VanzaKart"),
            is_female: self.is_female,
            is_favorite: self.is_favorite,
            favorite_color_index: self.favorite_color_index.min(11),
            birth_month: self.birth_month.clamp(1, 12),
            birth_day: self.birth_day.clamp(1, 31),
            height: self.height.min(127),
            weight: self.weight.min(127),
            mii_id: self.mii_id,
            system_id: self.system_id,

            face_shape: self.face_shape.min(7),
            skin_color: self.skin_color.min(5),
            facial_feature: self.facial_feature.min(11),

            hair_type: self.hair_type.min(71),
            hair_color: self.hair_color.min(7),
            hair_flipped: self.hair_flipped,

            eyebrow_type: self.eyebrow_type.min(23),
            eyebrow_rotation: self.eyebrow_rotation.min(15),
            eyebrow_color: self.eyebrow_color.min(7),
            eyebrow_size: self.eyebrow_size.min(15),
            eyebrow_vertical: self.eyebrow_vertical.min(31),
            eyebrow_spacing: self.eyebrow_spacing.min(15),

            eye_type: self.eye_type.min(47),
            eye_rotation: self.eye_rotation.min(7),
            eye_vertical: self.eye_vertical.min(31),
            eye_color: self.eye_color.min(5),
            eye_size: self.eye_size.min(7),
            eye_spacing: self.eye_spacing.min(15),

            nose_type: self.nose_type.min(11),
            nose_size: self.nose_size.min(15),
            nose_vertical: self.nose_vertical.min(31),

            mouth_type: self.mouth_type.min(23),
            mouth_color: self.mouth_color.min(2),
            mouth_size: self.mouth_size.min(15),
            mouth_vertical: self.mouth_vertical.min(31),

            glasses_type: self.glasses_type.min(8),
            glasses_color: self.glasses_color.min(5),
            glasses_size: self.glasses_size.min(7),
            glasses_vertical: self.glasses_vertical.min(31),

            mustache_type: self.mustache_type.min(3),
            beard_type: self.beard_type.min(3),
            facial_hair_color: self.facial_hair_color.min(7),
            mustache_size: self.mustache_size.min(15),
            mustache_vertical: self.mustache_vertical.min(31),

            mole_enabled: self.mole_enabled,
            mole_size: self.mole_size.min(15),
            mole_vertical: self.mole_vertical.min(31),
            mole_horizontal: self.mole_horizontal.min(31),
        }
    }

    /// `true` se il Mii non ha ancora un'identità propria.
    ///
    /// Un Mii id nullo o un system id tutto a zero rendono il Mii
    /// indistinguibile dagli altri dentro il database di Dolphin.
    pub fn needs_identity(&self) -> bool {
        self.mii_id == 0 || self.system_id == [0; 4]
    }

    /// Assegna Mii id e system id **solo se mancano**.
    ///
    /// L'entropia arriva dal chiamante: questo crate non legge l'orologio né
    /// il generatore di numeri casuali del sistema.
    #[must_use]
    pub fn with_identity(mut self, mii_id: u32, system_id: [u8; 4]) -> Self {
        if self.mii_id == 0 {
            self.mii_id = mii_id;
        }
        if self.system_id == [0; 4] {
            self.system_id = system_id;
        }
        self
    }
}

/// Decodifica i 74 byte nello stato completo dell'editor.
pub fn read_editor_state(block: &[u8]) -> SaveResult<MiiEditorState> {
    if block.len() != BLOCK_SIZE {
        return Err(SaveError::InvalidMii(format!(
            "un blocco Mii Wii deve essere di {BLOCK_SIZE} byte, ricevuti {}",
            block.len()
        )));
    }
    if !looks_like_wii_mii(block) {
        return Err(SaveError::InvalidMii(
            "il blocco non ha superato la validazione".into(),
        ));
    }

    let header = read_u16(block, 0);
    let face = read_u16(block, FACE_OFFSET);
    let hair = read_u16(block, HAIR_OFFSET);
    let brow = read_u32(block, BROW_OFFSET);
    let eye = read_u32(block, EYE_OFFSET);
    let nose = read_u16(block, NOSE_OFFSET);
    let mouth = read_u16(block, MOUTH_OFFSET);
    let glasses = read_u16(block, GLASSES_OFFSET);
    let facial = read_u16(block, FACIAL_HAIR_OFFSET);
    let mole = read_u16(block, MOLE_OFFSET);

    Ok(MiiEditorState {
        name: read_mii_string(block, NAME_OFFSET),
        creator_name: read_mii_string(block, CREATOR_OFFSET),
        is_female: header & 0x4000 != 0,
        is_favorite: header & 0x01 != 0,
        favorite_color_index: ((header >> 1) & 0x0F) as u8,
        birth_month: (((header >> 10) & 0x0F) as u8).clamp(1, 12),
        birth_day: (((header >> 5) & 0x1F) as u8).clamp(1, 31),
        height: block[HEIGHT_OFFSET],
        weight: block[WEIGHT_OFFSET],
        mii_id: read_u32(block, MII_ID_OFFSET),
        system_id: [
            block[SYSTEM_ID_OFFSET],
            block[SYSTEM_ID_OFFSET + 1],
            block[SYSTEM_ID_OFFSET + 2],
            block[SYSTEM_ID_OFFSET + 3],
        ],

        face_shape: (face >> 13) as u8,
        skin_color: ((face >> 10) & 0x07) as u8,
        facial_feature: ((face >> 6) & 0x0F) as u8,

        hair_type: (hair >> 9) as u8,
        hair_color: ((hair >> 6) & 0x07) as u8,
        hair_flipped: (hair >> 5) & 0x01 != 0,

        eyebrow_type: (brow >> 27) as u8,
        eyebrow_rotation: ((brow >> 22) & 0x0F) as u8,
        eyebrow_color: ((brow >> 13) & 0x07) as u8,
        eyebrow_size: ((brow >> 9) & 0x0F) as u8,
        eyebrow_vertical: ((brow >> 4) & 0x1F) as u8,
        eyebrow_spacing: (brow & 0x0F) as u8,

        eye_type: (eye >> 26) as u8,
        eye_rotation: ((eye >> 21) & 0x07) as u8,
        eye_vertical: ((eye >> 16) & 0x1F) as u8,
        eye_color: ((eye >> 13) & 0x07) as u8,
        eye_size: ((eye >> 9) & 0x07) as u8,
        eye_spacing: ((eye >> 5) & 0x0F) as u8,

        nose_type: (nose >> 12) as u8,
        nose_size: ((nose >> 8) & 0x0F) as u8,
        nose_vertical: ((nose >> 3) & 0x1F) as u8,

        mouth_type: (mouth >> 11) as u8,
        mouth_color: ((mouth >> 9) & 0x03) as u8,
        mouth_size: ((mouth >> 5) & 0x0F) as u8,
        mouth_vertical: (mouth & 0x1F) as u8,

        glasses_type: (glasses >> 12) as u8,
        glasses_color: ((glasses >> 9) & 0x07) as u8,
        glasses_size: ((glasses >> 5) & 0x07) as u8,
        glasses_vertical: (glasses & 0x1F) as u8,

        mustache_type: (facial >> 14) as u8,
        beard_type: ((facial >> 12) & 0x03) as u8,
        facial_hair_color: ((facial >> 9) & 0x07) as u8,
        mustache_size: ((facial >> 5) & 0x0F) as u8,
        mustache_vertical: (facial & 0x1F) as u8,

        mole_enabled: (mole >> 15) & 0x01 != 0,
        mole_size: ((mole >> 11) & 0x0F) as u8,
        mole_vertical: ((mole >> 6) & 0x1F) as u8,
        mole_horizontal: ((mole >> 1) & 0x1F) as u8,
    })
}

// Bit effettivamente modellati da [`MiiEditorState`]. Tutto ciò che sta fuori
// da queste maschere esiste nei Mii reali — il flag "non copiabile" nell'header,
// il flag "mingle" nella parola del viso, e alcuni bit di cui non si conosce il
// significato — e viene **preservato** quando si modifica un Mii esistente
// (vedi `docs/decisions.md` §D-026).
const HEADER_MASK: u16 = 0x7FFF;
const FACE_MASK: u16 = 0xFFC0;
const HAIR_MASK: u16 = 0xFFE0;
const BROW_MASK: u32 = 0xFBC0_FFFF;
const EYE_MASK: u32 = 0xFCFF_EFE0;
const NOSE_MASK: u16 = 0xFFF8;
const MOUTH_MASK: u16 = 0xFFFF;
const GLASSES_MASK: u16 = 0xFEFF;
const FACIAL_HAIR_MASK: u16 = 0xFFFF;
const MOLE_MASK: u16 = 0xFFFE;

/// Serializza lo stato dell'editor in un blocco nuovo di 74 byte.
///
/// La funzione è **deterministica**: `mii_id` e `system_id` vengono scritti
/// così come sono, anche se nulli. Assegnarli è compito di
/// [`MiiEditorState::with_identity`], che riceve l'entropia dal chiamante.
///
/// I bit fuori dal modello restano a zero, come in `CreateMiiBytes`. Per
/// modificare un Mii che esiste già usa [`apply_editor_state`], che li
/// conserva.
pub fn write_editor_state(state: &MiiEditorState) -> Vec<u8> {
    let mut block = vec![0u8; BLOCK_SIZE];
    write_state_into(&mut block, state);
    block
}

/// Riscrive un blocco esistente con lo stato dell'editor, preservando i bit
/// che il modello non descrive.
///
/// È la variante da usare quando si modifica un Mii importato: il launcher
/// legacy ricostruiva il blocco da zero e perdeva, per esempio, il flag
/// "mingle" e quello di non copiabilità che i Mii creati su una console reale
/// portano con sé.
pub fn apply_editor_state(base: &[u8], state: &MiiEditorState) -> SaveResult<Vec<u8>> {
    if base.len() != BLOCK_SIZE {
        return Err(SaveError::InvalidMii(format!(
            "un blocco Mii Wii deve essere di {BLOCK_SIZE} byte, ricevuti {}",
            base.len()
        )));
    }

    let mut block = base.to_vec();
    write_state_into(&mut block, state);
    Ok(block)
}

fn write_state_into(block: &mut [u8], state: &MiiEditorState) {
    let state = state.normalized();

    let mut header = 0u16;
    if state.is_female {
        header |= 0x4000;
    }
    header |= u16::from(state.birth_month & 0x0F) << 10;
    header |= u16::from(state.birth_day & 0x1F) << 5;
    header |= u16::from(state.favorite_color_index & 0x0F) << 1;
    if state.is_favorite {
        header |= 0x01;
    }
    merge_u16(block, 0, HEADER_MASK, header);

    write_mii_string(block, NAME_OFFSET, &state.name, "Vanza Mii");
    block[HEIGHT_OFFSET] = state.height;
    block[WEIGHT_OFFSET] = state.weight;
    write_u32(block, MII_ID_OFFSET, state.mii_id);
    block[SYSTEM_ID_OFFSET..SYSTEM_ID_OFFSET + 4].copy_from_slice(&state.system_id);

    merge_u16(
        block,
        FACE_OFFSET,
        FACE_MASK,
        (u16::from(state.face_shape & 0x07) << 13)
            | (u16::from(state.skin_color & 0x07) << 10)
            | (u16::from(state.facial_feature & 0x0F) << 6),
    );
    merge_u16(
        block,
        HAIR_OFFSET,
        HAIR_MASK,
        (u16::from(state.hair_type & 0x7F) << 9)
            | (u16::from(state.hair_color & 0x07) << 6)
            | (u16::from(state.hair_flipped) << 5),
    );
    merge_u32(
        block,
        BROW_OFFSET,
        BROW_MASK,
        (u32::from(state.eyebrow_type & 0x1F) << 27)
            | (u32::from(state.eyebrow_rotation & 0x0F) << 22)
            | (u32::from(state.eyebrow_color & 0x07) << 13)
            | (u32::from(state.eyebrow_size & 0x0F) << 9)
            | (u32::from(state.eyebrow_vertical & 0x1F) << 4)
            | u32::from(state.eyebrow_spacing & 0x0F),
    );
    merge_u32(
        block,
        EYE_OFFSET,
        EYE_MASK,
        (u32::from(state.eye_type & 0x3F) << 26)
            | (u32::from(state.eye_rotation & 0x07) << 21)
            | (u32::from(state.eye_vertical & 0x1F) << 16)
            | (u32::from(state.eye_color & 0x07) << 13)
            | (u32::from(state.eye_size & 0x07) << 9)
            | (u32::from(state.eye_spacing & 0x0F) << 5),
    );
    merge_u16(
        block,
        NOSE_OFFSET,
        NOSE_MASK,
        (u16::from(state.nose_type & 0x0F) << 12)
            | (u16::from(state.nose_size & 0x0F) << 8)
            | (u16::from(state.nose_vertical & 0x1F) << 3),
    );
    merge_u16(
        block,
        MOUTH_OFFSET,
        MOUTH_MASK,
        (u16::from(state.mouth_type & 0x1F) << 11)
            | (u16::from(state.mouth_color & 0x03) << 9)
            | (u16::from(state.mouth_size & 0x0F) << 5)
            | u16::from(state.mouth_vertical & 0x1F),
    );
    merge_u16(
        block,
        GLASSES_OFFSET,
        GLASSES_MASK,
        (u16::from(state.glasses_type & 0x0F) << 12)
            | (u16::from(state.glasses_color & 0x07) << 9)
            | (u16::from(state.glasses_size & 0x07) << 5)
            | u16::from(state.glasses_vertical & 0x1F),
    );
    merge_u16(
        block,
        FACIAL_HAIR_OFFSET,
        FACIAL_HAIR_MASK,
        (u16::from(state.mustache_type & 0x03) << 14)
            | (u16::from(state.beard_type & 0x03) << 12)
            | (u16::from(state.facial_hair_color & 0x07) << 9)
            | (u16::from(state.mustache_size & 0x0F) << 5)
            | u16::from(state.mustache_vertical & 0x1F),
    );
    merge_u16(
        block,
        MOLE_OFFSET,
        MOLE_MASK,
        (u16::from(state.mole_enabled) << 15)
            | (u16::from(state.mole_size & 0x0F) << 11)
            | (u16::from(state.mole_vertical & 0x1F) << 6)
            | (u16::from(state.mole_horizontal & 0x1F) << 1),
    );

    write_mii_string(block, CREATOR_OFFSET, &state.creator_name, "VanzaKart");
}

fn merge_u16(block: &mut [u8], offset: usize, mask: u16, value: u16) {
    let preserved = read_u16(block, offset) & !mask;
    write_u16(block, offset, preserved | (value & mask));
}

fn merge_u32(block: &mut [u8], offset: usize, mask: u32, value: u32) {
    let preserved = read_u32(block, offset) & !mask;
    write_u32(block, offset, preserved | (value & mask));
}

/// Mii id nella forma che produce la Wii.
///
/// Porta `GenerateMiiId`: il prefisso `0b100` sui bit alti marca un Mii creato
/// su una console, i 29 bit restanti sono un contatore a passi di 4 secondi
/// dal 1 gennaio 2006. Contatore ed entropia arrivano dal chiamante perché
/// questo crate non legge l'orologio di sistema.
pub fn generate_mii_id(counter: u32, entropy: u8) -> u32 {
    (0b100 << 29) | (counter.wrapping_add(u32::from(entropy & 0x1F)) & 0x1FFF_FFFF)
}

// ---------------------------------------------------------------------------
// Lettura e costruzione di blocchi
// ---------------------------------------------------------------------------

/// Decodifica un blocco da 74 byte.
///
/// Come nel legacy, un valore di colore bocca non valido (3) viene corretto a
/// 2 prima della validazione: alcuni editor di terze parti lo producono.
pub fn parse_block(block: &[u8]) -> SaveResult<WiiMii> {
    if block.len() != BLOCK_SIZE {
        return Err(SaveError::InvalidMii(format!(
            "un blocco Mii Wii deve essere di {BLOCK_SIZE} byte, ricevuti {}",
            block.len()
        )));
    }

    let mut raw = block.to_vec();

    let mouth = read_u16(&raw, MOUTH_OFFSET);
    if (mouth >> 9) & 0x03 > 2 {
        write_u16(&mut raw, MOUTH_OFFSET, mouth & 0xFDFF);
    }

    if !looks_like_wii_mii(&raw) {
        return Err(SaveError::InvalidMii(
            "il blocco non ha superato la validazione".into(),
        ));
    }

    let header = read_u16(&raw, 0);
    let name = read_mii_string(&raw, NAME_OFFSET);

    Ok(WiiMii {
        name: if name.trim().is_empty() {
            "Mii".to_string()
        } else {
            name
        },
        creator_name: read_mii_string(&raw, CREATOR_OFFSET),
        mii_id: read_u32(&raw, MII_ID_OFFSET),
        favorite_color_index: ((header >> 1) & 0x0F) as usize,
        height: raw[HEIGHT_OFFSET],
        weight: raw[WEIGHT_OFFSET],
        is_female: header & 0x4000 != 0,
        is_favorite: header & 0x01 != 0,
        birth_month: (((header >> 10) & 0x0F) as u8).clamp(1, 12),
        birth_day: (((header >> 5) & 0x1F) as u8).clamp(1, 31),
        raw,
    })
}

/// Cerca un blocco Mii valido dentro un file di formato sconosciuto.
///
/// Prova prima gli offset noti dei contenitori più diffusi, poi esegue una
/// scansione byte per byte (limitata a 4 KiB per i file grandi, come il
/// legacy).
pub fn extract_block(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < BLOCK_SIZE {
        return None;
    }

    for offset in candidate_offsets(bytes.len()) {
        if offset + BLOCK_SIZE > bytes.len() {
            continue;
        }
        let candidate = &bytes[offset..offset + BLOCK_SIZE];
        if looks_like_wii_mii(candidate) {
            return Some(candidate.to_vec());
        }
    }

    let max_offset = bytes.len() - BLOCK_SIZE;
    let scan_limit = if bytes.len() <= 1024 * 1024 {
        max_offset
    } else {
        max_offset.min(4096)
    };

    (0..=scan_limit)
        .map(|offset| &bytes[offset..offset + BLOCK_SIZE])
        .find(|candidate| looks_like_wii_mii(candidate))
        .map(<[u8]>::to_vec)
}

/// Offset noti in cui i contenitori più comuni annidano il blocco.
///
/// Stessa lista, nello stesso ordine, di `CandidateOffsets` del launcher
/// legacy: cambiarla cambierebbe quale blocco viene scelto nei file che ne
/// contengono più di uno.
fn candidate_offsets(length: usize) -> Vec<usize> {
    let mut offsets = vec![0usize, 2, 4, 8, 0x10, 0x20, 0x40, 0x60, 0xF0];
    offsets.push(length.saturating_sub(BLOCK_SIZE));
    offsets
}

/// Euristica di validità: replica completa di `LooksLikeWiiMii`.
///
/// Oltre a nome e header controlla che ogni indice di tratto stia dentro il
/// suo atlante: è ciò che impedisce a un riempimento casuale di superare la
/// validazione durante la scansione di un contenitore sconosciuto.
pub fn looks_like_wii_mii(block: &[u8]) -> bool {
    if block.len() != BLOCK_SIZE {
        return false;
    }
    if block.iter().all(|byte| *byte == 0x00) || block.iter().all(|byte| *byte == 0xFF) {
        return false;
    }

    let name = read_mii_string(block, NAME_OFFSET);
    if name.trim().is_empty() || name.encode_utf16().count() > NAME_UNITS {
        return false;
    }
    if name
        .chars()
        .any(|character| character.is_control() && character != '\t')
    {
        return false;
    }

    let header = read_u16(block, 0);
    let favorite_color = (header >> 1) & 0x0F;
    let month = (header >> 10) & 0x0F;
    let day = (header >> 5) & 0x1F;

    favorite_color <= 11
        && month <= 12
        && day <= 31
        && block[HEIGHT_OFFSET] <= 127
        && block[WEIGHT_OFFSET] <= 127
        && looks_like_valid_feature_block(block)
}

fn looks_like_valid_feature_block(block: &[u8]) -> bool {
    let face = read_u16(block, FACE_OFFSET);
    let hair = read_u16(block, HAIR_OFFSET);
    let brow = read_u32(block, BROW_OFFSET);
    let eye = read_u32(block, EYE_OFFSET);
    let nose = read_u16(block, NOSE_OFFSET);
    let mouth = read_u16(block, MOUTH_OFFSET);
    let glasses = read_u16(block, GLASSES_OFFSET);

    // Il legacy confronta anche colore capelli, colore sopracciglia e tipo di
    // barba con il proprio massimo, ma quei campi hanno esattamente tanti bit
    // quanti servono: il confronto è sempre vero e non è stato riportato.
    ((face >> 10) & 0x07) <= 5
        && ((face >> 6) & 0x0F) <= 11
        && (hair >> 9) <= 71
        && (brow >> 27) <= 23
        && (eye >> 26) <= 47
        && ((eye >> 13) & 0x07) <= 5
        && (nose >> 12) <= 11
        && (mouth >> 11) <= 23
        && (glasses >> 12) <= 8
}

/// Costruisce un blocco Mii predefinito, come `CreateDefaultMiiBytes`.
///
/// Il Mii id è derivato dal nome invece che dall'orologio: due chiamate con lo
/// stesso nome producono lo stesso blocco, il che rende i test riproducibili e
/// non fa collidere due Mii diversi nel database di Dolphin.
pub fn build_block(name: &str, favorite_color_index: usize, is_female: bool) -> Vec<u8> {
    let state = MiiEditorState {
        name: name.to_string(),
        favorite_color_index: favorite_color_index.min(11) as u8,
        is_female,
        mii_id: derive_mii_id(name),
        ..MiiEditorState::default()
    };
    write_editor_state(&state)
}

/// Sostituisce il nome dentro un blocco esistente, preservando tutto il resto.
pub fn set_name(block: &mut [u8], name: &str) -> SaveResult<()> {
    if block.len() != BLOCK_SIZE {
        return Err(SaveError::InvalidMii("blocco di dimensione errata".into()));
    }
    write_mii_string(block, NAME_OFFSET, name, "Mii");
    Ok(())
}

/// Id derivato dal nome: stabile, non nullo e con il prefisso della Wii.
fn derive_mii_id(name: &str) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    generate_mii_id(hash & 0x1FFF_FFFF, 0)
}

/// Nome normalizzato: al massimo 10 unità UTF-16, mai vuoto.
///
/// Il limite è in unità UTF-16 e non in caratteri perché è così che il campo
/// da 20 byte è fatto; una coppia surrogata che non ci sta viene scartata
/// intera invece di essere spezzata a metà.
pub fn normalize_name(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    let source = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };

    let mut units: Vec<u16> = source.encode_utf16().take(NAME_UNITS).collect();
    if units
        .last()
        .is_some_and(|unit| (0xD800..0xDC00).contains(unit))
    {
        units.pop();
    }

    String::from_utf16_lossy(&units)
}

// ---------------------------------------------------------------------------
// Formato "Mii Studio"
// ---------------------------------------------------------------------------

/// Converte un blocco Wii nella stringa "Mii Studio".
///
/// È il formato che il renderer di avatar accetta: 46 byte rimappati e poi
/// cifrati con l'XOR progressivo di `EncodeStudioData`. Porta
/// `MiiFileParserService.BuildStudioData`.
pub fn studio_data(block: &[u8]) -> String {
    if block.len() != BLOCK_SIZE {
        return String::new();
    }

    let mut studio = [0u8; 46];

    let basic = read_u16(block, 0);
    studio[0x16] = u8::from((basic >> 14) & 1 == 1);
    studio[0x15] = ((basic >> 1) & 0x0F) as u8;
    studio[0x1E] = block[HEIGHT_OFFSET];
    studio[0x02] = block[WEIGHT_OFFSET];

    let face = read_u16(block, FACE_OFFSET);
    let facial_feature = ((face >> 6) & 0x0F) as usize;
    studio[0x13] = (face >> 13) as u8;
    studio[0x11] = ((face >> 10) & 0x07) as u8;
    studio[0x14] = WRINKLES_MAP[facial_feature];
    studio[0x12] = MAKEUP_MAP[facial_feature];

    let hair = read_u16(block, HAIR_OFFSET);
    let hair_color = ((hair >> 6) & 0x07) as u8;
    studio[0x1D] = (hair >> 9) as u8;
    studio[0x1B] = if hair_color == 0 { 8 } else { hair_color };
    studio[0x1C] = ((hair >> 5) & 1) as u8;

    let brow = read_u32(block, BROW_OFFSET);
    let brow_color = ((brow >> 13) & 0x07) as u8;
    studio[0x0E] = (brow >> 27) as u8;
    studio[0x0C] = ((brow >> 22) & 0x0F) as u8;
    studio[0x0B] = if brow_color == 0 { 8 } else { brow_color };
    studio[0x0D] = ((brow >> 9) & 0x0F) as u8;
    studio[0x0A] = 3;
    studio[0x10] = ((brow >> 4) & 0x1F) as u8;
    studio[0x0F] = (brow & 0x0F) as u8;

    let eye = read_u32(block, EYE_OFFSET);
    studio[0x07] = (eye >> 26) as u8;
    studio[0x05] = ((eye >> 21) & 0x07) as u8;
    studio[0x09] = ((eye >> 16) & 0x1F) as u8;
    studio[0x04] = (((eye >> 13) & 0x07) + 8) as u8;
    studio[0x06] = ((eye >> 9) & 0x07) as u8;
    studio[0x03] = 3;
    studio[0x08] = ((eye >> 5) & 0x0F) as u8;

    let nose = read_u16(block, NOSE_OFFSET);
    studio[0x2C] = (nose >> 12) as u8;
    studio[0x2B] = ((nose >> 8) & 0x0F) as u8;
    studio[0x2D] = ((nose >> 3) & 0x1F) as u8;

    let mouth = read_u16(block, MOUTH_OFFSET);
    let mouth_color = ((mouth >> 9) & 0x03) as u8;
    studio[0x26] = (mouth >> 11) as u8;
    studio[0x24] = mouth_color + 19;
    studio[0x25] = ((mouth >> 5) & 0x0F) as u8;
    studio[0x23] = 3;
    studio[0x27] = (mouth & 0x1F) as u8;

    let glasses = read_u16(block, GLASSES_OFFSET);
    let glasses_color = ((glasses >> 9) & 0x07) as u8;
    studio[0x19] = (glasses >> 12) as u8;
    studio[0x17] = match glasses_color {
        0 => 8,
        color if color < 6 => color + 13,
        _ => 0,
    };
    studio[0x18] = ((glasses >> 5) & 0x07) as u8;
    studio[0x1A] = (glasses & 0x1F) as u8;

    let facial = read_u16(block, FACIAL_HAIR_OFFSET);
    let facial_hair_color = ((facial >> 9) & 0x07) as u8;
    studio[0x29] = (facial >> 14) as u8;
    studio[0x01] = ((facial >> 12) & 0x03) as u8;
    studio[0x00] = if facial_hair_color == 0 {
        8
    } else {
        facial_hair_color
    };
    studio[0x28] = ((facial >> 5) & 0x0F) as u8;
    studio[0x2A] = (facial & 0x1F) as u8;

    let mole = read_u16(block, MOLE_OFFSET);
    studio[0x20] = (mole >> 15) as u8;
    studio[0x1F] = ((mole >> 11) & 0x0F) as u8;
    studio[0x22] = ((mole >> 6) & 0x1F) as u8;
    studio[0x21] = ((mole >> 1) & 0x1F) as u8;

    encode_studio_data(&studio)
}

/// XOR progressivo di `EncodeStudioData`: ogni byte è cifrato con il
/// precedente già cifrato, più una costante.
fn encode_studio_data(studio: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity((studio.len() + 1) * 2);
    out.push_str("00");

    let mut rolling = 0u8;
    for value in studio {
        let encoded = 7u8.wrapping_add(value ^ rolling);
        rolling = encoded;
        let _ = write!(out, "{encoded:02x}");
    }

    out
}

// ---------------------------------------------------------------------------
// Primitive
// ---------------------------------------------------------------------------

fn read_mii_string(bytes: &[u8], offset: usize) -> String {
    if offset + NAME_BYTES > bytes.len() {
        return String::new();
    }

    let units: Vec<u16> = bytes[offset..offset + NAME_BYTES]
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();

    String::from_utf16_lossy(&units).trim().to_string()
}

fn write_mii_string(bytes: &mut [u8], offset: usize, value: &str, fallback: &str) {
    bytes[offset..offset + NAME_BYTES].fill(0);

    let normalized = normalize_name(value, fallback);
    let mut cursor = offset;
    for unit in normalized.encode_utf16() {
        if cursor + 2 > offset + NAME_BYTES {
            break;
        }
        bytes[cursor..cursor + 2].copy_from_slice(&unit.to_be_bytes());
        cursor += 2;
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Base64 standard, senza dipendenze esterne.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decodifica base64 standard; `None` se l'input non è valido.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn value(character: u8) -> Option<u32> {
        match character {
            b'A'..=b'Z' => Some(u32::from(character - b'A')),
            b'a'..=b'z' => Some(u32::from(character - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(character - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let cleaned: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if cleaned.len() % 4 != 0 {
        return None;
    }

    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks_exact(4) {
        let padding = chunk.iter().filter(|byte| **byte == b'=').count();
        if padding > 2 {
            return None;
        }

        let mut triple = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            let bits = if *byte == b'=' { 0 } else { value(*byte)? };
            triple |= bits << (18 - 6 * index);
        }

        out.push(((triple >> 16) & 0xFF) as u8);
        if padding < 2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if padding < 1 {
            out.push((triple & 0xFF) as u8);
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stato con ogni campo diverso dal default, per scovare i bit-field
    /// scambiati: se due campi condividessero gli stessi bit, il round-trip
    /// li restituirebbe uguali.
    fn distinctive_state() -> MiiEditorState {
        MiiEditorState {
            name: "Vanza".into(),
            creator_name: "Tester".into(),
            is_female: true,
            is_favorite: true,
            favorite_color_index: 9,
            birth_month: 11,
            birth_day: 27,
            height: 100,
            weight: 37,
            mii_id: 0x8123_4567,
            system_id: [0xDE, 0xAD, 0xBE, 0xEF],

            face_shape: 5,
            skin_color: 4,
            facial_feature: 9,

            hair_type: 60,
            hair_color: 6,
            hair_flipped: true,

            eyebrow_type: 21,
            eyebrow_rotation: 13,
            eyebrow_color: 5,
            eyebrow_size: 14,
            eyebrow_vertical: 29,
            eyebrow_spacing: 12,

            eye_type: 41,
            eye_rotation: 6,
            eye_vertical: 25,
            eye_color: 4,
            eye_size: 6,
            eye_spacing: 11,

            nose_type: 10,
            nose_size: 13,
            nose_vertical: 23,

            mouth_type: 19,
            mouth_color: 2,
            mouth_size: 12,
            mouth_vertical: 27,

            glasses_type: 7,
            glasses_color: 3,
            glasses_size: 6,
            glasses_vertical: 24,

            mustache_type: 2,
            beard_type: 3,
            facial_hair_color: 6,
            mustache_size: 11,
            mustache_vertical: 22,

            mole_enabled: true,
            mole_size: 13,
            mole_vertical: 26,
            mole_horizontal: 21,
        }
    }

    #[test]
    fn the_editor_state_round_trips_through_the_block() {
        let state = distinctive_state();
        let block = write_editor_state(&state);

        assert_eq!(block.len(), BLOCK_SIZE);
        assert_eq!(read_editor_state(&block).unwrap(), state);
    }

    #[test]
    fn the_default_state_round_trips() {
        let state = MiiEditorState::default();
        let block = write_editor_state(&state);
        assert_eq!(read_editor_state(&block).unwrap(), state);
    }

    #[test]
    fn every_field_moves_at_least_one_bit_of_its_own() {
        // Se due campi condividessero gli stessi bit, cambiarne uno solo
        // cambierebbe anche l'altro: il confronto campo per campo lo scopre.
        let base = distinctive_state();
        let reference = write_editor_state(&base);

        macro_rules! assert_isolated {
            ($field:ident, $value:expr) => {{
                let mut variant = base.clone();
                variant.$field = $value;
                let block = write_editor_state(&variant);
                assert_ne!(
                    block, reference,
                    concat!("il campo ", stringify!($field), " non scrive nulla")
                );
                let read = read_editor_state(&block).unwrap();
                assert_eq!(read, variant, concat!("round-trip di ", stringify!($field)));
            }};
        }

        assert_isolated!(face_shape, 2);
        assert_isolated!(skin_color, 1);
        assert_isolated!(facial_feature, 3);
        assert_isolated!(hair_type, 7);
        assert_isolated!(hair_color, 2);
        assert_isolated!(hair_flipped, false);
        assert_isolated!(eyebrow_type, 3);
        assert_isolated!(eyebrow_rotation, 4);
        assert_isolated!(eyebrow_color, 1);
        assert_isolated!(eyebrow_size, 2);
        assert_isolated!(eyebrow_vertical, 7);
        assert_isolated!(eyebrow_spacing, 3);
        assert_isolated!(eye_type, 8);
        assert_isolated!(eye_rotation, 1);
        assert_isolated!(eye_vertical, 4);
        assert_isolated!(eye_color, 1);
        assert_isolated!(eye_size, 2);
        assert_isolated!(eye_spacing, 3);
        assert_isolated!(nose_type, 2);
        assert_isolated!(nose_size, 5);
        assert_isolated!(nose_vertical, 8);
        assert_isolated!(mouth_type, 4);
        assert_isolated!(mouth_color, 1);
        assert_isolated!(mouth_size, 6);
        assert_isolated!(mouth_vertical, 9);
        assert_isolated!(glasses_type, 2);
        assert_isolated!(glasses_color, 1);
        assert_isolated!(glasses_size, 3);
        assert_isolated!(glasses_vertical, 5);
        assert_isolated!(mustache_type, 1);
        assert_isolated!(beard_type, 1);
        assert_isolated!(facial_hair_color, 2);
        assert_isolated!(mustache_size, 3);
        assert_isolated!(mustache_vertical, 7);
        assert_isolated!(mole_enabled, false);
        assert_isolated!(mole_size, 2);
        assert_isolated!(mole_vertical, 6);
        assert_isolated!(mole_horizontal, 8);
        assert_isolated!(is_female, false);
        assert_isolated!(is_favorite, false);
        assert_isolated!(favorite_color_index, 3);
        assert_isolated!(birth_month, 2);
        assert_isolated!(birth_day, 9);
        assert_isolated!(height, 12);
        assert_isolated!(weight, 90);
        assert_isolated!(mii_id, 0x8000_0001);
        assert_isolated!(system_id, [1, 2, 3, 4]);
    }

    #[test]
    fn writing_normalizes_out_of_range_values() {
        let state = MiiEditorState {
            name: "   ".into(),
            creator_name: String::new(),
            favorite_color_index: 200,
            birth_month: 0,
            birth_day: 99,
            height: 255,
            weight: 200,
            eye_type: 250,
            mouth_color: 3,
            ..MiiEditorState::default()
        };

        let read = read_editor_state(&write_editor_state(&state)).unwrap();

        assert_eq!(read.name, "Vanza Mii");
        assert_eq!(read.creator_name, "VanzaKart");
        assert_eq!(read.favorite_color_index, 11);
        assert_eq!(read.birth_month, 1);
        assert_eq!(read.birth_day, 31);
        assert_eq!(read.height, 127);
        assert_eq!(read.weight, 127);
        assert_eq!(read.eye_type, 47);
        assert_eq!(read.mouth_color, 2);
        assert_eq!(read, state.normalized());
    }

    /// Blocco con **tutti** i bit fuori dal modello accesi.
    fn block_with_unmodelled_bits() -> Vec<u8> {
        let mut block = write_editor_state(&distinctive_state());
        merge_u16(&mut block, 0, !HEADER_MASK, 0xFFFF);
        merge_u16(&mut block, FACE_OFFSET, !FACE_MASK, 0xFFFF);
        merge_u16(&mut block, HAIR_OFFSET, !HAIR_MASK, 0xFFFF);
        merge_u32(&mut block, BROW_OFFSET, !BROW_MASK, 0xFFFF_FFFF);
        merge_u32(&mut block, EYE_OFFSET, !EYE_MASK, 0xFFFF_FFFF);
        merge_u16(&mut block, NOSE_OFFSET, !NOSE_MASK, 0xFFFF);
        merge_u16(&mut block, GLASSES_OFFSET, !GLASSES_MASK, 0xFFFF);
        merge_u16(&mut block, MOLE_OFFSET, !MOLE_MASK, 0xFFFF);
        block
    }

    #[test]
    fn editing_an_existing_block_preserves_the_bits_outside_the_model() {
        // I Mii creati su una console reale portano bit che l'editor non
        // descrive: il flag "non copiabile", quello "mingle" e alcuni ignoti.
        // Rileggere e riscrivere senza modifiche deve restituire gli stessi
        // byte, bit per bit.
        let original = block_with_unmodelled_bits();
        let state = read_editor_state(&original).unwrap();

        let rewritten = apply_editor_state(&original, &state).unwrap();
        assert_eq!(rewritten, original);
    }

    #[test]
    fn editing_still_applies_the_requested_change() {
        let original = block_with_unmodelled_bits();
        let mut state = read_editor_state(&original).unwrap();
        state.eye_type = 7;
        state.name = "Cambiato".into();

        let rewritten = apply_editor_state(&original, &state).unwrap();
        let read = read_editor_state(&rewritten).unwrap();

        assert_eq!(read.eye_type, 7);
        assert_eq!(read.name, "Cambiato");
        // I bit fuori dal modello sono ancora al loro posto.
        assert_eq!(
            read_u16(&rewritten, 0) & !HEADER_MASK,
            read_u16(&original, 0) & !HEADER_MASK
        );
        assert_eq!(
            read_u32(&rewritten, EYE_OFFSET) & !EYE_MASK,
            read_u32(&original, EYE_OFFSET) & !EYE_MASK
        );
    }

    #[test]
    fn writing_from_scratch_leaves_the_unmodelled_bits_at_zero() {
        let state = read_editor_state(&block_with_unmodelled_bits()).unwrap();
        let fresh = write_editor_state(&state);

        assert_eq!(read_u16(&fresh, 0) & !HEADER_MASK, 0);
        assert_eq!(read_u16(&fresh, FACE_OFFSET) & !FACE_MASK, 0);
        assert_eq!(read_u32(&fresh, EYE_OFFSET) & !EYE_MASK, 0);
    }

    #[test]
    fn applying_to_a_wrong_sized_block_is_rejected() {
        assert!(apply_editor_state(&[0u8; 10], &MiiEditorState::default()).is_err());
    }

    #[test]
    fn normalizing_is_idempotent() {
        let once = distinctive_state().normalized();
        assert_eq!(once.normalized(), once);
    }

    #[test]
    fn identity_is_only_assigned_when_missing() {
        let state = MiiEditorState::default();
        assert!(state.needs_identity());

        let filled = state.with_identity(42, [1, 2, 3, 4]);
        assert_eq!(filled.mii_id, 42);
        assert_eq!(filled.system_id, [1, 2, 3, 4]);
        assert!(!filled.needs_identity());

        let again = filled.clone().with_identity(99, [9, 9, 9, 9]);
        assert_eq!(again.mii_id, 42, "un id esistente non viene sovrascritto");
        assert_eq!(again.system_id, [1, 2, 3, 4]);
    }

    #[test]
    fn generated_ids_carry_the_console_prefix() {
        let id = generate_mii_id(1_000, 0);
        assert_eq!(id >> 29, 0b100);
        assert_ne!(id, 0);
        assert_ne!(generate_mii_id(1_000, 0), generate_mii_id(2_000, 0));
        // L'entropia sposta il contatore ma non tocca il prefisso.
        assert_eq!(generate_mii_id(1_000, 31) >> 29, 0b100);
    }

    #[test]
    fn a_built_block_round_trips() {
        let block = build_block("Vanza", 5, false);
        assert_eq!(block.len(), BLOCK_SIZE);

        let mii = parse_block(&block).unwrap();
        assert_eq!(mii.name, "Vanza");
        assert_eq!(mii.creator_name, "VanzaKart");
        assert_eq!(mii.favorite_color_index, 5);
        assert_eq!(mii.favorite_color(), "#3B82F6");
        assert!(!mii.is_female);
        // `CreateDefaultMiiBytes` marca il Mii come preferito.
        assert!(mii.is_favorite);
        assert_eq!(mii.birth_month, 1);
        assert_eq!(mii.birth_day, 1);
        assert_ne!(mii.mii_id, 0);
        assert_eq!(mii.raw, block);
    }

    #[test]
    fn a_built_block_carries_the_legacy_default_features() {
        let state = read_editor_state(&build_block("Vanza", 0, false)).unwrap();
        assert_eq!(state.hair_type, 33);
        assert_eq!(state.eye_type, 2);
        assert_eq!(state.mouth_type, 23);
        assert_eq!(state.nose_type, 1);
        assert_eq!(state.skin_color, 1);
    }

    #[test]
    fn the_same_name_always_yields_the_same_block() {
        assert_eq!(
            build_block("Vanza", 1, false),
            build_block("Vanza", 1, false)
        );
        assert_ne!(
            build_block("Vanza", 1, false),
            build_block("Altro", 1, false)
        );
    }

    #[test]
    fn the_female_flag_survives_a_round_trip() {
        let block = build_block("Ada", 2, true);
        assert!(parse_block(&block).unwrap().is_female);
    }

    #[test]
    fn names_are_truncated_to_ten_characters() {
        let block = build_block("NomeMoltoMoltoLungo", 0, false);
        assert_eq!(parse_block(&block).unwrap().name, "NomeMoltoM");
    }

    #[test]
    fn an_empty_name_falls_back() {
        let block = build_block("   ", 0, false);
        assert_eq!(parse_block(&block).unwrap().name, "Vanza Mii");
    }

    #[test]
    fn non_ascii_names_survive() {
        let block = build_block("Andrèa", 0, false);
        assert_eq!(parse_block(&block).unwrap().name, "Andrèa");
    }

    #[test]
    fn renaming_preserves_every_other_field() {
        let mut block = build_block("Vanza", 7, true);
        let before = parse_block(&block).unwrap();

        set_name(&mut block, "Nuovo").unwrap();
        let after = parse_block(&block).unwrap();

        assert_eq!(after.name, "Nuovo");
        assert_eq!(after.favorite_color_index, before.favorite_color_index);
        assert_eq!(after.mii_id, before.mii_id);
        assert_eq!(after.is_female, before.is_female);
    }

    #[test]
    fn wrong_sizes_are_rejected() {
        assert!(parse_block(&[0u8; 10]).is_err());
        assert!(parse_block(&[0u8; 200]).is_err());
        assert!(read_editor_state(&[0u8; 10]).is_err());
        assert!(set_name(&mut [0u8; 10], "x").is_err());
    }

    #[test]
    fn empty_and_filled_blocks_are_rejected() {
        assert!(parse_block(&[0x00u8; BLOCK_SIZE]).is_err());
        assert!(parse_block(&[0xFFu8; BLOCK_SIZE]).is_err());
        assert!(read_editor_state(&[0x00u8; BLOCK_SIZE]).is_err());
    }

    #[test]
    fn an_invalid_mouth_colour_is_repaired() {
        let mut block = build_block("Vanza", 0, false);
        // Colore bocca = 3, non valido.
        let mouth = read_u16(&block, MOUTH_OFFSET);
        write_u16(&mut block, MOUTH_OFFSET, mouth | 0x0600);
        let mii = parse_block(&block).unwrap();
        assert_eq!((read_u16(&mii.raw, MOUTH_OFFSET) >> 9) & 0x03, 2);
    }

    #[test]
    fn a_block_is_found_inside_a_container() {
        let block = build_block("Trovato", 3, false);
        // Offset 0x40: uno dei punti noti, con riempimento a zero davanti
        // (qualunque finestra che lo attraversa ha il campo nome vuoto e viene
        // scartata dall'euristica).
        let mut file = vec![0u8; 0x40];
        file.extend_from_slice(&block);

        let found = extract_block(&file).expect("blocco non trovato");
        assert_eq!(parse_block(&found).unwrap().name, "Trovato");
    }

    #[test]
    fn structured_noise_no_longer_fools_the_heuristic() {
        // Con la sola validazione di nome e header un riempimento a 0xAA
        // passava: adesso altezza, peso e indici dei tratti lo scartano.
        assert!(!looks_like_wii_mii(&[0xAAu8; BLOCK_SIZE]));
        assert!(extract_block(&[0xAAu8; 4096]).is_none());
    }

    #[test]
    fn the_heuristic_still_has_a_known_blind_spot() {
        // Limite residuo, ereditato dal launcher legacy: un riempimento che
        // decodifica in UTF-16 come testo stampabile e i cui bit-field cadono
        // per caso dentro gli atlanti supera comunque la validazione. Per
        // questo l'import di un Mii mostra sempre nome e anteprima prima di
        // scrivere qualunque cosa.
        assert!(looks_like_wii_mii(&[0x55u8; BLOCK_SIZE]));
    }

    #[test]
    fn real_looking_blocks_still_pass_the_heuristic() {
        assert!(looks_like_wii_mii(&build_block("Vanza", 0, false)));
        assert!(looks_like_wii_mii(
            &write_editor_state(&distinctive_state())
        ));
    }

    #[test]
    fn a_block_at_offset_zero_is_found() {
        let block = build_block("Zero", 1, false);
        assert_eq!(extract_block(&block).unwrap(), block);
    }

    #[test]
    fn a_container_without_a_mii_returns_none() {
        assert!(extract_block(&[0x00u8; 512]).is_none());
        assert!(extract_block(&[1u8, 2, 3]).is_none());
    }

    #[test]
    fn favorite_colours_have_a_fallback() {
        assert_eq!(favorite_color(0), "#FF3B3B");
        assert_eq!(favorite_color(11), "#03010a");
        assert_eq!(favorite_color(99), "#FF3B3B");
    }

    #[test]
    fn favorite_colours_resolve_back_to_their_index() {
        assert_eq!(favorite_color_index("#FF3B3B"), Some(0));
        assert_eq!(favorite_color_index("#3b82f6"), Some(5));
        assert_eq!(favorite_color_index(" #03010A "), Some(11));
        assert_eq!(favorite_color_index("#123456"), None);
    }

    #[test]
    fn studio_data_is_hex_and_starts_with_the_marker() {
        let studio = studio_data(&build_block("Vanza", 0, false));

        assert_eq!(studio.len(), (46 + 1) * 2);
        assert!(studio.starts_with("00"));
        assert!(studio.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn studio_data_changes_with_the_mii() {
        let one = studio_data(&build_block("Vanza", 0, false));
        let two = studio_data(&build_block("Vanza", 0, true));
        assert_ne!(one, two);
        assert_eq!(studio_data(&[0u8; 10]), "");
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_round_trips_a_mii_block() {
        let block = build_block("Vanza", 4, false);
        let encoded = base64_encode(&block);
        assert_eq!(base64_decode(&encoded).unwrap(), block);
    }

    #[test]
    fn base64_rejects_malformed_input() {
        assert!(base64_decode("Zg=").is_none());
        assert!(base64_decode("Z!!!").is_none());
        assert_eq!(base64_decode("Zm9v\n").unwrap(), b"foo");
    }

    #[test]
    fn normalize_name_clamps_and_falls_back() {
        assert_eq!(normalize_name("  Ciao  ", "Mii"), "Ciao");
        assert_eq!(normalize_name("", "Mii"), "Mii");
        assert_eq!(normalize_name("0123456789ABC", "Mii"), "0123456789");
    }

    #[test]
    fn normalize_name_never_splits_a_surrogate_pair() {
        // Nove caratteri più un emoji: l'emoji occupa due unità UTF-16 e non
        // ci sta, quindi viene scartato intero invece di lasciare mezza coppia.
        let name = normalize_name("123456789🎉", "Mii");
        assert_eq!(name, "123456789");
        assert!(!name.contains('\u{FFFD}'));
    }
}
