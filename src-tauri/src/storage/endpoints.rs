//! Endpoint del server: default compilati, cache locale e refresh remoto.
//!
//! I default arrivano da `resources/endpoints.default.json`, che è una copia
//! di `endpoints.json` del launcher legacy. Il frontend non vede mai un URL di
//! configurazione (vedi `docs/decisions.md` §D-005).

use vk_core::endpoints::EndpointsInfo;

use crate::error::{AppError, AppResult};
use crate::storage::paths::AppPaths;

/// Copia compilata dei default, così il launcher funziona al primo avvio
/// anche senza rete e senza file di risorse installati.
const DEFAULT_ENDPOINTS_JSON: &str = include_str!("../../resources/endpoints.default.json");

/// URL di `versions.json`, non presente in `endpoints.json`.
pub const DEFAULT_VERSIONS_URL: &str = "https://sitodaking.it:8443/Launcher/versions.json";

/// Endpoint di default, dal file compilato nel binario.
pub fn defaults() -> EndpointsInfo {
    EndpointsInfo::parse(DEFAULT_ENDPOINTS_JSON).unwrap_or_else(|error| {
        // Non può accadere: il file è validato dal test qui sotto.
        tracing::error!(%error, "endpoints di default non parsabili");
        EndpointsInfo::default()
    })
}

/// Endpoint effettivi: default fusi con l'ultima copia remota valida.
pub async fn load(paths: &AppPaths) -> AppResult<EndpointsInfo> {
    let mut resolved = defaults();

    if let Some(raw) = vk_core::fsx::read_text_opt(&paths.endpoints_cache_file()).await? {
        match EndpointsInfo::parse(&raw) {
            Ok(cached) => {
                let rejected = resolved.merge_remote(&cached);
                if !rejected.is_empty() {
                    tracing::warn!(?rejected, "endpoint in cache scartati");
                }
            }
            Err(error) => tracing::warn!(%error, "cache degli endpoint illeggibile"),
        }
    }

    Ok(resolved)
}

/// Scarica `endpoints.json`, lo valida e lo mette in cache.
///
/// Restituisce gli endpoint risolti e l'elenco dei campi scartati. Un errore
/// di rete non è fatale: si continua con i default più la cache.
pub async fn refresh(
    paths: &AppPaths,
    downloader: &vk_core::net::Downloader,
    current: &EndpointsInfo,
) -> AppResult<(EndpointsInfo, Vec<String>)> {
    let url = {
        let configured = current.resolved_endpoints_url();
        if configured.is_empty() {
            defaults().resolved_endpoints_url().to_string()
        } else {
            configured.to_string()
        }
    };

    if url.is_empty() {
        return Ok((current.clone(), Vec::new()));
    }

    let no_cache = vk_core::endpoints::add_no_cache_query(&url, vk_core::now_millis());
    let raw = downloader.get_string(&no_cache).await?;
    let remote = EndpointsInfo::parse(&raw)?;

    let mut resolved = defaults();
    let rejected = resolved.merge_remote(&remote);

    vk_core::fsx::write_atomic(&paths.endpoints_cache_file(), raw.as_bytes()).await?;

    if !rejected.is_empty() {
        tracing::warn!(?rejected, "endpoint remoti scartati perché non sicuri");
    }

    Ok((resolved, rejected))
}

/// URL di `versions.json` a partire dagli endpoint risolti.
pub fn versions_url(endpoints: &EndpointsInfo) -> AppResult<String> {
    let candidate = if endpoints.versions_json_url.trim().is_empty() {
        DEFAULT_VERSIONS_URL.to_string()
    } else {
        endpoints.versions_json_url.trim().to_string()
    };

    vk_core::endpoints::require_safe_endpoint(&candidate)
        .map_err(|_| AppError::Configuration("URL di versions.json non valido".into()))?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vk_core::Channel;

    #[test]
    fn the_compiled_defaults_are_valid_and_complete() {
        let endpoints = defaults();

        assert_eq!(
            endpoints.mod_url,
            "https://sitodaking.it:8443/Modpack/VanzaKart.zip"
        );
        assert_eq!(
            endpoints.beta_mod_url,
            "https://sitodaking.it:8443/VanzakartBeta/VKBeta.zip"
        );
        assert_eq!(
            endpoints.hash_files_url_for(Channel::Stable),
            "https://sitodaking.it:8443/Modpack/_by_sha256/"
        );
        assert!(!endpoints.leaderboard_api_url.is_empty());
        assert!(!endpoints.rooms_api_url.is_empty());
        assert!(!endpoints.beta_token_verify_api_url.is_empty());
        assert!(!endpoints.resolved_news_url().is_empty());
    }

    #[test]
    fn every_default_endpoint_is_https() {
        let endpoints = defaults();
        for (name, url) in [
            ("mod_url", &endpoints.mod_url),
            ("beta_mod_url", &endpoints.beta_mod_url),
            ("launcher_url", &endpoints.launcher_url),
            ("music_pack_url", &endpoints.music_pack_url),
            ("leaderboard", &endpoints.leaderboard_api_url),
            ("rooms", &endpoints.rooms_api_url),
        ] {
            assert!(
                vk_core::endpoints::is_safe_endpoint(url),
                "{name} non è un endpoint sicuro: {url}"
            );
        }
    }

    #[test]
    fn the_versions_url_falls_back_to_the_default() {
        assert_eq!(versions_url(&defaults()).unwrap(), DEFAULT_VERSIONS_URL);

        let mut endpoints = defaults();
        endpoints.versions_json_url = "https://mirror.example/versions.json".into();
        assert_eq!(
            versions_url(&endpoints).unwrap(),
            "https://mirror.example/versions.json"
        );

        endpoints.versions_json_url = "http://insicuro.example/versions.json".into();
        assert!(versions_url(&endpoints).is_err());
    }

    #[tokio::test]
    async fn a_cached_copy_is_merged_over_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        std::fs::write(
            paths.endpoints_cache_file(),
            r#"{"mod_url":"https://mirror.example/VanzaKart.zip"}"#,
        )
        .unwrap();

        let endpoints = load(&paths).await.unwrap();
        assert_eq!(endpoints.mod_url, "https://mirror.example/VanzaKart.zip");
        // I campi non presenti nella cache restano ai default.
        assert_eq!(
            endpoints.beta_mod_url,
            "https://sitodaking.it:8443/VanzakartBeta/VKBeta.zip"
        );
    }

    #[tokio::test]
    async fn an_unsafe_cached_endpoint_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        std::fs::write(
            paths.endpoints_cache_file(),
            r#"{"mod_url":"file:///C:/evil.zip"}"#,
        )
        .unwrap();

        assert_eq!(
            load(&paths).await.unwrap().mod_url,
            "https://sitodaking.it:8443/Modpack/VanzaKart.zip"
        );
    }

    #[tokio::test]
    async fn a_corrupt_cache_falls_back_to_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();
        std::fs::write(paths.endpoints_cache_file(), "{ non json").unwrap();

        assert_eq!(load(&paths).await.unwrap().mod_url, defaults().mod_url);
    }
}
