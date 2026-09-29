//! Rooms e Leaderboard.
//!
//! Porta `ViewModels/RoomsViewModel.cs` e `ViewModels/LeaderboardViewModel.cs`.
//!
//! I payload del server portano la stessa informazione sotto più chiavi **nello
//! stesso oggetto**: la classifica manda `prestigeRank` e `rank`, e insieme
//! `vr_gain_24h`, `vr_last_24_hours` e `vrLast24Hours`. Con `#[serde(alias)]`
//! serde vede lo stesso campo due volte e rifiuta l'intero documento con
//! `duplicate field`, che è il modo esatto in cui la pagina si rompeva. Qui i
//! campi si leggono da un `serde_json::Value`, dove le chiavi ripetute sono
//! semplicemente sinonimi e vince la prima non nulla (§D-056).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::wii_text::humanize;
use crate::domain::{
    BadgeView, LeaderboardEntry, LeaderboardPage, PlayerStatsView, RoomPlayerView, RoomView,
    RoomsSummary,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Il server tronca `limit` a 100: chiederne di più restituisce comunque 100
/// righe e farebbe credere alla UI di avere già tutta la classifica.
const LEADERBOARD_PAGE_SIZE: u32 = 100;

/// Pagine che si scorrono al massimo per costruire l'indice dei giocatori.
/// Cinquecento nomi sono più di quanti ne abbia mai avuti il server.
const INDEX_MAX_PAGES: u32 = 5;

/// Per quanto l'indice dei giocatori resta valido senza richiederlo.
const INDEX_TTL: Duration = Duration::from_secs(120);

/// Chiavi con cui un giocatore può portare la streak. `leaderboard.php` usa
/// le prime; le altre sono le forme che il backend .NET ha già usato altrove.
const STREAK_KEYS: [&str; 3] = ["streak", "currentStreak", "current_streak"];
const VACATION_KEYS: [&str; 3] = ["streakVacation", "streak_vacation", "onVacation"];

/// Quanti giocatori chiedere alla classifica del sito in una volta: tutti.
/// Senza l'immagine del Mii ogni riga pesa poche centinaia di byte.
const STREAK_FETCH_LIMIT: u32 = 10_000;

/// Colori di ripiego per chi non ha un Mii: gli stessi degli amici.
const ACCENTS: [&str; 6] = [
    "#39E7FF", "#FF3B7A", "#FFD166", "#4DFFB0", "#9D5CFF", "#FF8800",
];

// ---------------------------------------------------------------------------
// Lettura tollerante dei payload
// ---------------------------------------------------------------------------

pub(crate) mod loose {
    use serde_json::Value;

    /// Primo valore non nullo fra le chiavi indicate.
    pub fn pick<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
        keys.iter()
            .filter_map(|key| value.get(*key))
            .find(|found| !found.is_null())
    }

    pub fn text(value: &Value, keys: &[&str]) -> String {
        match pick(value, keys) {
            Some(Value::String(text)) => text.trim().to_string(),
            Some(Value::Number(number)) => number.to_string(),
            _ => String::new(),
        }
    }

    /// Numero nella forma in cui il server lo manda: intero, decimale o
    /// stringa. Il backend .NET le ha già mandate tutte e tre.
    pub fn float(value: &Value, keys: &[&str]) -> f64 {
        match pick(value, keys) {
            Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
            Some(Value::String(text)) => text.trim().replace(',', ".").parse().unwrap_or(0.0),
            Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
            _ => 0.0,
        }
    }

    pub fn int(value: &Value, keys: &[&str]) -> i32 {
        let number = float(value, keys);
        if number.is_finite() {
            number
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
        } else {
            0
        }
    }

    pub fn count(value: &Value, keys: &[&str]) -> u32 {
        int(value, keys).max(0) as u32
    }

    pub fn flag(value: &Value, keys: &[&str]) -> bool {
        match pick(value, keys) {
            Some(Value::Bool(flag)) => *flag,
            Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0) != 0.0,
            Some(Value::String(text)) => matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            ),
            _ => false,
        }
    }

    pub fn array<'a>(value: &'a Value, keys: &[&str]) -> &'a [Value] {
        match pick(value, keys) {
            Some(Value::Array(items)) => items.as_slice(),
            _ => &[],
        }
    }
}

/// Faccia di un giocatore ricavata dal Mii che manda il server.
struct Face {
    studio_data: String,
    avatar_initial: String,
    accent_color: String,
}

/// Converte il Mii del server in ciò che serve alla UI per disegnarlo.
///
/// Il server manda il blocco Wii da 74 byte in base64 — lo stesso che sta nel
/// salvataggio — mentre il renderer accetta solo la "studio data": la
/// conversione è quella che `saves.rs` fa per gli amici. Un blocco assente o
/// non valido non è un errore: resta l'iniziale sul colore di ripiego.
fn face(mii_data: &str, name: &str, seed: usize) -> Face {
    let block = vk_save::mii::base64_decode(mii_data.trim())
        .filter(|block| vk_save::mii::looks_like_wii_mii(block));

    Face {
        studio_data: block
            .as_deref()
            .map(vk_save::mii::studio_data)
            .unwrap_or_default(),
        avatar_initial: initial(name),
        accent_color: block
            .as_deref()
            .and_then(|block| vk_save::mii::parse_block(block).ok())
            .map_or_else(
                || ACCENTS[seed % ACCENTS.len()].to_string(),
                |mii| mii.favorite_color().to_string(),
            ),
    }
}

fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

/// Porta l'istante del server in RFC 3339, l'unico formato che il frontend sa
/// leggere senza indovinare.
///
/// PostgreSQL lo serializza come `2026-08-25 11:33:49.459785+00`: spazio al
/// posto della `T` e fuso orario senza minuti.
fn rfc3339(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut value = trimmed.replacen(' ', "T", 1);

    let bytes = value.as_bytes();
    if bytes.len() >= 3 {
        let sign = bytes[bytes.len() - 3];
        let hour_only = (sign == b'+' || sign == b'-')
            && bytes[bytes.len() - 2].is_ascii_digit()
            && bytes[bytes.len() - 1].is_ascii_digit();
        if hour_only {
            value.push_str(":00");
        }
    }

    value
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------

/// Scarica l'elenco delle stanze.
pub async fn rooms(state: &Arc<AppState>) -> AppResult<RoomsSummary> {
    let url = state.endpoints.read().await.rooms_api_url.clone();
    if url.trim().is_empty() {
        return Err(AppError::Configuration(
            "rooms endpoint not configured".into(),
        ));
    }

    let raw = state.downloader.get_string(&url).await?;
    let payload: Value = serde_json::from_str(vk_core::json::strip_leading_noise(&raw))
        .map_err(|error| AppError::Internal(format!("invalid rooms response: {error}")))?;

    let listed = loose::pick(&payload, &["rooms"]).is_some();
    if !loose::flag(&payload, &["success"]) && !listed {
        return Err(AppError::Internal(
            "the server returned an invalid response".into(),
        ));
    }

    let meta = loose::pick(&payload, &["meta"])
        .cloned()
        .unwrap_or(Value::Null);
    let rooms: Vec<RoomView> = loose::array(&payload, &["rooms"])
        .iter()
        .map(room)
        .collect();

    let mut rooms = rooms;
    if let Some(index) = cached_player_index(state).await {
        for player in rooms.iter_mut().flat_map(|room| room.players.iter_mut()) {
            if let Some(stats) = index.get(&player.friend_code) {
                player.prestige_rank = stats.prestige_rank;
                player.rank_image = stats.rank_image.clone();
                player.rank_label = stats.rank_label.clone();
            }
        }
    }

    let declared_players = loose::count(&meta, &["total_players", "totalPlayers"]);
    let declared_rooms = loose::count(&meta, &["total_rooms", "totalRooms"]);

    Ok(RoomsSummary {
        total_players: if declared_players > 0 {
            declared_players
        } else {
            rooms.iter().map(|room| room.player_count).sum()
        },
        total_rooms: if declared_rooms > 0 {
            declared_rooms
        } else {
            rooms.len() as u32
        },
        public_rooms: loose::count(&meta, &["public_rooms", "publicRooms"]),
        private_rooms: loose::count(&meta, &["private_rooms", "privateRooms"]),
        last_updated: rfc3339(&loose::text(&meta, &["last_updated", "lastUpdated"])),
        status: loose::text(&meta, &["status"]),
        notice: loose::text(&payload, &["info", "notice", "message"]),
        rooms,
    })
}

fn room(value: &Value) -> RoomView {
    let players: Vec<RoomPlayerView> = loose::array(value, &["players", "Players"])
        .iter()
        .enumerate()
        .map(|(index, player)| room_player(player, index))
        .collect();

    let host = humanize(&loose::text(value, &["host", "Host"]));
    let counted = loose::count(value, &["player_count", "playerCount"]);

    RoomView {
        id: loose::text(value, &["id", "Id"]),
        name: loose::text(value, &["name", "Name"]),
        host: if host.is_empty() {
            players
                .iter()
                .find(|player| player.is_host)
                .map(|player| player.name.clone())
                .unwrap_or_default()
        } else {
            host
        },
        // L'elenco dei giocatori è la fonte più affidabile del contatore: è
        // quello che l'utente vede scritto sotto la stanza.
        player_count: if players.is_empty() {
            counted
        } else {
            players.len() as u32
        },
        max_players: match loose::count(value, &["max_players", "maxPlayers"]) {
            0 => 12,
            declared => declared,
        },
        mode: non_empty(loose::text(value, &["mode", "Mode"]), "Versus"),
        track: non_empty(loose::text(value, &["track", "Track"]), "Choosing Track..."),
        region: non_empty(loose::text(value, &["region", "Region"]), "Worldwide"),
        status: non_empty(loose::text(value, &["status", "Status"]), "In Lobby"),
        players,
    }
}

fn room_player(value: &Value, index: usize) -> RoomPlayerView {
    let name = non_empty(humanize(&loose::text(value, &["name", "Name"])), "Player");
    let face = face(
        &loose::text(value, &["mii_data", "miiData", "mii", "Mii"]),
        &name,
        index,
    );

    RoomPlayerView {
        friend_code: loose::text(value, &["friend_code", "friendCode", "fc"]),
        vr: loose::int(value, &["vr", "VR", "race_rating"]),
        br: loose::int(value, &["br", "BR", "battle_rating"]),
        is_host: loose::flag(value, &["is_host", "isHost", "IsOpenHost"]),
        name,
        studio_data: face.studio_data,
        avatar_initial: face.avatar_initial,
        accent_color: face.accent_color,
        ..RoomPlayerView::default()
    }
}

// ---------------------------------------------------------------------------
// Leaderboard
// ---------------------------------------------------------------------------

/// Scarica una pagina di classifica.
///
/// Le posizioni le numera il server sull'intera classifica, non sulla pagina:
/// `offset` scorre e i numeri restano quelli veri.
pub async fn leaderboard(state: &Arc<AppState>, offset: u32) -> AppResult<LeaderboardPage> {
    let base = state.endpoints.read().await.leaderboard_api_url.clone();
    if base.trim().is_empty() {
        return Err(AppError::Configuration(
            "leaderboard endpoint not configured".into(),
        ));
    }

    let separator = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{separator}limit={LEADERBOARD_PAGE_SIZE}&offset={offset}");

    let raw = state.downloader.get_string(&url).await?;
    let payload: Value = serde_json::from_str(vk_core::json::strip_leading_noise(&raw))
        .map_err(|error| AppError::Internal(format!("invalid leaderboard response: {error}")))?;

    let players = loose::array(&payload, &["players"]);
    let mut entries: Vec<LeaderboardEntry> = players
        .iter()
        .enumerate()
        .map(|(index, player)| entry(player, index, offset))
        .collect();

    // `vk_leaderboard.php` non manda ancora la streak: finché non lo fa la si
    // prende dalla classifica del sito. Quando comincerà a mandarla, questa
    // seconda richiesta smette da sola di partire (§D-085).
    if !players.is_empty() && !carries_streak(players) {
        attach_streaks(state, &mut entries).await;
    }

    let badges = attach_badges(state, &mut entries).await;

    Ok(LeaderboardPage {
        // Una pagina piena non prova che ce ne sia un'altra, ma è l'unico
        // indizio che il server dà: `meta.count` conta solo questa.
        has_more: entries.len() as u32 >= LEADERBOARD_PAGE_SIZE,
        offset,
        entries,
        badges,
    })
}

fn entry(value: &Value, index: usize, offset: u32) -> LeaderboardEntry {
    let position = match loose::int(value, &["position", "pos"]) {
        declared if declared > 0 => declared,
        _ => offset as i32 + index as i32 + 1,
    };

    let games = loose::int(value, &["games", "races"]);
    let wins = loose::int(value, &["wins"]);
    let winrate = match loose::float(value, &["winrate", "win_rate", "winRate"]) {
        rate if rate > 0.0 => rate,
        _ if games > 0 => f64::from(wins) / f64::from(games) * 100.0,
        _ => 0.0,
    };

    let prestige_rank = loose::int(value, &["prestigeRank", "prestige_rank", "pr", "rank"]);

    let name = humanize(&loose::text(value, &["name", "player"]));
    let face = face(
        &loose::text(value, &["mii_data", "miiData", "mii"]),
        &name,
        index,
    );

    LeaderboardEntry {
        position,
        points: loose::int(value, &["points", "vr", "ev"]),
        friend_code: loose::text(value, &["fc", "friendCode", "friend_code"]),
        prestige_rank,
        wins,
        games,
        winrate,
        last_seen: match loose::text(value, &["last_seen", "lastSeen"]) {
            seen if seen.is_empty() => None,
            seen => Some(rfc3339(&seen)),
        },
        is_suspicious: loose::flag(value, &["is_suspicious", "isSuspicious"]),
        vr_last_24_hours: loose::int(value, &["vr_last_24_hours", "vr_gain_24h", "vrLast24Hours"]),
        vr_last_week: loose::int(value, &["vr_gain_week", "vrLastWeek", "vr_last_week"]),
        vr_last_month: loose::int(value, &["vr_gain_month", "vrLastMonth", "vr_last_month"]),
        streak: loose::count(value, &STREAK_KEYS),
        streak_vacation: loose::flag(value, &VACATION_KEYS),
        // La riempie `attach_badges`, che prima deve scaricare il file.
        badge: String::new(),
        rank_image_url: loose::text(value, &RANK_IMAGE_KEYS),
        name,
        studio_data: face.studio_data,
        avatar_initial: face.avatar_initial,
        accent_color: face.accent_color,
    }
}

// ---------------------------------------------------------------------------
// Streak
// ---------------------------------------------------------------------------

/// `true` se almeno un giocatore della risposta porta la streak.
///
/// Basta la chiave, anche a zero: vuol dire che il server la manda, e una
/// streak persa è un'informazione, non un buco da riempire.
fn carries_streak(players: &[Value]) -> bool {
    players
        .iter()
        .any(|player| loose::pick(player, &STREAK_KEYS).is_some())
}

/// Streak dei giocatori per friend code.
#[derive(Debug, Default)]
pub struct StreakIndex {
    players: HashMap<String, (u32, bool)>,
}

impl StreakIndex {
    /// Giorni e vacanza di un friend code, comunque sia scritto.
    pub fn get(&self, friend_code: &str) -> Option<(u32, bool)> {
        self.players.get(&digits(friend_code)).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    fn from_payload(payload: &Value) -> Self {
        let players = loose::array(payload, &["players"])
            .iter()
            .filter_map(|player| {
                let key = digits(&loose::text(player, &["fc", "friendCode", "friend_code"]));
                (!key.is_empty()).then(|| {
                    (
                        key,
                        (
                            loose::count(player, &STREAK_KEYS),
                            loose::flag(player, &VACATION_KEYS),
                        ),
                    )
                })
            })
            .collect();
        Self { players }
    }
}

/// Mette in ogni riga la streak presa dalla classifica del sito.
///
/// Se la classifica del sito non risponde, le righe restano a zero: la
/// classifica si mostra lo stesso, senza fiammelle, invece di fallire.
async fn attach_streaks(state: &Arc<AppState>, entries: &mut [LeaderboardEntry]) {
    let index = streak_index(state).await;
    for entry in entries {
        if let Some((days, vacation)) = index.get(&entry.friend_code) {
            entry.streak = days;
            entry.streak_vacation = vacation;
        }
    }
}

/// Streak di tutti i giocatori, con la stessa validità dell'indice amici.
pub async fn streak_index(state: &Arc<AppState>) -> Arc<StreakIndex> {
    let cached = {
        let guard = state.streak_index.read().await;
        guard.clone()
    };
    if let Some((fetched_at, index)) = cached {
        if fetched_at.elapsed() < INDEX_TTL {
            return index;
        }
    }

    let index = Arc::new(fetch_streak_index(state).await.unwrap_or_else(|error| {
        tracing::debug!(
            error = %vk_core::redact::redact(&error.to_string()),
            "streak non disponibili"
        );
        StreakIndex::default()
    }));

    // Come per l'indice amici: un indice vuoto non si tiene, il prossimo
    // tentativo deve poter riuscire subito.
    if !index.is_empty() {
        *state.streak_index.write().await = Some((Instant::now(), index.clone()));
    }
    index
}

async fn fetch_streak_index(state: &Arc<AppState>) -> AppResult<StreakIndex> {
    let base = state
        .endpoints
        .read()
        .await
        .site_leaderboard_api_url
        .clone();
    if base.trim().is_empty() {
        return Ok(StreakIndex::default());
    }

    // `mii=0`: l'immagine del Mii pesa qualche KB a riga e qui non serve.
    let separator = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{separator}mii=0&limit={STREAK_FETCH_LIMIT}&offset=0");

    let raw = state.downloader.get_string(&url).await?;
    let payload: Value = serde_json::from_str(vk_core::json::strip_leading_noise(&raw))
        .map_err(|error| AppError::Internal(format!("invalid site leaderboard: {error}")))?;
    Ok(StreakIndex::from_payload(&payload))
}

// ---------------------------------------------------------------------------
// Immagini dei rank
// ---------------------------------------------------------------------------

/// Chiavi con cui il server può assegnare a un giocatore un'immagine sua: è
/// la strada dei rank speciali (staff, sviluppatori…). Sono quelle che il
/// launcher legacy già leggeva (`GetRankImageUrl`).
const RANK_IMAGE_KEYS: [&str; 8] = [
    "rank_image_url",
    "rankImageUrl",
    "rank_icon_url",
    "rankIconUrl",
    "rank_image",
    "rankImage",
    "badge_url",
    "badgeUrl",
];

/// Lato delle miniature: il doppio della misura più grande in cui compaiono
/// (32 px nel podio), per gli schermi ad alta densità.
const BADGE_SIDE: u32 = 80;

/// Una miniatura in cache si riscarica dopo una settimana: il sito può
/// cambiare disegno senza cambiare nome al file.
const BADGE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Da dove viene l'immagine di un rank, e come si chiama in cache.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BadgeSource {
    /// `rank-3`, oppure `custom-<impronta>` per un'immagine assegnata.
    key: String,
    /// Indirizzi da provare in ordine.
    urls: Vec<String>,
    /// Nome di un rank speciale, ricavato dal file; vuoto per i rank del
    /// gioco.
    label: String,
}

/// L'immagine da mostrare accanto a un giocatore, se ne ha una.
///
/// Un'immagine assegnata dal server vince sul rank del gioco: è il modo in
/// cui lo staff ha il suo stemma. Il rank del gioco si cerca prima sul sito,
/// che ha tutti i disegni attuali, poi sul server del gioco (§D-094).
fn badge_source(
    prestige_rank: i32,
    custom: &str,
    endpoints: &vk_core::endpoints::EndpointsInfo,
) -> Option<BadgeSource> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let urls = custom_badge_urls(custom, endpoints);
        if !urls.is_empty() {
            let digest = vk_core::hash::sha256_bytes(custom.as_bytes());
            return Some(BadgeSource {
                key: format!("custom-{}", &digest[..16]),
                urls,
                label: badge_label(custom),
            });
        }
    }

    if prestige_rank < 1 {
        return None;
    }
    let file = format!("rank-{prestige_rank}.png");
    let urls = [
        &endpoints.rank_images_site_url,
        &endpoints.rank_images_base_url,
    ]
    .into_iter()
    .filter(|base| !base.trim().is_empty())
    .map(|base| join_url(base, &file))
    .collect::<Vec<_>>();

    (!urls.is_empty()).then(|| BadgeSource {
        key: format!("rank-{prestige_rank}"),
        urls,
        label: String::new(),
    })
}

/// Indirizzi di un'immagine assegnata dal server.
///
/// Un indirizzo completo vale solo se è https e del progetto, come ogni altro
/// endpoint (§D-004). Un percorso relativo — come lo scrive il sito,
/// `/FOOTAGE/ranks/developer_full_00000.png` — si cerca sul sito e poi sul
/// server del gioco.
fn custom_badge_urls(raw: &str, endpoints: &vk_core::endpoints::EndpointsInfo) -> Vec<String> {
    if raw.contains("://") {
        return if vk_core::endpoints::is_safe_endpoint(raw)
            && vk_core::endpoints::is_project_url(raw)
        {
            vec![raw.to_string()]
        } else {
            Vec::new()
        };
    }

    let path = raw.trim_start_matches('/');
    if path.is_empty() || path.split(['/', '\\']).any(|part| part == "..") {
        return Vec::new();
    }

    let site_root = url::Url::parse(&endpoints.rank_images_site_url)
        .ok()
        .map(|url| format!("{}/", url.origin().ascii_serialization()));
    site_root
        .into_iter()
        .chain(std::iter::once(endpoints.server_base_url.clone()))
        .filter(|root| !root.trim().is_empty())
        .map(|root| join_url(&root, path))
        .collect()
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path)
}

/// Il nome di un rank speciale, dal nome del file: `staff_ghost_full_00010.png`
/// diventa "Staff Ghost".
fn badge_label(raw: &str) -> String {
    let file = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let stem = file.split('.').next().unwrap_or(file);

    stem.split(['_', '-', ' '])
        .filter(|word| !word.is_empty())
        .filter(|word| !word.eq_ignore_ascii_case("full"))
        .filter(|word| !word.chars().all(|ch| ch.is_ascii_digit()))
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Assegna a ogni riga la chiave della sua immagine e restituisce le
/// immagini, una per chiave.
///
/// Il download avviene **prima** di comporre la pagina, altrimenti al primo
/// avvio — con la cache vuota — nessuna riga avrebbe la sua immagine. Le
/// immagini diverse si scaricano insieme: al primo avvio sono al massimo una
/// dozzina.
async fn attach_badges(
    state: &Arc<AppState>,
    entries: &mut [LeaderboardEntry],
) -> BTreeMap<String, BadgeView> {
    let endpoints = state.endpoints.read().await.clone();

    let mut sources: BTreeMap<String, BadgeSource> = BTreeMap::new();
    for entry in entries.iter_mut() {
        entry.badge.clear();
        if let Some(source) = badge_source(entry.prestige_rank, &entry.rank_image_url, &endpoints) {
            entry.badge = source.key.clone();
            sources.entry(source.key.clone()).or_insert(source);
        }
    }

    let fetched = futures_util::future::join_all(sources.into_values().map(|source| async move {
        let image = badge_image(state, &source).await;
        (source, image)
    }))
    .await;

    let badges: BTreeMap<String, BadgeView> = fetched
        .into_iter()
        .filter_map(|(source, image)| {
            image.map(|image| {
                (
                    source.key,
                    BadgeView {
                        image,
                        label: source.label,
                    },
                )
            })
        })
        .collect();

    // Una chiave senza immagine non serve alla UI: il rank del gioco resta
    // leggibile dal suo numero.
    for entry in entries {
        if !badges.contains_key(&entry.badge) {
            entry.badge.clear();
        }
    }
    badges
}

/// La miniatura di un rank come data URI, dalla cache o scaricandola.
///
/// Se il download non riesce resta buona la miniatura vecchia: un rank
/// mostrato col disegno della settimana scorsa è meglio di nessun rank.
async fn badge_image(state: &Arc<AppState>, source: &BadgeSource) -> Option<String> {
    let directory = state.paths.rank_images_dir();
    let thumbnail = directory.join(format!("{}@{BADGE_SIDE}.png", source.key));

    let fresh = tokio::fs::metadata(&thumbnail)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < BADGE_TTL);

    if !fresh {
        if let Err(error) = refresh_badge(state, source, &thumbnail).await {
            tracing::debug!(
                key = %source.key,
                error = %vk_core::redact::redact(&error.to_string()),
                "immagine del rank non aggiornata"
            );
        }
    }

    let bytes = tokio::fs::read(&thumbnail).await.ok()?;
    (!bytes.is_empty()).then(|| {
        format!(
            "data:image/png;base64,{}",
            vk_save::mii::base64_encode(&bytes)
        )
    })
}

/// Scarica l'immagine, la ritaglia e la riduce, e mette la miniatura in
/// cache. L'originale non si tiene: arriva a 450 KB e non serve più.
async fn refresh_badge(
    state: &Arc<AppState>,
    source: &BadgeSource,
    thumbnail: &std::path::Path,
) -> AppResult<()> {
    let directory = state.paths.rank_images_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    let download = directory.join(format!("{}.download", source.key));
    let outcome = state
        .downloader
        .download_with_mirrors(
            &source.urls,
            &download,
            &vk_core::progress::noop_sink(),
            &vk_core::progress::CancelToken::new(),
        )
        .await;
    let bytes = match outcome {
        Ok(_) => tokio::fs::read(&download).await.ok(),
        Err(error) => {
            let _ = tokio::fs::remove_file(&download).await;
            return Err(error.into());
        }
    };
    let _ = tokio::fs::remove_file(&download).await;
    let bytes = bytes.ok_or_else(|| AppError::Internal("rank image unreadable".into()))?;

    let png = tokio::task::spawn_blocking(move || {
        vk_core::thumbnail::badge_thumbnail(&bytes, BADGE_SIDE)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))??;

    vk_core::fsx::write_atomic(thumbnail, &png).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Indice dei giocatori
// ---------------------------------------------------------------------------

/// La classifica indicizzata per friend code.
#[derive(Debug, Default)]
pub struct PlayerIndex {
    players: HashMap<String, PlayerStatsView>,
}

impl PlayerIndex {
    /// Statistiche di un friend code, comunque sia scritto.
    pub fn get(&self, friend_code: &str) -> Option<&PlayerStatsView> {
        self.players.get(&digits(friend_code))
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

/// Statistiche dei giocatori dal server, per friend code.
///
/// I numeri che `rksys.dat` tiene accanto a un amico li aggiorna il gioco solo
/// quando lo incontra online, quindi restano fermi a mesi fa; quelli del
/// server sono gli stessi della classifica, cioè quelli veri (§D-064).
///
/// L'indice vale due minuti: una lista amici si apre e si richiude spesso, e
/// rifare due richieste HTTP a ogni apertura non aggiungerebbe niente.
pub async fn player_index(state: &Arc<AppState>) -> Arc<PlayerIndex> {
    // Il guard si chiude qui dentro: sotto si scarica la classifica, e
    // tenerlo aperto bloccherebbe ogni altro lettore per tutta la durata.
    let cached = {
        let guard = state.leaderboard_index.read().await;
        guard.clone()
    };
    if let Some((fetched_at, index)) = cached {
        if fetched_at.elapsed() < INDEX_TTL {
            return index;
        }
    }

    let index = Arc::new(build_player_index(state).await);

    // Un indice vuoto — server irraggiungibile — non si mette in cache: il
    // prossimo tentativo deve poter riuscire subito.
    if !index.is_empty() {
        *state.leaderboard_index.write().await = Some((Instant::now(), index.clone()));
    }

    index
}

/// L'indice che c'è già, anche se scaduto, senza aspettare la rete.
///
/// Le stanze si aggiornano spesso e devono arrivare subito: se l'indice non
/// c'è ancora lo si costruisce in sottofondo, e i rank compaiono al giro
/// successivo invece di ritardare questo.
async fn cached_player_index(state: &Arc<AppState>) -> Option<Arc<PlayerIndex>> {
    let cached = {
        let guard = state.leaderboard_index.read().await;
        guard.clone()
    };
    if let Some((fetched_at, index)) = cached {
        if fetched_at.elapsed() >= INDEX_TTL {
            let state = state.clone();
            tokio::spawn(async move {
                player_index(&state).await;
            });
        }
        return Some(index);
    }

    let state = state.clone();
    tokio::spawn(async move {
        player_index(&state).await;
    });
    None
}

async fn build_player_index(state: &Arc<AppState>) -> PlayerIndex {
    let mut players = HashMap::new();
    let mut offset = 0u32;

    for _ in 0..INDEX_MAX_PAGES {
        let Ok(page) = leaderboard(state, offset).await else {
            break;
        };

        let fetched = page.entries.len() as u32;
        for entry in page.entries {
            let key = digits(&entry.friend_code);
            if !key.is_empty() {
                let badge = page.badges.get(&entry.badge);
                players.insert(key, stats_of(entry, badge));
            }
        }

        if !page.has_more || fetched == 0 {
            break;
        }
        offset += fetched;
    }

    PlayerIndex { players }
}

fn stats_of(entry: LeaderboardEntry, badge: Option<&BadgeView>) -> PlayerStatsView {
    PlayerStatsView {
        position: entry.position,
        name: entry.name,
        points: entry.points,
        wins: entry.wins,
        games: entry.games,
        winrate: entry.winrate,
        prestige_rank: entry.prestige_rank,
        rank_image: badge.map(|badge| badge.image.clone()),
        rank_label: badge.map(|badge| badge.label.clone()).unwrap_or_default(),
        last_seen: entry.last_seen,
        streak: entry.streak,
        streak_vacation: entry.streak_vacation,
    }
}

/// Le sole cifre di un friend code: il salvataggio e il server lo scrivono
/// con e senza trattini.
fn digits(friend_code: &str) -> String {
    friend_code.chars().filter(char::is_ascii_digit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(raw: &str) -> Value {
        serde_json::from_str(raw).unwrap()
    }

    fn player(raw: &str) -> LeaderboardEntry {
        entry(&json(raw), 0, 0)
    }

    /// La classifica reale manda `prestigeRank` **e** `rank` nello stesso
    /// oggetto: con gli alias di serde questo payload faceva fallire l'intera
    /// pagina con `duplicate field`.
    #[test]
    fn the_repeated_keys_of_the_server_are_synonyms_not_a_conflict() {
        let entry = player(
            r#"{
                "position": 1,
                "name": "lacly",
                "points": 15088,
                "fc": "0000-0002-0202",
                "prestigeRank": 3,
                "rank": 3,
                "wins": 151,
                "races": 259,
                "games": 259,
                "winrate": 58.3,
                "vr_gain_24h": 6817,
                "vr_last_24_hours": 6817,
                "vrLast24Hours": 6817
            }"#,
        );

        assert_eq!(entry.position, 1);
        assert_eq!(entry.name, "lacly");
        assert_eq!(entry.prestige_rank, 3);
        assert_eq!(entry.vr_last_24_hours, 6817);
        assert_eq!(entry.games, 259);
    }

    #[test]
    fn every_leaderboard_alias_maps_to_the_same_field() {
        for payload in [
            r#"{"prestigeRank":7}"#,
            r#"{"prestige_rank":7}"#,
            r#"{"pr":7}"#,
            r#"{"rank":7}"#,
        ] {
            assert_eq!(player(payload).prestige_rank, 7, "{payload}");
        }

        for payload in [
            r#"{"vr_last_24_hours":10}"#,
            r#"{"vr_gain_24h":10}"#,
            r#"{"vrLast24Hours":10}"#,
        ] {
            assert_eq!(player(payload).vr_last_24_hours, 10, "{payload}");
        }
    }

    #[test]
    fn the_friend_code_accepts_the_short_key() {
        assert_eq!(
            player(r#"{"fc":"0000-1111-2222"}"#).friend_code,
            "0000-1111-2222"
        );
    }

    #[test]
    fn unknown_fields_do_not_break_parsing() {
        assert_eq!(player(r#"{"name":"a","campo_nuovo":{"x":1}}"#).name, "a");
    }

    #[test]
    fn a_null_alias_does_not_hide_the_one_that_carries_the_value() {
        assert_eq!(player(r#"{"prestigeRank":null,"rank":4}"#).prestige_rank, 4);
    }

    #[test]
    fn numbers_sent_as_strings_are_still_numbers() {
        let entry = player(r#"{"points":"15088","wins":"10","races":"20"}"#);
        assert_eq!(entry.points, 15088);
        assert_eq!(entry.wins, 10);
        assert_eq!(entry.games, 20);
    }

    #[test]
    fn the_winrate_is_computed_when_the_server_omits_it() {
        assert!((player(r#"{"wins":5,"races":20}"#).winrate - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_position_falls_back_to_the_page_offset() {
        let entry = entry(&json(r#"{"name":"a"}"#), 2, 100);
        assert_eq!(entry.position, 103);
    }

    #[test]
    fn a_player_without_a_valid_mii_keeps_the_initial() {
        let entry = player(r#"{"name":"sossio","mii_data":"non-un-mii"}"#);
        assert!(entry.studio_data.is_empty());
        assert_eq!(entry.avatar_initial, "S");
        assert!(entry.accent_color.starts_with('#'));
    }

    #[test]
    fn a_real_mii_block_becomes_studio_data() {
        // Blocchi presi dalla risposta vera di `vk_leaderboard.php`: 74 byte
        // in base64, gli stessi che stanno nel salvataggio.
        for payload in [
            r#"{"name":"sossio","mii_data":"gAAAcwBvAHMAcwBpAG8AAAAAAAAAAEBAgAAAAAAAAAAAFVRAic4ookSMCFgUTbCNAIoAiiUEAAAAAAAAAAAAAAAAAAAAAAAAAAA="}"#,
            r#"{"name":"lacly","mii_data":"wBYAbABhAGMAbAB5AAAAAAAAAAAAAG4AgAAAAAAAAAAgTH/gkQQQjFxyDHgAdUAPcMQAigSaAAAAAAAAAAAAAAAAAAAAAAAAAAA="}"#,
        ] {
            let entry = player(payload);
            assert!(!entry.studio_data.is_empty(), "{payload}");
            assert!(entry.accent_color.starts_with('#'));
        }
    }

    #[test]
    fn the_friend_code_is_matched_however_it_is_written() {
        let mut players = HashMap::new();
        players.insert(
            digits("0000-0002-0202"),
            PlayerStatsView {
                points: 15088,
                ..Default::default()
            },
        );
        let index = PlayerIndex { players };

        assert_eq!(index.get("0000-0002-0202").unwrap().points, 15088);
        assert_eq!(index.get("000000020202").unwrap().points, 15088);
        assert!(index.get("1111-2222-3333").is_none());
    }

    #[test]
    fn the_streak_is_read_when_the_server_sends_it() {
        let entry = player(r#"{"name":"lacly","streak":6,"streakVacation":false}"#);
        assert_eq!(entry.streak, 6);
        assert!(!entry.streak_vacation);

        let on_vacation = player(r#"{"streak":"12","streak_vacation":1}"#);
        assert_eq!(on_vacation.streak, 12);
        assert!(on_vacation.streak_vacation);

        // Un valore negativo non è una streak: vale zero.
        assert_eq!(player(r#"{"streak":-3}"#).streak, 0);
    }

    /// È la forma di oggi di `vk_leaderboard.php`: nessuna chiave di streak,
    /// quindi va presa dalla classifica del sito.
    #[test]
    fn a_payload_without_streak_keys_asks_for_the_site_leaderboard() {
        let today = json(r#"[{"name":"a","points":10},{"name":"b"}]"#);
        assert!(!carries_streak(today.as_array().unwrap()));

        // Anche una streak a zero dice che il server la manda.
        let updated = json(r#"[{"name":"a","streak":0},{"name":"b"}]"#);
        assert!(carries_streak(updated.as_array().unwrap()));
    }

    #[test]
    fn the_site_leaderboard_is_indexed_by_friend_code() {
        // Righe prese dalla risposta vera di `leaderboard.php?mii=0`.
        let index = StreakIndex::from_payload(&json(
            r#"{
                "success": true,
                "meta": {"limit": 3, "offset": 0, "count": 3, "total": 124},
                "players": [
                    {"position":1,"name":"lacly","points":22123,"fc":"0000-0002-0202","streak":6,"streakVacation":false,"mii_image":null},
                    {"position":2,"name":"BAMM99x","points":11963,"fc":"4176-1182-7933","streak":0,"streakVacation":false},
                    {"position":3,"name":"sossio","fc":"5078-0614-0949","streak":4,"streakVacation":true},
                    {"position":4,"name":"senza fc","streak":9}
                ]
            }"#,
        ));

        assert_eq!(index.get("0000-0002-0202"), Some((6, false)));
        assert_eq!(index.get("000000020202"), Some((6, false)));
        assert_eq!(index.get("4176-1182-7933"), Some((0, false)));
        assert_eq!(index.get("5078 0614 0949"), Some((4, true)));
        assert_eq!(index.get("1111-2222-3333"), None);
        // Chi non ha un friend code non si può abbinare a nessuno.
        assert_eq!(index.players.len(), 3);
    }

    #[test]
    fn a_broken_site_leaderboard_yields_an_empty_index() {
        assert!(StreakIndex::from_payload(&json(r#"{"error":"x"}"#)).is_empty());
    }

    fn endpoints() -> vk_core::endpoints::EndpointsInfo {
        crate::storage::endpoints::defaults()
    }

    #[test]
    fn only_ranked_players_ask_for_a_rank_image() {
        assert!(badge_source(0, "", &endpoints()).is_none());
        assert!(badge_source(-2, "  ", &endpoints()).is_none());

        let source = badge_source(3, "", &endpoints()).expect("rank 3");
        assert_eq!(source.key, "rank-3");
        assert!(
            source.label.is_empty(),
            "i rank del gioco si chiamano col numero"
        );
        // Prima il sito, che ha tutti i disegni; poi il server del gioco.
        assert_eq!(
            source.urls,
            vec![
                "https://vwfc.vanzakart.net/FOOTAGE/ranks/rank-3.png".to_string(),
                "https://vanzakart.net:8443/FOOTAGE/ranks/rank-3.png".to_string(),
            ]
        );
    }

    /// Lo stemma dello staff: il server lo assegna con `rank_image_url`, e
    /// vince sul rank del gioco.
    #[test]
    fn an_image_assigned_by_the_server_wins_and_is_named_after_its_file() {
        let entry = player(
            r#"{"prestigeRank":2,"rank_image_url":"/FOOTAGE/ranks/staff_ghost_full_00010.png"}"#,
        );
        assert_eq!(
            entry.rank_image_url,
            "/FOOTAGE/ranks/staff_ghost_full_00010.png"
        );

        let source = badge_source(entry.prestige_rank, &entry.rank_image_url, &endpoints())
            .expect("immagine assegnata");
        assert!(source.key.starts_with("custom-"));
        assert_eq!(source.label, "Staff Ghost");
        assert_eq!(
            source.urls.first().map(String::as_str),
            Some("https://vwfc.vanzakart.net/FOOTAGE/ranks/staff_ghost_full_00010.png")
        );
    }

    #[test]
    fn an_assigned_image_outside_the_project_is_ignored() {
        for raw in [
            "http://vwfc.vanzakart.net/x.png",
            "https://example.com/x.png",
            "../../etc/passwd",
        ] {
            let source = badge_source(0, raw, &endpoints());
            assert!(source.is_none(), "{raw}");
        }
        // Un'immagine rifiutata lascia il rank del gioco.
        assert_eq!(
            badge_source(4, "https://example.com/x.png", &endpoints()).map(|source| source.key),
            Some("rank-4".to_string())
        );
    }

    #[test]
    fn a_special_rank_is_named_after_its_file() {
        assert_eq!(badge_label("developer_full_00000.png"), "Developer");
        assert_eq!(
            badge_label("https://x/FOOTAGE/ranks/creative_director_full_00011.png"),
            "Creative Director"
        );
        assert_eq!(
            badge_label("website-launcher-dev.png"),
            "Website Launcher Dev"
        );
    }

    #[test]
    fn the_rooms_payload_tolerates_missing_fields() {
        let view = room(&json(
            r#"{"id":"1","name":"Sala","host":"a","player_count":4}"#,
        ));

        assert_eq!(view.player_count, 4);
        assert_eq!(view.max_players, 12);
        assert_eq!(view.mode, "Versus");
        assert!(view.players.is_empty());
    }

    #[test]
    fn the_players_of_a_room_reach_the_frontend() {
        let view = room(&json(
            r#"{
                "id": "TQHUTZ",
                "name": "Stanza di sossio",
                "player_count": 2,
                "players": [
                    {"name":"sossio","friend_code":"5078-0614-0949","vr":6100,"br":5000,"is_host":true},
                    {"name":"lacly","friend_code":"0000-0002-0202","vr":5400,"br":5000,"is_host":false}
                ]
            }"#,
        ));

        assert_eq!(view.players.len(), 2);
        assert_eq!(view.player_count, 2);
        assert!(view.players[0].is_host);
        assert_eq!(view.players[0].vr, 6100);
        // Il nome dell'host manca dalla stanza: lo dà l'elenco.
        assert_eq!(view.host, "sossio");
        assert_eq!(view.players[1].avatar_initial, "L");
    }

    #[test]
    fn the_room_count_follows_the_listed_players() {
        let view = room(&json(
            r#"{"player_count":0,"players":[{"name":"a"},{"name":"b"}]}"#,
        ));
        assert_eq!(view.player_count, 2);
    }

    #[test]
    fn the_snapshot_timestamp_becomes_rfc3339() {
        assert_eq!(
            rfc3339("2026-08-25 11:33:49.459785+00"),
            "2026-08-25T11:33:49.459785+00:00"
        );
        assert_eq!(
            rfc3339("2026-08-24T21:37:06+00:00"),
            "2026-08-24T21:37:06+00:00"
        );
        assert_eq!(rfc3339("  "), "");
    }
}
