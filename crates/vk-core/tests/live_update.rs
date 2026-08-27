//! Verifica del motore di aggiornamento contro il **server di produzione**.
//!
//! Gli altri test girano contro un server locale: dimostrano che il codice è
//! coerente con sé stesso. Questi dimostrano che è coerente con ciò che il
//! server pubblica davvero — le due cose divergono, e quando divergono il
//! launcher non aggiorna più niente.
//!
//! Sono `#[ignore]` perché richiedono la rete: si eseguono a mano prima di un
//! rilascio, o quando si tocca `endpoints.json` sul server.
//!
//! ```bash
//! cargo test -p vk-core --test live_update -- --ignored --nocapture
//! ```

use vk_core::endpoints::{EndpointsInfo, MirrorPlan};
use vk_core::hash::sha256_file;
use vk_core::manifest::ModManifest;
use vk_core::net::Downloader;
use vk_core::progress::{noop_sink, CancelToken};
use vk_core::versions::{Channel, VersionInfo};

const VERSIONS_URL: &str = "https://sitodaking.it:8443/Launcher/versions.json";
const ENDPOINTS_URL: &str = "https://sitodaking.it:8443/Launcher/endpoints.json";

fn downloader() -> Downloader {
    Downloader::new("vk-core-live-test").expect("client")
}

async fn fetch_text(url: &str) -> String {
    let bytes = downloader()
        .get_bytes(url)
        .await
        .unwrap_or_else(|error| panic!("{url} non raggiungibile: {error}"));
    String::from_utf8(bytes).expect("risposta non UTF-8")
}

/// `versions.json` reale si legge con il parser reale.
///
/// Il file mescola stringhe e array nei changelog: `changelog` è una stringa,
/// `launcher_changelog` un array. Un parser che ne accetta uno solo fallisce
/// sull'intero file, e il launcher smette di vedere qualunque aggiornamento.
#[tokio::test]
#[ignore = "richiede la rete"]
async fn the_live_versions_file_parses() {
    let info = VersionInfo::parse(&fetch_text(VERSIONS_URL).await).expect("versions.json");

    assert!(!info.mod_version.trim().is_empty(), "manca mod_version");
    assert_eq!(info.mod_sha256.len(), 64, "hash della modpack non valido");
    assert!(!info.beta_mod_version.trim().is_empty());
    assert_eq!(info.beta_mod_sha256.len(), 64);
    assert!(!info.launcher_version.trim().is_empty());

    println!(
        "stable={} beta={} musicpack={} launcher={}",
        info.mod_version, info.beta_mod_version, info.music_pack_version, info.launcher_version
    );
}

/// `endpoints.json` reale si legge, e ogni URL supera l'allowlist.
#[tokio::test]
#[ignore = "richiede la rete"]
async fn the_live_endpoints_file_parses_and_is_safe() {
    let endpoints = EndpointsInfo::parse(&fetch_text(ENDPOINTS_URL).await).expect("endpoints.json");

    for (name, url) in [
        ("mod_url", &endpoints.mod_url),
        ("mod_manifest_url", &endpoints.mod_manifest_url),
        ("mod_files_url", &endpoints.mod_files_url),
        ("mod_hash_files_url", &endpoints.mod_hash_files_url),
        ("beta_mod_url", &endpoints.beta_mod_url),
        ("beta_mod_manifest_url", &endpoints.beta_mod_manifest_url),
        ("beta_mod_files_url", &endpoints.beta_mod_files_url),
        ("music_pack_url", &endpoints.music_pack_url),
        ("launcher_url", &endpoints.launcher_url),
    ] {
        assert!(!url.trim().is_empty(), "{name} è vuoto");
        assert!(
            vk_core::endpoints::is_safe_endpoint(url),
            "{name} non supera l'allowlist: {url}"
        );
    }
}

/// Il manifest reale si legge, e i suoi file sono scaricabili e verificabili.
///
/// Scarica **due** file veri: uno dal percorso diretto in `files/`, lo stesso
/// dalla cartella `_by_sha256/`. Sono le due sorgenti fra cui il differenziale
/// sceglie, e se la seconda non esiste il fallback non serve a niente.
#[tokio::test]
#[ignore = "richiede la rete"]
async fn a_real_manifest_file_downloads_and_verifies() {
    let endpoints = EndpointsInfo::parse(&fetch_text(ENDPOINTS_URL).await).expect("endpoints.json");
    let manifest = ModManifest::parse(&fetch_text(&endpoints.mod_manifest_url).await)
        .expect("manifest_files.json");

    assert!(
        !manifest.files.is_empty(),
        "il manifest non elenca nessun file"
    );
    println!(
        "manifest v{}: {} file",
        manifest.mod_version,
        manifest.files.len()
    );

    // Il file più piccolo: basta a dimostrare la catena e non spreca banda.
    let file = manifest
        .files
        .iter()
        .filter(|file| file.size > 0)
        .min_by_key(|file| file.size)
        .expect("almeno un file con dimensione nota");
    println!("prova su {} ({} byte)", file.path, file.size);

    let plan = MirrorPlan::for_channel(&endpoints, &endpoints, Channel::Stable);
    let candidates = plan.file_candidates(&file.path, &file.sha256, vk_core::now_millis());
    assert!(!candidates.is_empty(), "nessuna sorgente per {}", file.path);

    let directory = tempfile::tempdir().unwrap();
    let downloader = downloader();
    let cancel = CancelToken::new();

    // 1. Percorso diretto: la prima sorgente della lista.
    let direct = directory.path().join("diretto.bin");
    downloader
        .download_with_mirrors(&candidates[..1], &direct, &noop_sink(), &cancel)
        .await
        .unwrap_or_else(|error| panic!("download da {}: {error}", candidates[0]));
    assert_eq!(
        sha256_file(&direct).await.unwrap().to_lowercase(),
        file.sha256.to_lowercase(),
        "hash diverso da quello dichiarato dal manifest"
    );

    // 2. Fallback hash-addressed, da solo.
    let hash_url = format!(
        "{}{}",
        endpoints.mod_hash_files_url,
        file.sha256.to_lowercase()
    );
    let by_hash = directory.path().join("per-hash.bin");
    downloader
        .download_with_mirrors(
            std::slice::from_ref(&hash_url),
            &by_hash,
            &noop_sink(),
            &cancel,
        )
        .await
        .unwrap_or_else(|error| panic!("download da {hash_url}: {error}"));
    assert_eq!(
        sha256_file(&by_hash).await.unwrap().to_lowercase(),
        file.sha256.to_lowercase(),
        "il file in _by_sha256/ non corrisponde al suo nome"
    );
}

/// L'archivio completo esiste ed è servito con supporto al resume.
///
/// Il fallback dell'aggiornamento differenziale è scaricare l'intero pacchetto:
/// se quell'URL non risponde, un differenziale fallito non ha via d'uscita.
#[tokio::test]
#[ignore = "richiede la rete"]
async fn the_full_archive_is_reachable_and_resumable() {
    let endpoints = EndpointsInfo::parse(&fetch_text(ENDPOINTS_URL).await).expect("endpoints.json");

    let downloader = downloader();

    for (name, url) in [
        ("modpack", &endpoints.mod_url),
        ("beta", &endpoints.beta_mod_url),
        ("music pack", &endpoints.music_pack_url),
    ] {
        let response = downloader
            .client()
            .get(url)
            .header("Range", "bytes=0-1023")
            .send()
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        assert_eq!(
            response.status().as_u16(),
            206,
            "{name} risponde {} invece di 206: senza richieste Range il resume non funziona",
            response.status()
        );
    }
}
