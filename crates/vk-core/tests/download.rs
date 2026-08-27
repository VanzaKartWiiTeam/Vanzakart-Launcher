//! Test di integrazione del downloader: resume, retry, mirror.

mod support;

use std::path::Path;

use support::{Behaviour, TestServer};
use vk_core::error::CoreError;
use vk_core::hash::sha256_bytes;
use vk_core::net::Downloader;
use vk_core::progress::{noop_sink, CancelToken, ProgressSink, ProgressUpdate};

fn downloader() -> Downloader {
    Downloader::new("vk-core-test")
        .expect("client")
        .with_loopback_http(true)
}

fn payload(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index % 251) as u8).collect()
}

async fn read(path: &Path) -> Vec<u8> {
    tokio::fs::read(path).await.expect("file scaricato")
}

#[tokio::test]
async fn downloads_a_whole_file() {
    let body = payload(300_000);
    let server = TestServer::start(vec![("/a.bin", Behaviour::Serve(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");

    let outcome = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
    assert_eq!(outcome.bytes_received, body.len() as u64);
    assert_eq!(outcome.retry_count(), 0);
}

#[tokio::test]
async fn resumes_from_a_partial_file() {
    let body = payload(200_000);
    let server = TestServer::start(vec![("/a.bin", Behaviour::Serve(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");

    // Simula un download interrotto a metà.
    tokio::fs::write(&destination, &body[..80_000])
        .await
        .unwrap();

    let outcome = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
    // Sono stati trasferiti solo i byte mancanti.
    assert_eq!(outcome.bytes_received, (body.len() - 80_000) as u64);
    assert_eq!(outcome.attempts[0].status, Some(206));
}

#[tokio::test]
async fn treats_a_satisfied_range_as_complete() {
    let body = payload(1000);
    let server = TestServer::start(vec![("/a.bin", Behaviour::Serve(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");
    tokio::fs::write(&destination, &body).await.unwrap();

    let outcome = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(outcome.attempts[0].status, Some(416));
    assert_eq!(outcome.bytes_received, 0);
    assert_eq!(read(&destination).await, body);
}

#[tokio::test]
async fn restarts_when_the_server_ignores_range() {
    let body = payload(50_000);
    let server = TestServer::start(vec![("/a.bin", Behaviour::IgnoreRange(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");
    // Un frammento con contenuto sbagliato: deve essere scartato, non appeso.
    tokio::fs::write(&destination, vec![0xFFu8; 1000])
        .await
        .unwrap();

    downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
}

#[tokio::test]
async fn retries_a_transient_server_error() {
    let body = payload(10_000);
    let server = TestServer::start(vec![(
        "/a.bin",
        Behaviour::FailThenServe {
            status: 503,
            times: 2,
            body: body.clone(),
        },
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");

    let outcome = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
    assert_eq!(outcome.attempts.len(), 3);
    assert_eq!(outcome.retry_count(), 2);
    assert!(!outcome.attempts[0].success);
    assert!(outcome.attempts[2].success);
}

#[tokio::test]
async fn does_not_retry_a_permanent_error() {
    let server = TestServer::start(vec![("/a.bin", Behaviour::Always(404))]).await;
    let dir = tempfile::tempdir().unwrap();

    let error = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &dir.path().join("a.bin"),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::AllMirrorsFailed(_)));
    // Un solo tentativo: il 404 non è ritentabile.
    assert_eq!(server.hits("/a.bin"), 1);
}

#[tokio::test]
async fn resumes_after_a_truncated_response() {
    let body = payload(120_000);
    let server = TestServer::start(vec![(
        "/a.bin",
        Behaviour::TruncateThenServe {
            prefix: 40_000,
            body: body.clone(),
        },
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");

    let outcome = downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
    assert_eq!(sha256_bytes(&read(&destination).await), sha256_bytes(&body));
    // Il secondo tentativo ha ripreso da dove si era interrotto.
    assert!(outcome.attempts.len() >= 2);
    assert!(outcome.attempts.last().unwrap().existing_bytes >= 40_000);
}

#[tokio::test]
async fn falls_back_to_the_next_mirror() {
    let body = payload(20_000);
    let broken = TestServer::start(vec![("/a.bin", Behaviour::Always(404))]).await;
    let working = TestServer::start(vec![("/a.bin", Behaviour::Serve(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("a.bin");

    let outcome = downloader()
        .download_with_mirrors(
            &[broken.url("/a.bin"), working.url("/a.bin")],
            &destination,
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(read(&destination).await, body);
    assert_eq!(outcome.source_url, working.url("/a.bin"));
    assert_eq!(broken.hits("/a.bin"), 1);
}

#[tokio::test]
async fn reports_every_mirror_failure() {
    let a = TestServer::start(vec![("/a.bin", Behaviour::Always(404))]).await;
    let b = TestServer::start(vec![("/a.bin", Behaviour::Always(404))]).await;
    let dir = tempfile::tempdir().unwrap();

    let error = downloader()
        .download_with_mirrors(
            &[a.url("/a.bin"), b.url("/a.bin")],
            &dir.path().join("a.bin"),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap_err();

    let message = error.to_string();
    assert!(message.contains(&a.base()), "{message}");
    assert!(message.contains(&b.base()), "{message}");
}

#[tokio::test]
async fn progress_is_monotonic_and_reaches_the_total() {
    let body = payload(700_000);
    let server = TestServer::start(vec![("/a.bin", Behaviour::Serve(body.clone()))]).await;
    let dir = tempfile::tempdir().unwrap();

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(u64, u64)>::new()));
    let recorder = std::sync::Arc::clone(&seen);
    let sink: ProgressSink = std::sync::Arc::new(move |update: ProgressUpdate| {
        recorder
            .lock()
            .unwrap()
            .push((update.bytes_done, update.bytes_total));
    });

    downloader()
        .download_with_resume(
            &server.url("/a.bin"),
            &dir.path().join("a.bin"),
            &sink,
            &CancelToken::new(),
        )
        .await
        .unwrap();

    let updates = seen.lock().unwrap().clone();
    assert!(updates.len() >= 2, "aggiornamenti: {}", updates.len());
    assert!(updates.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(updates.last().unwrap().0, body.len() as u64);
}

#[tokio::test]
async fn rejects_plain_http_when_loopback_is_not_allowed() {
    let server = TestServer::start(vec![("/a.bin", Behaviour::Serve(payload(10)))]).await;
    let dir = tempfile::tempdir().unwrap();

    let error = Downloader::new("vk-core-test")
        .unwrap()
        .download_with_resume(
            &server.url("/a.bin"),
            &dir.path().join("a.bin"),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::InvalidUrl(_)));
    assert_eq!(server.total_requests(), 0);
}

#[tokio::test]
async fn fetches_json_strings() {
    let server = TestServer::start(vec![(
        "/versions.json",
        Behaviour::Serve(br#"{"mod_version":"1.0.0"}"#.to_vec()),
    )])
    .await;

    let raw = downloader()
        .get_string(&server.url("/versions.json"))
        .await
        .unwrap();
    assert_eq!(
        vk_core::VersionInfo::parse(&raw).unwrap().mod_version,
        "1.0.0"
    );
}
