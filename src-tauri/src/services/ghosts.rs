//! Classifica dei ghost del time trial.
//!
//! Il backend .NET del server ha già tutto: l'elenco delle piste, i record, la
//! classifica di ogni pista e il file `.rkg` di ogni tempo. Quello che il
//! server **non** sa è dove il gioco cerca i ghost, perché dipende dalla
//! modpack installata: il nome della cartella Pulsar sta in `Config.pul` e il
//! CRC di ogni pista in `FolderToTrackName.txt` (vedi `vk_save::ghost`).
//! Questo modulo mette insieme le due metà (§D-087).
//!
//! Scaricare un ghost non prende il turno delle operazioni lunghe: pesa pochi
//! KB, scrive nella NAND e non nella modpack, e non ha senso che aspetti la
//! fine di un aggiornamento da un gigabyte. Le scritture dei ghost sono però
//! in fila fra loro, così due download dello stesso tempo non scelgono lo
//! stesso nome di file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vk_core::Channel;
use vk_save::ghost::{self as pulsar_ghost, TrackCrcMap};

use crate::domain::wii_text::humanize;
use crate::error::{AppError, AppResult};
use crate::services::community::loose;
use crate::state::{now_iso, AppState};

/// Per quanto elenco delle piste e record restano validi senza richiederli.
const CATALOG_TTL: Duration = Duration::from_secs(300);

/// Tempi per pagina nella classifica di una pista.
pub const PAGE_SIZE: u32 = 25;

/// Cilindrata dei ghost. La modpack usa solo i 150cc, e la cartella che il
/// gioco legge si chiama così.
pub const CC: u16 = 150;

/// Nome del registro dei ghost scaricati dal launcher, nella cartella dati.
const INDEX_FILE: &str = "ghosts.json";

// ---------------------------------------------------------------------------
// Tipi per il frontend
// ---------------------------------------------------------------------------

/// Il record di una pista.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostRecordView {
    pub submission_id: i64,
    /// Chi ha fatto il tempo: il nome del Mii (vedi [`player_names`]).
    pub player_name: String,
    /// Profilo con cui il tempo è stato caricato, solo se dice qualcosa in
    /// più del nome; vuoto altrimenti.
    pub profile_name: String,
    pub country: String,
    pub finish_time_ms: u32,
    pub finish_time: String,
    pub date_set: String,
    pub character: String,
    pub vehicle: String,
    /// `true` se il ghost del record è già stato scaricato dal launcher.
    pub installed: bool,
}

/// Una pista, con ciò che serve alla lista delle mappe.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostTrackView {
    pub id: i64,
    pub course_id: u32,
    pub name: String,
    pub category: String,
    pub laps: u32,
    pub sort_order: i64,
    pub record: Option<GhostRecordView>,
    /// `true` quando un ghost di questa pista si può installare adesso.
    pub installable: bool,
    /// Perché no, quando non si può: un codice stabile per la UI.
    pub blocker: String,
    /// Ghost già presenti nella cartella della pista, di chiunque siano.
    pub installed_count: usize,
}

/// La lista delle mappe.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostCatalogView {
    pub tracks: Vec<GhostTrackView>,
    pub channel: Channel,
    pub cc: u16,
    /// Motivo per cui **nessuna** pista è installabile, se ce n'è uno.
    pub blocker: String,
    /// `false` quando i record non si sono potuti leggere: la lista c'è lo
    /// stesso, senza tempi.
    pub records_available: bool,
}

/// Un tempo nella classifica di una pista.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostEntryView {
    pub submission_id: i64,
    pub rank: u32,
    /// Chi ha fatto il tempo: il nome del Mii (vedi [`player_names`]).
    pub player_name: String,
    /// Profilo con cui il tempo è stato caricato, solo se dice qualcosa in
    /// più del nome; vuoto altrimenti.
    pub profile_name: String,
    pub country: String,
    pub country_name: String,
    pub finish_time_ms: u32,
    pub finish_time: String,
    pub fastest_lap: String,
    pub lap_splits: Vec<String>,
    pub character: String,
    pub vehicle: String,
    /// Id del controller come lo scrive il gioco: lo traduce la UI.
    pub controller: u32,
    pub automatic_drift: bool,
    pub shroomless: bool,
    pub date_set: String,
    /// `true` se questo ghost è stato scaricato dal launcher ed è ancora lì.
    pub installed: bool,
}

/// La classifica di una pista, una pagina alla volta.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostLeaderboardView {
    pub track: GhostTrackView,
    pub entries: Vec<GhostEntryView>,
    pub page: u32,
    pub total_pages: u32,
    pub total: u32,
    pub fastest_lap: String,
}

/// Esito dell'installazione di un ghost.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostInstallOutcome {
    pub submission_id: i64,
    pub track_name: String,
    pub file_name: String,
    /// `true` quando lo stesso identico ghost c'era già.
    pub already_present: bool,
    pub installed_count: usize,
}

// ---------------------------------------------------------------------------
// Catalogo remoto
// ---------------------------------------------------------------------------

/// Una pista come la descrive il server.
#[derive(Debug, Clone, Default)]
pub struct RemoteTrack {
    pub id: i64,
    pub course_id: u32,
    pub name: String,
    pub category: String,
    pub laps: u32,
    pub sort_order: i64,
}

/// Piste e record, come li ha mandati il server.
#[derive(Debug, Default)]
pub struct RemoteCatalog {
    pub tracks: Vec<RemoteTrack>,
    pub records: HashMap<i64, GhostRecordView>,
    pub records_available: bool,
}

fn api_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path)
}

async fn api_base(state: &Arc<AppState>) -> AppResult<String> {
    let base = state.endpoints.read().await.timetrial_api_url.clone();
    if base.trim().is_empty() {
        return Err(AppError::Configuration(
            "time trial endpoint not configured".into(),
        ));
    }
    Ok(base)
}

async fn get_json(state: &Arc<AppState>, url: &str) -> AppResult<Value> {
    let raw = state.downloader.get_string(url).await?;
    serde_json::from_str(vk_core::json::strip_leading_noise(&raw))
        .map_err(|error| AppError::Internal(format!("invalid time trial response: {error}")))
}

/// Piste e record, dalla cache o dal server.
async fn remote_catalog(state: &Arc<AppState>, refresh: bool) -> AppResult<Arc<RemoteCatalog>> {
    if !refresh {
        let cached = {
            let guard = state.ghost_catalog.read().await;
            guard.clone()
        };
        if let Some((fetched_at, catalog)) = cached {
            if fetched_at.elapsed() < CATALOG_TTL {
                return Ok(catalog);
            }
        }
    }

    let base = api_base(state).await?;
    let tracks = parse_tracks(&get_json(state, &api_url(&base, "tracks")).await?);

    // Senza record la lista resta utile: si vedono le piste e si aprono le
    // classifiche. È un ripiego, non un errore.
    let records = match get_json(state, &api_url(&base, &format!("worldrecords/all?cc={CC}"))).await
    {
        Ok(payload) => Some(parse_records(&payload)),
        Err(error) => {
            tracing::warn!(
                error = %vk_core::redact::redact(&error.to_string()),
                "record del time trial non disponibili"
            );
            None
        }
    };

    let catalog = Arc::new(RemoteCatalog {
        tracks,
        records_available: records.is_some(),
        records: records.unwrap_or_default(),
    });

    if !catalog.tracks.is_empty() {
        *state.ghost_catalog.write().await = Some((Instant::now(), catalog.clone()));
    }
    Ok(catalog)
}

fn parse_tracks(payload: &Value) -> Vec<RemoteTrack> {
    let items = match payload {
        Value::Array(items) => items.as_slice(),
        other => loose::array(other, &["tracks", "data"]),
    };

    let mut tracks: Vec<RemoteTrack> = items
        .iter()
        .map(|item| RemoteTrack {
            id: i64::from(loose::int(item, &["id", "Id", "trackId"])),
            course_id: loose::count(item, &["courseId", "course_id", "CourseId"]),
            name: humanize(&loose::text(item, &["name", "Name", "trackName"])),
            category: loose::text(item, &["category", "Category"]),
            laps: match loose::count(item, &["laps", "Laps"]) {
                0 => 3,
                laps => laps,
            },
            sort_order: i64::from(loose::int(item, &["sortOrder", "sort_order", "SortOrder"])),
        })
        .filter(|track| track.id > 0 && !track.name.is_empty())
        .collect();

    tracks.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    tracks
}

fn parse_records(payload: &Value) -> HashMap<i64, GhostRecordView> {
    let items = match payload {
        Value::Array(items) => items.as_slice(),
        other => loose::array(other, &["records", "data"]),
    };

    items
        .iter()
        .filter_map(|item| {
            let track_id = i64::from(loose::int(item, &["trackId", "track_id"]));
            let record = loose::pick(item, &["activeWorldRecord", "worldRecord", "record"])?;
            (track_id > 0 && record.is_object()).then(|| (track_id, record_view(record)))
        })
        .collect()
}

fn record_view(value: &Value) -> GhostRecordView {
    let entry = entry_view(value);
    GhostRecordView {
        submission_id: entry.submission_id,
        player_name: entry.player_name,
        profile_name: entry.profile_name,
        country: entry.country,
        finish_time_ms: entry.finish_time_ms,
        finish_time: entry.finish_time,
        date_set: entry.date_set,
        character: entry.character,
        vehicle: entry.vehicle,
        installed: false,
    }
}

/// Profili che il server usa per i tempi caricati in blocco. Non sono
/// persone: sono il nome di una tabella, e non si mostrano mai. Il confronto
/// ignora maiuscole e segni (`TT_Leaderboard`, `tt-leaderboard`…).
const SYSTEM_PROFILES: &[&str] = &["ttleaderboard"];

fn is_system_profile(name: &str) -> bool {
    let key: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    SYSTEM_PROFILES.contains(&key.as_str())
}

/// Chi ha fatto il tempo, e con quale profilo è stato caricato.
///
/// Il server mette in `playerName` il **profilo** del time trial — spesso
/// quello di sistema, `TT_Leaderboard` — e in `miiName` il nome del Mii che
/// ha corso, cioè il giocatore. Si mostra il secondo; il profilo resta solo
/// quando è di una persona e dice qualcosa in più (`Staff Ghost IT`).
fn player_names(value: &Value) -> (String, String) {
    let mii = humanize(&loose::text(value, &["miiName", "mii_name"]));
    let profile = humanize(&loose::text(value, &["playerName", "player_name", "name"]));
    let profile = if is_system_profile(&profile) {
        String::new()
    } else {
        profile
    };

    if mii.is_empty() {
        return (profile, String::new());
    }
    if profile.eq_ignore_ascii_case(&mii) {
        return (mii, String::new());
    }
    (mii, profile)
}

fn entry_view(value: &Value) -> GhostEntryView {
    let finish_time_ms = loose::count(value, &["finishTimeMs", "finish_time_ms"]);
    let country = loose::text(value, &["countryAlpha2", "country_alpha2", "country"]);
    let (player_name, profile_name) = player_names(value);

    GhostEntryView {
        submission_id: i64::from(loose::int(value, &["id", "submissionId", "submission_id"])),
        rank: loose::count(value, &["rank", "position"]),
        player_name,
        profile_name,
        // Due lettere, o niente: la UI lo mostra com'è.
        country: if country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()) {
            country.to_ascii_uppercase()
        } else {
            String::new()
        },
        country_name: loose::text(value, &["countryName", "country_name"]),
        finish_time: non_empty_or(
            loose::text(value, &["finishTimeDisplay", "finish_time_display"]),
            || format_time(finish_time_ms),
        ),
        finish_time_ms,
        fastest_lap: loose::text(value, &["fastestLapDisplay", "fastest_lap_display"]),
        lap_splits: loose::array(value, &["lapSplitsDisplay", "lap_splits_display"])
            .iter()
            .filter_map(|lap| lap.as_str().map(str::to_string))
            .collect(),
        character: pulsar_ghost::character_name(loose::count(value, &["characterId"])).to_string(),
        vehicle: pulsar_ghost::vehicle_name(loose::count(value, &["vehicleId"])).to_string(),
        controller: loose::count(value, &["controllerType", "controller"]),
        automatic_drift: loose::count(value, &["driftType"]) == 1,
        shroomless: loose::flag(value, &["shroomless"]),
        date_set: loose::text(value, &["dateSet", "date_set"]),
        installed: false,
    }
}

fn non_empty_or(value: String, fallback: impl FnOnce() -> String) -> String {
    if value.is_empty() {
        fallback()
    } else {
        value
    }
}

/// `1:57.383`, come lo scrive il server.
fn format_time(milliseconds: u32) -> String {
    if milliseconds == 0 {
        return String::new();
    }
    format!(
        "{}:{:02}.{:03}",
        milliseconds / 60_000,
        (milliseconds / 1000) % 60,
        milliseconds % 1000
    )
}

// ---------------------------------------------------------------------------
// Contesto locale
// ---------------------------------------------------------------------------

/// Ciò che il launcher sa della modpack installata e della NAND.
#[derive(Debug)]
struct LocalContext {
    channel: Channel,
    user_folder: PathBuf,
    pulsar_folder: Option<String>,
    map: TrackCrcMap,
    /// Motivo per cui nessuna pista è installabile, vuoto se si può.
    blocker: &'static str,
}

impl LocalContext {
    async fn load(state: &Arc<AppState>) -> Self {
        let channel = state.channel().await;
        let layout = state.layout(channel).await;
        let user_folder = state.settings.read().await.user_folder();

        // `<Riivolution>/VanzaKart/VanzaKart`: la cartella con Binaries e
        // Ghosts dentro.
        let content = layout.mod_root().join(layout.directory_name());
        let pulsar_folder = std::fs::read(content.join("Binaries").join("Config.pul"))
            .ok()
            .and_then(|config| pulsar_ghost::pulsar_folder_name(&config));
        let map = std::fs::read_to_string(
            content
                .join(pulsar_ghost::GHOSTS_DIR)
                .join(pulsar_ghost::TRACK_MAP_FILE),
        )
        .map(|text| TrackCrcMap::parse(&text))
        .unwrap_or_default();

        let blocker = if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
            "no-user-folder"
        } else if !layout.is_installed() {
            "mod-not-installed"
        } else if pulsar_folder.is_none() || map.is_empty() {
            "no-track-map"
        } else {
            ""
        };

        Self {
            channel,
            user_folder,
            pulsar_folder,
            map,
            blocker,
        }
    }

    fn crc_for(&self, track: &RemoteTrack) -> Option<u32> {
        self.map.crc_for_track(&track.name, track.course_id)
    }

    fn ghost_dir(&self, crc: u32) -> Option<PathBuf> {
        let folder = self.pulsar_folder.as_deref()?;
        Some(pulsar_ghost::track_ghost_dir(
            &self.user_folder,
            folder,
            crc,
            CC,
        ))
    }

    fn track_view(&self, track: &RemoteTrack, record: Option<&GhostRecordView>) -> GhostTrackView {
        let crc = self.crc_for(track);
        let blocker = if !self.blocker.is_empty() {
            self.blocker
        } else if crc.is_none() {
            "track-not-in-modpack"
        } else {
            ""
        };

        GhostTrackView {
            id: track.id,
            course_id: track.course_id,
            name: track.name.clone(),
            category: track.category.clone(),
            laps: track.laps,
            sort_order: track.sort_order,
            record: record.cloned(),
            installable: blocker.is_empty(),
            blocker: blocker.to_string(),
            installed_count: crc
                .and_then(|crc| self.ghost_dir(crc))
                .map_or(0, |dir| pulsar_ghost::count_ghosts(&dir)),
        }
    }
}

// ---------------------------------------------------------------------------
// Registro dei ghost scaricati
// ---------------------------------------------------------------------------

/// Un ghost scaricato dal launcher.
///
/// Serve a due cose: mostrare "Installato" accanto al tempo giusto, e
/// permettere di togliere **solo** ciò che il launcher ha messo. Un ghost
/// fatto dall'utente non sta qui e non si tocca mai.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct InstalledGhost {
    submission_id: i64,
    channel: Channel,
    track_id: i64,
    path: PathBuf,
    sha256: String,
    installed_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct GhostIndex {
    ghosts: Vec<InstalledGhost>,
}

impl GhostIndex {
    fn path(state: &AppState) -> PathBuf {
        state.paths.root().join(INDEX_FILE)
    }

    async fn load(state: &AppState) -> Self {
        crate::storage::settings::read_json(&Self::path(state))
            .await
            .unwrap_or_default()
    }

    async fn save(&self, state: &AppState) -> AppResult<()> {
        vk_core::fsx::write_json_atomic(&Self::path(state), self).await?;
        Ok(())
    }

    /// `true` se il ghost è ancora dove il launcher l'ha messo, identico.
    fn is_present(entry: &InstalledGhost) -> bool {
        std::fs::read(&entry.path).is_ok_and(|bytes| {
            vk_core::hash::hash_eq(&vk_core::hash::sha256_bytes(&bytes), &entry.sha256)
        })
    }

    fn installed(&self, channel: Channel) -> Vec<i64> {
        self.ghosts
            .iter()
            .filter(|entry| entry.channel == channel && Self::is_present(entry))
            .map(|entry| entry.submission_id)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Casi d'uso
// ---------------------------------------------------------------------------

/// La lista delle mappe, con record e stato locale.
pub async fn catalog(state: &Arc<AppState>, refresh: bool) -> AppResult<GhostCatalogView> {
    let remote = remote_catalog(state, refresh).await?;
    let local = LocalContext::load(state).await;
    let installed = GhostIndex::load(state).await.installed(local.channel);

    Ok(GhostCatalogView {
        tracks: remote
            .tracks
            .iter()
            .map(|track| {
                let record = remote.records.get(&track.id).map(|record| GhostRecordView {
                    installed: installed.contains(&record.submission_id),
                    ..record.clone()
                });
                local.track_view(track, record.as_ref())
            })
            .collect(),
        channel: local.channel,
        cc: CC,
        blocker: local.blocker.to_string(),
        records_available: remote.records_available,
    })
}

/// Una pagina della classifica di una pista.
pub async fn leaderboard(
    state: &Arc<AppState>,
    track_id: i64,
    page: u32,
) -> AppResult<GhostLeaderboardView> {
    let track = find_track(state, track_id).await?;
    let page = page.clamp(1, 10_000);

    let base = api_base(state).await?;
    let payload = get_json(
        state,
        &api_url(
            &base,
            &format!(
                "leaderboard?trackId={track_id}&cc={CC}&glitch=false&page={page}&pageSize={PAGE_SIZE}"
            ),
        ),
    )
    .await?;

    let local = LocalContext::load(state).await;
    let installed = GhostIndex::load(state).await.installed(local.channel);
    let record = state
        .ghost_catalog
        .read()
        .await
        .as_ref()
        .and_then(|(_, catalog)| catalog.records.get(&track.id).cloned())
        .map(|record| GhostRecordView {
            installed: installed.contains(&record.submission_id),
            ..record
        });

    let mut view = parse_leaderboard(&payload, page);
    for entry in &mut view.entries {
        entry.installed = installed.contains(&entry.submission_id);
    }
    view.track = local.track_view(&track, record.as_ref());
    Ok(view)
}

fn parse_leaderboard(payload: &Value, page: u32) -> GhostLeaderboardView {
    let entries: Vec<GhostEntryView> = loose::array(payload, &["submissions", "entries"])
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut entry = entry_view(item);
            if entry.rank == 0 {
                entry.rank = (page - 1) * PAGE_SIZE + index as u32 + 1;
            }
            entry
        })
        .filter(|entry| entry.submission_id > 0)
        .collect();

    GhostLeaderboardView {
        page: match loose::count(payload, &["currentPage", "page"]) {
            0 => page,
            declared => declared,
        },
        total_pages: loose::count(payload, &["totalPages", "total_pages"]).max(1),
        total: loose::count(payload, &["totalSubmissions", "total"]),
        fastest_lap: loose::text(payload, &["fastestLapDisplay", "fastest_lap_display"]),
        entries,
        track: GhostTrackView::default(),
    }
}

async fn find_track(state: &Arc<AppState>, track_id: i64) -> AppResult<RemoteTrack> {
    if track_id <= 0 {
        return Err(AppError::BadRequest(format!("invalid track: {track_id}")));
    }
    let catalog = remote_catalog(state, false).await?;
    catalog
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .cloned()
        .ok_or_else(|| AppError::BadRequest(format!("unknown track: {track_id}")))
}

/// Scarica un ghost e lo mette dove il gioco lo cerca.
///
/// Dal frontend arrivano solo i due id: l'indirizzo del file lo costruisce il
/// backend, come per GameBanana (§D-030). Funziona anche con Dolphin aperto:
/// Pulsar rilegge la cartella quando si apre la pista.
pub async fn install(
    state: &Arc<AppState>,
    track_id: i64,
    submission_id: i64,
) -> AppResult<GhostInstallOutcome> {
    if submission_id <= 0 {
        return Err(AppError::BadRequest(format!(
            "invalid ghost: {submission_id}"
        )));
    }

    let track = find_track(state, track_id).await?;
    let local = LocalContext::load(state).await;
    let view = local.track_view(&track, None);
    if !view.installable {
        return Err(AppError::Configuration(blocker_message(&view.blocker)));
    }
    let directory = local
        .crc_for(&track)
        .and_then(|crc| local.ghost_dir(crc))
        .ok_or_else(|| AppError::Configuration(blocker_message("no-track-map")))?;

    let base = api_base(state).await?;
    let bytes = state
        .downloader
        .get_bytes(&api_url(&base, &format!("ghost/{submission_id}/download")))
        .await?;

    let _writes = state.ghost_writes.lock().await;
    let (file_name, already_present) = write_ghost(&directory, &bytes).await?;

    let mut index = GhostIndex::load(state).await;
    index
        .ghosts
        .retain(|entry| !(entry.submission_id == submission_id && entry.channel == local.channel));
    index.ghosts.push(InstalledGhost {
        submission_id,
        channel: local.channel,
        track_id,
        path: directory.join(&file_name),
        sha256: vk_core::hash::sha256_bytes(&bytes),
        installed_at: now_iso(),
    });
    index.save(state).await?;

    tracing::info!(
        track = %track.name,
        submission_id,
        file = %file_name,
        already_present,
        "ghost installato"
    );

    Ok(GhostInstallOutcome {
        submission_id,
        track_name: track.name,
        file_name,
        already_present,
        installed_count: pulsar_ghost::count_ghosts(&directory),
    })
}

/// Controlla il ghost e lo scrive in `directory`.
///
/// Restituisce il nome del file e `true` se lo stesso identico ghost c'era
/// già, nel qual caso non scrive niente.
async fn write_ghost(directory: &Path, bytes: &[u8]) -> AppResult<(String, bool)> {
    // Il CRC sbagliato qui vuol dire download rovinato, non salvataggio
    // corrotto: lo si dice con le parole giuste.
    let info = pulsar_ghost::parse_rkg(bytes).map_err(|error| match error {
        vk_save::SaveError::ChecksumMismatch { .. } => {
            AppError::Save(vk_save::SaveError::InvalidGhost(
                "the downloaded file is damaged (CRC mismatch)".into(),
            ))
        }
        other => AppError::Save(other),
    })?;

    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| AppError::io(directory, error))?;

    // Stesso tempo e stessi byte sotto un altro nome: è già installato.
    if let Some(existing) = find_identical(directory, bytes) {
        return Ok((existing, true));
    }

    match pulsar_ghost::free_ghost_name(directory, &info, bytes)? {
        None => Ok((info.file_name(), true)),
        Some(name) => {
            vk_core::fsx::write_atomic(&directory.join(&name), bytes).await?;
            Ok((name, false))
        }
    }
}

fn find_identical(directory: &Path, bytes: &[u8]) -> Option<String> {
    std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rkg"))
                && std::fs::read(entry.path()).is_ok_and(|existing| existing == bytes)
        })
        .map(|entry| entry.file_name().to_string_lossy().to_string())
}

/// Toglie un ghost scaricato dal launcher.
///
/// Si cancella solo se il file è ancora quello scritto dal launcher, byte per
/// byte: se nel frattempo al suo posto c'è altro, resta dov'è e sparisce solo
/// dal registro.
pub async fn remove(state: &Arc<AppState>, submission_id: i64) -> AppResult<usize> {
    let channel = state.channel().await;
    let _writes = state.ghost_writes.lock().await;
    let mut index = GhostIndex::load(state).await;

    let (matching, rest): (Vec<InstalledGhost>, Vec<InstalledGhost>) = index
        .ghosts
        .into_iter()
        .partition(|entry| entry.submission_id == submission_id && entry.channel == channel);
    index.ghosts = rest;

    let mut remaining = 0;
    for entry in &matching {
        if GhostIndex::is_present(entry) {
            tokio::fs::remove_file(&entry.path)
                .await
                .map_err(|error| AppError::io(&entry.path, error))?;
        }
        if let Some(parent) = entry.path.parent() {
            remaining = pulsar_ghost::count_ghosts(parent);
        }
    }

    index.save(state).await?;
    Ok(remaining)
}

/// Cartella dei ghost di una pista, o la radice dei ghost della modpack.
///
/// Serve al pulsante "Apri cartella": il frontend passa l'id della pista, mai
/// un percorso (§D-017).
pub async fn folder(state: &Arc<AppState>, track_id: Option<i64>) -> AppResult<PathBuf> {
    let local = LocalContext::load(state).await;
    if matches!(local.blocker, "no-user-folder" | "mod-not-installed") {
        return Err(AppError::Configuration(blocker_message(local.blocker)));
    }
    let pulsar_folder = local
        .pulsar_folder
        .as_deref()
        .ok_or_else(|| AppError::Configuration(blocker_message("no-track-map")))?;

    match track_id {
        None => Ok(pulsar_ghost::pulsar_ghosts_root(
            &local.user_folder,
            pulsar_folder,
        )),
        Some(id) => {
            let track = find_track(state, id).await?;
            local
                .crc_for(&track)
                .and_then(|crc| local.ghost_dir(crc))
                .ok_or_else(|| AppError::Configuration(blocker_message("track-not-in-modpack")))
        }
    }
}

/// Il testo di un blocco, per gli errori. La UI ha le sue traduzioni per
/// codice; questo è il ripiego leggibile.
fn blocker_message(code: &str) -> String {
    match code {
        "no-user-folder" => "Set the Dolphin User folder in Settings first.".into(),
        "mod-not-installed" => {
            "Install the modpack first: the ghosts go into its save data.".into()
        }
        "no-track-map" => {
            "The installed modpack has no track table: repair it from the Mods page.".into()
        }
        "track-not-in-modpack" => "This track is not in your version of the modpack.".into(),
        other => format!("Ghosts cannot be installed right now ({other})."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    fn json(raw: &str) -> Value {
        serde_json::from_str(raw).unwrap()
    }

    /// Un ghost valido come lo scrive il gioco, con il CRC finale.
    fn ghost(minutes: u8, seconds: u8, milliseconds: u16, filler: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; pulsar_ghost::RKG_HEADER_LEN + 32];
        bytes[..4].copy_from_slice(pulsar_ghost::RKG_MAGIC);
        bytes[4] = (minutes << 1) | (seconds >> 6);
        bytes[5] = ((seconds & 0x3F) << 2) | ((milliseconds >> 8) as u8 & 0x03);
        bytes[6] = (milliseconds & 0xFF) as u8;
        bytes[0x10] = 3;
        bytes[pulsar_ghost::RKG_HEADER_LEN..].fill(filler);
        let crc = vk_save::crc::crc32(&bytes);
        bytes.extend_from_slice(&crc.to_be_bytes());
        bytes
    }

    #[test]
    fn the_tracks_of_the_server_are_read_and_sorted() {
        // Forma vera di `/api/timetrial/tracks`.
        let tracks = parse_tracks(&json(
            r#"[
                {"id":3,"name":"Anthill & Apian Apts","courseId":257,"category":"Custom","laps":3,"supportsGlitch":false,"sortOrder":2},
                {"id":2,"name":"Alpine Peak","courseId":256,"category":"Custom","laps":3,"supportsGlitch":false,"sortOrder":1},
                {"id":0,"name":"senza id"},
                {"id":9,"name":""}
            ]"#,
        ));

        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].name, "Alpine Peak");
        assert_eq!(tracks[0].course_id, 256);
        assert_eq!(tracks[1].id, 3);
    }

    #[test]
    fn only_tracks_with_a_record_carry_one() {
        // Forma vera di `/api/timetrial/worldrecords/all?cc=150`.
        let records = parse_records(&json(
            r#"[
                {"trackId":2,"trackName":"Alpine Peak","activeWorldRecord":null},
                {"trackId":143,"trackName":"SW2 Bowser's Castle","activeWorldRecord":{
                    "id":1,"trackId":143,"playerName":"Staff Ghost IT","countryAlpha2":"IT",
                    "cc":150,"finishTimeMs":117383,"finishTimeDisplay":"1:57.383",
                    "vehicleId":23,"characterId":3,"controllerType":1,"driftType":0,
                    "dateSet":"2026-08-09","rank":1
                }}
            ]"#,
        ));

        assert_eq!(records.len(), 1);
        let record = &records[&143];
        assert_eq!(record.submission_id, 1);
        assert_eq!(record.finish_time, "1:57.383");
        assert_eq!(record.country, "IT");
        assert_eq!(record.character, "Bowser");
        assert_eq!(record.vehicle, "Flame Runner");
        assert_eq!(
            record.player_name, "Staff Ghost IT",
            "senza Mii resta il profilo"
        );
        assert!(record.profile_name.is_empty());
    }

    /// Il caso che si vedeva nel launcher: `TT_Leaderboard` in grande e il
    /// giocatore in piccolo. Il profilo di sistema non compare mai.
    #[test]
    fn the_player_comes_first_and_the_system_profile_never_shows() {
        let names = |raw: &str| player_names(&json(raw));

        assert_eq!(
            names(r#"{"playerName":"TT_Leaderboard","miiName":"lacly"}"#),
            ("lacly".to_string(), String::new())
        );
        assert_eq!(
            names(r#"{"playerName":"tt-leaderboard","miiName":""}"#),
            (String::new(), String::new()),
            "nessun nome è meglio del nome di una tabella"
        );
        assert_eq!(
            names(r#"{"playerName":"Staff Ghost IT","miiName":"NITROFOX"}"#),
            ("NITROFOX".to_string(), "Staff Ghost IT".to_string())
        );
        assert_eq!(
            names(r#"{"playerName":"Lacly","miiName":"lacly"}"#),
            ("lacly".to_string(), String::new()),
            "lo stesso nome non si ripete"
        );
    }

    #[test]
    fn a_leaderboard_page_is_read() {
        let view = parse_leaderboard(
            &json(
                r#"{
                    "track":{"id":143,"name":"SW2 Bowser's Castle"},
                    "cc":150,
                    "submissions":[{
                        "id":1,"trackId":143,"playerName":"Staff Ghost IT","countryAlpha2":"IT",
                        "countryName":"Italy","finishTimeMs":117383,"finishTimeDisplay":"1:57.383",
                        "vehicleId":23,"characterId":3,"controllerType":1,"driftType":0,
                        "shroomless":false,"miiName":"NITROFOX","lapSplitsDisplay":["0:39.008","0:38.918","0:39.457"],
                        "fastestLapDisplay":"0:38.918","dateSet":"2026-08-09","rank":1
                    },{"id":7,"playerName":"senza rank","finishTimeMs":125000}],
                    "totalSubmissions":2,"currentPage":1,"pageSize":25,"totalPages":1,
                    "fastestLapDisplay":"0:38.918"
                }"#,
            ),
            1,
        );

        assert_eq!(view.entries.len(), 2);
        assert_eq!(view.total, 2);
        assert_eq!(view.total_pages, 1);
        assert_eq!(view.fastest_lap, "0:38.918");

        let first = &view.entries[0];
        assert_eq!(first.player_name, "NITROFOX", "prima il nome del Mii");
        assert_eq!(first.profile_name, "Staff Ghost IT");
        assert_eq!(first.lap_splits.len(), 3);
        assert_eq!(first.controller, 1);
        assert!(!first.automatic_drift);

        // Senza rank dal server lo si deduce dalla pagina; senza tempo
        // leggibile lo si ricava dai millisecondi.
        assert_eq!(view.entries[1].rank, 2);
        assert_eq!(view.entries[1].finish_time, "2:05.000");
    }

    #[test]
    fn an_empty_leaderboard_still_has_one_page() {
        let view = parse_leaderboard(
            &json(r#"{"submissions":[],"totalSubmissions":0,"totalPages":0}"#),
            1,
        );
        assert!(view.entries.is_empty());
        assert_eq!(view.total_pages, 1);
    }

    #[test]
    fn the_time_is_formatted_like_the_server() {
        assert_eq!(format_time(117_383), "1:57.383");
        assert_eq!(format_time(61_005), "1:01.005");
        assert_eq!(format_time(0), "");
    }

    #[tokio::test]
    async fn a_ghost_is_written_once_under_its_pulsar_name() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Ghosts").join("9c230dbe").join("150");
        let bytes = ghost(2, 56, 903, 4);

        let (name, already) = write_ghost(&target, &bytes).await.unwrap();
        assert_eq!(name, "2m56s903.rkg");
        assert!(!already);
        assert_eq!(std::fs::read(target.join(&name)).unwrap(), bytes);

        // Riscaricarlo non crea una copia.
        let (again, already) = write_ghost(&target, &bytes).await.unwrap();
        assert_eq!(again, name);
        assert!(already);
        assert_eq!(pulsar_ghost::count_ghosts(&target), 1);
    }

    #[tokio::test]
    async fn a_ghost_already_saved_under_another_name_is_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = ghost(1, 44, 855, 9);
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("1m44s85A.rkg"), &bytes).unwrap();

        let (name, already) = write_ghost(dir.path(), &bytes).await.unwrap();
        assert!(already);
        assert_eq!(name, "1m44s85A.rkg");
    }

    #[tokio::test]
    async fn a_broken_download_never_reaches_the_nand() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("150");

        let error = write_ghost(&target, b"<html>502 Bad Gateway</html>")
            .await
            .unwrap_err();
        assert_eq!(error.code(), "ghost-invalid");
        assert!(
            !target.exists(),
            "nessuna cartella creata per un file rotto"
        );
    }

    /// Prepara una modpack installata con la tabella delle piste e il
    /// `Config.pul`, dentro una cartella User.
    async fn state_with_modpack(dir: &Path) -> Arc<AppState> {
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap();
        let user = dir.join("User");
        std::fs::create_dir_all(&user).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let layout = state.layout(Channel::Stable).await;
        crate::testkit::install_modpack(&layout);

        let content = layout.mod_root().join(layout.directory_name());
        std::fs::create_dir_all(content.join("Binaries")).unwrap();
        std::fs::create_dir_all(content.join("Ghosts")).unwrap();

        let mut config = b"PULS\0\0\0\x03\0\0\0\x24\0\0\0\x74\0\0\x0a\x94".to_vec();
        let mut name = [0u8; 16];
        name[..10].copy_from_slice(b"/VanzaKart");
        config.extend_from_slice(&name);
        std::fs::write(content.join("Binaries").join("Config.pul"), config).unwrap();
        std::fs::write(
            content.join("Ghosts").join("FolderToTrackName.txt"),
            "Alpine Peak = 90538C70\nColor Wonderland = 9C230DBE\n",
        )
        .unwrap();

        *state.ghost_catalog.write().await = Some((
            Instant::now(),
            Arc::new(RemoteCatalog {
                tracks: vec![
                    RemoteTrack {
                        id: 2,
                        course_id: 256,
                        name: "Alpine Peak".into(),
                        laps: 3,
                        ..Default::default()
                    },
                    RemoteTrack {
                        id: 400,
                        course_id: 497,
                        name: "GBA Ribbon Road".into(),
                        laps: 3,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
        ));
        state
    }

    #[tokio::test]
    async fn the_catalog_knows_where_each_track_keeps_its_ghosts() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_modpack(dir.path()).await;

        // Un ghost fatto dall'utente, già nella cartella giusta.
        let target = dir
            .path()
            .join("User/Wii/shared2/Pulsar/VanzaKart/Ghosts/90538c70/150");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("1m50s000.rkg"), ghost(1, 50, 0, 1)).unwrap();

        let view = catalog(&state, false).await.unwrap();
        assert_eq!(view.blocker, "");
        assert_eq!(view.cc, 150);

        let alpine = view.tracks.iter().find(|t| t.id == 2).unwrap();
        assert!(alpine.installable);
        assert_eq!(alpine.installed_count, 1);

        let missing = view.tracks.iter().find(|t| t.id == 400).unwrap();
        assert!(!missing.installable);
        assert_eq!(missing.blocker, "track-not-in-modpack");

        let folder = folder(&state, Some(2)).await.unwrap();
        assert_eq!(folder, target);
    }

    #[tokio::test]
    async fn without_the_modpack_nothing_is_installable() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.path().join("VanzaKart")))
            .await
            .unwrap();
        let user = dir.path().join("User");
        std::fs::create_dir_all(&user).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let local = LocalContext::load(&state).await;
        assert_eq!(local.blocker, "mod-not-installed");
        let view = local.track_view(
            &RemoteTrack {
                id: 2,
                name: "Alpine Peak".into(),
                ..Default::default()
            },
            None,
        );
        assert!(!view.installable);
        assert_eq!(view.blocker, "mod-not-installed");
    }

    #[tokio::test]
    async fn an_invalid_id_is_refused_before_touching_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_modpack(dir.path()).await;

        assert_eq!(
            install(&state, 2, 0).await.unwrap_err().code(),
            "bad-request"
        );
        assert_eq!(
            install(&state, -1, 5).await.unwrap_err().code(),
            "bad-request"
        );
        assert_eq!(
            install(&state, 999, 5).await.unwrap_err().code(),
            "bad-request"
        );
        // Pista che nella modpack non c'è: niente download.
        assert_eq!(
            install(&state, 400, 5).await.unwrap_err().code(),
            "configuration"
        );
    }

    #[tokio::test]
    async fn only_ghosts_written_by_the_launcher_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_modpack(dir.path()).await;
        let target = dir.path().join("ghosts");

        let mine = ghost(1, 50, 0, 1);
        let (name, _) = write_ghost(&target, &mine).await.unwrap();
        let mut index = GhostIndex::default();
        index.ghosts.push(InstalledGhost {
            submission_id: 11,
            channel: Channel::Stable,
            track_id: 2,
            path: target.join(&name),
            sha256: vk_core::hash::sha256_bytes(&mine),
            installed_at: now_iso(),
        });
        // Il file registrato per il 12 è stato sostituito dall'utente.
        let theirs = target.join("2m00s000.rkg");
        std::fs::write(&theirs, ghost(2, 0, 0, 2)).unwrap();
        index.ghosts.push(InstalledGhost {
            submission_id: 12,
            channel: Channel::Stable,
            track_id: 2,
            path: theirs.clone(),
            sha256: vk_core::hash::sha256_bytes(b"un altro contenuto"),
            installed_at: now_iso(),
        });
        index.save(&state).await.unwrap();

        assert_eq!(
            GhostIndex::load(&state).await.installed(Channel::Stable),
            vec![11]
        );

        remove(&state, 11).await.unwrap();
        assert!(!target.join(&name).exists());

        remove(&state, 12).await.unwrap();
        assert!(theirs.exists(), "un file che non è più il nostro resta");
        assert!(GhostIndex::load(&state).await.ghosts.is_empty());
    }
}
