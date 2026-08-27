use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DolphinError {
    #[error("I/O su {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("percorso di Dolphin non valido: {0}")]
    InvalidDolphinPath(String),

    #[error("cartella User di Dolphin non valida: {0}")]
    InvalidUserFolder(String),

    #[error("ROM di Mario Kart Wii non valida: {0}")]
    InvalidRom(String),

    #[error("modpack non installata: manca {0}")]
    ModNotInstalled(String),

    #[error("modpack incompleta o danneggiata: {0}")]
    ModIncomplete(String),

    #[error("avvio di Dolphin fallito: {0}")]
    LaunchFailed(String),

    #[error("configurazione non valida: {0}")]
    InvalidConfiguration(String),

    #[error("JSON non valido: {0}")]
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
