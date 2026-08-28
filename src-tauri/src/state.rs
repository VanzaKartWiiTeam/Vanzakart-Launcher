//! Stato condiviso dell'applicazione.

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use vk_core::endpoints::EndpointsInfo;
use vk_core::net::Downloader;
use vk_core::progress::CancelToken;

use crate::error::{AppError, AppResult};
use crate::storage::install_state::InstallState;
use crate::storage::paths::AppPaths;
use crate::storage::preferences::UserPreferences;
use crate::storage::secrets::Secrets;
use crate::storage::settings::LauncherSettings;

/// Versione del launcher, dal `Cargo.toml`.
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ultime informazioni di versione scaricate dal server.
#[derive(Debug, Clone, Default)]
pub struct RemoteVersions {
    pub info: vk_core::VersionInfo,
    pub checked: bool,
    pub message: String,
}

/// Stato condiviso fra i comandi.
#[derive(Debug)]
pub struct AppState {
    pub paths: AppPaths,
    pub downloader: Downloader,
    /// `false` per uno stato isolato: il render degli avatar non contatta Mii
    /// Studio. La suite verifica cache, chiavi e rifiuti senza rete, che
    /// altrimenti renderebbe i test lenti e dipendenti da un servizio esterno.
    pub avatar_render_online: bool,
    pub settings: RwLock<LauncherSettings>,
    pub preferences: RwLock<UserPreferences>,
    pub install_state: RwLock<InstallState>,
    pub secrets: RwLock<Secrets>,
    pub endpoints: RwLock<EndpointsInfo>,
    pub remote: RwLock<RemoteVersions>,
    /// Garantisce che un solo aggiornamento della modpack sia in corso.
    pub mod_operation: Mutex<()>,
    /// Token dell'operazione in corso, per l'annullamento dalla UI.
    pub cancel: RwLock<CancelToken>,
    /// Processo di gioco attivo, se presente.
    pub game_session: RwLock<Option<GameSession>>,
    /// Catalogo GameBanana, scaricato una volta sola per sessione.
    ///
    /// Serve alla ricerca per nome: `Mod/Index` non accetta un filtro
    /// testuale, quindi la corrispondenza si fa in locale sui nomi.
    pub gamebanana_catalog: RwLock<Option<crate::services::gamebanana::Catalog>>,
    /// Classifica indicizzata per friend code, con il momento in cui è stata
    /// presa: serve alla lista amici, che altrimenti la richiederebbe a ogni
    /// apertura (§D-064).
    pub leaderboard_index: RwLock<
        Option<(
            std::time::Instant,
            std::sync::Arc<crate::services::community::PlayerIndex>,
        )>,
    >,
}

/// Sessione di gioco in corso.
#[derive(Debug, Clone)]
pub struct GameSession {
    pub pid: u32,
    pub started_at: std::time::Instant,
}

impl AppState {
    /// Costruisce lo stato leggendo tutto ciò che è già su disco, importando
    /// i dati del launcher legacy se presenti.
    pub async fn bootstrap(paths: AppPaths) -> AppResult<Arc<Self>> {
        let sources = crate::storage::migration::legacy_sources();
        Self::bootstrap_with(paths, &sources, true).await
    }

    /// Come [`Self::bootstrap`] ma senza cercare dati legacy.
    ///
    /// È la variante usata dai test: garantisce che una suite non possa
    /// leggere l'installazione reale della macchina su cui gira.
    pub async fn bootstrap_isolated(paths: AppPaths) -> AppResult<Arc<Self>> {
        Self::bootstrap_with(paths, &[], false).await
    }

    async fn bootstrap_with(
        paths: AppPaths,
        legacy_sources: &[std::path::PathBuf],
        avatar_render_online: bool,
    ) -> AppResult<Arc<Self>> {
        paths.ensure()?;

        let import = crate::storage::migration::run_legacy_import(&paths, legacy_sources).await?;
        if import.performed {
            tracing::info!(files = import.files.len(), "dati legacy importati");
        }

        let settings = crate::storage::settings::load(&paths).await?;
        let preferences = crate::storage::preferences::load(&paths).await?;
        let install_state = crate::storage::install_state::load(&paths).await?;
        let secrets = crate::storage::secrets::load(&paths).await?;
        let endpoints = crate::storage::endpoints::load(&paths).await?;

        let downloader = Downloader::new(&vk_core::user_agent(LAUNCHER_VERSION))
            .map_err(|error| AppError::Internal(error.to_string()))?;

        Ok(Arc::new(Self {
            paths,
            downloader,
            avatar_render_online,
            settings: RwLock::new(settings),
            preferences: RwLock::new(preferences),
            install_state: RwLock::new(install_state),
            secrets: RwLock::new(secrets),
            endpoints: RwLock::new(endpoints),
            remote: RwLock::new(RemoteVersions::default()),
            mod_operation: Mutex::new(()),
            cancel: RwLock::new(CancelToken::new()),
            game_session: RwLock::new(None),
            gamebanana_catalog: RwLock::new(None),
            leaderboard_index: RwLock::new(None),
        }))
    }

    /// Canale selezionato.
    pub async fn channel(&self) -> vk_core::Channel {
        self.preferences.read().await.channel
    }

    /// Layout della modpack per un canale.
    pub async fn layout(&self, channel: vk_core::Channel) -> vk_core::ModLayout {
        let settings = self.settings.read().await;
        vk_core::ModLayout::new(settings.mod_folder(&self.paths), channel)
    }

    /// Sostituisce il token di annullamento e restituisce quello nuovo.
    pub async fn renew_cancel_token(&self) -> CancelToken {
        let token = CancelToken::new();
        *self.cancel.write().await = token.clone();
        token
    }

    /// Annulla l'operazione in corso, se ce n'è una.
    pub async fn cancel_current(&self) {
        self.cancel.read().await.cancel();
    }

    /// Persiste le impostazioni correnti.
    pub async fn persist_settings(&self) -> AppResult<()> {
        let settings = self.settings.read().await.clone();
        crate::storage::settings::save(&self.paths, &settings).await
    }

    /// Persiste le preferenze correnti.
    pub async fn persist_preferences(&self) -> AppResult<()> {
        let preferences = self.preferences.read().await.clone();
        crate::storage::preferences::save(&self.paths, &preferences).await
    }

    /// Persiste lo stato di installazione corrente.
    pub async fn persist_install_state(&self) -> AppResult<()> {
        let state = self.install_state.read().await.clone();
        crate::storage::install_state::save(&self.paths, &state).await
    }
}

/// Timestamp ISO-8601 UTC, il formato usato nei file di stato.
pub fn now_iso() -> String {
    let now = time::OffsetDateTime::now_utc();
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bootstrap_creates_the_data_tree_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.path().join("VanzaKart")))
            .await
            .unwrap();

        assert!(state.paths.logs_dir().is_dir());
        assert_eq!(state.channel().await, vk_core::Channel::Stable);
        assert!(state.settings.read().await.dolphin_path.is_empty());
    }

    #[tokio::test]
    async fn bootstrap_reads_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path().join("VanzaKart"));
        paths.ensure().unwrap();

        crate::storage::settings::save(
            &paths,
            &LauncherSettings {
                dolphin_path: "/opt/dolphin/Dolphin".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let state = AppState::bootstrap_isolated(paths).await.unwrap();
        assert_eq!(
            state.settings.read().await.dolphin_path,
            "/opt/dolphin/Dolphin"
        );
    }

    #[tokio::test]
    async fn the_layout_follows_the_configured_user_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.path().join("VanzaKart")))
            .await
            .unwrap();

        state.settings.write().await.user_folder_path = "/home/a/Dolphin Emulator".into();

        let layout = state.layout(vk_core::Channel::Beta).await;
        assert_eq!(
            layout.mod_root(),
            std::path::PathBuf::from("/home/a/Dolphin Emulator/Load/Riivolution/VKBeta")
        );
    }

    #[tokio::test]
    async fn renewing_the_cancel_token_detaches_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap_isolated(AppPaths::at(dir.path().join("VanzaKart")))
            .await
            .unwrap();

        let first = state.renew_cancel_token().await;
        let second = state.renew_cancel_token().await;

        state.cancel_current().await;
        assert!(second.is_cancelled());
        assert!(!first.is_cancelled());
    }

    #[test]
    fn the_timestamp_is_rfc3339() {
        let stamp = now_iso();
        assert!(stamp.contains('T'), "{stamp}");
        assert!(stamp.ends_with('Z') || stamp.contains('+'), "{stamp}");
    }
}
