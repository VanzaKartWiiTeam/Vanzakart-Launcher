//! Installazione e disinstallazione complete, contro un server HTTP vero.
//!
//! È il test che conta: manifest letto dalla rete, pacchetto scaricato,
//! impronta verificata, cartella popolata, registro scritto e — al giro dopo —
//! tutto rimosso. Gli unit test coprono i pezzi; questo copre l'ordine in cui
//! si tengono, che è dove stanno gli errori veri (una cartella toccata prima
//! di aver verificato il download, un registro scritto prima dei file).

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use support::TestServer;
use tokio::sync::{Mutex, MutexGuard};
use vk_core::progress::{noop_sink, CancelToken};
use vk_install::install::{InstallMode, InstallOptions, Installer};
use vk_install::record::InstallRecord;
use vk_install::{paths, uninstall, UninstallOptions};

/// Radice dei dati dell'installer per questo binario di test: **mai** quella
/// vera dell'utente che esegue i test.
///
/// La radice è una sola perché la sposta una variabile d'ambiente, che è del
/// processo e non del thread: i test che la usano si mettono quindi in fila.
/// Senza, la disinstallazione di un test cancellava il registro condiviso
/// mentre un altro lo stava verificando.
async fn isolated_data_root() -> (&'static Path, MutexGuard<'static, ()>) {
    static ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
    // Un mutex asincrono: la fila la si aspetta senza bloccare il thread del
    // runtime, e il resto del test è tutto `await`.
    static LOCK: Mutex<()> = Mutex::const_new(());

    let guard = LOCK.lock().await;
    let root = ROOT.get_or_init(|| tempfile::tempdir().expect("radice isolata"));
    std::env::set_var(paths::DATA_ROOT_ENV, root.path());
    (root.path(), guard)
}

fn installer() -> Installer {
    Installer::new("test", None)
        .expect("installer")
        .with_downloader(
            vk_core::net::Downloader::new("vk-install-test")
                .expect("client")
                // Il server di prova parla http su loopback: fuori dai test
                // restano ammessi solo indirizzi https (§D-004).
                .with_loopback_http(true),
        )
}

/// Pacchetto ZIP con dentro l'eseguibile atteso dalla piattaforma corrente.
fn package() -> (Vec<u8>, String) {
    let executable = paths::launcher_executable_name().to_string();
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // Su macOS l'eseguibile è un bundle: nell'archivio è una cartella.
        let entry = if executable.ends_with(".app") {
            format!("{executable}/Contents/MacOS/launcher")
        } else {
            executable.clone()
        };
        writer.start_file(entry, options).expect("voce");
        writer.write_all(b"eseguibile finto").expect("contenuto");

        writer
            .start_file("resources/endpoints.default.json", options)
            .expect("voce");
        writer.write_all(b"{}").expect("contenuto");
        writer.finish().expect("chiuso");
    }
    (buffer.into_inner(), executable)
}

fn manifest_json(url: &str, sha256: &str, size: usize, executable: &str) -> Vec<u8> {
    format!(
        r#"{{
            "version": "2.0.0",
            "notes": "Test.",
            "pub_date": "2026-08-27T10:00:00Z",
            "platforms": {{
                "{key}": {{
                    "url": "{url}",
                    "sha256": "{sha256}",
                    "size": {size},
                    "format": "zip",
                    "executable": "{executable}"
                }}
            }}
        }}"#,
        key = vk_install::Target::current().key(),
    )
    .into_bytes()
}

fn options(install_dir: PathBuf) -> InstallOptions {
    InstallOptions {
        install_dir,
        mode: InstallMode::Fresh,
        // I test non copiano le impostazioni dell'utente né creano
        // scorciatoie: toccherebbero il computer di chi li esegue.
        backup_data: false,
        backup_dir: std::env::temp_dir().join("vk-install-test-backup"),
        desktop_shortcut: false,
        start_menu_shortcut: false,
        quick_launch_shortcut: false,
        uninstall_entry: false,
        path_symlink: false,
        copy_uninstaller: false,
        register_system: false,
    }
}

async fn serve(sha256: &str) -> (TestServer, String) {
    let (body, executable) = package();
    let size = body.len();
    let server = TestServer::start(vec![("/payload.zip", body)]).await;
    let manifest = manifest_json(&server.url("/payload.zip"), sha256, size, &executable);
    server.replace("/install.json", manifest);
    (server, executable)
}

fn digest_of_package() -> String {
    vk_core::hash::sha256_bytes(&package().0)
}

#[tokio::test]
async fn installs_the_package_and_writes_the_record() {
    let (data_root, _lock) = isolated_data_root().await;
    let (server, executable) = serve(&digest_of_package()).await;
    let temp = tempfile::tempdir().expect("temp");
    let install_dir = temp.path().join("programmi").join("VanzaKart Launcher");

    let installer = installer();
    let manifest = installer
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");

    let report = installer
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect("installazione");

    assert_eq!(report.version, "2.0.0");
    assert_eq!(report.executable, install_dir.join(&executable));
    assert!(report.executable.exists(), "eseguibile mancante");
    assert!(install_dir
        .join("resources/endpoints.default.json")
        .is_file());
    assert!(report.bytes > 0);
    assert!(report.backup.is_none());

    // Il registro esiste in tutte e due le copie: quella condivisa e quella
    // accanto al programma.
    let local = InstallRecord::load(&install_dir.join(paths::RECORD_FILE_NAME)).expect("registro");
    assert_eq!(local.version, "2.0.0");
    assert_eq!(local.install_dir, install_dir);
    assert!(local.owns_install_dir);
    assert!(local.payload.contains(
        &PathBuf::from(&executable)
            .components()
            .next()
            .map(|c| PathBuf::from(c.as_os_str()))
            .expect("prima voce")
    ));
    assert!(data_root.join("install.json").is_file());

    // Il registro si cita anche da sé, altrimenti la disinstallazione lo
    // lascerebbe indietro.
    assert!(local
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == vk_install::ArtifactKind::Record));
}

#[tokio::test]
async fn a_package_with_the_wrong_hash_never_touches_the_folder() {
    let (_data_root, _lock) = isolated_data_root().await;
    let (server, _) = serve(&"a".repeat(64)).await;
    let temp = tempfile::tempdir().expect("temp");
    let install_dir = temp.path().join("programmi").join("VanzaKart Launcher");

    let installer = installer();
    let manifest = installer
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");

    let error = installer
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect_err("impronta sbagliata");

    assert_eq!(error.code(), "hash-mismatch");
    // Nessun file estratto: la cartella non è mai stata creata.
    assert!(!install_dir.exists(), "la cartella non doveva nascere");
}

#[tokio::test]
async fn an_update_replaces_the_program_and_keeps_the_rest() {
    let (_data_root, _lock) = isolated_data_root().await;
    let (server, executable) = serve(&digest_of_package()).await;
    let temp = tempfile::tempdir().expect("temp");
    let install_dir = temp.path().join("programmi").join("VanzaKart Launcher");

    let installer = installer();
    let manifest = installer
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");

    installer
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect("prima installazione");

    // Qualcosa che l'utente ha messo lì dentro.
    let extra = install_dir.join("appunti.txt");
    std::fs::write(&extra, b"non cancellarmi").expect("scritto");

    let mut update = options(install_dir.clone());
    update.mode = InstallMode::Update;
    installer
        .install(&manifest, &update, &noop_sink(), &CancelToken::new())
        .await
        .expect("aggiornamento");

    assert!(install_dir.join(&executable).exists());
    assert!(
        extra.is_file(),
        "l'aggiornamento non deve svuotare la cartella"
    );

    // La reinstallazione pulita, invece, la svuota.
    let mut clean = options(install_dir.clone());
    clean.mode = InstallMode::CleanReinstall;
    installer
        .install(&manifest, &clean, &noop_sink(), &CancelToken::new())
        .await
        .expect("reinstallazione");

    assert!(install_dir.join(&executable).exists());
    assert!(
        !extra.exists(),
        "la reinstallazione pulita svuota la cartella"
    );
}

#[tokio::test]
async fn uninstalling_removes_exactly_what_was_installed() {
    let (_data_root, _lock) = isolated_data_root().await;
    let (server, _) = serve(&digest_of_package()).await;
    let temp = tempfile::tempdir().expect("temp");
    let install_dir = temp.path().join("programmi").join("VanzaKart Launcher");
    // Una cartella vicina che non c'entra: deve restare.
    let vicina = temp.path().join("programmi").join("altro-programma");
    std::fs::create_dir_all(&vicina).expect("mkdir");

    let installer = installer();
    let manifest = installer
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");
    installer
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect("installazione");

    let record = InstallRecord::load(&install_dir.join(paths::RECORD_FILE_NAME)).expect("registro");
    let report = uninstall::run(&record, &UninstallOptions::default(), &noop_sink())
        .expect("disinstallazione");

    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(!install_dir.exists(), "cartella non rimossa");
    assert!(vicina.is_dir(), "rimosso qualcosa che non era nostro");
    assert!(
        !_data_root.join("install.json").exists(),
        "il registro condiviso doveva sparire"
    );
}

#[tokio::test]
async fn a_manifest_that_is_not_there_is_reported_and_not_guessed() {
    let (_data_root, _lock) = isolated_data_root().await;
    let server = TestServer::start(vec![]).await;

    let error = installer()
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect_err("manifest assente");

    assert!(
        matches!(error.code(), "core" | "manifest"),
        "codice inatteso: {}",
        error.code()
    );
}

/// Il manifest prodotto da `scripts/Publish-SetupRelease.ps1`, salvato com'è.
///
/// Se lo script e questo parser divergono — un nome di campo, un formato
/// scritto in un altro modo — l'installer smette di trovare i pacchetti il
/// giorno del rilascio. Questa fixture è il punto in cui la divergenza si
/// vede subito.
#[test]
fn the_manifest_written_by_the_publish_script_is_readable() {
    use vk_install::release::{PackageFormat, ReleaseManifest};

    let raw = include_str!("../fixtures/install.json");
    let manifest = ReleaseManifest::parse(raw).expect("manifest della fixture");

    assert_eq!(manifest.version, "2.0.0");
    assert_eq!(manifest.platforms.len(), 3);

    for (key, expected) in [
        ("windows-x86_64", PackageFormat::Zip),
        ("darwin-universal", PackageFormat::TarGz),
        ("linux-x86_64", PackageFormat::AppImage),
    ] {
        let package = manifest.platforms.get(key).expect(key);
        assert_eq!(package.format().expect("formato"), expected, "{key}");
        assert!(vk_core::hash::is_valid_sha256(&package.sha256), "{key}");
        assert!(!package.executable.is_empty(), "{key}");
        assert!(package.url.starts_with("https://"), "{key}");
    }

    // Ogni piattaforma su cui gira questo test trova il proprio pacchetto.
    assert!(manifest.select(vk_install::Target::current()).is_ok());
}
