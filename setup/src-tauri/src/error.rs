//! Errori verso il frontend.
//!
//! Stessa forma del launcher: `{ code, message }`. Il codice serve alla UI per
//! distinguere i casi che sa gestire (spazio insufficiente, launcher aperto,
//! annullamento) da un guasto generico.

use serde::Serialize;
use vk_install::InstallError;

pub type SetupResult<T> = Result<T, SetupError>;

#[derive(Debug)]
pub struct SetupError {
    code: String,
    message: String,
}

impl SetupError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn busy() -> Self {
        Self::new("busy", "An operation is already running.")
    }

    pub fn code(&self) -> &str {
        &self.code
    }
}

impl From<InstallError> for SetupError {
    fn from(error: InstallError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Serialize for SetupError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Payload<'a> {
            code: &'a str,
            message: &'a str,
        }
        Payload {
            code: &self.code,
            message: &self.message,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_error_keeps_its_code() {
        let error: SetupError = InstallError::NotInstalled.into();
        assert_eq!(error.code(), "not-installed");
        assert!(error
            .to_string()
            .contains("no VanzaKart Launcher installation"));
    }

    #[test]
    fn the_frontend_sees_code_and_message() {
        let json = serde_json::to_string(&SetupError::busy()).expect("json");
        assert!(json.contains("\"code\":\"busy\""), "{json}");
        assert!(json.contains("message"), "{json}");
    }
}
