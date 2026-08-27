//! I Mii dell'utente.
//!
//! **L'unica fonte è `RFL_DB.dat`**, il database Mii che Dolphin tiene in
//! `Wii/shared2/menu/FaceLib/`. Il launcher non ha Mii propri: legge quelli del
//! gioco, e crearne, modificarne, duplicarne o eliminarne uno significa
//! scrivere in quel file (vedi `docs/decisions.md` §D-037).
//!
//! È ciò che rende il launcher onesto: la lista che vedi è quella che vede il
//! gioco, e non esistono due copie dello stesso Mii che possono divergere.
//!
//! Ogni scrittura passa dalle protezioni di `services::saves`, nello stesso
//! ordine di ogni altro file di Dolphin (§D-012):
//!
//! 1. **Dolphin non deve essere in esecuzione**, altrimenti riscriverebbe il
//!    database all'uscita e la modifica sparirebbe;
//! 2. **backup obbligatorio, verificato per hash**, prima di toccare il file;
//! 3. **il database viene modificato, non ricostruito**, e riscritto in modo
//!    atomico: i byte fra `0x1D00` e il CRC restano esattamente com'erano.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use vk_save::mii::{self, MiiEditorState};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Estensioni accettate in import, oltre ai contenitori Mii binari.
const PROFILE_EXTENSIONS: &[&str] = &[".json", ".vk-mii"];

/// Epoch dei Mii id della Wii: 1 gennaio 2006, UTC.
const MII_ID_EPOCH: i64 = 1_136_073_600;

/// Un Mii come lo vede il frontend.
///
/// Non contiene percorsi: la faccia si chiede con `studio_data`, che è il Mii
/// stesso codificato (vedi `docs/decisions.md` §D-017 e §D-035).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiiView {
    /// Il Mii id in esadecimale: è la chiave con cui il gioco lo cerca.
    pub id: String,
    pub mii_id: u32,
    pub name: String,
    pub creator_name: String,
    pub favorite_color: String,
    pub favorite_color_index: u8,
    pub is_female: bool,
    /// Il flag "preferito" che il Mii porta con sé, non una preferenza locale.
    pub is_favorite: bool,
    pub avatar_initial: String,
    pub studio_data: String,
    pub height: u8,
    pub weight: u8,
}

/// Vista di un blocco da 74 byte.
fn view_of(block: &[u8]) -> AppResult<MiiView> {
    let parsed = mii::parse_block(block)?;

    Ok(MiiView {
        id: format_id(parsed.mii_id),
        mii_id: parsed.mii_id,
        name: parsed.name.clone(),
        creator_name: parsed.creator_name.clone(),
        favorite_color: parsed.favorite_color().to_string(),
        favorite_color_index: parsed.favorite_color_index as u8,
        is_female: parsed.is_female,
        is_favorite: parsed.is_favorite,
        avatar_initial: initial(&parsed.name),
        studio_data: mii::studio_data(block),
        height: parsed.height,
        weight: parsed.weight,
    })
}

// ---------------------------------------------------------------------------
// Identificatori
// ---------------------------------------------------------------------------

/// Il Mii id come lo vede il frontend: otto cifre esadecimali minuscole.
fn format_id(mii_id: u32) -> String {
    format!("{mii_id:08x}")
}

/// Il Mii id dietro un identificatore del frontend.
///
/// L'identificatore arriva dalla webview: senza questo controllo un valore
/// arbitrario finirebbe in una ricerca dentro il database.
fn parse_id(id: &str) -> AppResult<u32> {
    let trimmed = id.trim();
    if trimmed.len() == 8 && trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        u32::from_str_radix(trimmed, 16)
            .ok()
            .filter(|value| *value != 0)
            .ok_or_else(|| AppError::BadRequest(format!("identificatore Mii non valido: {id}")))
    } else {
        Err(AppError::BadRequest(format!(
            "identificatore Mii non valido: {id}"
        )))
    }
}

// ---------------------------------------------------------------------------
// Il database
// ---------------------------------------------------------------------------

/// Percorso di `RFL_DB.dat` per la cartella User configurata.
pub async fn database_path(state: &Arc<AppState>) -> AppResult<PathBuf> {
    let user_folder = state.settings.read().await.user_folder();
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Err(AppError::Configuration(
            "Configura la cartella User di Dolphin per leggere e modificare i Mii.".into(),
        ));
    }

    Ok(vk_save::miidb::database_path(&user_folder))
}

/// Il database, o uno vuoto ma valido se Dolphin non l'ha mai creato.
async fn read_database(state: &Arc<AppState>) -> AppResult<Vec<u8>> {
    let path = database_path(state).await?;
    match tokio::fs::read(&path).await {
        Ok(bytes) if bytes.len() >= vk_save::miidb::DB_SIZE => Ok(bytes),
        // Senza il Canale Mii il file non esiste: crearlo è l'unico modo per
        // dare al gioco un Mii da associare a una licenza.
        _ => Ok(vk_save::miidb::create_empty()),
    }
}

/// Riscrive il database, con le protezioni di ogni file di Dolphin.
async fn write_database(state: &Arc<AppState>, bytes: &[u8]) -> AppResult<()> {
    crate::services::saves::require_save_writes()?;
    crate::services::saves::guard_dolphin_not_running(state).await?;

    let path = database_path(state).await?;
    crate::services::saves::write_protected(state, &path, bytes, "mii-database", "RFL_DB").await
}

// ---------------------------------------------------------------------------
// Lettura
// ---------------------------------------------------------------------------

/// Tutti i Mii del database.
///
/// L'ordine è quello del launcher legacy: prima i preferiti, poi per nome.
pub async fn list(state: &Arc<AppState>) -> Vec<MiiView> {
    let Ok(path) = database_path(state).await else {
        return Vec::new();
    };
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return Vec::new();
    };

    let mut out: Vec<MiiView> = vk_save::miidb::read(&bytes)
        .into_values()
        .filter_map(|parsed| view_of(&parsed.raw).ok())
        .collect();

    out.sort_by(|a, b| {
        b.is_favorite
            .cmp(&a.is_favorite)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// I 74 byte di un Mii del database.
pub async fn load_block(state: &Arc<AppState>, id: &str) -> AppResult<Vec<u8>> {
    let mii_id = parse_id(id)?;
    let path = database_path(state).await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| AppError::io(&path, error))?;

    let offset = vk_save::miidb::find_slot(&bytes, mii_id)
        .ok_or_else(|| AppError::BadRequest(format!("Mii sconosciuto: {id}")))?;

    Ok(bytes[offset..offset + mii::BLOCK_SIZE].to_vec())
}

/// Un singolo Mii.
pub async fn load(state: &Arc<AppState>, id: &str) -> AppResult<MiiView> {
    view_of(&load_block(state, id).await?)
}

/// Stato dell'editor di un Mii.
pub async fn editor_state(state: &Arc<AppState>, id: &str) -> AppResult<MiiEditorState> {
    Ok(mii::read_editor_state(&load_block(state, id).await?)?)
}

// ---------------------------------------------------------------------------
// Identità
// ---------------------------------------------------------------------------

/// Stato di partenza per un Mii nuovo, con la data di creazione di oggi.
///
/// `vk-save` è puro e non legge l'orologio: la data la mette qui il servizio,
/// che è l'unico livello autorizzato a conoscere l'ambiente.
pub fn default_state(name: &str, favorite_color_index: u8, is_female: bool) -> MiiEditorState {
    let now = time::OffsetDateTime::now_utc();

    MiiEditorState {
        name: name.to_string(),
        favorite_color_index,
        is_female,
        birth_month: u8::from(now.month()),
        birth_day: now.day(),
        ..MiiEditorState::default()
    }
}

/// Assegna Mii id e system id quando mancano, evitando le collisioni.
///
/// Il contatore della Wii avanza di un passo ogni 4 secondi: due Mii creati
/// nella stessa finestra riceverebbero lo stesso identificativo, e nel database
/// il secondo sovrascriverebbe il primo. Per questo l'id viene cercato fra
/// quelli liberi, non semplicemente generato.
fn ensure_identity(
    editor: MiiEditorState,
    taken: &std::collections::BTreeSet<u32>,
) -> MiiEditorState {
    if !editor.needs_identity() {
        return editor;
    }

    let counter = u32::try_from(
        time::OffsetDateTime::now_utc()
            .unix_timestamp()
            .saturating_sub(MII_ID_EPOCH)
            .max(0)
            / 4,
    )
    .unwrap_or(0);
    let entropy = crate::platform::random_bytes::<1>()[0];

    // Il contatore avanza finché non si trova un identificativo libero: 8192
    // passi coprono nove ore di orologio, ben oltre qualunque uso reale.
    let mii_id = (0..8192u32)
        .map(|step| mii::generate_mii_id(counter.wrapping_add(step), entropy))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| mii::generate_mii_id(counter, entropy));

    editor.with_identity(mii_id, crate::platform::random_bytes::<4>())
}

/// Identificativi già occupati nel database.
fn taken_mii_ids(database: &[u8]) -> std::collections::BTreeSet<u32> {
    vk_save::miidb::read(database).keys().copied().collect()
}

// ---------------------------------------------------------------------------
// Scrittura
// ---------------------------------------------------------------------------

/// Inserisce o sostituisce un blocco nel database e lo salva.
async fn upsert(state: &Arc<AppState>, database: &mut [u8], block: &[u8]) -> AppResult<MiiView> {
    vk_save::miidb::upsert(database, block)?;
    write_database(state, database).await?;
    view_of(block)
}

/// Crea un Mii nuovo dai soli nome e colore, come il pulsante "Nuovo Mii".
pub async fn create(
    state: &Arc<AppState>,
    name: &str,
    favorite_color_index: u8,
    is_female: bool,
) -> AppResult<MiiView> {
    if name.trim().is_empty() {
        return Err(AppError::BadRequest("Scegli un nome per il Mii.".into()));
    }

    create_from_state(state, &default_state(name, favorite_color_index, is_female)).await
}

/// Crea un Mii a partire da uno stato completo dell'editor.
pub async fn create_from_state(
    state: &Arc<AppState>,
    editor: &MiiEditorState,
) -> AppResult<MiiView> {
    if editor.name.trim().is_empty() {
        return Err(AppError::BadRequest("Scegli un nome per il Mii.".into()));
    }

    let mut database = read_database(state).await?;
    let editor = ensure_identity(editor.clone(), &taken_mii_ids(&database));
    let block = mii::write_editor_state(&editor);

    let view = upsert(state, &mut database, &block).await?;
    tracing::info!(mii = %view.id, "Mii creato nel database di Dolphin");
    Ok(view)
}

/// Riscrive un Mii esistente con un nuovo stato dell'editor.
///
/// Il blocco originale viene **modificato**, non ricostruito: i bit che il
/// modello dell'editor non descrive — il flag "mingle", quello di non
/// copiabilità — sopravvivono alla modifica (§D-026).
pub async fn update(
    state: &Arc<AppState>,
    id: &str,
    editor: &MiiEditorState,
) -> AppResult<MiiView> {
    let existing = load_block(state, id).await?;
    let mut database = read_database(state).await?;

    let editor = ensure_identity(editor.clone(), &taken_mii_ids(&database));
    let block = mii::apply_editor_state(&existing, &editor)?;

    // Cambiare il Mii id sposterebbe il Mii in un altro slot e lascerebbe il
    // vecchio dov'era: la modifica ne creerebbe un secondo.
    let previous = parse_id(id)?;
    if mii::read_editor_state(&block)?.mii_id != previous {
        vk_save::miidb::remove(&mut database, previous)?;
    }

    let view = upsert(state, &mut database, &block).await?;
    tracing::info!(mii = %view.id, "Mii aggiornato nel database di Dolphin");
    Ok(view)
}

/// Duplica un Mii.
///
/// Il duplicato riceve un'identità nuova: due Mii con lo stesso Mii id si
/// sovrascriverebbero a vicenda dentro il database.
pub async fn duplicate(state: &Arc<AppState>, id: &str) -> AppResult<MiiView> {
    let source = load_block(state, id).await?;
    let mut editor = mii::read_editor_state(&source)?;

    editor.name = duplicate_name(&editor.name);
    editor.mii_id = 0;
    editor.system_id = [0; 4];
    editor.is_favorite = false;

    let mut database = read_database(state).await?;
    let editor = ensure_identity(editor, &taken_mii_ids(&database));
    let block = mii::apply_editor_state(&source, &editor)?;

    let view = upsert(state, &mut database, &block).await?;
    tracing::info!(mii = %view.id, "Mii duplicato nel database di Dolphin");
    Ok(view)
}

/// Nome del duplicato: al massimo 10 unità UTF-16, con un suffisso numerico.
fn duplicate_name(name: &str) -> String {
    let base = mii::normalize_name(name, "Vanza Mii");

    let mut units: Vec<u16> = base.encode_utf16().take(8).collect();
    if units
        .last()
        .is_some_and(|unit| (0xD800..0xDC00).contains(unit))
    {
        units.pop();
    }

    let trimmed = String::from_utf16_lossy(&units);
    mii::normalize_name(&format!("{} 2", trimmed.trim()), "Vanza Mii")
}

/// Elimina un Mii dal database di Dolphin.
///
/// Non c'è una copia nel launcher da conservare: il Mii sparisce anche dal
/// gioco, che è ciò che l'utente chiede quando preme Elimina. Il database
/// viene copiato e verificato prima di essere toccato.
pub async fn delete(state: &Arc<AppState>, id: &str) -> AppResult<()> {
    let mii_id = parse_id(id)?;
    let mut database = read_database(state).await?;

    if !vk_save::miidb::remove(&mut database, mii_id)? {
        return Err(AppError::BadRequest(format!("Mii sconosciuto: {id}")));
    }

    write_database(state, &database).await?;
    tracing::info!(mii = %id, "Mii eliminato dal database di Dolphin");
    Ok(())
}

// ---------------------------------------------------------------------------
// Import ed export
// ---------------------------------------------------------------------------

/// `true` se l'estensione è uno dei contenitori Mii riconosciuti.
pub fn is_supported_source(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    let extension = format!(".{}", extension.to_string_lossy().to_lowercase());

    mii::SUPPORTED_EXTENSIONS.contains(&extension.as_str())
        || PROFILE_EXTENSIONS.contains(&extension.as_str())
}

/// Importa un Mii da un file scelto dall'utente, dentro il database.
///
/// Il file di partenza non viene toccato. Se porta un Mii id già presente
/// sostituisce quel Mii, com'è giusto: è lo stesso Mii.
pub async fn import_file(state: &Arc<AppState>, source: &Path) -> AppResult<MiiView> {
    if !source.is_file() {
        return Err(AppError::BadRequest(
            "Il file Mii selezionato non esiste.".into(),
        ));
    }
    if !is_supported_source(source) {
        return Err(AppError::BadRequest(
            "Formato non riconosciuto: sono supportati .mii, .miigx, .mae, .rcd, .rsd e i Mii in JSON.".into(),
        ));
    }

    let bytes = tokio::fs::read(source)
        .await
        .map_err(|error| AppError::io(source, error))?;

    let block = extract_block_from(&bytes).ok_or_else(|| {
        AppError::BadRequest("Il file selezionato non contiene dati Mii Wii reali.".into())
    })?;

    let mut database = read_database(state).await?;

    // Un Mii senza identificativo — capita nei file prodotti da editor di
    // terze parti — ne riceve uno libero, altrimenti non entra nel database.
    let editor = mii::read_editor_state(&block)?;
    let block = if editor.mii_id == 0 {
        let editor = ensure_identity(editor, &taken_mii_ids(&database));
        mii::apply_editor_state(&block, &editor)?
    } else {
        block
    };

    let view = upsert(state, &mut database, &block).await?;
    tracing::info!(mii = %view.id, "Mii importato nel database di Dolphin");
    Ok(view)
}

/// Estrae i 74 byte da un file, sia esso binario o un Mii in JSON.
fn extract_block_from(bytes: &[u8]) -> Option<Vec<u8>> {
    if let Some(block) = extract_block_from_json(bytes) {
        return Some(block);
    }
    mii::extract_block(bytes)
}

fn extract_block_from_json(bytes: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let value: serde_json::Value =
        serde_json::from_str(vk_core::json::strip_leading_noise(text)).ok()?;

    // `rawMiiBase64` è il nome nuovo, `RawMiiBase64` quello del launcher C#.
    let encoded = value
        .get("rawMiiBase64")
        .or_else(|| value.get("RawMiiBase64"))?
        .as_str()?;

    let block = mii::base64_decode(encoded)?;
    (block.len() == mii::BLOCK_SIZE && mii::looks_like_wii_mii(&block)).then_some(block)
}

/// Un Mii esportato in JSON: la stessa forma che l'import sa rileggere.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MiiFile {
    name: String,
    creator_name: String,
    mii_id: u32,
    favorite_color: String,
    raw_mii_base64: String,
    studio_data: String,
    sha256: String,
}

/// Esporta un Mii.
///
/// Con estensione `.mii`, `.rcd` o `.rsd` scrive i 74 byte grezzi, che è ciò
/// che gli altri strumenti si aspettano; con `.json` o `.vk-mii` scrive lo
/// stesso blocco dentro un JSON, che questo launcher sa rileggere.
pub async fn export(state: &Arc<AppState>, id: &str, destination: &Path) -> AppResult<String> {
    let block = load_block(state, id).await?;
    let view = view_of(&block)?;

    let extension = destination
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::io(parent, error))?;
        }
    }

    match extension.as_str() {
        "mii" | "rcd" | "rsd" => vk_core::fsx::write_atomic(destination, &block).await?,
        "json" | "vk-mii" => {
            let file = MiiFile {
                name: view.name.clone(),
                creator_name: view.creator_name.clone(),
                mii_id: view.mii_id,
                favorite_color: view.favorite_color.clone(),
                raw_mii_base64: mii::base64_encode(&block),
                studio_data: view.studio_data.clone(),
                sha256: sha256_hex(&block),
            };
            vk_core::fsx::write_json_atomic(destination, &file).await?;
        }
        _ => {
            return Err(AppError::BadRequest(
                "Estensione non supportata: usa .mii, .rcd, .rsd, .json o .vk-mii.".into(),
            ))
        }
    }

    tracing::info!(mii = %view.id, "Mii esportato");
    Ok(vk_core::redact::redact(&destination.to_string_lossy()))
}

// ---------------------------------------------------------------------------
// Mii casuale
// ---------------------------------------------------------------------------

/// Stato casuale, come il pulsante "Random" dell'editor legacy.
///
/// Gli intervalli sono quelli di `CreateRandomMiiState`: più stretti dei
/// massimi assoluti, perché un valore estremo produce un Mii deforme.
pub fn random_state(name: &str) -> MiiEditorState {
    let bytes = crate::platform::random_bytes::<48>();
    let mut cursor = 0usize;
    let mut next = |min: u8, max_exclusive: u8| -> u8 {
        let value = bytes[cursor % bytes.len()];
        cursor += 1;
        min + value % (max_exclusive - min)
    };

    MiiEditorState {
        name: name.to_string(),
        is_female: next(0, 2) == 0,
        is_favorite: true,
        favorite_color_index: next(0, 12),
        height: next(32, 96),
        weight: next(32, 96),

        face_shape: next(0, 8),
        skin_color: next(0, 6),
        facial_feature: next(0, 12),

        hair_type: next(0, 72),
        hair_color: next(0, 8),
        hair_flipped: next(0, 2) == 0,

        eyebrow_type: next(0, 24),
        eyebrow_rotation: next(0, 12),
        eyebrow_color: next(0, 8),
        eyebrow_size: next(2, 9),
        eyebrow_vertical: next(4, 18),
        eyebrow_spacing: next(0, 8),

        eye_type: next(0, 48),
        eye_rotation: next(0, 8),
        eye_vertical: next(6, 20),
        eye_color: next(0, 6),
        eye_size: next(2, 7),
        eye_spacing: next(0, 8),

        nose_type: next(0, 12),
        nose_size: next(2, 9),
        nose_vertical: next(6, 18),

        mouth_type: next(0, 24),
        mouth_color: next(0, 3),
        mouth_size: next(2, 9),
        mouth_vertical: next(8, 20),

        glasses_type: next(0, 8),
        glasses_color: next(0, 6),
        glasses_size: next(2, 7),
        glasses_vertical: next(6, 18),

        mustache_type: next(0, 4),
        beard_type: next(0, 4),
        facial_hair_color: next(0, 8),
        mustache_size: next(2, 9),
        mustache_vertical: next(6, 18),

        mole_enabled: next(0, 3) == 0,
        mole_size: next(2, 9),
        mole_vertical: next(4, 20),
        mole_horizontal: next(4, 20),

        ..default_state(name, 0, false)
    }
}

// ---------------------------------------------------------------------------
// Utilità
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "M".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    /// Database Mii reale e anonimizzato: 22 Mii, 21 identificativi distinti.
    const MII_DATABASE: &[u8] = include_bytes!("../../../crates/vk-save/fixtures/RFL_DB.dat");

    async fn state_at(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    /// Cartella User di Dolphin, con o senza il database della fixture.
    async fn seed_user_folder(dir: &Path, state: &Arc<AppState>, with_database: bool) -> PathBuf {
        let user = dir.join("Dolphin Emulator");
        let database = vk_save::miidb::database_path(&user);
        std::fs::create_dir_all(database.parent().unwrap()).unwrap();
        if with_database {
            std::fs::write(&database, MII_DATABASE).unwrap();
        }

        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();
        database
    }

    fn stored(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    /// La lista è il database di Dolphin, senza import né copie.
    #[tokio::test]
    async fn the_list_is_dolphins_database() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let all = list(&state).await;

        assert_eq!(all.len(), 21, "un Mii per identificativo distinto");
        assert!(all.iter().all(|mii| mii.mii_id != 0));
        assert!(all.iter().all(|mii| !mii.studio_data.is_empty()));
        // L'id è il Mii id: stabile, e lo stesso che il gioco usa.
        assert!(all
            .iter()
            .all(|mii| mii.id == format!("{:08x}", mii.mii_id)));
    }

    #[tokio::test]
    async fn without_a_user_folder_there_is_nothing_to_show() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        assert!(list(&state).await.is_empty());
        assert_eq!(
            create(&state, "Vanza", 0, false).await.unwrap_err().code(),
            "configuration"
        );
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn creating_a_mii_writes_it_into_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let database = seed_user_folder(dir.path(), &state, true).await;

        let created = create(&state, "Vanza", 5, false).await.unwrap();

        assert_eq!(created.name, "Vanza");
        assert_eq!(created.favorite_color, "#3B82F6");
        assert_ne!(created.mii_id, 0);

        let all = list(&state).await;
        assert_eq!(all.len(), 22, "uno in più di prima");
        assert!(all.iter().any(|mii| mii.id == created.id));

        // Il database resta leggibile e il CRC torna.
        vk_save::miidb::verify_crc(&stored(&database)).unwrap();
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn creating_without_a_database_creates_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let database = seed_user_folder(dir.path(), &state, false).await;

        assert!(
            !database.exists(),
            "Dolphin non ha mai aperto il Canale Mii"
        );

        let created = create(&state, "Vanza", 0, false).await.unwrap();

        assert!(database.is_file());
        vk_save::miidb::verify_crc(&stored(&database)).unwrap();
        assert_eq!(list(&state).await.len(), 1);
        assert_eq!(list(&state).await[0].id, created.id);
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn deleting_removes_the_mii_from_dolphin_too() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        let database = seed_user_folder(dir.path(), &state, true).await;

        let target = list(&state).await[0].clone();
        delete(&state, &target.id).await.unwrap();

        assert_eq!(list(&state).await.len(), 20);
        assert!(
            vk_save::miidb::find_slot(&stored(&database), target.mii_id).is_none(),
            "il Mii non è più nel database del gioco"
        );
        assert!(delete(&state, &target.id).await.is_err(), "non c'è più");
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn every_write_backs_the_database_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        create(&state, "Vanza", 0, false).await.unwrap();

        let backups = state.paths.backups_dir().join("mii-database");
        let copies: Vec<_> = std::fs::read_dir(&backups).unwrap().flatten().collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(
            std::fs::read(copies[0].path()).unwrap(),
            MII_DATABASE,
            "il backup è il database prima della modifica"
        );
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn editing_replaces_the_mii_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let before = list(&state).await;
        let target = before[0].clone();

        let mut editor = editor_state(&state, &target.id).await.unwrap();
        editor.name = "Rinominato".into();
        let updated = update(&state, &target.id, &editor).await.unwrap();

        assert_eq!(updated.id, target.id, "l'identità non cambia");
        assert_eq!(updated.name, "Rinominato");

        let after = list(&state).await;
        assert_eq!(after.len(), before.len(), "nessun doppione");
        assert_eq!(after.iter().filter(|mii| mii.id == target.id).count(), 1);
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn duplicating_gives_the_copy_a_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let source = list(&state).await[0].clone();
        let copy = duplicate(&state, &source.id).await.unwrap();

        assert_ne!(copy.mii_id, source.mii_id);
        assert_ne!(copy.id, source.id);
        assert_eq!(list(&state).await.len(), 22);
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_mii_created_now_survives_a_round_trip_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let created = create(&state, "Vanza", 3, true).await.unwrap();
        let file = dir.path().join("fuori/vanza.mii");
        export(&state, &created.id, &file).await.unwrap();

        delete(&state, &created.id).await.unwrap();
        assert!(load(&state, &created.id).await.is_err());

        let back = import_file(&state, &file).await.unwrap();
        assert_eq!(back.id, created.id, "stesso Mii, stesso identificativo");
        assert_eq!(back.name, "Vanza");
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_json_export_can_be_imported_back() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let created = create(&state, "Vanza", 0, false).await.unwrap();
        let file = dir.path().join("vanza.vk-mii");
        export(&state, &created.id, &file).await.unwrap();
        delete(&state, &created.id).await.unwrap();

        assert_eq!(import_file(&state, &file).await.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn importing_refuses_unknown_and_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        let junk = dir.path().join("nota.txt");
        std::fs::write(&junk, b"non un Mii").unwrap();
        assert!(import_file(&state, &junk).await.is_err());

        let empty = dir.path().join("vuoto.mii");
        std::fs::write(&empty, b"").unwrap();
        assert!(import_file(&state, &empty).await.is_err());

        assert!(import_file(&state, &dir.path().join("assente.mii"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn an_unknown_identifier_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state, true).await;

        for id in ["", "non-un-id", "../../settings", "0", "00000000", "123"] {
            assert!(load(&state, id).await.is_err(), "{id}");
        }
        assert!(
            load(&state, "deadbeef").await.is_err(),
            "id ben formato ma assente"
        );
    }

    #[test]
    fn the_identifier_is_the_mii_id_in_hex() {
        assert_eq!(format_id(0x8000_0001), "80000001");
        assert_eq!(parse_id("80000001").unwrap(), 0x8000_0001);
        assert!(parse_id("00000000").is_err(), "zero non è un Mii");
    }

    #[test]
    fn the_favourite_colours_are_the_twelve_of_the_format() {
        assert_eq!(mii::FAVORITE_COLORS.len(), 12);
    }

    #[test]
    fn two_random_states_differ() {
        let first = random_state("Vanza");
        let second = random_state("Vanza");
        assert_ne!(first, second);
    }

    #[test]
    fn the_duplicate_name_stays_within_ten_units() {
        assert_eq!(duplicate_name("Vanza"), "Vanza 2");
        assert!(duplicate_name("NomeLunghissimo").encode_utf16().count() <= 10);
        assert_eq!(duplicate_name(""), "Vanza Mi 2");
    }

    #[test]
    fn only_the_known_containers_are_accepted() {
        for name in [
            "a.mii", "a.miigx", "a.mae", "a.rcd", "a.rsd", "a.json", "a.vk-mii",
        ] {
            assert!(is_supported_source(Path::new(name)), "{name}");
        }
        for name in ["a.txt", "a.png", "a"] {
            assert!(!is_supported_source(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn the_initial_falls_back_to_the_letter_m() {
        assert_eq!(initial("vanza"), "V");
        assert_eq!(initial("  "), "M");
    }
}
