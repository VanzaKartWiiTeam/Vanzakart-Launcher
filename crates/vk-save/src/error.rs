use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("I/O su {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("salvataggio non valido: {0}")]
    InvalidSave(String),

    #[error("dati Mii non validi: {0}")]
    InvalidMii(String),

    #[error("friend code non valido: {0}")]
    InvalidFriendCode(String),

    #[error("checksum non corrispondente: atteso {expected:#010X}, calcolato {actual:#010X}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("operazione di scrittura non abilitata: {0}")]
    WriteNotEnabled(String),
}

impl SaveError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type SaveResult<T> = Result<T, SaveError>;
