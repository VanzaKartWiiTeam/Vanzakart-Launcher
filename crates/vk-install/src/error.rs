//! Errori dell'installer.
//!
//! Ogni variante racconta cosa non è riuscito e su quale percorso, perché il
//! messaggio finisce tale e quale nel registro dell'installazione mostrato
//! all'utente.

use std::path::{Path, PathBuf};

use serde::Serialize;

pub type InstallResult<T> = Result<T, InstallError>;

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{0}")]
    Core(#[from] vk_core::CoreError),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid release manifest: {0}")]
    InvalidManifest(String),

    #[error("no package available for {0}")]
    UnsupportedTarget(String),

    #[error("package checksum mismatch: expected {expected}, computed {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("launcher executable not found after extracting {0}")]
    ExecutableNotFound(PathBuf),

    #[error("path not suitable for an installation: {0}")]
    UnsafePath(String),

    #[error("not enough space: {required} bytes needed, {available} left")]
    NotEnoughSpace { required: u64, available: u64 },

    #[error("no VanzaKart Launcher installation found")]
    NotInstalled,

    #[error("the launcher is running: close it and try again")]
    LauncherRunning,

    #[error("operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Platform(String),
}

impl InstallError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn platform(message: impl Into<String>) -> Self {
        Self::Platform(message.into())
    }

    /// Codice stabile, usato dalla UI per distinguere i casi trattabili
    /// (spazio insufficiente, annullamento) da un errore generico.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Core(_) => "core",
            Self::Io { .. } => "io",
            Self::InvalidManifest(_) => "manifest",
            Self::UnsupportedTarget(_) => "unsupported-target",
            Self::HashMismatch { .. } => "hash-mismatch",
            Self::ExecutableNotFound(_) => "executable-not-found",
            Self::UnsafePath(_) => "unsafe-path",
            Self::NotEnoughSpace { .. } => "not-enough-space",
            Self::NotInstalled => "not-installed",
            Self::LauncherRunning => "launcher-running",
            Self::Cancelled => "cancelled",
            Self::Platform(_) => "platform",
        }
    }

    /// `true` quando l'operazione è stata fermata dall'utente: la UI non deve
    /// mostrarla come un guasto.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled) || matches!(self, Self::Core(vk_core::CoreError::Cancelled))
    }
}

/// Forma serializzata verso il frontend, uguale a quella del launcher.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl From<&InstallError> for ApiError {
    fn from(error: &InstallError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}
