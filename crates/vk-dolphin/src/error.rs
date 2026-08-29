use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DolphinError {
    #[error("I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid Dolphin path: {0}")]
    InvalidDolphinPath(String),

    #[error("invalid Dolphin User folder: {0}")]
    InvalidUserFolder(String),

    #[error("invalid Mario Kart Wii ROM: {0}")]
    InvalidRom(String),

    #[error("modpack not installed: {0} is missing")]
    ModNotInstalled(String),

    #[error("modpack incomplete or damaged: {0}")]
    ModIncomplete(String),

    #[error("Dolphin could not be started: {0}")]
    LaunchFailed(String),

    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl DolphinError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type DolphinResult<T> = Result<T, DolphinError>;
