//! Rooms e Leaderboard.
//!
//! Porta `ViewModels/RoomsViewModel.cs` e `ViewModels/LeaderboardViewModel.cs`.
//! I payload del server sono preservati con tutti i loro alias di campo: il
//! server può inviare `prestigeRank`, `prestige_rank`, `pr` o `rank` per la
//! stessa informazione.

use std::sync::Arc;

use serde::Deserialize;

use crate::domain::{LeaderboardEntry, RoomView, RoomsSummary};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const LEADERBOARD_PAGE_SIZE: u32 = 200;

// ---------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RoomsResponse {
    success: bool,
    meta: Option<RoomsMeta>,
    rooms: Option<Vec<RoomPayload>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RoomsMeta {
    total_players: u32,
    total_rooms: u32,
    public_rooms: u32,
    private_rooms: u32,
    last_updated: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RoomPayload {
    id: String,
    name: String,
    host: String,
    #[serde(alias = "player_count", alias = "playerCount")]
    player_count: u32,
    #[serde(alias = "max_players", alias = "maxPlayers")]
    max_players: Option<u32>,
    mode: Option<String>,
    track: Option<String>,
    region: Option<String>,
    status: Option<String>,
}

/// Scarica l'elenco delle stanze.
pub async fn rooms(state: &Arc<AppState>) -> AppResult<RoomsSummary> {
    let url = state.endpoints.read().await.rooms_api_url.clone();
    if url.trim().is_empty() {
        return Err(AppError::Configuration(
            "endpoint delle stanze non configurato".into(),
        ));
    }

    let raw = state.downloader.get_string(&url).await?;
    let response: RoomsResponse = serde_json::from_str(vk_core::json::strip_leading_noise(&raw))
        .map_err(|error| {
            AppError::Internal(format!("risposta delle stanze non valida: {error}"))
        })?;

    if !response.success && response.rooms.is_none() {
        return Err(AppError::Internal(
            "il server ha restituito una risposta non valida".into(),
        ));
    }

    let meta = response.meta.unwrap_or_default();
    let rooms: Vec<RoomView> = response
        .rooms
        .unwrap_or_default()
        .into_iter()
        .map(|room| RoomView {
            id: room.id,
            name: room.name,
            host: room.host,
            player_count: room.player_count,
            max_players: room.max_players.unwrap_or(12),
            mode: room.mode.unwrap_or_else(|| "Versus".into()),
            track: room.track.unwrap_or_else(|| "Choosing Track...".into()),
            region: room.region.unwrap_or_else(|| "Worldwide".into()),
            status: room.status.unwrap_or_else(|| "In Lobby".into()),
        })
        .collect();

    Ok(RoomsSummary {
        total_players: if meta.total_players > 0 {
            meta.total_players
        } else {
            rooms.iter().map(|room| room.player_count).sum()
        },
        total_rooms: if meta.total_rooms > 0 {
            meta.total_rooms
        } else {
            rooms.len() as u32
        },
        public_rooms: meta.public_rooms,
        private_rooms: meta.private_rooms,
        last_updated: meta.last_updated.unwrap_or_default(),
        rooms,
    })
}

// ---------------------------------------------------------------------------
// Leaderboard
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LeaderboardResponse {
    players: Option<Vec<PlayerPayload>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct PlayerPayload {
    position: i32,
    name: String,
    points: i32,
    #[serde(alias = "fc", alias = "friendCode", alias = "friend_code")]
    friend_code: String,
    #[serde(
        alias = "prestigeRank",
        alias = "prestige_rank",
        alias = "pr",
        alias = "rank"
    )]
    prestige_rank: i32,
    wins: i32,
    races: i32,
    games: i32,
    winrate: f64,
    #[serde(alias = "last_seen", alias = "lastSeen")]
    last_seen: Option<String>,
    #[serde(alias = "is_suspicious", alias = "isSuspicious")]
    is_suspicious: bool,
    #[serde(
        alias = "vr_last_24_hours",
        alias = "vr_gain_24h",
        alias = "vrLast24Hours"
    )]
    vr_last_24_hours: i32,
    #[serde(alias = "vr_gain_week", alias = "vrLastWeek")]
    vr_last_week: i32,
    #[serde(alias = "vr_gain_month", alias = "vrLastMonth")]
    vr_last_month: i32,
}

/// Scarica la classifica.
pub async fn leaderboard(state: &Arc<AppState>, offset: u32) -> AppResult<Vec<LeaderboardEntry>> {
    let base = state.endpoints.read().await.leaderboard_api_url.clone();
    if base.trim().is_empty() {
        return Err(AppError::Configuration(
            "endpoint della classifica non configurato".into(),
        ));
    }

    let separator = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{separator}limit={LEADERBOARD_PAGE_SIZE}&offset={offset}");

    let raw = state.downloader.get_string(&url).await?;
    let response: LeaderboardResponse =
        serde_json::from_str(vk_core::json::strip_leading_noise(&raw)).map_err(|error| {
            AppError::Internal(format!("risposta della classifica non valida: {error}"))
        })?;

    let rank_cache = state.paths.rank_images_dir();

    Ok(response
        .players
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, player)| {
            let games = if player.games > 0 {
                player.games
            } else {
                player.races
            };
            let winrate = if player.winrate > 0.0 {
                player.winrate
            } else if games > 0 {
                f64::from(player.wins) / f64::from(games) * 100.0
            } else {
                0.0
            };

            let rank_image = (player.prestige_rank >= 1)
                .then(|| rank_cache.join(format!("rank-{}.png", player.prestige_rank)))
                .filter(|path| path.is_file())
                .map(|path| path.to_string_lossy().to_string());

            LeaderboardEntry {
                position: if player.position > 0 {
                    player.position
                } else {
                    offset as i32 + index as i32 + 1
                },
                name: player.name,
                points: player.points,
                friend_code: player.friend_code,
                prestige_rank: player.prestige_rank,
                wins: player.wins,
                games,
                winrate,
                last_seen: player.last_seen,
                is_suspicious: player.is_suspicious,
                vr_last_24_hours: player.vr_last_24_hours,
                vr_last_week: player.vr_last_week,
                vr_last_month: player.vr_last_month,
                rank_image,
            }
        })
        .collect())
}

/// Scarica in cache le immagini dei rank citate dalla classifica.
///
/// Non è un errore se una singola immagine manca: il rank viene mostrato con
/// il solo numero.
pub async fn cache_rank_images(state: &Arc<AppState>, ranks: &[i32]) -> AppResult<usize> {
    let base = state.endpoints.read().await.rank_images_base_url.clone();
    if base.trim().is_empty() {
        return Ok(0);
    }

    let directory = state.paths.rank_images_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    let mut cached = 0usize;
    let mut wanted: Vec<i32> = ranks.iter().copied().filter(|rank| *rank >= 1).collect();
    wanted.sort_unstable();
    wanted.dedup();

    for rank in wanted {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rooms_payload_tolerates_missing_fields() {
        let response: RoomsResponse = serde_json::from_str(
            r#"{"success":true,"rooms":[{"id":"1","name":"Sala","host":"a","player_count":4}]}"#,
        )
        .unwrap();

        let room = &response.rooms.unwrap()[0];
        assert_eq!(room.player_count, 4);
        assert_eq!(room.max_players, None);
    }

    #[test]
    fn every_leaderboard_alias_maps_to_the_same_field() {
        for payload in [
            r#"{"prestigeRank":7}"#,
            r#"{"prestige_rank":7}"#,
            r#"{"pr":7}"#,
            r#"{"rank":7}"#,
        ] {
            let player: PlayerPayload = serde_json::from_str(payload).unwrap();
            assert_eq!(player.prestige_rank, 7, "{payload}");
        }

        for payload in [
            r#"{"vr_last_24_hours":10}"#,
            r#"{"vr_gain_24h":10}"#,
            r#"{"vrLast24Hours":10}"#,
        ] {
            let player: PlayerPayload = serde_json::from_str(payload).unwrap();
            assert_eq!(player.vr_last_24_hours, 10, "{payload}");
        }
    }

    #[test]
    fn the_friend_code_accepts_the_short_key() {
        let player: PlayerPayload = serde_json::from_str(r#"{"fc":"0000-1111-2222"}"#).unwrap();
        assert_eq!(player.friend_code, "0000-1111-2222");
    }

    #[test]
    fn unknown_fields_do_not_break_parsing() {
        let player: PlayerPayload =
            serde_json::from_str(r#"{"name":"a","campo_nuovo":{"x":1}}"#).unwrap();
        assert_eq!(player.name, "a");
    }
}
