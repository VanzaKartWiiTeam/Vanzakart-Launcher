//! # vk-core
//!
//! Dominio del launcher VanzaKart: contratti JSON del server, download con
//! resume e mirror, verifica SHA-256, estrazione ZIP sicura, protezione dei
//! dati utente e aggiornamento transazionale della modpack.
//!
//! Il crate **non conosce Tauri, il sistema operativo o la posizione dei dati
//! dell'applicazione**: riceve percorsi assoluti già risolti dal chiamante.
//! È la garanzia che impedisce alle API di piattaforma di contaminare il core
//! (vedi `docs/decisions.md` §D-001 e §D-002).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod backup;
pub mod endpoints;
pub mod error;
pub mod fsx;
pub mod hash;
pub mod json;
pub mod manifest;
pub mod net;
pub mod progress;
pub mod protect;
pub mod redact;
pub mod update;
pub mod versions;
pub mod zipx;

pub use error::{CoreError, CoreResult};
pub use manifest::{ModManifest, ModManifestFile};
pub use progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};
pub use protect::{ModLayout, ProtectionRules};
pub use versions::{is_newer, Channel, VersionInfo};

/// User-Agent usato per tutte le richieste HTTP del launcher.
pub fn user_agent(version: &str) -> String {
    format!("VanzaKartLauncher/{version} (Tauri)")
}

/// Millisecondi dall'epoch, per le query anti-cache.
pub fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_carries_the_version() {
        assert_eq!(user_agent("2.0.0"), "VanzaKartLauncher/2.0.0 (Tauri)");
    }

    #[test]
    fn now_millis_is_after_2020() {
        assert!(now_millis() > 1_577_836_800_000);
    }
}
