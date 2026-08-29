//! Aggiornamento del launcher stesso.
//!
//! Porta la parte di `MainWindow.PerformLauncherUpdateAsync` che **rileva**
//! l'aggiornamento: `versions.json` pubblica `launcher_version`, e il launcher
//! lo confronta con la propria.
//!
//! L'installazione non passa da qui. Il launcher legacy scaricava uno zip e lo
//! srotolava sopra la propria cartella con uno script PowerShell non firmato;
//! questo usa l'updater di Tauri, che verifica una firma Ed25519 prima di
//! installare qualunque cosa (vedi `docs/release.md` §5 e §7). Finché il
//! manifest firmato non è pubblicato, questa pagina dice che c'è una versione
//! nuova e da dove prenderla, invece di far finta di niente.

use std::sync::Arc;

use serde::Serialize;

use crate::error::AppResult;
use crate::state::{AppState, LAUNCHER_VERSION};

/// Stato dell'aggiornamento del launcher, per il frontend.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateStatus {
    /// Versione in esecuzione.
    pub current: String,
    /// Versione pubblicata su `versions.json`, vuota se non l'abbiamo letta.
    pub latest: String,
    /// `true` solo se quella pubblicata è **più recente** di quella in uso.
    pub available: bool,
    pub changelog: Vec<String>,
    /// Pagina da cui scaricarla, finché l'updater firmato non è attivo.
    pub download_page: String,
    /// `false` quando `versions.json` non è stato letto in questa sessione.
    pub checked: bool,
    pub message: String,
}

/// Confronta la versione in esecuzione con quella pubblicata.
///
/// Non contatta nessuno: legge ciò che `mods::check_updates` ha già scaricato.
pub async fn status(state: &Arc<AppState>) -> AppResult<LauncherUpdateStatus> {
    let remote = state.remote.read().await.clone();
    let download_page = state.endpoints.read().await.download_page_url.clone();

    let latest = remote.info.launcher_version.trim().to_string();
    let available = !latest.is_empty() && vk_core::is_newer(&latest, LAUNCHER_VERSION);

    let message = if !remote.checked {
        "No check run in this session.".to_string()
    } else if latest.is_empty() {
        "The server publishes no launcher version.".to_string()
    } else if available {
        format!("Version {latest} is available.")
    } else {
        format!("The launcher is up to date (v{LAUNCHER_VERSION}).")
    };

    Ok(LauncherUpdateStatus {
        current: LAUNCHER_VERSION.to_string(),
        latest,
        available,
        changelog: remote.info.launcher_changelog.clone(),
        download_page,
        checked: remote.checked,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use std::path::Path;

    async fn state_at(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    async fn with_published(dir: &Path, version: &str) -> Arc<AppState> {
        let state = state_at(dir).await;
        {
            let mut remote = state.remote.write().await;
            remote.info.launcher_version = version.to_string();
            remote.checked = true;
        }
        state
    }

    #[tokio::test]
    async fn without_a_check_nothing_is_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let status = status(&state).await.unwrap();
        assert_eq!(status.current, LAUNCHER_VERSION);
        assert!(!status.checked);
        assert!(!status.available);
        assert!(status.latest.is_empty());
    }

    #[tokio::test]
    async fn a_newer_published_version_is_offered() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), "99.0.0").await;

        let status = status(&state).await.unwrap();
        assert!(status.available);
        assert_eq!(status.latest, "99.0.0");
        assert!(status.message.contains("99.0.0"));
    }

    /// Il caso reale: `versions.json` pubblica il launcher C# 1.5.1 mentre qui
    /// gira la 2.0.0. Annunciarlo come aggiornamento manderebbe tutti indietro.
    #[tokio::test]
    async fn an_older_published_version_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), "1.5.1").await;

        let status = status(&state).await.unwrap();
        assert!(!status.available, "1.5.1 non è più recente di 2.0.0");
        assert!(status.message.contains("up to date"));
    }

    #[tokio::test]
    async fn the_same_version_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let state = with_published(dir.path(), LAUNCHER_VERSION).await;

        assert!(!status(&state).await.unwrap().available);
    }
}
