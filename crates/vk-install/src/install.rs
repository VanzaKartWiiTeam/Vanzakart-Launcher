//! L'installazione, dall'inizio alla fine.
//!
//! Gli stessi passi del setup legacy — controlli, backup, download, verifica,
//! estrazione, scorciatoie, registrazione — con due differenze che contano:
//! il pacchetto è quello della piattaforma su cui si sta girando, e alla fine
//! resta un registro di ciò che è stato fatto ([`crate::record`]).
//!
//! Ordine non negoziabile: **si scarica e si verifica prima di toccare la
//! cartella d'installazione**. Un download interrotto o un pacchetto corrotto
//! non deve lasciare un'installazione a metà, che è esattamente ciò che
//! succedeva estraendo direttamente sopra i file esistenti.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vk_core::net::Downloader;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

use crate::error::{InstallError, InstallResult};
use crate::record::{Artifact, ArtifactKind, InstallRecord};
use crate::release::{ReleaseManifest, ReleasePackage};
use crate::target::Target;
use crate::{discovery, fsops, paths, payload, platform};

/// Spazio che si pretende oltre al pacchetto: il launcher ci scriverà dati,
/// log e backup, e riempire il disco al primo avvio non è un'installazione
/// riuscita.
const HEADROOM_BYTES: u64 = 512 * 1024 * 1024;

/// Come trattare ciò che c'è già.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMode {
    /// Prima installazione, o installazione sopra una cartella vuota.
    #[default]
    Fresh,
    /// Sovrascrive i file del programma e lascia il resto dov'è.
    Update,
    /// Svuota la cartella prima di estrarre. I dati dell'utente, che stanno
    /// altrove, non vengono toccati comunque.
    CleanReinstall,
}

/// Scelte fatte nella procedura guidata.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub install_dir: PathBuf,
    pub mode: InstallMode,
    /// Copia le impostazioni del launcher prima di procedere.
    pub backup_data: bool,
    pub backup_dir: PathBuf,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    /// Solo Windows.
    pub quick_launch_shortcut: bool,
    /// Voce "Disinstalla" accanto a quella del launcher.
    pub uninstall_entry: bool,
    /// Solo Linux: collegamento in `~/.local/bin`.
    pub path_symlink: bool,
    /// Copia l'installer come disinstallatore dentro la cartella.
    pub copy_uninstaller: bool,
    /// Registra il launcher fra i programmi installati del sistema (su
    /// Windows la chiave di disinstallazione). Spento produce
    /// un'installazione che non lascia tracce fuori dalla propria cartella.
    pub register_system: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            install_dir: paths::default_install_dir(),
            mode: InstallMode::Fresh,
            backup_data: true,
            backup_dir: paths::default_backup_dir(),
            desktop_shortcut: true,
            start_menu_shortcut: true,
            quick_launch_shortcut: false,
            uninstall_entry: true,
            path_symlink: cfg!(all(unix, not(target_os = "macos"))),
            copy_uninstaller: true,
            register_system: true,
        }
    }
}

/// Esito dei controlli preliminari, mostrato nella pagina "Verifiche".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub target: String,
    pub version: String,
    pub install_dir: PathBuf,
    pub download_bytes: u64,
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub enough_space: bool,
    pub writable: bool,
    pub launcher_running: bool,
    /// Il pacchetto dichiara un'impronta con cui verificare il download.
    pub verifiable: bool,
}

impl Preflight {
    /// `true` quando si può procedere.
    pub fn is_ready(&self) -> bool {
        self.enough_space && self.writable && !self.launcher_running
    }
}

/// Che cosa è stato installato.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub install_dir: PathBuf,
    pub executable: PathBuf,
    pub uninstaller: Option<PathBuf>,
    pub version: String,
    pub target: String,
    pub bytes: u64,
    pub artifacts: Vec<Artifact>,
    pub backup: Option<PathBuf>,
    /// Riepilogo del download, con i tentativi: finisce nel registro visibile.
    pub download_summary: String,
}

/// Il motore. Si costruisce una volta e serve tutta la procedura.
#[derive(Debug)]
pub struct Installer {
    downloader: Downloader,
    /// Icona da installare nel tema su Linux, se l'installer ne porta una.
    icon: Option<PathBuf>,
    /// L'installer stesso, da copiare come disinstallatore.
    setup_bundle: PathBuf,
}

impl Installer {
    pub fn new(app_version: &str, icon: Option<PathBuf>) -> InstallResult<Self> {
        Ok(Self {
            downloader: Downloader::new(&crate::user_agent(app_version))?,
            icon: icon.filter(|path| path.exists()),
            setup_bundle: platform::self_bundle_path()?,
        })
    }

    /// Solo per i test: un downloader già configurato (per esempio con
    /// `with_loopback_http`).
    pub fn with_downloader(mut self, downloader: Downloader) -> Self {
        self.downloader = downloader;
        self
    }

    pub fn downloader(&self) -> &Downloader {
        &self.downloader
    }

    /// Scarica e valida `install.json`, provando gli URL in ordine.
    pub async fn fetch_manifest(&self, urls: &[String]) -> InstallResult<ReleaseManifest> {
        let mut last_error: Option<InstallError> = None;

        for url in vk_core::net::dedupe_urls(urls) {
            let separator = if url.contains('?') { '&' } else { '?' };
            let busted = format!("{url}{separator}t={}", vk_core::now_millis());

            match self.downloader.get_string(&busted).await {
                Ok(raw) => match ReleaseManifest::parse(&raw) {
                    Ok(manifest) => return Ok(manifest),
                    Err(error) => {
                        tracing::warn!(%error, "manifest non valido");
                        last_error = Some(error);
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, "manifest non raggiungibile");
                    last_error = Some(error.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            InstallError::InvalidManifest("no address to read the manifest from".into())
        }))
    }

    /// Controlli preliminari su spazio, permessi e processi in esecuzione.
    pub fn preflight(
        &self,
        manifest: &ReleaseManifest,
        install_dir: &Path,
    ) -> InstallResult<Preflight> {
        let (target, package) = manifest.select(Target::current())?;
        let install_dir = fsops::ensure_safe_target(install_dir)?;

        let download_bytes = package.size;
        let required_bytes = required_space(download_bytes);
        let available_bytes = fsops::available_space(&install_dir).unwrap_or(0);
        let executable = install_dir.join(paths::launcher_executable_name());

        Ok(Preflight {
            target,
            version: manifest.version.clone(),
            download_bytes,
            required_bytes,
            available_bytes,
            // Uno spazio libero che non si riesce a leggere non deve bloccare
            // l'installazione: si prosegue e sarà il disco a dire di no.
            enough_space: available_bytes == 0 || available_bytes >= required_bytes,
            writable: is_writable(&install_dir),
            launcher_running: platform::is_running(&executable),
            verifiable: !package.sha256.is_empty(),
            install_dir,
        })
    }

    /// Esegue l'installazione.
    pub async fn install(
        &self,
        manifest: &ReleaseManifest,
        options: &InstallOptions,
        progress: &ProgressSink,
        cancel: &CancelToken,
    ) -> InstallResult<InstallReport> {
        let (target, package) = manifest.select(Target::current())?;
        let install_dir = fsops::ensure_safe_target(&options.install_dir)?;
        let format = package.format()?;

        let preflight = self.preflight(manifest, &install_dir)?;
        if preflight.launcher_running {
            return Err(InstallError::LauncherRunning);
        }
        if !preflight.enough_space {
            return Err(InstallError::NotEnoughSpace {
                required: preflight.required_bytes,
                available: preflight.available_bytes,
            });
        }

        // 1. Backup — prima di tutto, perché è l'unico passo che protegge da
        //    ciò che verrà dopo.
        let backup = if options.backup_data {
            backup_launcher_data(&options.backup_dir)?
        } else {
            None
        };

        // 2. Download e verifica, con la cartella d'installazione ancora
        //    intatta.
        let archive = paths::download_temp_path(format.temp_extension());
        let download_summary = self
            .download_and_verify(package, &archive, progress, cancel)
            .await?;

        // 3. Da qui in poi si tocca il disco.
        cancel.check()?;
        fsops::ensure_dir(&install_dir)?;

        if options.mode == InstallMode::CleanReinstall {
            clean_install_dir(&install_dir, progress)?;
        }

        let installed = payload::install_payload(
            &archive,
            format,
            &install_dir,
            &package.executable,
            progress,
            cancel,
        )?;
        fsops::remove_path_best_effort(&archive);

        #[cfg(target_os = "macos")]
        platform::clear_quarantine(&installed.executable);

        progress(ProgressUpdate::new(
            Phase::Installing,
            "Creating the shortcuts",
        ));

        // 4. Disinstallatore, scorciatoie, registrazione.
        let uninstaller = if options.copy_uninstaller {
            self.place_uninstaller(&install_dir)?
        } else {
            None
        };

        let mut record = InstallRecord::new(&manifest.version, &target, install_dir.clone());
        record.executable = installed.executable.clone();
        record.payload = installed.entries.clone();
        if let Some(uninstaller) = &uninstaller {
            record.uninstaller = uninstaller.clone();
            if let Ok(relative) = uninstaller.strip_prefix(&install_dir) {
                let relative = relative.to_path_buf();
                if !record.payload.contains(&relative) {
                    record.payload.push(relative);
                }
            }
        }

        let request = platform::ShortcutRequest {
            executable: &installed.executable,
            working_dir: &install_dir,
            uninstaller: uninstaller.as_deref(),
            icon: self.icon.as_deref(),
            desktop: options.desktop_shortcut,
            start_menu: options.start_menu_shortcut,
            quick_launch: options.quick_launch_shortcut,
            uninstall_entry: options.uninstall_entry,
            path_symlink: options.path_symlink,
        };
        for artifact in platform::create_shortcuts(&request) {
            record.add_artifact(artifact);
        }

        if options.register_system {
            let registration = platform::UninstallRegistration {
                install_dir: &install_dir,
                executable: &installed.executable,
                uninstaller: uninstaller.as_deref(),
                version: &manifest.version,
                size_bytes: installed.bytes,
            };
            match platform::register_uninstall(&registration) {
                Ok(artifacts) => {
                    for artifact in artifacts {
                        record.add_artifact(artifact);
                    }
                }
                // Non comparire fra i programmi installati è un peccato, non
                // un motivo per buttare via un'installazione riuscita.
                Err(error) => tracing::warn!(%error, "registrazione non riuscita"),
            }
        }

        for path in record.save()? {
            record.add_artifact(Artifact::file(ArtifactKind::Record, &path));
        }
        // Il registro appena arricchito va riscritto: deve contenere anche se
        // stesso, altrimenti la disinstallazione lo lascia lì.
        let _ = record.save();

        progress(
            ProgressUpdate::new(Phase::Completed, "Installation complete").with_percent(100.0),
        );

        Ok(InstallReport {
            install_dir,
            executable: installed.executable,
            uninstaller,
            version: manifest.version.clone(),
            target,
            bytes: installed.bytes,
            artifacts: record.artifacts,
            backup,
            download_summary,
        })
    }

    async fn download_and_verify(
        &self,
        package: &ReleasePackage,
        archive: &Path,
        progress: &ProgressSink,
        cancel: &CancelToken,
    ) -> InstallResult<String> {
        progress(ProgressUpdate::new(
            Phase::Connecting,
            "Contacting the server",
        ));

        // Un file rimasto da un tentativo precedente farebbe ripartire il
        // download a metà di qualcos'altro.
        fsops::remove_path_best_effort(archive);

        let outcome = self
            .downloader
            .download_with_mirrors(&package.urls(), archive, progress, cancel)
            .await?;

        if !package.sha256.is_empty() {
            progress(ProgressUpdate::new(
                Phase::Verifying,
                "Verifying the package checksum",
            ));
            let actual = vk_core::hash::sha256_file(archive).await?;
            if !vk_core::hash::hash_eq(&actual, &package.sha256) {
                fsops::remove_path_best_effort(archive);
                return Err(InstallError::HashMismatch {
                    expected: package.sha256.clone(),
                    actual,
                });
            }
        } else {
            tracing::warn!("il manifest non dichiara un'impronta: pacchetto non verificabile");
        }

        Ok(outcome.summary("launcher package"))
    }

    /// Copia l'installer accanto al launcher, con il nome del
    /// disinstallatore.
    ///
    /// È lo stesso binario: riconosce di essere il disinstallatore dal nome o
    /// dall'argomento `--uninstall` (§D-053). Copiarlo evita di dover
    /// mantenere due applicazioni quasi identiche.
    fn place_uninstaller(&self, install_dir: &Path) -> InstallResult<Option<PathBuf>> {
        let target = install_dir.join(paths::uninstaller_name());
        if same_file(&self.setup_bundle, &target) {
            return Ok(Some(target));
        }

        fsops::remove_path_best_effort(&target);
        match fsops::copy_tree(&self.setup_bundle, &target) {
            Ok(_) => {
                fsops::set_executable(&target)?;
                Ok(Some(target))
            }
            Err(error) => {
                // Senza disinstallatore locale l'utente può sempre rilanciare
                // l'installer, che riconosce l'installazione e offre di
                // rimuoverla.
                tracing::warn!(%error, "disinstallatore non copiato");
                Ok(None)
            }
        }
    }
}

/// Spazio richiesto: il pacchetto scaricato, quello estratto e un margine.
pub fn required_space(download_bytes: u64) -> u64 {
    download_bytes
        .saturating_mul(2)
        .saturating_add(HEADROOM_BYTES)
}

fn is_writable(install_dir: &Path) -> bool {
    let probe_dir = match fsops::nearest_existing(install_dir) {
        Some(existing) => existing,
        None => return false,
    };
    let probe = probe_dir.join(format!(".vanzakart-setup-{}", std::process::id()));
    let writable = std::fs::write(&probe, b"ok").is_ok();
    fsops::remove_path_best_effort(&probe);
    writable
}

/// Svuota la cartella d'installazione, senza toccare il registro.
fn clean_install_dir(install_dir: &Path, progress: &ProgressSink) -> InstallResult<()> {
    fsops::ensure_safe_target(install_dir)?;
    progress(ProgressUpdate::new(
        Phase::Installing,
        "Cleaning the install folder",
    ));

    let entries =
        std::fs::read_dir(install_dir).map_err(|error| InstallError::io(install_dir, error))?;
    for entry in entries.flatten() {
        let path = entry.path();
        // Il disinstallatore in esecuzione, se è lui ad aver avviato la
        // reinstallazione, non si può cancellare da sotto i piedi.
        if is_self(&path) {
            continue;
        }
        fsops::remove_path_best_effort(&path);
    }
    Ok(())
}

fn is_self(path: &Path) -> bool {
    platform::self_bundle_path()
        .map(|current| same_file(&current, path))
        .unwrap_or(false)
}

/// `true` se due percorsi indicano lo stesso file.
///
/// Prima si normalizza in modo puramente testuale, poi si chiede al
/// filesystem. Non è un dettaglio di stile: `canonicalize` su Linux pretende
/// che **ogni** componente del percorso esista — un `b/../a.txt` con `b`
/// inesistente fallisce — mentre su Windows lo risolve lo stesso. Senza il
/// primo passaggio la stessa domanda avrebbe due risposte diverse sui due
/// sistemi.
fn same_file(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| {
        let lexical = fsops::absolutize(path).unwrap_or_else(|_| path.to_path_buf());
        std::fs::canonicalize(&lexical).unwrap_or(lexical)
    };
    resolve(left) == resolve(right)
}

/// Copia le impostazioni del launcher in una cartella datata.
///
/// Il token beta (`secrets.json`) resta fuori di proposito: un backup finisce
/// in Documenti, e un segreto copiato in chiaro dove nessuno se lo aspetta è
/// un problema in più, non una tutela.
pub fn backup_launcher_data(backup_root: &Path) -> InstallResult<Option<PathBuf>> {
    let Some(data_root) = paths::launcher_data_root() else {
        return Ok(None);
    };
    if !data_root.is_dir() {
        return Ok(None);
    }

    let destination = backup_root.join(format!(
        "VanzaKart_Backup_{}",
        vk_core::fsx::backup_timestamp()
    ));
    let mut copied = 0u64;

    for name in [
        "settings.json",
        "preferences.json",
        "install_state.json",
        "endpoints.cache.json",
        "VanzaKart_launcher.json",
        "VKBeta_launcher.json",
        "mod_version.txt",
        "mod_beta_version.txt",
        "musicpack_version.txt",
        "musicpack_beta_version.txt",
    ] {
        let source = data_root.join(name);
        if source.is_file() {
            fsops::ensure_dir(&destination)?;
            copied += fsops::copy_tree(&source, &destination.join(name))?;
        }
    }

    if copied == 0 && fsops::is_dir_empty(&destination) {
        fsops::remove_path_best_effort(&destination);
        return Ok(None);
    }

    tracing::info!(destination = %destination.display(), "backup delle impostazioni");
    Ok(Some(destination))
}

/// Cartella proposta dalla procedura guidata: quella già in uso, se c'è.
pub fn suggested_install_dir() -> PathBuf {
    discovery::find()
        .map(|existing| existing.install_dir)
        .unwrap_or_else(paths::default_install_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_required_space_covers_package_and_headroom() {
        assert_eq!(required_space(0), HEADROOM_BYTES);
        assert_eq!(required_space(100), 200 + HEADROOM_BYTES);
        // Nessun overflow con un manifest che dichiara numeri assurdi.
        assert_eq!(required_space(u64::MAX), u64::MAX);
    }

    #[test]
    fn a_writable_folder_is_recognised() {
        let temp = tempfile::tempdir().expect("temp");
        assert!(is_writable(&temp.path().join("nuova")));
    }

    #[test]
    fn the_default_options_install_where_the_platform_expects() {
        let options = InstallOptions::default();
        assert_eq!(options.install_dir, paths::default_install_dir());
        assert_eq!(options.mode, InstallMode::Fresh);
        assert!(options.copy_uninstaller);
    }

    #[test]
    fn a_clean_reinstall_empties_the_folder() {
        let temp = tempfile::tempdir().expect("temp");
        let install_dir = temp.path().join("app").join("VanzaKart");
        std::fs::create_dir_all(install_dir.join("vecchia")).expect("mkdir");
        std::fs::write(install_dir.join("vecchio.exe"), b"MZ").expect("scritto");

        clean_install_dir(&install_dir, &vk_core::progress::noop_sink()).expect("pulita");

        assert!(install_dir.is_dir());
        assert!(fsops::is_dir_empty(&install_dir));
    }

    #[test]
    fn a_backup_of_nothing_is_no_backup() {
        let temp = tempfile::tempdir().expect("temp");
        // Nessun dato del launcher su questa macchina di test: la funzione
        // non deve creare una cartella vuota in Documenti.
        let result = backup_launcher_data(&temp.path().join("backup")).expect("backup");
        if result.is_none() {
            assert!(!temp.path().join("backup").exists());
        }
    }

    #[test]
    fn two_names_for_the_same_file_are_the_same_file() {
        let temp = tempfile::tempdir().expect("temp");
        let file = temp.path().join("a.txt");
        std::fs::write(&file, b"x").expect("scritto");

        // Con la cartella intermedia che esiste davvero.
        let esistente = temp.path().join("sotto");
        std::fs::create_dir(&esistente).expect("mkdir");
        assert!(same_file(&file, &esistente.join("..").join("a.txt")));

        // E con una che non esiste: su Linux `canonicalize` qui fallisce, e
        // senza la normalizzazione testuale la risposta cambierebbe da un
        // sistema all'altro.
        assert!(same_file(
            &file,
            &temp.path().join("mai").join("..").join("a.txt")
        ));

        // Due file diversi restano diversi.
        let altro = temp.path().join("b.txt");
        std::fs::write(&altro, b"x").expect("scritto");
        assert!(!same_file(&file, &altro));
    }

    #[test]
    fn the_suggested_folder_is_always_absolute() {
        assert!(suggested_install_dir().is_absolute());
    }
}
