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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::wii_text::humanize;
use crate::domain::{
    LeaderboardEntry, LeaderboardPage, PlayerStatsView, RoomPlayerView, RoomView, RoomsSummary,
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

/// Colori di ripiego per chi non ha un Mii: gli stessi degli amici.
const ACCENTS: [&str; 6] = [
    "#39E7FF", "#FF3B7A", "#FFD166", "#4DFFB0", "#9D5CFF", "#FF8800",
];

// ---------------------------------------------------------------------------
// Lettura tollerante dei payload
// ---------------------------------------------------------------------------

mod loose {
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
            "endpoint delle stanze non configurato".into(),
        ));
    }

    let raw = state.downloader.get_string(&url).await?;
    let payload: Value =
        serde_json::from_str(vk_core::json::strip_leading_noise(&raw)).map_err(|error| {
            AppError::Internal(format!("risposta delle stanze non valida: {error}"))
        })?;

    let listed = loose::pick(&payload, &["rooms"]).is_some();
    if !loose::flag(&payload, &["success"]) && !listed {
        return Err(AppError::Internal(
            "il server ha restituito una risposta non valida".into(),
        ));
    }

    let meta = loose::pick(&payload, &["meta"])
        .cloned()
        .unwrap_or(Value::Null);
    let rooms: Vec<RoomView> = loose::array(&payload, &["rooms"])
        .iter()
        .map(room)
        .collect();

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
            "endpoint della classifica non configurato".into(),
        ));
    }

    let separator = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{separator}limit={LEADERBOARD_PAGE_SIZE}&offset={offset}");

    let raw = state.downloader.get_string(&url).await?;
    let payload: Value =
        serde_json::from_str(vk_core::json::strip_leading_noise(&raw)).map_err(|error| {
            AppError::Internal(format!("risposta della classifica non valida: {error}"))
        })?;

    let mut entries: Vec<LeaderboardEntry> = loose::array(&payload, &["players"])
        .iter()
        .enumerate()
        .map(|(index, player)| entry(player, index, offset))
        .collect();

    attach_rank_images(state, &mut entries).await;

    Ok(LeaderboardPage {
        // Una pagina piena non prova che ce ne sia un'altra, ma è l'unico
        // indizio che il server dà: `meta.count` conta solo questa.
        has_more: entries.len() as u32 >= LEADERBOARD_PAGE_SIZE,
        offset,
        entries,
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
        // La riempie `attach_rank_images`, che prima deve scaricare il file.
        rank_image: None,
        name,
        studio_data: face.studio_data,
        avatar_initial: face.avatar_initial,
        accent_color: face.accent_color,
    }
}

// ---------------------------------------------------------------------------
// Immagini dei rank
// ---------------------------------------------------------------------------

/// Mette in ogni riga l'immagine del proprio rank.
///
/// Il percorso su disco non serviva a niente: la webview non apre file locali,
/// e la riga mostrava il numero anche quando l'immagine c'era. Viaggia come
/// data URI, come già fanno le facce dei Mii (§D-065). Il download avviene
/// **prima** di comporre le righe, altrimenti al primo avvio — quando la cache
/// è vuota — nessuna riga avrebbe la sua immagine.
async fn attach_rank_images(state: &Arc<AppState>, entries: &mut [LeaderboardEntry]) {
    let ranks: Vec<i32> = entries.iter().map(|entry| entry.prestige_rank).collect();
    let images = rank_images(state, &ranks).await;

    for entry in entries {
        entry.rank_image = images.get(&entry.prestige_rank).cloned();
    }
}

/// Immagini dei rank citati, come data URI, scaricando quelle che mancano.
async fn rank_images(state: &Arc<AppState>, ranks: &[i32]) -> HashMap<i32, String> {
    let wanted = distinct_ranks(ranks);
    if wanted.is_empty() {
        return HashMap::new();
    }

    let _ = cache_rank_images(state, &wanted).await;

    let directory = state.paths.rank_images_dir();
    let mut images = HashMap::new();

    for rank in wanted {
        let path = directory.join(format!("rank-{rank}.png"));
        match tokio::fs::read(&path).await {
            Ok(bytes) if !bytes.is_empty() => {
                images.insert(
                    rank,
                    format!(
                        "data:image/png;base64,{}",
                        vk_save::mii::base64_encode(&bytes)
                    ),
                );
            }
            _ => {}
        }
    }

    images
}

/// Rank rankati, una volta sola ciascuno.
fn distinct_ranks(ranks: &[i32]) -> Vec<i32> {
    let mut wanted: Vec<i32> = ranks.iter().copied().filter(|rank| *rank >= 1).collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted
}

/// Scarica in cache le immagini dei rank citate dalla classifica.
///
/// Non è un errore se una singola immagine manca: il rank viene mostrato con
/// il solo numero.
async fn cache_rank_images(state: &Arc<AppState>, ranks: &[i32]) -> AppResult<usize> {
    let base = state.endpoints.read().await.rank_images_base_url.clone();
    if base.trim().is_empty() {
        return Ok(0);
    }

    let directory = state.paths.rank_images_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    let mut cached = 0usize;

    for &rank in ranks {
        let destination = directory.join(format!("rank-{rank}.png"));
        if destination.is_file() {
            continue;
        }

        let url = format!("{}/rank-{rank}.png", base.trim_end_matches('/'));
        match state
            .downloader
            .download_with_resume(
                &url,
                &destination,
                &vk_core::progress::noop_sink(),
                &vk_core::progress::CancelToken::new(),
            )
            .await
        {
            Ok(_) => cached += 1,
            Err(error) => {
                let _ = tokio::fs::remove_file(&destination).await;
                tracing::debug!(
                    rank,
                    error = %vk_core::redact::redact(&error.to_string()),
                    "immagine del rank non disponibile"
                );
            }
        }
    }

    Ok(cached)
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
                players.insert(key, stats_of(entry));
            }
        }

        if !page.has_more || fetched == 0 {
            break;
        }
        offset += fetched;
    }

    PlayerIndex { players }
}

fn stats_of(entry: LeaderboardEntry) -> PlayerStatsView {
    PlayerStatsView {
        position: entry.position,
        name: entry.name,
        points: entry.points,
        wins: entry.wins,
        games: entry.games,
        winrate: entry.winrate,
        prestige_rank: entry.prestige_rank,
        rank_image: entry.rank_image,
        last_seen: entry.last_seen,
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
    fn only_ranked_players_ask_for_a_rank_image() {
        assert_eq!(distinct_ranks(&[0, 3, 3, 1, 0, -2]), vec![1, 3]);
        assert!(distinct_ranks(&[0, 0]).is_empty());
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
