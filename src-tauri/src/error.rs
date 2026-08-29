use std::path::PathBuf;

use serde::Serialize;

/// Errore applicativo. Viene serializzato verso il frontend in forma
/// **già sanitizzata**: nessun percorso utente completo, nessun token.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Core(#[from] vk_core::CoreError),

    #[error("{0}")]
    Dolphin(#[from] vk_dolphin::DolphinError),

    #[error("{0}")]
    Save(#[from] vk_save::SaveError),

    #[error("storage: {0}")]
    Storage(String),

    #[error("incomplete configuration: {0}")]
    Configuration(String),

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("an operation is already running")]
    Busy,

    #[error("operation cancelled")]
    Cancelled,

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Codice stabile, usato dal frontend per decidere dove indirizzare
    /// l'utente senza dover interpretare il testo del messaggio.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::Core(vk_core::CoreError::HashMismatch { .. }) => "hash-mismatch",
            Self::Core(vk_core::CoreError::AllMirrorsFailed(_)) => "network",
            Self::Core(vk_core::CoreError::Network(_)) => "network",
            Self::Core(vk_core::CoreError::HttpStatus { .. }) => "network",
            Self::Core(vk_core::CoreError::Cancelled) | Self::Cancelled => "cancelled",
            Self::Core(_) => "core",
            Self::Dolphin(vk_dolphin::DolphinError::InvalidDolphinPath(_)) => "dolphin-path",
            Self::Dolphin(vk_dolphin::DolphinError::InvalidRom(_)) => "rom-path",
            Self::Dolphin(vk_dolphin::DolphinError::InvalidUserFolder(_)) => "user-folder",
            Self::Dolphin(vk_dolphin::DolphinError::ModNotInstalled(_)) => "mod-not-installed",
            Self::Dolphin(vk_dolphin::DolphinError::ModIncomplete(_)) => "mod-incomplete",
            Self::Dolphin(_) => "dolphin",
            Self::Save(_) => "save",
            Self::Storage(_) => "storage",
            Self::Configuration(_) => "configuration",
            Self::BadRequest(_) => "bad-request",
            Self::Busy => "busy",
            Self::Internal(_) => "internal",
        }
    }
}

/// Forma dell'errore vista dal frontend.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorPayload {
            code: self.code().to_string(),
            message: vk_core::redact::redact(&self.to_string()),
        }
        .serialize(serializer)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_route_the_user_to_the_right_page() {
        assert_eq!(
            AppError::Dolphin(vk_dolphin::DolphinError::InvalidRom("x".into())).code(),
            "rom-path"
        );
        assert_eq!(
            AppError::Dolphin(vk_dolphin::DolphinError::ModNotInstalled("x".into())).code(),
            "mod-not-installed"
        );
        assert_eq!(
            AppError::Dolphin(vk_dolphin::DolphinError::ModIncomplete("x".into())).code(),
            "mod-incomplete"
        );
        assert_eq!(
            AppError::Core(vk_core::CoreError::AllMirrorsFailed("x".into())).code(),
            "network"
        );
        assert_eq!(AppError::Busy.code(), "busy");
    }

    #[test]
    fn serialization_redacts_urls() {
        let error = AppError::Core(vk_core::CoreError::AllMirrorsFailed(
            "https://a.example/x.zip?token=segreto -> 404".into(),
        ));
        let json = serde_json::to_string(&error).unwrap();

        assert!(json.contains("\"code\":\"network\""));
        assert!(!json.contains("segreto"), "{json}");
    }

    #[test]
    fn serialization_exposes_code_and_message_only() {
        let json = serde_json::to_value(AppError::Busy).unwrap();
        let object = json.as_object().unwrap();
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("code"));
        assert!(object.contains_key("message"));
    }
}
