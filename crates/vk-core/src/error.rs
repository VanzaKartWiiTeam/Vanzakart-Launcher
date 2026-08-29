use std::path::PathBuf;

/// Errori del dominio. Nessuna variante trasporta segreti: i messaggi che
/// includono URL passano prima da [`crate::redact::redact`].
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O: {0}")]
    PlainIo(#[from] std::io::Error),

    #[error("network error: {0}")]
    Network(String),

    #[error("every mirror failed: {0}")]
    AllMirrorsFailed(String),

    #[error("HTTP {status} for {url}")]
    HttpStatus { status: u16, url: String },

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("invalid archive: {0}")]
    InvalidArchive(String),

    #[error("unsafe archive entry: {0}")]
    UnsafeArchiveEntry(String),

    #[error("unsafe path: {0}")]
    UnsafePath(String),

    #[error("backup restore failed: {0}")]
    RestoreFailed(String),

    #[error("operation cancelled")]
    Cancelled,
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// `true` se ha senso ritentare la stessa sorgente.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::HttpStatus { status, .. } => {
                matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
            }
            Self::Io { source, .. } | Self::PlainIo(source) => !matches!(
                source.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::AlreadyExists
            ),
            _ => false,
        }
    }
}

impl From<reqwest::Error> for CoreError {
    fn from(value: reqwest::Error) -> Self {
        if let Some(status) = value.status() {
            return Self::HttpStatus {
                status: status.as_u16(),
                url: value
                    .url()
                    .map(|u| crate::redact::redact_url(u.as_str()))
                    .unwrap_or_default(),
            };
        }
        Self::Network(crate::redact::redact(&value.to_string()))
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
