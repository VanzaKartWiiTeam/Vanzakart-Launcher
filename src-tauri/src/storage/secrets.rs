//! Segreti del launcher: al momento solo il token di accesso beta.
//!
//! Vive in un file separato dalle preferenze e **non viene mai inviato al
//! frontend**: la UI riceve solo `has_token` e le ultime cifre mascherate.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::paths::AppPaths;
use crate::storage::settings::read_json;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Secrets {
    pub beta_access_token: String,
}

impl Secrets {
    pub fn has_beta_token(&self) -> bool {
        !self.beta_access_token.trim().is_empty()
    }

    /// Forma mostrabile nella UI: `ab***`.
    pub fn masked_beta_token(&self) -> String {
        vk_core::redact::redact_token(&self.beta_access_token)
    }
}

pub async fn load(paths: &AppPaths) -> AppResult<Secrets> {
    Ok(read_json(&paths.secrets_file()).await.unwrap_or_default())
}

pub async fn save(paths: &AppPaths, secrets: &Secrets) -> AppResult<()> {
    let path = paths.secrets_file();
    vk_core::fsx::write_json_atomic(&path, secrets).await?;
    restrict_permissions(&path);
    Ok(())
}

/// Su unix il file dei segreti è leggibile solo dal proprietario.
fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_never_reveals_the_token() {
        let secrets = Secrets {
            beta_access_token: "SUPERSEGRETO123".into(),
        };
        let masked = secrets.masked_beta_token();

        assert_eq!(masked, "SU***");
        assert!(!masked.contains("SEGRETO"));
        assert!(secrets.has_beta_token());
    }

    #[test]
    fn a_blank_token_counts_as_absent() {
        let secrets = Secrets {
            beta_access_token: "   ".into(),
        };
        assert!(!secrets.has_beta_token());
        assert!(secrets.masked_beta_token().is_empty());
    }

    #[tokio::test]
    async fn secrets_round_trip_and_stay_out_of_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();

        let secrets = Secrets {
            beta_access_token: "token-di-prova".into(),
        };
        save(&paths, &secrets).await.unwrap();

        assert_eq!(load(&paths).await.unwrap(), secrets);
        assert!(!paths.preferences_file().exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_secrets_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        paths.ensure().unwrap();
        save(
            &paths,
            &Secrets {
                beta_access_token: "x".into(),
            },
        )
        .await
        .unwrap();

        let mode = std::fs::metadata(paths.secrets_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
