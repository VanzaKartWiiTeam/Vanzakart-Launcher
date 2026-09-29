//! Aggiornamento di un'installazione in `Programmi`, con la UAC (§D-093).
//!
//! Il launcher gira come l'utente, e l'utente non può scrivere in `Programmi`.
//! L'aggiornamento si divide quindi fra due processi:
//!
//! 1. il launcher scarica e verifica il pacchetto
//!    ([`Installer::download_update`]) con la sua barra e il suo pulsante per
//!    annullare, e scrive un [`ElevatedJob`] in una cartella temporanea;
//! 2. poi avvia **sé stesso** con i permessi di amministratore e l'argomento
//!    [`APPLY_UPDATE_FLAG`]. Quel processo non apre finestre: esegue
//!    [`Installer::apply_elevated_job`] ed esce con un codice ([`exit_code`]).
//!
//! Il job sta in una cartella che l'utente può scrivere, quindi per il
//! processo elevato è un dato **non fidato** — il caso da tenere a mente è
//! l'account standard su cui un genitore digita la password. Per questo:
//!
//! - la cartella da aggiornare non viene dal job, ma dal percorso del
//!   processo stesso ([`UpdateTarget::current`]);
//! - il manifest non viene dal job: il processo elevato lo rilegge dagli
//!   indirizzi compilati nel launcher;
//! - il pacchetto viene copiato **dentro** la cartella d'installazione, dove
//!   l'utente non scrive, e lì verificato di nuovo — impronta e firma — prima
//!   di essere srotolato; un pacchetto senza né l'una né l'altra non passa;
//! - l'esito torna con il codice d'uscita e con un file nella cartella
//!   d'installazione: il processo elevato non scrive dove sceglie l'utente.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vk_core::progress::{noop_sink, CancelToken};

use crate::error::{InstallError, InstallResult};
use crate::release::ReleaseManifest;
use crate::target::Target;
use crate::update::{self, Access, UpdateReport, UpdateTarget};
use crate::{fsops, Installer};

/// Argomento con cui il launcher riconosce di essere il processo elevato.
pub const APPLY_UPDATE_FLAG: &str = "--vk-apply-update";

const JOB_FILE_NAME: &str = "job.json";

/// Un job è una manciata di campi: un file più grande non è un job.
const MAX_JOB_BYTES: u64 = 16 * 1024;

/// Esito dell'ultimo aggiornamento elevato, dentro la cartella
/// d'installazione. Il nome comincia come quello della cartella di appoggio,
/// così la pulizia dell'aggiornamento successivo lo toglie.
const RESULT_FILE_NAME: &str = ".vk-update-result.json";

/// Che cosa il launcher chiede al processo elevato.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ElevatedJob {
    /// Versione che il launcher ha scaricato: se nel frattempo ne è uscita
    /// un'altra, il processo elevato si ferma invece di applicarla.
    pub version: String,
    /// Pacchetto scaricato e già verificato dal launcher.
    pub archive: PathBuf,
    /// Valore casuale che torna nell'esito: distingue l'esito di questo
    /// aggiornamento da quello di uno precedente.
    pub nonce: String,
}

impl ElevatedJob {
    /// Scrive il job in `directory` e ne restituisce il percorso.
    pub fn write_to(&self, directory: &Path) -> InstallResult<PathBuf> {
        fsops::ensure_dir(directory)?;
        let path = directory.join(JOB_FILE_NAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| InstallError::platform(error.to_string()))?;
        std::fs::write(&path, json).map_err(|error| InstallError::io(&path, error))?;
        Ok(path)
    }

    /// Legge e valida un job. Qualunque cosa fuori forma è rifiutata.
    pub fn read(path: &Path) -> InstallResult<Self> {
        let meta =
            std::fs::symlink_metadata(path).map_err(|error| InstallError::io(path, error))?;
        if !meta.is_file() || meta.len() > MAX_JOB_BYTES {
            return Err(bad_job("the job is not a small regular file"));
        }

        let raw = std::fs::read_to_string(path).map_err(|error| InstallError::io(path, error))?;
        let job: Self =
            serde_json::from_str(&raw).map_err(|error| bad_job(&format!("unreadable: {error}")))?;
        job.validate()?;
        Ok(job)
    }

    fn validate(&self) -> InstallResult<()> {
        if !crate::release::is_valid_version(self.version.trim()) {
            return Err(bad_job("the version is not valid"));
        }
        if self.nonce.is_empty() || self.nonce.len() > 64 {
            return Err(bad_job("the nonce is not valid"));
        }
        if !self.archive.is_absolute() {
            return Err(bad_job("the package path is not absolute"));
        }
        // `symlink_metadata`: un collegamento simbolico non è un pacchetto.
        let is_file = std::fs::symlink_metadata(&self.archive)
            .map(|meta| meta.is_file())
            .unwrap_or(false);
        if !is_file {
            return Err(bad_job("the package is not a regular file"));
        }
        Ok(())
    }
}

fn bad_job(reason: &str) -> InstallError {
    InstallError::platform(format!("invalid update job: {reason}"))
}

/// Esito del processo elevato, letto dal launcher.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ElevatedResult {
    pub nonce: String,
    pub ok: bool,
    /// Codice dell'errore, come [`InstallError::code`]. Vuoto se è andata.
    pub code: String,
    pub message: String,
    pub version: String,
    pub bytes: u64,
    /// Qualcosa della versione precedente non si è potuto cancellare subito.
    pub cleanup_pending: bool,
}

impl ElevatedResult {
    pub fn success(job: &ElevatedJob, report: &UpdateReport) -> Self {
        Self {
            nonce: job.nonce.clone(),
            ok: true,
            code: String::new(),
            message: String::new(),
            version: report.version.clone(),
            bytes: report.bytes,
            cleanup_pending: !report.pending_cleanup.is_empty(),
        }
    }

    pub fn failure(nonce: &str, error: &InstallError) -> Self {
        Self {
            nonce: nonce.to_string(),
            ok: false,
            code: error.code().to_string(),
            message: vk_core::redact::redact(&error.to_string()),
            ..Self::default()
        }
    }
}

/// Dove il processo elevato lascia l'esito.
pub fn result_path(install_dir: &Path) -> PathBuf {
    install_dir.join(RESULT_FILE_NAME)
}

/// Scrive l'esito. Un esito che non si riesce a scrivere non cambia niente:
/// il codice d'uscita dice comunque com'è andata.
pub fn write_result(install_dir: &Path, result: &ElevatedResult) {
    let path = result_path(install_dir);
    match serde_json::to_string(result) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                tracing::warn!(%error, "esito dell'aggiornamento elevato non scritto");
            }
        }
        Err(error) => tracing::warn!(%error, "esito dell'aggiornamento elevato non serializzato"),
    }
}

/// Legge l'esito, solo se appartiene al job con questo `nonce`.
pub fn read_result(install_dir: &Path, nonce: &str) -> Option<ElevatedResult> {
    let path = result_path(install_dir);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > MAX_JOB_BYTES {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<ElevatedResult>(&raw)
        .ok()
        .filter(|result| result.nonce == nonce)
}

/// Codici d'uscita del processo elevato: uno per ogni codice di
/// [`InstallError`], così il launcher sa cosa è successo anche se il file
/// dell'esito manca.
const EXIT_CODES: &[(&str, u32)] = &[
    ("core", 20),
    ("io", 21),
    ("manifest", 22),
    ("unsupported-target", 23),
    ("hash-mismatch", 24),
    ("invalid-signature", 25),
    ("not-updatable", 26),
    ("needs-elevation", 27),
    ("executable-not-found", 28),
    ("unsafe-path", 29),
    ("not-enough-space", 30),
    ("cancelled", 31),
    ("platform", 32),
];

/// Uscita riuscita.
pub const EXIT_OK: u32 = 0;

/// Uscita per un errore che non ha un codice suo.
const EXIT_OTHER: u32 = 40;

/// Il codice d'uscita per un errore.
pub fn exit_code(error: &InstallError) -> u32 {
    EXIT_CODES
        .iter()
        .find(|(code, _)| *code == error.code())
        .map_or(EXIT_OTHER, |(_, exit)| *exit)
}

/// L'errore da mostrare per un processo elevato uscito con `exit`.
///
/// Il messaggio viene dall'esito quando c'è; altrimenti resta il solo codice,
/// che la UI sa tradurre.
pub fn error_for_exit(exit: u32, result: Option<&ElevatedResult>) -> InstallError {
    let code = EXIT_CODES
        .iter()
        .find(|(_, value)| *value == exit)
        .map_or("platform", |(code, _)| code);
    let message = result
        .filter(|result| !result.ok && !result.message.is_empty())
        .map_or_else(
            || format!("the administrator update stopped with code {exit}"),
            |result| result.message.clone(),
        );
    InstallError::Elevated { code, message }
}

impl Installer {
    /// La parte dell'aggiornamento che gira come amministratore.
    ///
    /// `manifest_urls` sono gli indirizzi **compilati** di `install.json`:
    /// mai quelli arrivati dal job o da una configurazione scrivibile
    /// dall'utente.
    pub async fn apply_elevated_job(
        &self,
        job: &ElevatedJob,
        manifest_urls: &[String],
    ) -> InstallResult<UpdateReport> {
        let target = UpdateTarget::current()?;
        self.apply_elevated_job_to(job, &target, manifest_urls)
            .await
    }

    /// Come [`Self::apply_elevated_job`], su un'installazione data. Serve ai
    /// test.
    pub async fn apply_elevated_job_to(
        &self,
        job: &ElevatedJob,
        target: &UpdateTarget,
        manifest_urls: &[String],
    ) -> InstallResult<UpdateReport> {
        if target.access()? == Access::Elevated {
            return Err(InstallError::NeedsElevation(format!(
                "{} cannot be written even with administrator rights",
                target.install_dir.display()
            )));
        }

        // Le copie lasciate di lato dall'aggiornamento precedente: il
        // launcher, senza permessi, non ha potuto toglierle.
        update::sweep_leftovers(&target.install_dir);

        let manifest = self.fetch_manifest(manifest_urls).await?;
        self.apply_job_with_manifest(job, target, &manifest).await
    }

    async fn apply_job_with_manifest(
        &self,
        job: &ElevatedJob,
        target: &UpdateTarget,
        manifest: &ReleaseManifest,
    ) -> InstallResult<UpdateReport> {
        if manifest.version.trim() != job.version.trim() {
            return Err(InstallError::InvalidManifest(format!(
                "version {} was downloaded, but {} is published now: try again",
                job.version.trim(),
                manifest.version.trim()
            )));
        }

        let plan = update::plan(manifest, target, &self.app_version)?;
        if !plan.available {
            return Err(InstallError::NotUpdatable(
                "the published version is not newer than the installed one".into(),
            ));
        }
        if !plan.enough_space {
            return Err(InstallError::NotEnoughSpace {
                required: plan.required_bytes,
                available: plan.available_bytes,
            });
        }

        let (_, package) = manifest.select(Target::current())?;
        if package.sha256.trim().is_empty() && package.signature.trim().is_empty() {
            return Err(InstallError::InvalidManifest(
                "the package declares neither a checksum nor a signature: \
                 it cannot be installed with administrator rights"
                    .into(),
            ));
        }
        let format = package.format()?;

        // La copia sta dove l'utente non scrive: la verifica che segue vale
        // per il file che verrà srotolato, non per uno che si può cambiare
        // nel frattempo.
        let copy = update::staging_package_path(&target.install_dir, format.temp_extension());
        fsops::remove_path_best_effort(&copy);
        if let Err(error) = tokio::fs::copy(&job.archive, &copy).await {
            fsops::remove_path_best_effort(&copy);
            return Err(InstallError::io(&job.archive, error));
        }

        let progress = noop_sink();
        // In caso di errore la verifica cancella la copia da sé.
        crate::install::verify_archive(package, &copy, &progress).await?;

        let applied =
            update::apply_archive(manifest, target, &copy, &progress, &CancelToken::new());
        fsops::remove_path_best_effort(&copy);
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(dir: &Path) -> ElevatedJob {
        let archive = dir.join("pacchetto.zip");
        std::fs::write(&archive, b"zip").expect("scritto");
        ElevatedJob {
            version: "2.3.0".into(),
            archive,
            nonce: "abc123".into(),
        }
    }

    #[test]
    fn a_job_survives_the_round_trip() {
        let temp = tempfile::tempdir().expect("temp");
        let original = job(temp.path());
        let path = original
            .write_to(&temp.path().join("job"))
            .expect("scritto");
        assert_eq!(ElevatedJob::read(&path).expect("letto"), original);
    }

    #[test]
    fn a_malformed_job_is_refused() {
        let temp = tempfile::tempdir().expect("temp");
        let valid = job(temp.path());

        let cases = [
            ElevatedJob {
                version: "../../x".into(),
                ..valid.clone()
            },
            ElevatedJob {
                archive: PathBuf::from("relativo.zip"),
                ..valid.clone()
            },
            ElevatedJob {
                archive: temp.path().join("non-esiste.zip"),
                ..valid.clone()
            },
            ElevatedJob {
                archive: temp.path().to_path_buf(),
                ..valid.clone()
            },
            ElevatedJob {
                nonce: String::new(),
                ..valid.clone()
            },
        ];
        for (index, case) in cases.iter().enumerate() {
            let path = case
                .write_to(&temp.path().join(format!("job-{index}")))
                .expect("scritto");
            assert!(ElevatedJob::read(&path).is_err(), "caso {index} accettato");
        }

        let huge = temp.path().join("enorme.json");
        std::fs::write(&huge, vec![b' '; (MAX_JOB_BYTES + 1) as usize]).expect("scritto");
        assert!(
            ElevatedJob::read(&huge).is_err(),
            "un file enorme non è un job"
        );
    }

    #[test]
    fn every_error_code_has_its_own_exit_code() {
        let mut exits: Vec<u32> = EXIT_CODES.iter().map(|(_, exit)| *exit).collect();
        exits.sort_unstable();
        exits.dedup();
        assert_eq!(
            exits.len(),
            EXIT_CODES.len(),
            "due codici con la stessa uscita"
        );
        assert!(!exits.contains(&EXIT_OK));
        assert!(!exits.contains(&EXIT_OTHER));

        let error = InstallError::HashMismatch {
            expected: "a".into(),
            actual: "b".into(),
        };
        let exit = exit_code(&error);
        assert_eq!(error_for_exit(exit, None).code(), "hash-mismatch");
    }

    #[test]
    fn the_result_message_travels_back_only_for_this_job() {
        let temp = tempfile::tempdir().expect("temp");
        let error = InstallError::NotEnoughSpace {
            required: 10,
            available: 1,
        };
        write_result(temp.path(), &ElevatedResult::failure("uno", &error));

        assert!(
            read_result(temp.path(), "due").is_none(),
            "l'esito di un altro job"
        );
        let result = read_result(temp.path(), "uno").expect("esito");
        let error = error_for_exit(exit_code(&error), Some(&result));
        assert_eq!(error.code(), "not-enough-space");
        assert!(error.to_string().contains("not enough space"));
    }

    #[test]
    fn the_result_file_is_swept_like_the_staging_folder() {
        let temp = tempfile::tempdir().expect("temp");
        write_result(temp.path(), &ElevatedResult::default());
        assert_eq!(update::sweep_leftovers(temp.path()), 1);
    }
}
