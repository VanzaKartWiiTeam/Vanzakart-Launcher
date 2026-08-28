//! Stato dell'applicazione di setup.
//!
//! Tiene il motore, il manifest scaricato una volta sola e il token di
//! annullamento dell'operazione in corso. Gli indirizzi del server stanno qui
//! e non nel frontend, come nel launcher (§D-005).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use vk_core::endpoints::EndpointsInfo;
use vk_core::progress::CancelToken;
use vk_install::release::ReleaseManifest;
use vk_install::{InstallError, InstallResult, Installer};

/// Versione dell'installer, dal `Cargo.toml`.
pub const SETUP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// L'unico indirizzo che l'installer deve conoscere per forza: da qui legge
/// tutti gli altri. È l'elenco degli indirizzi del progetto, lo stesso che
/// usa il launcher.
const ENDPOINTS_URL: &str = "https://sitodaking.it:8443/Launcher/endpoints.json";

/// Ricadute, usate solo quando `endpoints.json` non risponde o non dichiara
/// la chiave. Devono restare allineate al file sul server.
const INSTALL_MANIFEST_URL: &str = "https://sitodaking.it:8443/Launcher/install.json";
const DOWNLOAD_PAGE_URL: &str = "https://vwfc.sitodaking.it/";

/// In quale veste è stato avviato il programma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Uninstall => "uninstall",
        }
    }

    /// Riconosce il disinstallatore dagli argomenti o dal proprio nome.
    ///
    /// Il binario è uno solo: l'installer copiato nella cartella
    /// d'installazione si chiama "VanzaKart Uninstaller" e parte già in
    /// modalità rimozione anche se qualcuno lo avvia con un doppio clic,
    /// senza argomenti (§D-053).
    pub fn detect<S: AsRef<str>>(arguments: &[S], executable_name: &str) -> Self {
        let asked = arguments
            .iter()
            .any(|argument| argument.as_ref().eq_ignore_ascii_case("--uninstall"));
        let named = executable_name.to_ascii_lowercase().contains("uninstall");

        if asked || named {
            Self::Uninstall
        } else {
            Self::Install
        }
    }
}

/// `true` se fra gli argomenti c'è `--quiet`: disinstallazione senza finestra,
/// come chiede `QuietUninstallString` nel registro di Windows.
pub fn wants_quiet<S: AsRef<str>>(arguments: &[S]) -> bool {
    arguments
        .iter()
        .any(|argument| argument.as_ref().eq_ignore_ascii_case("--quiet"))
}

#[derive(Debug)]
pub struct SetupState {
    pub mode: Mode,
    pub installer: Installer,
    manifest: Mutex<Option<ReleaseManifest>>,
    endpoints: Mutex<Option<EndpointsInfo>>,
    cancel: Mutex<CancelToken>,
    busy: AtomicBool,
}

impl SetupState {
    pub fn new(mode: Mode, icon: Option<PathBuf>) -> InstallResult<Self> {
        Ok(Self {
            mode,
            installer: build_installer(icon)?,
            manifest: Mutex::new(None),
            endpoints: Mutex::new(None),
            cancel: Mutex::new(CancelToken::new()),
            busy: AtomicBool::new(false),
        })
    }

    /// `endpoints.json`, letto una volta sola e tenuto da parte.
    ///
    /// È l'elenco degli indirizzi del progetto: tenerli lì invece che dentro
    /// l'installer vuol dire poter spostare un file sul server senza dover
    /// ricompilare e ridistribuire l'installer a chi lo ha già scaricato.
    /// Se il file non risponde si va avanti con le ricadute compilate.
    async fn endpoints(&self) -> EndpointsInfo {
        if let Some(gia_letto) = self.endpoints.lock().ok().and_then(|slot| slot.clone()) {
            return gia_letto;
        }

        let indirizzo = format!("{ENDPOINTS_URL}?t={}", vk_core::now_millis());
        let letti = match self.installer.downloader().get_string(&indirizzo).await {
            Ok(raw) => serde_json::from_str::<EndpointsInfo>(vk_core::json::strip_leading_noise(
                &raw,
            ))
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "endpoints.json illeggibile: uso gli indirizzi compilati");
                EndpointsInfo::default()
            }),
            Err(error) => {
                tracing::warn!(%error, "endpoints.json non raggiungibile: uso gli indirizzi compilati");
                EndpointsInfo::default()
            }
        };

        if let Ok(mut slot) = self.endpoints.lock() {
            *slot = Some(letti.clone());
        }
        letti
    }

    /// Indirizzi da cui leggere il manifest, in ordine di tentativo.
    ///
    /// Prima quelli dichiarati dal server, poi la ricaduta compilata: se il
    /// manifest trasloca, basta aggiornare `endpoints.json`.
    pub async fn manifest_urls(&self) -> Vec<String> {
        let endpoints = self.endpoints().await;
        let mut urls: Vec<String> = std::iter::once(endpoints.launcher_install_url.clone())
            .chain(endpoints.launcher_install_mirrors.clone())
            .filter_map(solo_https)
            .collect();
        urls.push(INSTALL_MANIFEST_URL.to_string());

        vk_core::net::dedupe_urls(&urls)
    }

    /// Pagina dei download del sito, da `endpoints.json`.
    pub async fn download_page_url(&self) -> String {
        solo_https(self.endpoints().await.download_page_url.clone())
            .unwrap_or_else(|| DOWNLOAD_PAGE_URL.to_string())
    }

    /// Manifest già scaricato, se c'è.
    pub fn manifest(&self) -> Option<ReleaseManifest> {
        self.manifest.lock().ok().and_then(|value| value.clone())
    }

    pub fn store_manifest(&self, manifest: ReleaseManifest) {
        if let Ok(mut slot) = self.manifest.lock() {
            *slot = Some(manifest);
        }
    }

    /// Il manifest, scaricandolo se serve.
    pub async fn require_manifest(&self) -> InstallResult<ReleaseManifest> {
        if let Some(manifest) = self.manifest() {
            return Ok(manifest);
        }
        let urls = self.manifest_urls().await;
        let manifest = self.installer.fetch_manifest(&urls).await?;
        self.store_manifest(manifest.clone());
        Ok(manifest)
    }

    /// Prende in carico un'operazione lunga. Restituisce un token nuovo e una
    /// guardia che rilascia l'occupato anche in caso di errore.
    pub fn begin(&self) -> Option<(CancelToken, BusyGuard<'_>)> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return None;
        }
        let token = CancelToken::new();
        if let Ok(mut slot) = self.cancel.lock() {
            *slot = token.clone();
        }
        Some((token, BusyGuard { state: self }))
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Annulla l'operazione in corso.
    pub fn cancel(&self) {
        if let Ok(token) = self.cancel.lock() {
            token.cancel();
        }
    }
}

/// Rilascia il flag "occupato" quando l'operazione finisce, comunque finisca.
#[derive(Debug)]
pub struct BusyGuard<'a> {
    state: &'a SetupState,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.state.busy.store(false, Ordering::SeqCst);
    }
}

/// Costruisce il motore.
///
/// Nelle build di sviluppo `VK_SETUP_ALLOW_LOOPBACK` permette di scaricare da
/// un server locale in chiaro: è l'unico modo di provare l'installazione vera,
/// finestra compresa, senza pubblicare niente. La riga è dentro
/// `#[cfg(debug_assertions)]`, quindi in un binario di rilascio la variabile
/// non esiste nemmeno e resta solo `https` (§D-004).
fn build_installer(icon: Option<PathBuf>) -> InstallResult<Installer> {
    let installer = Installer::new(SETUP_VERSION, icon)?;

    #[cfg(debug_assertions)]
    if std::env::var_os("VK_SETUP_ALLOW_LOOPBACK").is_some_and(|value| !value.is_empty()) {
        tracing::warn!("http su loopback abilitato: build di sviluppo");
        return Ok(installer.with_downloader(
            vk_core::net::Downloader::new(&vk_install::user_agent(SETUP_VERSION))?
                .with_loopback_http(true),
        ));
    }

    Ok(installer)
}

/// Accetta un indirizzo solo se è `https` (§D-004): un `endpoints.json`
/// manomesso non deve poter dirottare l'installer su http.
fn solo_https(url: String) -> Option<String> {
    let pulito = url.trim().to_string();
    pulito
        .to_ascii_lowercase()
        .starts_with("https://")
        .then_some(pulito)
}

/// Errore da restituire quando manca il manifest e non c'è rete.
pub fn offline_error() -> InstallError {
    InstallError::InvalidManifest(
        "impossibile leggere l'elenco dei pacchetti dal server: controlla la connessione".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uninstaller_recognises_itself_by_name() {
        assert_eq!(
            Mode::detect::<&str>(&[], "VanzaKart Uninstaller.exe"),
            Mode::Uninstall
        );
        assert_eq!(
            Mode::detect::<&str>(&[], "vanzakart-uninstaller"),
            Mode::Uninstall
        );
    }

    #[test]
    fn the_flag_wins_over_the_name() {
        assert_eq!(
            Mode::detect(&["--uninstall"], "VanzaKart Setup.exe"),
            Mode::Uninstall
        );
    }

    #[test]
    fn without_either_it_installs() {
        assert_eq!(
            Mode::detect(&["--verbose"], "VanzaKart Setup.exe"),
            Mode::Install
        );
        assert_eq!(Mode::Install.as_str(), "install");
    }

    #[test]
    fn quiet_is_recognised_anywhere_in_the_arguments() {
        assert!(wants_quiet(&["--uninstall", "--quiet"]));
        assert!(!wants_quiet(&["--uninstall"]));
    }

    #[test]
    fn only_one_long_operation_at_a_time() {
        let state = SetupState::new(Mode::Install, None).expect("stato");
        let first = state.begin().expect("prima operazione");
        assert!(state.is_busy());
        assert!(state.begin().is_none());

        drop(first);
        assert!(!state.is_busy());
        assert!(state.begin().is_some());
    }

    #[test]
    fn cancelling_marks_the_current_token() {
        let state = SetupState::new(Mode::Install, None).expect("stato");
        let (token, _guard) = state.begin().expect("operazione");
        assert!(!token.is_cancelled());
        state.cancel();
        assert!(token.is_cancelled());
    }
}
