use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid save file: {0}")]
    InvalidSave(String),

    #[error("invalid Mii data: {0}")]
    InvalidMii(String),

    #[error("invalid friend code: {0}")]
    InvalidFriendCode(String),

    #[error("checksum mismatch: expected {expected:#010X}, computed {actual:#010X}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("write operation not enabled: {0}")]
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
