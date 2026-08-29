//! Verifica del token di accesso beta.
//!
//! Porta `Launcher/Services/BetaAccessService.cs`. Il token non attraversa mai
//! l'IPC dopo essere stato salvato: la UI riceve solo `hasToken` e la forma
//! mascherata.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::secrets::Secrets;

/// Esito della verifica, nella forma che riceve la UI.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BetaStatus {
    pub has_token: bool,
    pub masked_token: String,
    pub verified: bool,
    pub message: String,
    /// `true` quando l'esito negativo dipende dalla rete, non dal token:
    /// in quel caso il token salvato **non** viene rimosso.
    pub network_error: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct VerifyResponse {
    success: bool,
    message: String,
    error: String,
}

/// Stato corrente, senza contattare il server.
pub async fn status(state: &Arc<AppState>) -> BetaStatus {
    let secrets = state.secrets.read().await;
    BetaStatus {
        has_token: secrets.has_beta_token(),
        masked_token: secrets.masked_beta_token(),
        verified: false,
        message: if secrets.has_beta_token() {
            "Token saved.".into()
        } else {
            "No token saved.".into()
        },
        network_error: false,
    }
}

/// Verifica un token contro il server e, se valido, lo salva.
pub async fn verify_and_store(state: &Arc<AppState>, token: &str) -> AppResult<BetaStatus> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AppError::BadRequest("Enter an access token.".into()));
    }

    let outcome = verify(state, &token).await;

    if outcome.verified {
        let secrets = Secrets {
            beta_access_token: token,
        };
        crate::storage::secrets::save(&state.paths, &secrets).await?;
        *state.secrets.write().await = secrets;
        tracing::info!("token beta verificato e salvato");
    }

    Ok(BetaStatus {
        has_token: state.secrets.read().await.has_beta_token(),
        masked_token: state.secrets.read().await.masked_beta_token(),
        ..outcome
    })
}

/// Verifica all'avvio il token già salvato.
///
/// Un errore di rete lascia il token al suo posto: l'utente non deve perdere
/// l'accesso alla beta perché il server è momentaneamente irraggiungibile.
pub async fn validate_saved(state: &Arc<AppState>) -> BetaStatus {
    let token = state.secrets.read().await.beta_access_token.clone();
    if token.trim().is_empty() {
        return status(state).await;
    }

    let outcome = verify(state, &token).await;

    if !outcome.verified && !outcome.network_error {
        tracing::warn!("il token beta salvato non è più valido: rimosso");
        let empty = Secrets::default();
        let _ = crate::storage::secrets::save(&state.paths, &empty).await;
        *state.secrets.write().await = empty;

        // Il canale beta non è più utilizzabile: si torna a stable.
        let mut preferences = state.preferences.write().await;
        if preferences.channel == vk_core::Channel::Beta {
            preferences.channel = vk_core::Channel::Stable;
            drop(preferences);
            let _ = state.persist_preferences().await;
        }
    }

    BetaStatus {
        has_token: state.secrets.read().await.has_beta_token(),
        masked_token: state.secrets.read().await.masked_beta_token(),
        ..outcome
    }
}

/// Rimuove il token salvato e riporta il canale su stable.
pub async fn clear(state: &Arc<AppState>) -> AppResult<BetaStatus> {
    let empty = Secrets::default();
    crate::storage::secrets::save(&state.paths, &empty).await?;
    *state.secrets.write().await = empty;

    let mut preferences = state.preferences.write().await;
    if preferences.channel == vk_core::Channel::Beta {
        preferences.channel = vk_core::Channel::Stable;
    }
    drop(preferences);
    state.persist_preferences().await?;

    Ok(status(state).await)
}

async fn verify(state: &Arc<AppState>, token: &str) -> BetaStatus {
    let url = state
        .endpoints
        .read()
        .await
        .beta_token_verify_api_url
        .clone();

    if url.trim().is_empty() {
        return BetaStatus {
            verified: false,
            message: "Verification endpoint not configured.".into(),
            network_error: true,
            ..Default::default()
        };
    }

    let body = serde_json::json!({ "token": token });

    match state.downloader.post_json(&url, &body).await {
        Ok(raw) => {
            match serde_json::from_str::<VerifyResponse>(vk_core::json::strip_leading_noise(&raw)) {
                Ok(response) if response.success => BetaStatus {
                    verified: true,
                    message: if response.message.trim().is_empty() {
                        "Token valid.".into()
                    } else {
                        response.message
                    },
                    ..Default::default()
                },
                Ok(response) => BetaStatus {
                    verified: false,
                    message: if response.error.trim().is_empty() {
                        "Invalid access token.".into()
                    } else {
                        response.error
                    },
                    ..Default::default()
                },
                Err(_) => BetaStatus {
                    verified: false,
                    message: "Invalid response from the verification server.".into(),
                    network_error: true,
                    ..Default::default()
                },
            }
        }
        Err(error) => {
            tracing::warn!(
                error = %vk_core::redact::redact(&error.to_string()),
                "verifica del token beta non riuscita"
            );
            BetaStatus {
                verified: false,
                message: "Verification server unreachable. Try again later.".into(),
                network_error: true,
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    async fn state_with(dir: &std::path::Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_status_reports_a_missing_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let status = status(&state).await;
        assert!(!status.has_token);
        assert!(status.masked_token.is_empty());
    }

    #[tokio::test]
    async fn a_blank_token_is_refused_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let error = verify_and_store(&state, "   ").await.unwrap_err();
        assert_eq!(error.code(), "bad-request");
    }

    #[tokio::test]
    async fn a_network_failure_keeps_the_saved_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        state.secrets.write().await.beta_access_token = "token-salvato".into();
        // L'endpoint punta a un host irraggiungibile.
        state.endpoints.write().await.beta_token_verify_api_url =
            "https://127.0.0.1:9/verify".into();

        let outcome = validate_saved(&state).await;

        assert!(!outcome.verified);
        assert!(outcome.network_error);
        assert!(outcome.has_token, "il token non doveva essere rimosso");
    }

    #[tokio::test]
    async fn clearing_the_token_returns_to_stable() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        state.secrets.write().await.beta_access_token = "token".into();
        state.preferences.write().await.channel = vk_core::Channel::Beta;

        let status = clear(&state).await.unwrap();

        assert!(!status.has_token);
        assert_eq!(state.channel().await, vk_core::Channel::Stable);
    }

    #[tokio::test]
    async fn a_missing_endpoint_is_treated_as_a_network_error() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        state.endpoints.write().await.beta_token_verify_api_url = String::new();

        let outcome = verify(&state, "x").await;
        assert!(outcome.network_error);
        assert!(!outcome.verified);
    }

    #[test]
    fn the_status_never_carries_the_raw_token() {
        let status = BetaStatus {
            has_token: true,
            masked_token: "ab***".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("ab***"));
        assert!(!json.contains("beta_access_token"));
    }
}
