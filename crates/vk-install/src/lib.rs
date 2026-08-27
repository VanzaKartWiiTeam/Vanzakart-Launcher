//! # vk-install
//!
//! Motore dell'installer e del disinstallatore del launcher VanzaKart.
//!
//! Porta `Setup/MainWindow.xaml.cs` e `Uninstaller/MainWindow.xaml.cs` del
//! launcher legacy, che erano WPF e quindi Windows-only, su Windows, macOS e
//! Linux (vedi `docs/decisions.md` §D-050).
//!
//! Il crate conosce il filesystem e — in `platform` — le API del sistema
//! operativo, perché installare *è* un'operazione di piattaforma: sono le
//! scorciatoie, le voci del menu applicazioni e la registrazione fra i
//! programmi installati a fare la differenza fra un'app copiata e un'app
//! installata. Tutto il resto (rete, hash, ZIP sicuro) arriva da `vk-core`,
//! che resta ignaro del sistema operativo.
//!
//! Il flusso è quello del setup legacy: si legge il manifest di rilascio dal
//! server, si scarica il pacchetto della piattaforma corrente, se ne verifica
//! l'impronta SHA-256, lo si estrae nella cartella scelta, si creano le
//! scorciatoie e si scrive un **registro d'installazione** ([`record`]) che il
//! disinstallatore userà per rimuovere esattamente ciò che è stato creato,
//! invece di indovinare i percorsi come faceva il legacy.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod discovery;
pub mod error;
pub mod fsops;
pub mod install;
pub mod paths;
pub mod payload;
pub mod platform;
pub mod record;
pub mod release;
pub mod target;
pub mod uninstall;

pub use error::{InstallError, InstallResult};
pub use install::{InstallMode, InstallOptions, InstallReport, Installer};
pub use record::{Artifact, ArtifactKind, InstallRecord};
pub use release::{PackageFormat, ReleaseManifest, ReleasePackage};
pub use target::Target;
pub use uninstall::{RemovalItem, UninstallOptions, UninstallReport};

/// Nome del prodotto, usato ovunque compaia all'utente.
pub const PRODUCT_NAME: &str = "VanzaKart Launcher";

/// Editore, mostrato fra i programmi installati.
pub const PUBLISHER: &str = "VanzaKart";

/// Identificatore del bundle, uguale a quello di `tauri.conf.json`.
///
/// Su Windows è anche il nome della chiave di disinstallazione: è la stessa
/// che userebbe l'installer NSIS di Tauri, così i due non si sdoppiano fra i
/// programmi installati (§D-052).
pub const BUNDLE_IDENTIFIER: &str = "it.sitodaking.vanzakart.launcher";

/// User-Agent delle richieste HTTP dell'installer.
pub fn user_agent(version: &str) -> String {
    format!("VanzaKartSetup/{version} (Tauri)")
}
