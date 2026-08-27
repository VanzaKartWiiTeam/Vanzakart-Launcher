//! Licenze, amici e salvataggi di Mario Kart Wii.
//!
//! Porta `MkwiiSaveParserService.cs`, `SaveManagerService.cs` e la parte
//! amici di `RksysManager.cs`.
//!
//! Ogni scrittura su `rksys.dat` passa da qui e rispetta tre regole, in questo
//! ordine (vedi `docs/decisions.md` §D-012):
//!
//! 1. **Dolphin non deve essere in esecuzione.** Tiene il salvataggio in
//!    memoria e lo riscrive all'uscita: qualunque modifica fatta adesso
//!    sparirebbe, o peggio, si mescolerebbe alla sua.
//! 2. **Backup obbligatorio, verificato per hash, mai sovrascritto.** Se il
//!    backup non riesce, la scrittura non avviene.
//! 3. **Il file viene modificato, non ricostruito**, e riscritto in modo
//!    atomico. Il CRC globale è rifirmato con la stessa variante che il
//!    salvataggio già usava; se nessuna coincide, il file non si tocca.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::{FriendView, LicenseView};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// `true` se questa build espone le scritture sui formati di salvataggio.
///
/// La feature esiste come interruttore: i test di round-trip su fixture reali
/// passano, quindi è attiva per default, ma resta il modo di produrre una
/// build in sola lettura senza toccare il codice.
pub const SAVE_WRITES_ENABLED: bool = cfg!(feature = "save-writes");

/// Colori di accento dei Mii, usati quando manca un'immagine renderizzata.
const ACCENTS: [&str; 6] = [
    "#39E7FF", "#FF3B7A", "#FFD166", "#4DFFB0", "#9D5CFF", "#FF8800",
];

/// Elenca i salvataggi trovati, nello stesso ordine per tutta la sessione.
///
/// L'indice in questa lista è ciò che il frontend usa per indicare un file:
/// i percorsi che gli arrivano sono redatti e non sono riconvertibili.
///
/// Come `SaveManagerService.GetSaveProfiles`, il salvataggio della **modpack**
/// viene per primo ed è l'unico se esiste: con "Seperate Savegame" attivo il
/// gioco non scrive nella NAND ma sotto `Load/Riivolution/<Mod>/`, e sono
/// quelle le licenze con cui si gioca. Solo quando la modpack non ha ancora un
/// salvataggio si ricade sugli altri `rksys.dat` della cartella User, così la
/// pagina resta utile invece di restare vuota.
pub async fn save_files(state: &Arc<AppState>) -> Vec<PathBuf> {
    let user_folder = state.settings.read().await.user_folder();
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Vec::new();
    }

    let layout = state.layout(state.channel().await).await;
    let modpack = vk_save::rksys::find_mod_save_files(
        &user_folder,
        &layout.mod_root(),
        layout.directory_name(),
    );
    if !modpack.is_empty() {
        return modpack;
    }

    vk_save::rksys::find_save_files(&user_folder)
}

async fn save_at(state: &Arc<AppState>, index: usize) -> AppResult<PathBuf> {
    save_files(state)
        .await
        .into_iter()
        .nth(index)
        .ok_or_else(|| AppError::Configuration("Salvataggio non trovato.".into()))
}

/// Elenca le licenze di tutti i salvataggi trovati.
pub async fn list_licenses(state: &Arc<AppState>) -> AppResult<Vec<LicenseView>> {
    let user_folder = state.settings.read().await.user_folder();
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Ok(Vec::new());
    }

    let mii_database = read_mii_database(&user_folder).await;
    let mut out = Vec::new();

    for (save_index, save_path) in save_files(state).await.into_iter().enumerate() {
        let Ok(bytes) = tokio::fs::read(&save_path).await else {
            continue;
        };

        let game_id = vk_save::rksys::game_id_from_path(&save_path).unwrap_or_default();
        let region = vk_save::rksys::region_label(&game_id);
        let save_label = vk_core::redact::redact(&save_path.to_string_lossy());

        // Uno slot senza nome e senza profile ID conta come vuoto solo se il
        // suo Mii non è nel database di Dolphin, come nel legacy.
        let cards = vk_save::rksys::read_license_cards_with(&bytes, &|mii_id| {
            mii_database.contains_key(&mii_id)
        });

        for card in cards {
            let mii = mii_database.get(&card.mii_id);

            out.push(LicenseView {
                save_index,
                slot: card.slot,
                is_empty: card.is_empty,
                mii_name: mii.map(|mii| mii.name.clone()).unwrap_or_else(|| {
                    if card.is_empty {
                        String::new()
                    } else {
                        "Mii non trovato in RFL_DB.dat".into()
                    }
                }),
                accent_color: mii
                    .map(|mii| mii.favorite_color().to_string())
                    .unwrap_or_else(|| ACCENTS[0].to_string()),
                mii_id: card.mii_id,
                studio_data: mii
                    .map(|mii| vk_save::mii::studio_data(&mii.raw))
                    .unwrap_or_default(),
                avatar_initial: initial(mii.map_or(card.name.as_str(), |mii| mii.name.as_str())),
                win_rate: card.win_rate(),
                friend_count: vk_save::rksys::read_friends(&bytes, card.slot).len(),
                name: card.name,
                friend_code: card.friend_code,
                vr: u32::from(card.vr),
                br: u32::from(card.br),
                races: card.races,
                wins: card.wins,
                source_label: "Dolphin save".into(),
                save_path: save_label.clone(),
                region: region.clone(),
            });
        }
    }

    Ok(out)
}

/// Database Mii di Dolphin, indicizzato per Mii id.
async fn read_mii_database(user_folder: &Path) -> std::collections::BTreeMap<u32, vk_save::WiiMii> {
    let path = vk_save::miidb::database_path(user_folder);
    match tokio::fs::read(&path).await {
        Ok(bytes) => vk_save::miidb::read(&bytes),
        Err(_) => std::collections::BTreeMap::new(),
    }
}

/// Riepilogo dei salvataggi trovati, per la pagina Mii & Licenses.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOverview {
    pub user_folder_configured: bool,
    pub save_files: Vec<String>,
    pub mii_count: usize,
    pub license_count: usize,
    pub backup_count: usize,
    pub message: String,
}

/// Stato dei salvataggi, senza leggerne il contenuto per intero.
pub async fn overview(state: &Arc<AppState>) -> AppResult<SaveOverview> {
    let user_folder = state.settings.read().await.user_folder();
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Ok(SaveOverview {
            message: "Seleziona la cartella User di Dolphin per leggere licenze e Mii.".into(),
            ..Default::default()
        });
    }

    let saves = save_files(state).await;
    let mii_count = read_mii_database(&user_folder).await.len();
    let licenses = list_licenses(state).await?;

    Ok(SaveOverview {
        user_folder_configured: true,
        save_files: saves
            .iter()
            .map(|path| vk_core::redact::redact(&path.to_string_lossy()))
            .collect(),
        mii_count,
        license_count: licenses.iter().filter(|card| !card.is_empty).count(),
        backup_count: save_backups(state).len(),
        message: if saves.is_empty() {
            "Nessun salvataggio di Mario Kart Wii trovato in questa cartella User.".into()
        } else {
            format!("{} file di salvataggio trovati.", saves.len())
        },
    })
}

// ---------------------------------------------------------------------------
// Amici
// ---------------------------------------------------------------------------

/// Legge la lista amici di una licenza.
pub async fn list_friends(
    state: &Arc<AppState>,
    save_index: usize,
    license: usize,
) -> AppResult<Vec<FriendView>> {
    let path = save_at(state, save_index).await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| AppError::io(&path, error))?;

    Ok(to_views(vk_save::rksys::read_friends(&bytes, license)))
}

/// Aggiunge un amico a una licenza a partire dal suo friend code.
pub async fn add_friend(
    state: &Arc<AppState>,
    save_index: usize,
    license: usize,
    friend_code: &str,
) -> AppResult<Vec<FriendView>> {
    require_save_writes()?;
    let profile_id = vk_save::friend_code::parse(friend_code)?;

    let path = save_at(state, save_index).await?;
    let mut bytes = read_for_write(state, &path).await?;

    let slot = vk_save::rksys::add_friend(&mut bytes, license, profile_id)?;
    write_save(state, &path, &bytes).await?;

    tracing::info!(license, slot, "amico aggiunto alla licenza");
    Ok(to_views(vk_save::rksys::read_friends(&bytes, license)))
}

/// Rimuove l'amico che occupa uno slot.
pub async fn remove_friend(
    state: &Arc<AppState>,
    save_index: usize,
    license: usize,
    slot: usize,
) -> AppResult<Vec<FriendView>> {
    require_save_writes()?;

    let path = save_at(state, save_index).await?;
    let mut bytes = read_for_write(state, &path).await?;

    vk_save::rksys::remove_friend(&mut bytes, license, slot)?;
    write_save(state, &path, &bytes).await?;

    tracing::info!(license, slot, "amico rimosso dalla licenza");
    Ok(to_views(vk_save::rksys::read_friends(&bytes, license)))
}

// ---------------------------------------------------------------------------
// Mii della licenza
// ---------------------------------------------------------------------------

/// Assegna a una licenza uno dei Mii del database di Dolphin.
///
/// Porta `SaveManagerService.ApplyMiiToLicenseAsync`. Il Mii sta già in
/// `RFL_DB.dat` — è l'unico posto in cui i Mii esistono (§D-037) — quindi
/// resta una scrittura sola: la licenza comincia a indicarlo. Il salvataggio
/// viene copiato e verificato prima di essere toccato.
pub async fn set_license_mii(
    state: &Arc<AppState>,
    save_index: usize,
    license: usize,
    mii_id: &str,
) -> AppResult<Vec<LicenseView>> {
    require_save_writes()?;

    let block = crate::services::mii::load_block(state, mii_id).await?;
    let mii = vk_save::mii::parse_block(&block)?;

    let path = save_at(state, save_index).await?;
    let mut bytes = read_for_write(state, &path).await?;

    vk_save::rksys::update_license_mii(&mut bytes, license, &mii.name, mii.mii_id, &block)?;
    write_save(state, &path, &bytes).await?;

    tracing::info!(license, "Mii assegnato alla licenza");
    list_licenses(state).await
}

fn to_views(friends: Vec<vk_save::rksys::SaveFriend>) -> Vec<FriendView> {
    friends
        .into_iter()
        .map(|friend| FriendView {
            avatar_initial: initial(&friend.mii_name),
            accent_color: ACCENTS[friend.slot % ACCENTS.len()].to_string(),
            slot: friend.slot,
            friend_code: friend.friend_code,
            mii_name: friend.mii_name,
            studio_data: friend.studio_data,
            wins: u32::from(friend.wins),
            losses: u32::from(friend.losses),
            race_rating: u32::from(friend.race_rating),
            battle_rating: u32::from(friend.battle_rating),
            is_pending: friend.is_pending,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scrittura protetta
// ---------------------------------------------------------------------------

pub(crate) fn require_save_writes() -> AppResult<()> {
    if SAVE_WRITES_ENABLED {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "questa build del launcher non abilita le scritture sui salvataggi".into(),
        ))
    }
}

/// Rifiuta l'operazione se Dolphin è aperto.
///
/// Tiene il salvataggio in memoria e lo riscrive all'uscita: qualunque
/// modifica fatta adesso andrebbe persa, o peggio si mescolerebbe alla sua.
pub(crate) async fn guard_dolphin_not_running(state: &Arc<AppState>) -> AppResult<()> {
    let dolphin = state.settings.read().await.dolphin();
    if !dolphin.as_os_str().is_empty() && crate::platform::is_executable_running(&dolphin) {
        return Err(AppError::BadRequest(
            "Chiudi Dolphin prima di modificare i suoi file: altrimenti riscriverebbe le modifiche all'uscita.".into(),
        ));
    }
    Ok(())
}

/// Legge un salvataggio che sta per essere modificato.
async fn read_for_write(state: &Arc<AppState>, path: &Path) -> AppResult<Vec<u8>> {
    guard_dolphin_not_running(state).await?;

    tokio::fs::read(path)
        .await
        .map_err(|error| AppError::io(path, error))
}

/// Scrive un file di Dolphin dopo averne verificato il backup.
///
/// `backups` è la sottocartella di destinazione del backup e `prefix` il
/// prefisso del suo nome: `rksys` per i salvataggi, `RFL_DB` per il database
/// Mii. Se il backup fallisce o non coincide con l'originale, il file non
/// viene toccato.
pub(crate) async fn write_protected(
    state: &Arc<AppState>,
    path: &Path,
    bytes: &[u8],
    backups: &str,
    prefix: &str,
) -> AppResult<()> {
    if path.exists() {
        let backup = backup_verified(state, path, backups, prefix).await?;
        tracing::info!(
            backup = %vk_core::redact::redact(&backup.to_string_lossy()),
            "backup verificato prima della scrittura"
        );
    }

    vk_core::fsx::write_atomic(path, bytes).await?;
    Ok(())
}

async fn write_save(state: &Arc<AppState>, path: &Path, bytes: &[u8]) -> AppResult<()> {
    let game_id = vk_save::rksys::game_id_from_path(path).unwrap_or_else(|| "unknown".into());
    write_protected(state, path, bytes, "saves", &format!("rksys-{game_id}")).await
}

/// Copia un file nella cartella dei backup e ne verifica l'hash.
///
/// Il nome contiene il timestamp e non viene mai riusato: un backup esistente
/// non si sovrascrive, si affianca.
pub(crate) async fn backup_verified(
    state: &Arc<AppState>,
    source: &Path,
    backups: &str,
    prefix: &str,
) -> AppResult<PathBuf> {
    let directory = state.paths.backups_dir().join(backups);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    let stamp = vk_core::fsx::backup_timestamp();
    let mut destination = directory.join(format!("{prefix}-{stamp}.dat"));
    let mut attempt = 1;
    while destination.exists() {
        destination = directory.join(format!("{prefix}-{stamp}-{attempt}.dat"));
        attempt += 1;
        if attempt > 100 {
            return Err(AppError::Storage(
                "impossibile creare un nome di backup libero".into(),
            ));
        }
    }

    vk_core::fsx::copy_file(source, &destination).await?;

    let expected = vk_core::hash::sha256_file(source).await?;
    let actual = vk_core::hash::sha256_file(&destination).await?;
    if !expected.eq_ignore_ascii_case(&actual) {
        let _ = tokio::fs::remove_file(&destination).await;
        return Err(AppError::Storage(
            "il backup non coincide con l'originale: nulla è stato modificato".into(),
        ));
    }

    Ok(destination)
}

/// Il salvataggio su cui operano import, export e ripristino.
///
/// È il primo dell'elenco, cioè quello della modpack quando esiste: porta
/// `SaveManagerService.GetPrimarySaveProfile`.
async fn primary_save(state: &Arc<AppState>) -> AppResult<PathBuf> {
    save_files(state).await.into_iter().next().ok_or_else(|| {
        AppError::Configuration(
            "Nessun salvataggio di Mario Kart Wii trovato nella cartella User di Dolphin.".into(),
        )
    })
}

/// Crea un backup su richiesta esplicita dell'utente.
pub async fn backup_save(state: &Arc<AppState>) -> AppResult<String> {
    let source = primary_save(state).await?;

    let game_id = vk_save::rksys::game_id_from_path(&source).unwrap_or_else(|| "unknown".into());
    let destination = backup_verified(state, &source, "saves", &format!("rksys-{game_id}")).await?;

    tracing::info!("backup del salvataggio creato");
    Ok(vk_core::redact::redact(&destination.to_string_lossy()))
}

/// Sostituisce il salvataggio corrente con un `rksys.dat` scelto dall'utente.
///
/// Porta `SaveManagerService.ImportSaveFileAsync`: il file corrente viene
/// copiato e verificato prima di essere sovrascritto.
///
/// *Divergenza dal legacy*: il file di partenza deve avere la firma
/// `RKSD0006`. Il legacy copiava qualunque cosa gli si desse, e un file
/// sbagliato distruggeva il salvataggio senza dirlo.
pub async fn import_save(state: &Arc<AppState>, source: &Path) -> AppResult<String> {
    require_save_writes()?;
    guard_dolphin_not_running(state).await?;

    let bytes = tokio::fs::read(source)
        .await
        .map_err(|error| AppError::io(source, error))?;

    if !vk_save::rksys::has_rksys_magic(&bytes) {
        return Err(AppError::BadRequest(
            "Il file scelto non è un salvataggio di Mario Kart Wii: nulla è stato modificato."
                .into(),
        ));
    }

    let destination = primary_save(state).await?;
    write_save(state, &destination, &bytes).await?;

    tracing::info!("salvataggio importato sopra quello corrente");
    Ok(vk_core::redact::redact(&destination.to_string_lossy()))
}

/// Copia il salvataggio corrente dove l'utente ha scelto.
///
/// Porta `SaveManagerService.ExportPrimarySaveAsync`. Non modifica nulla, e
/// quindi non richiede né Dolphin chiuso né le scritture abilitate.
pub async fn export_save(state: &Arc<AppState>, destination: &Path) -> AppResult<String> {
    let source = primary_save(state).await?;
    vk_core::fsx::copy_file(&source, destination).await?;

    tracing::info!("salvataggio esportato");
    Ok(vk_core::redact::redact(&destination.to_string_lossy()))
}

/// Rimette in gioco un backup, dopo averne fatto uno del file corrente.
///
/// Porta `SaveManagerService.RestoreBackupAsync`. Il backup si indica per
/// **nome**, uno di quelli che `save_backups` elenca: il frontend non riceve
/// percorsi e non può costruirne (§D-017).
pub async fn restore_backup(state: &Arc<AppState>, name: &str) -> AppResult<String> {
    require_save_writes()?;
    guard_dolphin_not_running(state).await?;

    if !save_backups(state)
        .iter()
        .any(|candidate| candidate == name)
    {
        return Err(AppError::BadRequest("Questo backup non esiste più.".into()));
    }

    let source = save_backup_dir(state).join(name);
    let bytes = tokio::fs::read(&source)
        .await
        .map_err(|error| AppError::io(&source, error))?;

    if !vk_save::rksys::has_rksys_magic(&bytes) {
        return Err(AppError::BadRequest(
            "Il backup scelto non è un salvataggio leggibile: nulla è stato modificato.".into(),
        ));
    }

    let destination = primary_save(state).await?;
    write_save(state, &destination, &bytes).await?;

    tracing::info!("backup ripristinato sopra il salvataggio corrente");
    Ok(vk_core::redact::redact(&destination.to_string_lossy()))
}

/// Backup dei salvataggi presenti, dal più recente.
pub fn save_backups(state: &Arc<AppState>) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(save_backup_dir(state)) else {
        return Vec::new();
    };

    let mut out: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    out.sort_by(|a, b| b.cmp(a));
    out
}

fn save_backup_dir(state: &Arc<AppState>) -> PathBuf {
    state.paths.backups_dir().join("saves")
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

    /// Il `rksys.dat` reale e anonimizzato usato anche dai test di `vk-save`.
    const FIXTURE: &[u8] = include_bytes!("../../../crates/vk-save/fixtures/rksys.dat");

    async fn state_with(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    async fn seed_user_folder(dir: &Path, state: &Arc<AppState>) -> PathBuf {
        let user = dir.join("Dolphin Emulator");
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");
        std::fs::create_dir_all(save.parent().unwrap()).unwrap();
        std::fs::write(&save, FIXTURE).unwrap();

        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();
        user
    }

    #[tokio::test]
    async fn without_a_user_folder_there_are_no_licenses() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert!(list_licenses(&state).await.unwrap().is_empty());

        let overview = overview(&state).await.unwrap();
        assert!(!overview.user_folder_configured);
        assert!(overview.message.contains("Seleziona"));
    }

    #[tokio::test]
    async fn licenses_are_read_from_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let licenses = list_licenses(&state).await.unwrap();

        assert_eq!(licenses.len(), 4, "quattro slot sempre presenti");
        let first = &licenses[0];
        assert!(!first.is_empty);
        assert!(!first.name.trim().is_empty());
        assert_eq!(first.save_index, 0);
        assert_eq!(first.region, "NTSC-J (Japan)");
        assert!(!first.friend_code.is_empty());
        assert_eq!(first.friend_count, 30);
        assert_eq!(licenses[1].friend_count, 0);
    }

    #[tokio::test]
    async fn the_overview_counts_what_it_found() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let overview = overview(&state).await.unwrap();
        assert!(overview.user_folder_configured);
        assert_eq!(overview.save_files.len(), 1);
        assert_eq!(overview.license_count, 4);
        assert_eq!(overview.mii_count, 0, "senza RFL_DB.dat non ci sono Mii");
    }

    #[tokio::test]
    async fn the_save_path_is_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let licenses = list_licenses(&state).await.unwrap();
        assert!(licenses[0].save_path.ends_with("rksys.dat"));
    }

    #[tokio::test]
    async fn friends_are_read_per_license() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let friends = list_friends(&state, 0, 0).await.unwrap();
        assert_eq!(friends.len(), 30);
        assert!(friends.iter().all(|friend| !friend.friend_code.is_empty()));
        assert!(friends
            .iter()
            .all(|friend| !friend.avatar_initial.is_empty()));

        assert!(list_friends(&state, 0, 1).await.unwrap().is_empty());
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn adding_a_friend_backs_the_save_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        let code = vk_save::friend_code::format(0x1234_5678);
        let friends = add_friend(&state, 0, 1, &code).await.unwrap();

        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].friend_code, code);
        assert!(friends[0].is_pending);

        // Il backup esiste, è uno solo e contiene il file *prima* della
        // modifica.
        let backups = save_backups(&state);
        assert_eq!(backups.len(), 1);
        let backup = save_backup_dir(&state).join(&backups[0]);
        assert_eq!(std::fs::read(&backup).unwrap(), FIXTURE);

        // Il salvataggio su disco è cambiato ed è ancora valido.
        let written = std::fs::read(&save).unwrap();
        assert_ne!(written, FIXTURE);
        vk_save::rksys::verify_global_crc(&written).unwrap();
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn removing_the_friend_restores_the_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        let code = vk_save::friend_code::format(0x1234_5678);
        add_friend(&state, 0, 1, &code).await.unwrap();
        let friends = remove_friend(&state, 0, 1, 0).await.unwrap();

        assert!(friends.is_empty());
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
        assert_eq!(save_backups(&state).len(), 2, "un backup per scrittura");
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_malformed_friend_code_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        for code in ["", "1234", "0000-0000-0000", "abcd-efgh-ijkl"] {
            assert!(add_friend(&state, 0, 1, code).await.is_err(), "{code}");
        }

        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
        assert!(save_backups(&state).is_empty(), "nessun backup inutile");
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_full_license_refuses_new_friends_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        let code = vk_save::friend_code::format(0x1234_5678);
        assert!(add_friend(&state, 0, 0, &code).await.is_err());

        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
        assert!(save_backups(&state).is_empty());
    }

    #[tokio::test]
    async fn an_unknown_save_index_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        assert_eq!(
            list_friends(&state, 7, 0).await.unwrap_err().code(),
            "configuration"
        );
        #[cfg(feature = "save-writes")]
        assert_eq!(
            add_friend(&state, 7, 0, &vk_save::friend_code::format(1))
                .await
                .unwrap_err()
                .code(),
            "configuration"
        );
    }

    /// La regressione che faceva sparire le licenze: con "Seperate Savegame"
    /// il gioco scrive sotto `Load/Riivolution/<Mod>/`, non nella NAND, e il
    /// launcher deve mostrare **quelle** licenze.
    #[tokio::test]
    async fn the_modpack_save_wins_over_the_nand_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;

        let modpack =
            user.join("Load/Riivolution/VanzaKart/riivolution/save/VanzaWFC2/RMCP/rksys.dat");
        std::fs::create_dir_all(modpack.parent().unwrap()).unwrap();
        std::fs::write(&modpack, FIXTURE).unwrap();

        let found = save_files(&state).await;
        assert_eq!(
            found,
            vec![modpack],
            "il salvataggio della modpack è l'unico"
        );

        let licenses = list_licenses(&state).await.unwrap();
        assert_eq!(licenses.len(), 4);
        assert_eq!(licenses[0].region, "PAL (Europe)");
    }

    /// Senza il salvataggio della modpack la pagina non resta vuota: si mostra
    /// quello che c'è nella cartella User.
    #[tokio::test]
    async fn without_a_modpack_save_the_other_ones_are_still_listed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;

        assert_eq!(
            save_files(&state).await,
            vec![user.join("Wii/title/00010004/524d434a/data/rksys.dat")]
        );
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn assigning_a_mii_to_a_license_updates_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        let mii = crate::services::mii::create(&state, "Vanza", 3, false)
            .await
            .unwrap();

        let licenses = set_license_mii(&state, 0, 0, &mii.id).await.unwrap();

        // La licenza porta il nome del Mii ed è ancora leggibile.
        assert_eq!(licenses[0].name, "Vanza");
        assert_eq!(licenses[0].mii_name, "Vanza");
        assert!(!licenses[0].is_empty);

        // Il salvataggio è cambiato, resta valido e c'è il backup.
        let written = std::fs::read(&save).unwrap();
        assert_ne!(written, FIXTURE);
        vk_save::rksys::verify_global_crc(&written).unwrap();
        assert_eq!(save_backups(&state).len(), 1);

        // Il Mii è finito anche nel database di Dolphin, altrimenti il gioco
        // mostrerebbe una licenza senza faccia.
        let database = std::fs::read(vk_save::miidb::database_path(&user)).unwrap();
        assert!(vk_save::miidb::find_slot(&database, mii.mii_id).is_some());
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn an_unknown_mii_leaves_the_save_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        assert!(set_license_mii(&state, 0, 0, "non-esiste").await.is_err());
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
        assert!(save_backups(&state).is_empty());
    }

    #[tokio::test]
    async fn backing_up_requires_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        assert_eq!(
            backup_save(&state).await.unwrap_err().code(),
            "configuration"
        );
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_save_can_be_exported_imported_and_restored() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        // Export: il file esce identico.
        let exported = dir.path().join("fuori/rksys_export.dat");
        export_save(&state, &exported).await.unwrap();
        assert_eq!(std::fs::read(&exported).unwrap(), FIXTURE);

        // Una modifica qualunque, poi l'import lo rimette com'era.
        let code = vk_save::friend_code::format(0x1234_5678);
        add_friend(&state, 0, 1, &code).await.unwrap();
        assert_ne!(std::fs::read(&save).unwrap(), FIXTURE);

        import_save(&state, &exported).await.unwrap();
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);

        // Il ripristino di un backup passa dal nome, non dal percorso.
        add_friend(&state, 0, 1, &code).await.unwrap();
        let backups = save_backups(&state);
        let original = backups
            .iter()
            .find(|name| std::fs::read(save_backup_dir(&state).join(name)).unwrap() == FIXTURE)
            .cloned()
            .expect("un backup contiene il file originale");

        restore_backup(&state, &original).await.unwrap();
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
    }

    #[cfg(feature = "save-writes")]
    #[tokio::test]
    async fn a_file_that_is_not_a_save_is_never_imported() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        let user = seed_user_folder(dir.path(), &state).await;
        let save = user.join("Wii/title/00010004/524d434a/data/rksys.dat");

        let junk = dir.path().join("junk.dat");
        std::fs::write(&junk, b"non sono un salvataggio").unwrap();

        assert!(import_save(&state, &junk).await.is_err());
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
        assert!(save_backups(&state).is_empty(), "nessun backup inutile");

        // E un nome di backup inventato non diventa un percorso.
        assert!(restore_backup(&state, "../../altrove.dat").await.is_err());
        assert_eq!(std::fs::read(&save).unwrap(), FIXTURE);
    }

    #[tokio::test]
    async fn each_backup_gets_its_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        backup_save(&state).await.unwrap();
        backup_save(&state).await.unwrap();

        let backups = save_backups(&state);
        assert_eq!(backups.len(), 2, "un backup non sovrascrive l'altro");
        assert!(backups.iter().all(|name| name.starts_with("rksys-")));
    }

    #[test]
    fn the_save_write_switch_agrees_with_the_build() {
        // I test di round-trip su fixture reali passano, quindi la feature è
        // nel set predefinito: qui si verifica solo che la guardia segua il
        // flag di compilazione, in entrambe le direzioni.
        assert_eq!(require_save_writes().is_ok(), SAVE_WRITES_ENABLED);
        assert_eq!(
            cfg!(feature = "save-writes"),
            SAVE_WRITES_ENABLED,
            "la costante non segue la feature"
        );
    }

    #[test]
    fn the_initial_falls_back_to_the_letter_m() {
        assert_eq!(initial("vanza"), "V");
        assert_eq!(initial("  "), "M");
        assert_eq!(initial("èlite"), "È");
    }
}
