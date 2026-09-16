//! Aggiornamento in loco del launcher già installato.
//!
//! È l'altra metà di [`crate::install`], e la differenza sta tutta in una
//! frase: qui **la cartella d'installazione non si sceglie**. Si aggiorna
//! quella in cui il launcher sta girando, che è quella che l'utente ha scelto
//! quando ha installato, e non se ne crea una seconda da nessun'altra parte
//! (§D-084).
//!
//! Da questo discende il resto. Non si creano scorciatoie: ci sono già, e
//! puntano allo stesso percorso di prima. Non si scrive una seconda chiave di
//! disinstallazione: si aggiorna la versione dentro quella che c'è. Non si
//! copia un secondo disinstallatore: quello nella cartella è ancora valido,
//! perché legge `install.json` dal server e non contiene niente che invecchi
//! con il launcher.
//!
//! Il pacchetto è lo stesso che scarica l'installer — zip su Windows,
//! `.app.tar.gz` su macOS, AppImage su Linux — quindi il giro è identico su
//! tutti e tre i sistemi: nessuno di loro passa da un formato che gli altri
//! non hanno.
//!
//! ## Come si sostituisce un programma mentre è in esecuzione
//!
//! Windows non lascia **cancellare** un eseguibile in uso, ma lascia
//! **rinominarlo**: il file resta aperto dove sta, con un altro nome. Lo
//! scambio è quindi in tre tempi — si sposta di lato ciò che c'è, si mette al
//! suo posto ciò che è arrivato, si prova a cancellare ciò che è stato
//! spostato — e l'unico pezzo che può rimanere indietro è proprio il binario
//! in esecuzione, che [`sweep_leftovers`] toglie al riavvio successivo. Su
//! macOS e Linux la rinomina funziona per gli stessi motivi (l'inode resta
//! vivo finché qualcuno lo tiene aperto), e la stessa procedura vale senza
//! modifiche.

use std::path::{Path, PathBuf};

use serde::Serialize;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

use crate::error::{InstallError, InstallResult};
use crate::record::InstallRecord;
use crate::release::ReleaseManifest;
use crate::target::Target;
use crate::{discovery, fsops, paths, payload, platform, Installer};

/// Prefisso della cartella in cui il pacchetto viene srotolato prima dello
/// scambio. Sta **dentro** la cartella d'installazione, così lo scambio è una
/// rinomina sullo stesso volume e non una copia.
const STAGING_PREFIX: &str = ".vk-update";

/// Suffisso delle copie messe di lato durante lo scambio.
const QUARANTINE_MARK: &str = ".vk-old-";

/// Margine preteso oltre al pacchetto: lo si scarica, lo si srotola, e per un
/// istante coesiste con la copia precedente.
const HEADROOM_BYTES: u64 = 256 * 1024 * 1024;

/// L'installazione che si sta aggiornando.
#[derive(Debug, Clone)]
pub struct UpdateTarget {
    /// Cartella d'installazione: quella scelta a suo tempo dall'installer.
    pub install_dir: PathBuf,
    /// Eseguibile in esecuzione (su macOS il bundle `.app`).
    pub executable: PathBuf,
    /// Registro dell'installazione, quando c'è.
    pub record: Option<InstallRecord>,
}

impl UpdateTarget {
    /// L'installazione a cui appartiene il processo in esecuzione.
    ///
    /// È la sola domanda che dà sempre la risposta giusta: non "dove
    /// installerei", non "dove risulta installato", ma "dove sono".
    pub fn current() -> InstallResult<Self> {
        Self::for_bundle(&platform::self_bundle_path()?)
    }

    /// Come [`Self::current`], ma per un percorso dato. Serve ai test.
    pub fn for_bundle(bundle: &Path) -> InstallResult<Self> {
        let bundle = fsops::absolutize(bundle)?;
        let install_dir = bundle.parent().ok_or_else(|| {
            InstallError::NotUpdatable(format!("{} has no parent folder", bundle.display()))
        })?;
        let install_dir = fsops::ensure_safe_target(install_dir)?;

        if is_development_tree(&bundle) {
            return Err(InstallError::NotUpdatable(
                "this is a build from the source tree, not an installed copy".into(),
            ));
        }

        let record = discovery::find_near(&bundle)
            .filter(|record| same_path(&record.install_dir, &install_dir));

        Ok(Self {
            install_dir,
            executable: bundle,
            record,
        })
    }

    /// `true` quando esiste il registro scritto dall'installer.
    pub fn managed(&self) -> bool {
        self.record.is_some()
    }

    /// Versione registrata dall'installazione, quando la si conosce.
    pub fn installed_version(&self) -> Option<String> {
        self.record
            .as_ref()
            .map(|record| record.version.clone())
            .filter(|version| !version.trim().is_empty())
    }

    /// Perché questa copia non può aggiornarsi da sé, se non può.
    fn ensure_updatable(&self) -> InstallResult<()> {
        if !is_writable(&self.install_dir) {
            return Err(InstallError::NotUpdatable(format!(
                "{} cannot be written to",
                self.install_dir.display()
            )));
        }

        // Su Linux il pacchetto è un AppImage, cioè un file solo che sta
        // dentro la cartella d'installazione. Una copia che arriva da un
        // pacchetto della distribuzione non ha quella forma: sostituirla
        // vorrebbe dire scrivere in `/usr` al posto di `apt` (§D-069).
        #[cfg(all(unix, not(target_os = "macos")))]
        if self.record.is_none() && std::env::var_os("APPIMAGE").is_none() {
            return Err(InstallError::NotUpdatable(
                "this copy was installed by a system package: update it the same way".into(),
            ));
        }

        Ok(())
    }
}

/// Che cosa succederebbe aggiornando, prima di scaricare qualunque cosa.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlan {
    pub current_version: String,
    pub latest_version: String,
    /// `true` solo se quella pubblicata è **più recente** di quella in uso.
    pub available: bool,
    pub notes: String,
    pub pub_date: String,
    pub install_dir: PathBuf,
    pub executable: PathBuf,
    pub package_key: String,
    pub size_bytes: u64,
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub enough_space: bool,
    /// Il pacchetto dichiara un'impronta con cui verificare il download.
    pub verifiable: bool,
    /// Il pacchetto dichiara una firma: si può dimostrare chi l'ha
    /// pubblicato, non solo che è arrivato intero.
    pub signed: bool,
    /// `true` quando l'installazione ha un registro: l'aggiornamento saprà
    /// esattamente cosa stava dove.
    pub managed: bool,
}

impl UpdatePlan {
    /// `true` quando l'aggiornamento si può davvero eseguire ora.
    pub fn can_install(&self) -> bool {
        self.available && self.enough_space
    }
}

/// Confronta ciò che il manifest pubblica con ciò che sta girando.
///
/// Non scarica niente e non tocca niente: è la pagina che l'utente vede prima
/// di decidere.
pub fn plan(
    manifest: &ReleaseManifest,
    target: &UpdateTarget,
    current_version: &str,
) -> InstallResult<UpdatePlan> {
    target.ensure_updatable()?;

    let (package_key, package) = manifest.select(Target::current())?;
    let latest = manifest.version.trim().to_string();
    let available = !latest.is_empty() && vk_core::is_newer(&latest, current_version);

    let required_bytes = required_space(package.size);
    let available_bytes = fsops::available_space(&target.install_dir).unwrap_or(0);

    Ok(UpdatePlan {
        current_version: current_version.trim().to_string(),
        latest_version: latest,
        available,
        notes: manifest.notes.clone(),
        pub_date: manifest.pub_date.clone(),
        install_dir: target.install_dir.clone(),
        executable: target.executable.clone(),
        package_key,
        size_bytes: package.size,
        required_bytes,
        // Uno spazio libero che non si riesce a leggere non blocca niente:
        // sarà il disco a dire di no, con un errore che si capisce.
        enough_space: available_bytes == 0 || available_bytes >= required_bytes,
        available_bytes,
        verifiable: !package.sha256.is_empty(),
        signed: !package.signature.trim().is_empty(),
        managed: target.managed(),
    })
}

/// Che cosa è stato sostituito.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    pub version: String,
    pub install_dir: PathBuf,
    pub executable: PathBuf,
    pub bytes: u64,
    /// Voci di primo livello sostituite.
    pub replaced: Vec<PathBuf>,
    /// Copie precedenti che non si è potuto cancellare subito — di norma solo
    /// il binario in esecuzione. Spariscono al riavvio (vedi
    /// [`sweep_leftovers`]).
    pub pending_cleanup: Vec<PathBuf>,
    /// Riepilogo del download, con i tentativi.
    pub download_summary: String,
    /// `true` se il registro dell'installazione è stato aggiornato.
    pub record_updated: bool,
}

/// Spazio richiesto: il pacchetto scaricato, quello srotolato e un margine.
pub fn required_space(download_bytes: u64) -> u64 {
    download_bytes
        .saturating_mul(2)
        .saturating_add(HEADROOM_BYTES)
}

impl Installer {
    /// Scarica il pacchetto e lo mette al posto di quello in uso.
    ///
    /// L'ordine non è negoziabile, ed è lo stesso dell'installazione: si
    /// scarica e si verifica **prima** di toccare la cartella. Un download
    /// interrotto o un pacchetto che non supera la firma lascia
    /// l'installazione esattamente com'era.
    pub async fn update_in_place(
        &self,
        manifest: &ReleaseManifest,
        target: &UpdateTarget,
        progress: &ProgressSink,
        cancel: &CancelToken,
    ) -> InstallResult<UpdateReport> {
        let plan = plan(manifest, target, &self.app_version)?;
        if !plan.enough_space {
            return Err(InstallError::NotEnoughSpace {
                required: plan.required_bytes,
                available: plan.available_bytes,
            });
        }

        let (_, package) = manifest.select(Target::current())?;
        let format = package.format()?;

        // 1. Scarico e verifica, con la cartella d'installazione intatta.
        let archive = paths::download_temp_path(format.temp_extension());
        let download_summary = self
            .download_and_verify(package, &archive, progress, cancel)
            .await?;

        // 2. Srotolamento in una cartella di appoggio, sullo stesso volume.
        cancel.check()?;
        let staging = staging_dir(&target.install_dir);
        fsops::remove_path_best_effort(&staging);
        let staged = payload::install_payload(
            &archive,
            format,
            &staging,
            &package.executable,
            progress,
            cancel,
        );
        fsops::remove_path_best_effort(&archive);

        let staged = match staged {
            Ok(staged) => staged,
            Err(error) => {
                fsops::remove_path_best_effort(&staging);
                return Err(error);
            }
        };

        // 3. Scambio. Da qui in poi non si annulla più: fermarsi a metà
        //    lascerebbe una cartella con pezzi di due versioni.
        progress(ProgressUpdate::new(
            Phase::Installing,
            "Replacing the installed files",
        ));

        let swap = swap_entries(&staging, &target.install_dir, &staged.entries);
        fsops::remove_path_best_effort(&staging);
        let swap = swap?;

        let executable = relocate(&staged.executable, &staging, &target.install_dir);
        fsops::set_executable(&executable)?;

        #[cfg(target_os = "macos")]
        platform::clear_quarantine(&executable);

        // 4. Ciò che resta da dire al sistema: la versione nuova, nel
        //    registro dell'installazione e fra i programmi installati. Non
        //    una riga di più: scorciatoie, icone e disinstallatore sono
        //    quelli di prima e restano dove sono.
        let record_updated = refresh_registration(target, &manifest.version, &executable, &staged);

        progress(ProgressUpdate::new(Phase::Completed, "Update complete").with_percent(100.0));

        Ok(UpdateReport {
            version: manifest.version.clone(),
            install_dir: target.install_dir.clone(),
            executable,
            bytes: staged.bytes,
            replaced: swap.replaced,
            pending_cleanup: swap.pending_cleanup,
            download_summary,
            record_updated,
        })
    }
}

/// Cancella le copie messe di lato da un aggiornamento precedente.
///
/// Si chiama all'avvio: a quel punto il binario che le teneva aperte è morto,
/// e ciò che l'aggiornamento non aveva potuto togliere se ne va senza che
/// nessuno se ne accorga. Restituisce quante ne sono sparite.
pub fn sweep_leftovers(install_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(install_dir) else {
        return 0;
    };

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_leftover(path))
        .filter(|path| fsops::remove_path_best_effort(path))
        .count()
}

/// `true` per una copia messa di lato o per una cartella di appoggio
/// abbandonata da un aggiornamento interrotto.
fn is_leftover(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.contains(QUARANTINE_MARK) || name.starts_with(STAGING_PREFIX)
}

/// Esito dello scambio.
struct Swap {
    replaced: Vec<PathBuf>,
    pending_cleanup: Vec<PathBuf>,
}

/// Mette al loro posto le voci srotolate, una per una.
fn swap_entries(staging: &Path, install_dir: &Path, entries: &[PathBuf]) -> InstallResult<Swap> {
    let stamp = format!("{}-{}", std::process::id(), vk_core::now_millis());
    let mut replaced = Vec::with_capacity(entries.len());
    let mut aside = Vec::new();

    for entry in entries {
        let source = staging.join(entry);
        let destination = install_dir.join(entry);
        vk_core::zipx::ensure_within(install_dir, &destination)?;

        if let Some(parent) = destination.parent() {
            fsops::ensure_dir(parent)?;
        }

        if std::fs::symlink_metadata(&destination).is_ok() {
            let quarantine = quarantine_path(&destination, &stamp);
            fsops::remove_path_best_effort(&quarantine);

            // Rinominare riesce anche su un eseguibile in uso; cancellare no.
            // Si prova quindi prima a spostare di lato, e solo se il sistema
            // non lo permette si tenta la rimozione diretta.
            match std::fs::rename(&destination, &quarantine) {
                Ok(()) => aside.push(quarantine),
                Err(rename_error) => {
                    fsops::remove_path(&destination)
                        .map_err(|_| InstallError::io(&destination, rename_error))?;
                }
            }
        }

        std::fs::rename(&source, &destination)
            .map_err(|error| InstallError::io(&destination, error))?;
        replaced.push(entry.clone());
    }

    // Le copie di lato si tolgono adesso; quella del binario in esecuzione
    // resisterà, ed è previsto.
    let pending_cleanup = aside
        .into_iter()
        .filter(|path| !fsops::remove_path_best_effort(path))
        .collect();

    Ok(Swap {
        replaced,
        pending_cleanup,
    })
}

/// Aggiorna il registro dell'installazione e la voce fra i programmi
/// installati. Nessuno dei due è indispensabile all'avvio del launcher: se
/// falliscono, l'aggiornamento resta riuscito.
fn refresh_registration(
    target: &UpdateTarget,
    version: &str,
    executable: &Path,
    staged: &payload::InstalledPayload,
) -> bool {
    let Some(record) = target.record.clone() else {
        tracing::info!("nessun registro accanto all'installazione: non c'è niente da aggiornare");
        return false;
    };

    let mut record = record;
    record.version = version.trim().to_string();
    record.target = Target::current().key();
    record.installed_at = crate::record::now_iso8601();
    record.executable = executable.to_path_buf();
    for entry in &staged.entries {
        if !record.payload.contains(entry) {
            record.payload.push(entry.clone());
        }
    }

    let saved = record.save().is_ok();
    if !saved {
        tracing::warn!("registro dell'installazione non riscritto");
    }

    // Si riscrive la registrazione **solo** se l'installazione ne aveva una:
    // un'installazione che aveva scelto di non comparire fra i programmi
    // installati non deve comparirci dopo un aggiornamento.
    let registered = record
        .artifacts_of(crate::record::ArtifactKind::RegistryKey)
        .next()
        .is_some();
    if registered {
        let registration = platform::UninstallRegistration {
            install_dir: &record.install_dir,
            executable,
            uninstaller: Some(record.uninstaller.as_path()).filter(|path| path.exists()),
            version: record.version.as_str(),
            size_bytes: staged.bytes,
        };
        if let Err(error) = platform::register_uninstall(&registration) {
            tracing::warn!(%error, "voce fra i programmi installati non aggiornata");
        }
    }

    saved
}

fn staging_dir(install_dir: &Path) -> PathBuf {
    install_dir.join(format!("{STAGING_PREFIX}-{}", std::process::id()))
}

fn quarantine_path(destination: &Path, stamp: &str) -> PathBuf {
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "entry".to_string());
    destination.with_file_name(format!("{name}{QUARANTINE_MARK}{stamp}"))
}

/// Lo stesso percorso, con la cartella di appoggio sostituita da quella
/// d'installazione.
fn relocate(path: &Path, from: &Path, to: &Path) -> PathBuf {
    match path.strip_prefix(from) {
        Ok(relative) => to.join(relative),
        Err(_) => path.to_path_buf(),
    }
}

fn is_writable(directory: &Path) -> bool {
    let probe = directory.join(format!(".vanzakart-update-{}", std::process::id()));
    let writable = std::fs::write(&probe, b"ok").is_ok();
    fsops::remove_path_best_effort(&probe);
    writable
}

/// `true` per un binario che sta dentro `target/debug` o `target/release`.
///
/// Un `cargo run` non è un'installazione, e sostituirgli i file sotto i piedi
/// vorrebbe dire srotolare un rilascio dentro l'albero dei sorgenti.
fn is_development_tree(bundle: &Path) -> bool {
    let mut components = bundle.components().rev().skip(1);
    let profile = components.next();
    let parent = components.next();

    let name_is = |component: Option<std::path::Component>, expected: &[&str]| {
        component.is_some_and(|component| {
            let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
            expected.contains(&name.as_str())
        })
    };

    name_is(profile, &["debug", "release"]) && name_is(parent, &["target"])
}

fn same_path(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| {
        let lexical = fsops::absolutize(path).unwrap_or_else(|_| path.to_path_buf());
        std::fs::canonicalize(&lexical).unwrap_or(lexical)
    };
    resolve(left) == resolve(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, body).expect("scritto");
    }

    /// Una cartella d'installazione finta, con dentro il "launcher", le sue
    /// risorse e il registro che l'installer avrebbe lasciato.
    fn installed(root: &Path) -> PathBuf {
        let install_dir = root.join("app").join("VanzaKart Launcher");
        write(&install_dir.join("launcher"), "versione 1");
        write(&install_dir.join("resources").join("endpoints.json"), "{}");

        let mut record = InstallRecord::new("1.9.0", Target::current().key(), install_dir.clone());
        record.executable = install_dir.join("launcher");
        record.payload = vec![PathBuf::from("launcher"), PathBuf::from("resources")];
        write(
            &install_dir.join(paths::RECORD_FILE_NAME),
            &serde_json::to_string(&record).expect("json"),
        );

        install_dir
    }

    /// Un pacchetto già srotolato, come lo lascerebbe `payload`.
    fn staged_payload(install_dir: &Path) -> PathBuf {
        let staging = staging_dir(install_dir);
        write(&staging.join("launcher"), "versione 2");
        write(
            &staging.join("resources").join("endpoints.json"),
            "{\"v\":2}",
        );
        staging
    }

    #[test]
    fn the_swap_replaces_every_entry() {
        let temp = tempfile::tempdir().expect("temp");
        let install_dir = installed(temp.path());
        let staging = staged_payload(&install_dir);

        let entries = vec![PathBuf::from("launcher"), PathBuf::from("resources")];
        let swap = swap_entries(&staging, &install_dir, &entries).expect("scambio");

        assert_eq!(swap.replaced, entries);
        assert!(swap.pending_cleanup.is_empty(), "niente resta indietro");
        assert_eq!(
            std::fs::read_to_string(install_dir.join("launcher")).expect("letto"),
            "versione 2"
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("resources").join("endpoints.json"))
                .expect("letto"),
            "{\"v\":2}"
        );
    }

    /// Il caso che conta su Windows: il file da sostituire è aperto. Qui lo
    /// si simula tenendo aperto un handle, che su Windows blocca la
    /// cancellazione ma non la rinomina.
    #[test]
    fn a_file_that_cannot_be_deleted_is_moved_aside_instead() {
        let temp = tempfile::tempdir().expect("temp");
        let install_dir = installed(temp.path());
        let staging = staged_payload(&install_dir);

        let open = std::fs::File::open(install_dir.join("launcher")).expect("aperto");

        let entries = vec![PathBuf::from("launcher")];
        let swap = swap_entries(&staging, &install_dir, &entries).expect("scambio");
        assert_eq!(
            std::fs::read_to_string(install_dir.join("launcher")).expect("letto"),
            "versione 2",
            "la versione nuova è al suo posto anche con la vecchia aperta"
        );

        drop(open);
        // Su Windows la copia di lato resta finché l'handle è vivo; su Unix
        // sparisce subito. In entrambi i casi la pulizia successiva non deve
        // lasciare niente.
        assert!(
            swap.pending_cleanup.len() <= 1,
            "al massimo il binario in uso resta indietro"
        );
        sweep_leftovers(&install_dir);
        assert!(
            std::fs::read_dir(&install_dir)
                .expect("letta")
                .flatten()
                .all(|entry| !is_leftover(&entry.path())),
            "nessuna copia di lato sopravvive alla pulizia"
        );
    }

    #[test]
    fn the_sweep_removes_leftovers_and_nothing_else() {
        let temp = tempfile::tempdir().expect("temp");
        let install_dir = installed(temp.path());
        write(&install_dir.join("launcher.vk-old-123-456"), "vecchio");
        std::fs::create_dir_all(install_dir.join(".vk-update-999")).expect("mkdir");

        assert_eq!(sweep_leftovers(&install_dir), 2);
        assert!(install_dir.join("launcher").is_file(), "il launcher resta");
        assert!(install_dir.join("resources").is_dir(), "le risorse restano");
        assert_eq!(sweep_leftovers(&install_dir), 0);
    }

    #[test]
    fn a_build_from_the_source_tree_is_not_an_installation() {
        assert!(is_development_tree(Path::new(
            "/progetto/target/debug/vanzakart-launcher"
        )));
        assert!(is_development_tree(Path::new(
            "/progetto/target/release/VanzaKart Launcher.exe"
        )));
        assert!(!is_development_tree(Path::new(
            "/home/tizio/.local/opt/vanzakart-launcher/vanzakart-launcher.AppImage"
        )));

        // I separatori di Windows li riconosce solo Windows.
        #[cfg(windows)]
        {
            assert!(is_development_tree(Path::new(
                "C:\\progetto\\target\\release\\VanzaKart Launcher.exe"
            )));
            assert!(!is_development_tree(Path::new(
                "C:\\Users\\tizio\\AppData\\Local\\VanzaKart Launcher\\VanzaKart Launcher.exe"
            )));
        }
    }

    #[test]
    fn the_target_refuses_a_folder_it_must_not_write_into() {
        let home = dirs::home_dir().expect("home");
        let error =
            UpdateTarget::for_bundle(&home.join("VanzaKart Launcher.exe")).expect_err("rifiutata");
        assert_eq!(error.code(), "unsafe-path");
    }

    #[test]
    fn the_required_space_covers_package_and_headroom() {
        assert_eq!(required_space(0), HEADROOM_BYTES);
        assert_eq!(required_space(100), 200 + HEADROOM_BYTES);
        assert_eq!(required_space(u64::MAX), u64::MAX);
    }

    #[test]
    fn a_staging_folder_lives_inside_the_installation() {
        let install_dir = PathBuf::from("/opt/vanzakart");
        let staging = staging_dir(&install_dir);
        assert!(staging.starts_with(&install_dir));
        assert!(is_leftover(&staging));
    }

    #[test]
    fn a_quarantined_copy_keeps_the_original_name_in_front() {
        let path = quarantine_path(Path::new("/opt/app/VanzaKart Launcher.exe"), "1-2");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("VanzaKart Launcher.exe.vk-old-1-2")
        );
        assert_eq!(path.parent(), Some(Path::new("/opt/app")));
        assert!(is_leftover(&path));
    }

    #[test]
    fn a_plan_without_a_newer_version_offers_nothing() {
        let temp = tempfile::tempdir().expect("temp");
        let install_dir = installed(temp.path());
        let target = UpdateTarget::for_bundle(&install_dir.join("launcher")).expect("bersaglio");

        let manifest = manifest_at("2.0.0");
        let plan = plan(&manifest, &target, "2.0.0").expect("piano");
        assert!(!plan.available);
        assert!(!plan.can_install());
        assert_eq!(plan.install_dir, install_dir, "si aggiorna dove si è");

        let plan = plan_for(&manifest, &target, "1.9.0");
        assert!(plan.available);
        assert_eq!(plan.latest_version, "2.0.0");
        assert!(plan.verifiable && plan.signed);
    }

    fn plan_for(manifest: &ReleaseManifest, target: &UpdateTarget, version: &str) -> UpdatePlan {
        plan(manifest, target, version).expect("piano")
    }

    /// Un manifest con il pacchetto della piattaforma su cui gira il test.
    fn manifest_at(version: &str) -> ReleaseManifest {
        let key = Target::current().key();
        let extension = if cfg!(windows) {
            "zip"
        } else if cfg!(target_os = "macos") {
            "tar.gz"
        } else {
            "AppImage"
        };
        let raw = format!(
            r#"{{
                "version": "{version}",
                "platforms": {{
                    "{key}": {{
                        "url": "https://example.test/p_{version}.{extension}",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "size": 1024,
                        "signature": "{signature}"
                    }}
                }}
            }}"#,
            signature = SAMPLE_SIGNATURE
        );
        ReleaseManifest::parse(&raw).expect("manifest")
    }

    /// Una firma ben formata, di un altro pacchetto: qui serve solo perché il
    /// manifest la accetti, non viene verificata.
    const SAMPLE_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRS1dSS0x5YjJ5Y1VZeVZCVEJkdkMxN0pRUGtSSDZ6UDByVzhjaFBKRmNMRGNLR1JJa2UrV3oxN0JEZ25kelF3bm9YOEVHZTdOZCtMQ1dwaHV6b2NCazdsdnJidGNuT2djPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzg3Nzg3MzYyCWZpbGU6VmFuemFLYXJ0IExhdW5jaGVyXzIuMC4wX3g2NC1zZXR1cC5leGUKSUROa2QvSC9LcitpOWQxejQwbVp0L1RLc0RDbjkrdStVV25uTmlhazR1YVRjUUs2dWErQnI3L003R3NkUDdoZUE1emVJOEg0MHZJUVpDNXJmYmlkRGc9PQo=";
}
