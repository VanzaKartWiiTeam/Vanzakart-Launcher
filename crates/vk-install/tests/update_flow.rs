//! Aggiornamento in loco, contro un server HTTP vero.
//!
//! Il test risponde alla domanda che ha motivato tutto il modulo: dopo un
//! aggiornamento, dove si trova il launcher? La risposta deve essere «nella
//! stessa cartella di prima», su qualunque sistema operativo — e nella
//! cartella non deve comparire niente che l'installer non ci avesse messo:
//! niente seconda copia, niente secondo disinstallatore, niente scorciatoie
//! nuove (§D-084).

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use support::TestServer;
use tokio::sync::{Mutex, MutexGuard};
use vk_core::progress::{noop_sink, CancelToken};
use vk_install::install::{InstallMode, InstallOptions, Installer};
use vk_install::paths;
use vk_install::record::InstallRecord;
use vk_install::update::{self, UpdateTarget};

/// Come in `install_flow`: i test non scrivono mai nella cartella dati vera.
async fn isolated_data_root() -> (&'static Path, MutexGuard<'static, ()>) {
    static ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::const_new(());

    let guard = LOCK.lock().await;
    let root = ROOT.get_or_init(|| tempfile::tempdir().expect("radice isolata"));
    std::env::set_var(paths::DATA_ROOT_ENV, root.path());
    (root.path(), guard)
}

fn installer(version: &str) -> Installer {
    Installer::new(version, None)
        .expect("installer")
        .with_downloader(
            vk_core::net::Downloader::new("vk-install-test")
                .expect("client")
                .with_loopback_http(true),
        )
}

/// Pacchetto ZIP con dentro l'eseguibile atteso dalla piattaforma corrente e
/// un contenuto riconoscibile, per distinguere una versione dall'altra.
fn package(body: &str) -> (Vec<u8>, String) {
    let executable = paths::launcher_executable_name().to_string();
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        let entry = if executable.ends_with(".app") {
            format!("{executable}/Contents/MacOS/launcher")
        } else {
            executable.clone()
        };
        writer.start_file(entry, options).expect("voce");
        writer.write_all(body.as_bytes()).expect("contenuto");

        writer
            .start_file("resources/endpoints.default.json", options)
            .expect("voce");
        writer.write_all(body.as_bytes()).expect("contenuto");
        writer.finish().expect("chiuso");
    }
    (buffer.into_inner(), executable)
}

fn manifest_json(version: &str, url: &str, sha256: &str, size: usize, executable: &str) -> Vec<u8> {
    format!(
        r#"{{
            "version": "{version}",
            "notes": "Novità della {version}.",
            "pub_date": "2026-09-01T10:00:00Z",
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
        backup_data: false,
        backup_dir: std::env::temp_dir().join("vk-update-test-backup"),
        desktop_shortcut: false,
        start_menu_shortcut: false,
        quick_launch_shortcut: false,
        uninstall_entry: false,
        path_symlink: false,
        copy_uninstaller: false,
        // I test non toccano il registro di Windows del computer su cui
        // girano: `refresh_registration` lo rispetta.
        register_system: false,
    }
}

/// Pubblica una versione sul server: pacchetto, impronta e manifest.
fn publish(server: &TestServer, version: &str, body: &str) -> String {
    let (payload, executable) = package(body);
    let digest = vk_core::hash::sha256_bytes(&payload);
    let path = format!("/{version}.zip");
    let size = payload.len();
    server.replace(&path, payload);
    server.replace(
        "/install.json",
        manifest_json(version, &server.url(&path), &digest, size, &executable),
    );
    executable
}

/// Legge il contenuto dell'eseguibile installato, bundle di macOS compreso.
fn installed_body(install_dir: &Path, executable: &str) -> String {
    let path = if executable.ends_with(".app") {
        install_dir
            .join(executable)
            .join("Contents")
            .join("MacOS")
            .join("launcher")
    } else {
        install_dir.join(executable)
    };
    std::fs::read_to_string(path).expect("eseguibile installato")
}

#[tokio::test]
async fn the_update_lands_in_the_folder_the_installer_chose() {
    let (_data_root, _lock) = isolated_data_root().await;
    let server = TestServer::start(vec![]).await;
    let temp = tempfile::tempdir().expect("temp");
    // Una cartella scelta dall'utente, diversa da qualunque percorso
    // predefinito: è esattamente il caso che l'updater NSIS sbagliava.
    let install_dir = temp.path().join("Giochi").join("VanzaKart");

    let executable = publish(&server, "2.0.0", "versione 2.0.0");
    let engine = installer("2.0.0");
    let manifest = engine
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");
    engine
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect("installazione");

    // Qualcosa che l'utente ha messo lì dentro, e che deve sopravvivere.
    let extra = install_dir.join("appunti.txt");
    std::fs::write(&extra, b"non cancellarmi").expect("scritto");
    let before: Vec<PathBuf> = listing(&install_dir);

    // Esce la 2.1.0.
    publish(&server, "2.1.0", "versione 2.1.0");
    let engine = installer("2.0.0");
    let manifest = engine
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");

    let target = UpdateTarget::for_bundle(&install_dir.join(&executable)).expect("bersaglio");
    assert!(target.managed(), "il registro dell'installazione c'è");
    assert_eq!(target.install_dir, install_dir);

    let plan = update::plan(&manifest, &target, "2.0.0").expect("piano");
    assert!(plan.available, "la 2.1.0 è più recente della 2.0.0");
    assert!(plan.can_install());
    assert_eq!(plan.install_dir, install_dir, "si aggiorna dove si è");

    let report = engine
        .update_in_place(&manifest, &target, &noop_sink(), &CancelToken::new())
        .await
        .expect("aggiornamento");

    assert_eq!(report.version, "2.1.0");
    assert_eq!(report.install_dir, install_dir);
    assert_eq!(
        installed_body(&install_dir, &executable),
        "versione 2.1.0",
        "il programma è stato sostituito"
    );
    assert!(extra.is_file(), "i file dell'utente restano");

    // Il registro sa di essere alla 2.1.0, e sta ancora dove stava.
    let record = InstallRecord::load(&install_dir.join(paths::RECORD_FILE_NAME)).expect("registro");
    assert_eq!(record.version, "2.1.0");
    assert_eq!(record.install_dir, install_dir);
    assert!(report.record_updated);

    // Niente di nuovo nella cartella: nessuna copia di lato sopravvissuta,
    // nessuna cartella di appoggio, nessun secondo disinstallatore.
    update::sweep_leftovers(&install_dir);
    assert_eq!(
        listing(&install_dir),
        before,
        "l'aggiornamento non aggiunge né toglie voci"
    );

    // E soprattutto: niente seconda installazione altrove.
    assert!(
        !paths::default_install_dir().join("appunti.txt").exists(),
        "l'aggiornamento non deve installare nella cartella predefinita"
    );
}

/// Un pacchetto manomesso non arriva mai alla cartella d'installazione.
#[tokio::test]
async fn a_package_that_fails_verification_leaves_the_installation_alone() {
    let (_data_root, _lock) = isolated_data_root().await;
    let server = TestServer::start(vec![]).await;
    let temp = tempfile::tempdir().expect("temp");
    let install_dir = temp.path().join("Giochi").join("VanzaKart");

    let executable = publish(&server, "2.0.0", "versione 2.0.0");
    let engine = installer("2.0.0");
    let manifest = engine
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");
    engine
        .install(
            &manifest,
            &options(install_dir.clone()),
            &noop_sink(),
            &CancelToken::new(),
        )
        .await
        .expect("installazione");

    // La 2.1.0 viene pubblicata con l'impronta di qualcos'altro.
    let (payload, _) = package("versione 2.1.0");
    let size = payload.len();
    server.replace("/2.1.0.zip", payload);
    server.replace(
        "/install.json",
        manifest_json(
            "2.1.0",
            &server.url("/2.1.0.zip"),
            &"a".repeat(64),
            size,
            &executable,
        ),
    );

    let engine = installer("2.0.0");
    let manifest = engine
        .fetch_manifest(&[server.url("/install.json")])
        .await
        .expect("manifest");
    let target = UpdateTarget::for_bundle(&install_dir.join(&executable)).expect("bersaglio");

    let error = engine
        .update_in_place(&manifest, &target, &noop_sink(), &CancelToken::new())
        .await
        .expect_err("impronta sbagliata");

    assert_eq!(error.code(), "hash-mismatch");
    assert_eq!(
        installed_body(&install_dir, &executable),
        "versione 2.0.0",
        "l'installazione non è stata toccata"
    );
    assert_eq!(update::sweep_leftovers(&install_dir), 0);
}

/// Voci di primo livello della cartella, ordinate: serve a dimostrare che
/// l'aggiornamento non ci lascia dentro niente di nuovo.
fn listing(directory: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("cartella")
        .flatten()
        .map(|entry| PathBuf::from(entry.file_name()))
        .collect();
    entries.sort();
    entries
}
