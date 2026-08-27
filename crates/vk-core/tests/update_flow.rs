//! Test end-to-end del motore di aggiornamento: differenziale, fallback
//! hash-addressed, archivio completo, protezione dei dati utente e rollback.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};

use support::{Behaviour, TestServer};
use vk_core::backup::{create_backup, restore_backup};
use vk_core::endpoints::MirrorPlan;
use vk_core::error::CoreError;
use vk_core::hash::sha256_bytes;
use vk_core::manifest::{ModManifest, ModManifestFile};
use vk_core::net::Downloader;
use vk_core::progress::{noop_sink, CancelToken};
use vk_core::protect::ModLayout;
use vk_core::update::{apply_differential, apply_full_archive, UpdateContext, UpdateMode};
use vk_core::versions::Channel;

const NOW: u128 = 1_700_000_000_000;

fn downloader() -> Downloader {
    Downloader::new("vk-core-test")
        .expect("client")
        .with_loopback_http(true)
}

/// File di una release fittizia, con lo stesso layout della modpack reale.
fn release_files() -> Vec<(&'static str, &'static [u8])> {
    vec![
        (
            "Riivolution/VanzaKart.xml",
            b"<wiidisc version=\"1\"/>" as &[u8],
        ),
        ("Riivolution/config/RMCP.xml", b"<config/>"),
        ("VanzaKart/Race/Course/beginner.szs", b"corsa-v2"),
        ("VanzaKart/CTBRSTM/track01.brstm", b"musica-v2"),
    ]
}

fn manifest_for(files: &[(&str, &[u8])]) -> ModManifest {
    ModManifest {
        mod_version: "1.5.0".into(),
        archive_sha256: String::new(),
        files: files
            .iter()
            .map(|(path, body)| ModManifestFile {
                path: (*path).to_string(),
                sha256: sha256_bytes(body),
                size: body.len() as i64,
            })
            .collect(),
    }
}

/// Installa localmente una versione precedente, più dati utente da preservare.
fn seed_installation(root: &Path) -> ModLayout {
    let layout = ModLayout::new(root, Channel::Stable);
    let mod_root = layout.mod_root();

    write(
        &mod_root.join("Riivolution/VanzaKart.xml"),
        b"<wiidisc version=\"1\"/>",
    );
    write(&mod_root.join("Riivolution/config/RMCP.xml"), b"<config/>");
    write(
        &mod_root.join("VanzaKart/Race/Course/beginner.szs"),
        b"corsa-v1",
    );
    write(
        &mod_root.join("VanzaKart/CTBRSTM/track01.brstm"),
        b"musica-v2",
    );
    write(
        &mod_root.join("VanzaKart/Race/Course/obsoleta.szs"),
        b"vecchia-pista",
    );

    // Dati utente che nessun aggiornamento può toccare.
    write(&layout.my_stuff().join("custom.szs"), b"mod-personale");
    write(&mod_root.join("Saves/rksys.dat"), b"licenza-utente");

    layout
}

fn write(path: &Path, body: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Server che espone `files/` e `_by_sha256/` come il server reale.
async fn serve_release(files: &[(&str, &[u8])], expose_files_dir: bool) -> TestServer {
    let mut routes: Vec<(String, Behaviour)> = Vec::new();
    for (path, body) in files {
        if expose_files_dir {
            routes.push((
                format!("/Modpack/files/{path}"),
                Behaviour::Serve(body.to_vec()),
            ));
        }
        routes.push((
            format!("/Modpack/_by_sha256/{}", sha256_bytes(body)),
            Behaviour::Serve(body.to_vec()),
        ));
    }

    let borrowed: Vec<(&str, Behaviour)> = routes
        .iter()
        .map(|(path, behaviour)| (path.as_str(), behaviour.clone()))
        .collect();
    TestServer::start(borrowed).await
}

fn mirror_plan(server: &TestServer) -> MirrorPlan {
    MirrorPlan {
        files_url: format!("{}/Modpack/files/", server.base()),
        hash_files_url: format!("{}/Modpack/_by_sha256/", server.base()),
        ..Default::default()
    }
}

fn context(layout: ModLayout, mirrors: MirrorPlan, dir: &Path) -> UpdateContext {
    let mut context = UpdateContext::new(layout, mirrors, dir.join("Backups/ModUpdates"));
    context.staging_root = dir.join("staging");
    context.concurrency = 4;
    context.is_update = true;
    context
}

#[tokio::test]
async fn differential_update_downloads_only_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let files = release_files();
    let server = serve_release(&files, true).await;
    let manifest = manifest_for(&files);

    let context = context(layout.clone(), mirror_plan(&server), dir.path());
    let report = apply_differential(
        &downloader(),
        &context,
        &manifest,
        NOW,
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(report.mode, Some(UpdateMode::Differential));
    // Solo beginner.szs è cambiato.
    assert_eq!(report.files_written, 1);
    // obsoleta.szs non è più nel manifest.
    assert_eq!(report.files_pruned, 1);
    assert!(!report.has_errors(), "{:?}", report.errors);

    let mod_root = layout.mod_root();
    assert_eq!(
        std::fs::read(mod_root.join("VanzaKart/Race/Course/beginner.szs")).unwrap(),
        b"corsa-v2"
    );
    assert!(!mod_root.join("VanzaKart/Race/Course/obsoleta.szs").exists());
    // I file identici non sono stati riscaricati.
    assert_eq!(server.hits("/Modpack/files/Riivolution/VanzaKart.xml"), 0);
}

#[tokio::test]
async fn differential_update_never_touches_user_data() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let files = release_files();
    let server = serve_release(&files, true).await;

    let context = context(layout.clone(), mirror_plan(&server), dir.path());
    apply_differential(
        &downloader(),
        &context,
        &manifest_for(&files),
        NOW,
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read(layout.my_stuff().join("custom.szs")).unwrap(),
        b"mod-personale"
    );
    assert_eq!(
        std::fs::read(layout.mod_root().join("Saves/rksys.dat")).unwrap(),
        b"licenza-utente"
    );
}

#[tokio::test]
async fn differential_update_falls_back_to_the_hash_directory() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let files = release_files();
    // `files/` non è pubblicata: resta solo `_by_sha256/`.
    let server = serve_release(&files, false).await;

    let context = context(layout.clone(), mirror_plan(&server), dir.path());
    let report = apply_differential(
        &downloader(),
        &context,
        &manifest_for(&files),
        NOW,
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(report.files_written, 1);
    assert_eq!(
        std::fs::read(layout.mod_root().join("VanzaKart/Race/Course/beginner.szs")).unwrap(),
        b"corsa-v2"
    );
    assert_eq!(
        server.hits(&format!(
            "/Modpack/_by_sha256/{}",
            sha256_bytes(b"corsa-v2")
        )),
        1
    );
}

#[tokio::test]
async fn differential_update_rejects_a_corrupted_payload() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());

    // Il manifest dichiara un hash che il server non rispetta.
    let manifest = ModManifest {
        mod_version: "1.5.0".into(),
        archive_sha256: String::new(),
        files: vec![ModManifestFile {
            path: "VanzaKart/Race/Course/beginner.szs".into(),
            sha256: sha256_bytes(b"contenuto-atteso"),
            size: 16,
        }],
    };

    let server = TestServer::start(vec![(
        "/Modpack/files/VanzaKart/Race/Course/beginner.szs",
        Behaviour::Serve(b"contenuto-manomesso".to_vec()),
    )])
    .await;

    let context = context(layout.clone(), mirror_plan(&server), dir.path());
    let error = apply_differential(
        &downloader(),
        &context,
        &manifest,
        NOW,
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, CoreError::HashMismatch { .. }));
    // L'installazione è rimasta alla versione precedente.
    assert_eq!(
        std::fs::read(layout.mod_root().join("VanzaKart/Race/Course/beginner.szs")).unwrap(),
        b"corsa-v1"
    );
    // E i file obsoleti non sono stati rimossi.
    assert!(layout
        .mod_root()
        .join("VanzaKart/Race/Course/obsoleta.szs")
        .is_file());
}

#[tokio::test]
async fn differential_update_rejects_a_traversal_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());

    // `validated()` intercetta il path prima ancora del download.
    let malicious = ModManifest {
        mod_version: "1.5.0".into(),
        archive_sha256: String::new(),
        files: vec![ModManifestFile {
            path: "../../evil.dll".into(),
            sha256: sha256_bytes(b"x"),
            size: 1,
        }],
    };
    assert!(malicious.clone().validated().is_err());

    // Anche saltando la validazione, il motore rifiuta il percorso.
    let server = TestServer::start(vec![]).await;
    let context = context(layout, mirror_plan(&server), dir.path());
    let error = apply_differential(
        &downloader(),
        &context,
        &malicious,
        NOW,
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, CoreError::UnsafePath(_)), "{error:?}");
    assert!(!dir.path().join("evil.dll").exists());
}

#[tokio::test]
async fn full_archive_update_replaces_assets_and_keeps_user_data() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let files = release_files();

    let archive_bytes = build_archive(&files);
    let server = TestServer::start(vec![(
        "/Modpack/VanzaKart.zip",
        Behaviour::Serve(archive_bytes.clone()),
    )])
    .await;

    let mut context = context(
        layout.clone(),
        MirrorPlan {
            archive_url: format!("{}/Modpack/VanzaKart.zip", server.base()),
            ..Default::default()
        },
        dir.path(),
    );
    context.expected_archive_sha256 = sha256_bytes(&archive_bytes);

    let report = apply_full_archive(
        &downloader(),
        &context,
        &dir.path().join("download/VanzaKart.zip"),
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(report.mode, Some(UpdateMode::FullArchive));
    assert!(!report.has_errors(), "{:?}", report.errors);

    let mod_root = layout.mod_root();
    assert_eq!(
        std::fs::read(mod_root.join("VanzaKart/Race/Course/beginner.szs")).unwrap(),
        b"corsa-v2"
    );
    assert!(!mod_root.join("VanzaKart/Race/Course/obsoleta.szs").exists());
    assert_eq!(
        std::fs::read(layout.my_stuff().join("custom.szs")).unwrap(),
        b"mod-personale"
    );
    assert_eq!(
        std::fs::read(mod_root.join("Saves/rksys.dat")).unwrap(),
        b"licenza-utente"
    );
    // `My Stuff` esiste sempre dopo un'installazione.
    assert!(layout.my_stuff().is_dir());
}

#[tokio::test]
async fn full_archive_update_refuses_a_hash_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let archive_bytes = build_archive(&release_files());

    let server = TestServer::start(vec![(
        "/Modpack/VanzaKart.zip",
        Behaviour::Serve(archive_bytes),
    )])
    .await;

    let mut context = context(
        layout.clone(),
        MirrorPlan {
            archive_url: format!("{}/Modpack/VanzaKart.zip", server.base()),
            ..Default::default()
        },
        dir.path(),
    );
    context.expected_archive_sha256 = "f".repeat(64);

    let error = apply_full_archive(
        &downloader(),
        &context,
        &dir.path().join("download/VanzaKart.zip"),
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, CoreError::HashMismatch { .. }));
    // L'archivio non è stato estratto.
    assert_eq!(
        std::fs::read(layout.mod_root().join("VanzaKart/Race/Course/beginner.szs")).unwrap(),
        b"corsa-v1"
    );
}

#[tokio::test]
async fn backup_and_rollback_survive_a_destructive_update() {
    let dir = tempfile::tempdir().unwrap();
    let layout = seed_installation(dir.path());
    let backup_root = dir.path().join("Backups/ModUpdates");

    let backup = create_backup(&layout, &backup_root, &noop_sink(), &CancelToken::new())
        .await
        .unwrap();
    assert_eq!(backup.files.len(), 2);

    // Un aggiornamento andato male distrugge i dati utente.
    std::fs::remove_dir_all(layout.my_stuff()).unwrap();
    std::fs::remove_file(layout.mod_root().join("Saves/rksys.dat")).unwrap();

    let restored = restore_backup(&backup, &noop_sink(), &CancelToken::new())
        .await
        .unwrap();

    assert_eq!(restored, 2);
    assert_eq!(
        std::fs::read(layout.my_stuff().join("custom.szs")).unwrap(),
        b"mod-personale"
    );
    assert_eq!(
        std::fs::read(layout.mod_root().join("Saves/rksys.dat")).unwrap(),
        b"licenza-utente"
    );
}

#[tokio::test]
async fn a_fresh_install_from_the_full_archive_creates_the_expected_layout() {
    let dir = tempfile::tempdir().unwrap();
    let layout = ModLayout::new(dir.path().join("Riivolution"), Channel::Beta);
    let files: Vec<(&str, &[u8])> = vec![
        ("Riivolution/VKBeta.xml", b"<wiidisc version=\"1\"/>"),
        ("VKBeta/Race/Course/beginner.szs", b"beta"),
    ];

    let archive_bytes = build_archive_with_root("VKBeta", &files);
    let server = TestServer::start(vec![(
        "/VanzakartBeta/VKBeta.zip",
        Behaviour::Serve(archive_bytes.clone()),
    )])
    .await;

    let mut context = context(
        layout.clone(),
        MirrorPlan {
            archive_url: format!("{}/VanzakartBeta/VKBeta.zip", server.base()),
            ..Default::default()
        },
        dir.path(),
    );
    context.is_update = false;
    context.expected_archive_sha256 = sha256_bytes(&archive_bytes);

    apply_full_archive(
        &downloader(),
        &context,
        &dir.path().join("download/VKBeta.zip"),
        &noop_sink(),
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert!(layout.is_installed(), "manca Riivolution/VKBeta.xml");
    assert_eq!(
        layout.riivolution_xml(),
        dir.path().join("Riivolution/VKBeta/Riivolution/VKBeta.xml")
    );
    assert!(layout.my_stuff().is_dir());
}

/// Costruisce un archivio con la cartella `VanzaKart/` come radice, come le
/// release reali.
fn build_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    build_archive_with_root("VanzaKart", files)
}

fn build_archive_with_root(root: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default();
        writer.add_directory(format!("{root}/"), options).unwrap();
        for (path, body) in files {
            writer
                .start_file(format!("{root}/{path}"), options)
                .unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }
    buffer.into_inner()
}

/// Percorso dello staging usato dai test, per verificarne la pulizia.
#[allow(dead_code)]
fn staging(dir: &Path) -> PathBuf {
    dir.join("staging")
}
