//! Tipi scambiati con il frontend.
//!
//! Sono deliberatamente separati dai tipi di dominio: il frontend riceve dati
//! già risolti e sanitizzati, mai URL di configurazione o percorsi che non gli
//! servono (vedi `docs/decisions.md` §D-005 e §D-017).

use serde::{Deserialize, Serialize};
use vk_core::Channel;

/// Stato complessivo mostrato all'avvio.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    pub launcher_version: String,
    pub platform: String,
    pub channel: Channel,
    pub settings_complete: bool,
    pub missing_settings: Vec<String>,
    pub mod_state: ModStatus,
    pub stats: PlayStatsView,
    pub has_beta_token: bool,
    pub beta_token_masked: String,
    pub dolphin_detected: bool,
    pub dolphin_running: bool,
    /// `false` in una build compilata senza la feature `save-writes`: la UI
    /// nasconde tutto ciò che modificherebbe `rksys.dat` o `RFL_DB.dat`.
    pub save_writes_enabled: bool,
}

/// Stato della modpack per il canale selezionato.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModStatus {
    pub channel: Channel,
    pub installed: bool,
    pub installed_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub checked: bool,
    pub check_message: String,
    pub mod_folder: String,
    pub other_channel_installed: bool,
    pub other_channel_version: String,
    pub changelog: Vec<String>,
    /// `true` quando la modpack risulta installata ma il suo descrittore
    /// Riivolution non è utilizzabile: Dolphin partirebbe sul disco originale.
    pub needs_repair: bool,
    /// Motivo leggibile del punto precedente, vuoto se non c'è nulla da
    /// riparare.
    pub repair_reason: String,
}

impl ModStatus {
    /// Etichetta del badge, con le stesse parole della UI legacy.
    pub fn badge(&self) -> &'static str {
        if !self.checked {
            "Idle"
        } else if !self.installed {
            "Not installed"
        } else if self.needs_repair {
            "Repair"
        } else if self.update_available {
            "Update"
        } else {
            "Up to date"
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStatsView {
    pub last_played_utc: Option<String>,
    pub launch_count: u64,
    pub total_play_time_minutes: f64,
}

/// Percorsi configurabili, con l'esito della validazione.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub dolphin_path: String,
    pub dolphin_valid: bool,
    pub rom_path: String,
    pub rom_valid: bool,
    pub user_folder_path: String,
    pub user_folder_valid: bool,
    pub mod_folder: String,
    pub controller_mode: String,
    pub detected_user_folders: Vec<String>,
    pub separate_savegame: bool,
    pub my_stuff_enabled: bool,
    pub auto_check_updates: bool,
    pub download_concurrency: usize,
}

/// Aggiornamento di stato inviato durante un'operazione lunga.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub operation: String,
    pub phase: String,
    pub detail: String,
    pub percent: Option<f64>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u32,
    pub files_total: u32,
    /// "12,4 MB / 40,0 MB", vuoto quando la dimensione totale non è nota.
    pub bytes_label: String,
    /// "3,2 MB/s", vuoto finché non c'è abbastanza traffico per misurarla.
    pub speed_label: String,
}

/// Esito di un'installazione o di un aggiornamento.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub channel: Channel,
    pub was_update: bool,
    pub version: String,
    pub mode: String,
    pub files_written: u32,
    pub files_skipped: u32,
    pub files_pruned: u32,
    pub summary: String,
    pub warnings: Vec<String>,
    pub backup_id: Option<String>,
}

/// Notizia mostrata nella pagina News.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsItem {
    pub title: String,
    pub category: String,
    pub version: String,
    pub summary: String,
    pub date_label: String,
    pub is_pinned: bool,
    pub media_path: Option<String>,
    pub media_kind: Option<String>,
}

/// Giocatore dentro una stanza.
///
/// Il server manda l'elenco insieme alla stanza: senza di esso una stanza è
/// solo un contatore, ed è chi c'è dentro che interessa a chi guarda.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoomPlayerView {
    pub name: String,
    pub friend_code: String,
    pub vr: i32,
    pub br: i32,
    pub is_host: bool,
    /// Payload di render del Mii, vuoto quando il server non lo manda.
    pub studio_data: String,
    pub avatar_initial: String,
    pub accent_color: String,
}

/// Stanza online.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub host: String,
    pub player_count: u32,
    pub max_players: u32,
    pub mode: String,
    pub track: String,
    pub region: String,
    pub status: String,
    pub players: Vec<RoomPlayerView>,
}

/// Statistiche aggregate delle stanze.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomsSummary {
    pub total_players: u32,
    pub total_rooms: u32,
    pub public_rooms: u32,
    pub private_rooms: u32,
    /// Istante dello snapshot in RFC 3339, vuoto se il server non lo manda.
    ///
    /// Non è un dettaglio: lo snapshot lo scrive il server di gioco, e se
    /// smette di scriverlo l'elenco resta fermo senza sembrarlo (§D-057).
    pub last_updated: String,
    /// Stato dichiarato dal server ("Online", "Online (Demo)", …).
    pub status: String,
    /// Avviso del server, presente quando risponde con dati dimostrativi.
    pub notice: String,
    pub rooms: Vec<RoomView>,
}

/// Riga di classifica.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntry {
    pub position: i32,
    pub name: String,
    pub points: i32,
    pub friend_code: String,
    pub prestige_rank: i32,
    pub wins: i32,
    pub games: i32,
    pub winrate: f64,
    pub last_seen: Option<String>,
    pub is_suspicious: bool,
    pub vr_last_24_hours: i32,
    pub vr_last_week: i32,
    pub vr_last_month: i32,
    /// Percorso locale dell'immagine del rank, se già in cache.
    pub rank_image: Option<String>,
    /// Payload di render del Mii del giocatore, vuoto quando il server non
    /// manda un blocco Mii valido.
    pub studio_data: String,
    pub avatar_initial: String,
    pub accent_color: String,
}

/// Una pagina di classifica.
///
/// La classifica non sta in una risposta sola: il server ne manda al massimo
/// cento righe per volta, e la UI deve sapere se chiederne altre.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardPage {
    pub entries: Vec<LeaderboardEntry>,
    pub offset: u32,
    /// `true` quando la pagina è piena: il server potrebbe averne un'altra.
    pub has_more: bool,
}

/// Licenza letta da `rksys.dat`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseView {
    /// Posizione del salvataggio nell'elenco dei file trovati.
    ///
    /// È così che il frontend indica su quale file operare: i percorsi che
    /// riceve sono redatti e non sono riconvertibili in un percorso reale.
    pub save_index: usize,
    pub slot: usize,
    pub is_empty: bool,
    pub name: String,
    pub mii_name: String,
    /// Identificativo del Mii che la licenza indica in `RFL_DB.dat`.
    pub mii_id: u32,
    /// Payload di render del Mii della licenza, vuoto quando non è nel
    /// database di Dolphin. Non è un percorso: è il Mii stesso, codificato.
    pub studio_data: String,
    pub friend_code: String,
    pub vr: u32,
    pub br: u32,
    pub races: u32,
    pub wins: u32,
    pub win_rate: f64,
    pub accent_color: String,
    pub avatar_initial: String,
    pub source_label: String,
    pub save_path: String,
    pub region: String,
    pub friend_count: usize,
}

/// Statistiche di un giocatore secondo il server.
///
/// Sono le stesse righe della classifica, riusate dove serve sapere come va
/// davvero un giocatore invece di come andava l'ultima volta che il
/// salvataggio l'ha visto (§D-064).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatsView {
    pub position: i32,
    pub name: String,
    pub points: i32,
    pub wins: i32,
    pub games: i32,
    pub winrate: f64,
    pub prestige_rank: i32,
    /// Immagine del rank come data URI, quando esiste.
    pub rank_image: Option<String>,
    pub last_seen: Option<String>,
}

/// Un amico salvato dentro una licenza.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendView {
    pub slot: usize,
    pub friend_code: String,
    pub mii_name: String,
    /// Payload di render del Mii dell'amico, letto dal salvataggio.
    pub studio_data: String,
    pub wins: u32,
    pub losses: u32,
    pub race_rating: u32,
    pub battle_rating: u32,
    /// Richiesta inviata dal launcher, non ancora confermata dal server.
    pub is_pending: bool,
    pub avatar_initial: String,
    pub accent_color: String,
    /// Come va questo giocatore secondo il server; `None` se non è in
    /// classifica o se il server non risponde.
    pub stats: Option<PlayerStatsView>,
}

/// Voce della pagina Debug.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub label: String,
    pub value: String,
    pub ok: Option<bool>,
}

/// Controller rilevato.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub dolphin_device: String,
    pub connected: bool,
    pub supports_rumble: bool,
    pub is_configured: bool,
}

/// Addon installato.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AddonView {
    pub id: String,
    pub name: String,
    pub author: String,
    pub source: String,
    pub source_url: String,
    pub preview_url: String,
    pub installed_utc: String,
    pub file_count: usize,
    pub enabled: bool,
    pub managed: bool,
}

/// Conflitto fra file di addon.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub file_name: String,
    pub count: usize,
    pub locations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badge_follows_the_legacy_wording() {
        let mut status = ModStatus::default();
        assert_eq!(status.badge(), "Idle");

        status.checked = true;
        assert_eq!(status.badge(), "Not installed");

        status.installed = true;
        status.update_available = true;
        assert_eq!(status.badge(), "Update");

        status.update_available = false;
        assert_eq!(status.badge(), "Up to date");
    }

    #[test]
    fn dtos_serialize_in_camel_case() {
        let json = serde_json::to_string(&ModStatus::default()).unwrap();
        assert!(json.contains("installedVersion"));
        assert!(json.contains("updateAvailable"));
        assert!(!json.contains("installed_version"));
    }

    #[test]
    fn news_items_accept_the_server_payload() {
        let item: NewsItem = serde_json::from_str(
            r#"{"Title":"x","title":"Novità","category":"UPDATE","isPinned":true,"extra":1}"#,
        )
        .unwrap();
        assert_eq!(item.title, "Novità");
        assert!(item.is_pinned);
    }
}
