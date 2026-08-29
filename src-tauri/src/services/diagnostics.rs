//! Pagina di diagnostica: stato dell'installazione e log sanitizzati.
//!
//! Porta `MainWindow.xaml.cs::RefreshDebugInfo`, aggiungendo la redazione:
//! il launcher legacy scriveva percorsi utente completi e URL con query.

use std::sync::Arc;

use crate::domain::DiagnosticEntry;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Numero massimo di righe di log restituite alla UI.
const LOG_TAIL_LINES: usize = 400;

/// Raccoglie lo stato dell'installazione.
pub async fn collect(state: &Arc<AppState>) -> AppResult<Vec<DiagnosticEntry>> {
    let settings = state.settings.read().await.clone();
    let preferences = state.preferences.read().await.clone();
    let install_state = state.install_state.read().await.clone();
    let channel = preferences.channel;
    let layout = state.layout(channel).await;
    let endpoints = state.endpoints.read().await.clone();

    let mut entries = vec![
        entry("Launcher", crate::state::LAUNCHER_VERSION, None),
        entry("Platform", crate::platform::platform_name(), None),
        entry(
            "Data folder",
            &redact_path(&state.paths.root().to_string_lossy()),
            Some(state.paths.root().is_dir()),
        ),
        entry("Channel", channel.display_name(), None),
    ];

    entries.push(entry(
        "Dolphin",
        &redact_path(&settings.dolphin_path),
        Some(!settings.dolphin_path.is_empty() && settings.dolphin().exists()),
    ));
    entries.push(entry(
        "ROM",
        &redact_path(&settings.rom_path),
        Some(!settings.rom_path.is_empty() && settings.rom().is_file()),
    ));
    entries.push(entry(
        "User folder",
        &redact_path(&settings.user_folder_path),
        Some(!settings.user_folder_path.is_empty() && settings.user_folder().is_dir()),
    ));
    entries.push(entry(
        "Modpack folder",
        &redact_path(&layout.mod_root().to_string_lossy()),
        Some(layout.mod_root().is_dir()),
    ));
    entries.push(entry(
        "Modpack installed",
        if layout.is_installed() { "yes" } else { "no" },
        Some(layout.is_installed()),
    ));

    for channel in [vk_core::Channel::Stable, vk_core::Channel::Beta] {
        let version = install_state.get(channel).version.clone();
        entries.push(entry(
            &format!("{} version", channel.display_name()),
            if version.trim().is_empty() {
                "not installed"
            } else {
                &version
            },
            None,
        ));
    }

    entries.push(entry(
        "Server",
        &host_of(&endpoints.server_base_url),
        Some(!endpoints.server_base_url.is_empty()),
    ));
    entries.push(entry(
        "Beta token",
        if state.secrets.read().await.has_beta_token() {
            "present"
        } else {
            "absent"
        },
        None,
    ));
    entries.push(entry(
        "Concorrenza download",
        &preferences.effective_concurrency().to_string(),
        None,
    ));
    entries.push(entry(
        "Backup disponibili",
        &vk_core::backup::list_backups(&state.paths.backups_dir())
            .len()
            .to_string(),
        None,
    ));

    Ok(entries)
}

/// Ultime righe del log applicativo, già sanitizzate alla scrittura.
pub async fn tail_log(state: &Arc<AppState>) -> AppResult<String> {
    let Some(path) = newest_log(state) else {
        return Ok("No log file produced yet.".into());
    };

    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| AppError::io(&path, error))?;

    let lines: Vec<&str> = raw.lines().collect();
    let start = lines.len().saturating_sub(LOG_TAIL_LINES);

    // Doppia sicurezza: il log è già redatto in scrittura, ma un log ereditato
    // da una versione precedente potrebbe non esserlo.
    Ok(vk_core::redact::redact(&lines[start..].join("\n")))
}

fn newest_log(state: &Arc<AppState>) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(state.paths.logs_dir()).ok()?;

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
}

/// Elenca i backup dei dati utente.
pub async fn backups(state: &Arc<AppState>) -> Vec<vk_core::backup::BackupSummary> {
    vk_core::backup::list_backups(&state.paths.backups_dir())
}

/// Cancella i dati del launcher. Operazione esplicita e distruttiva.
///
/// **Non** tocca la modpack installata né i salvataggi di Dolphin: quelli
/// vivono nella cartella User e appartengono all'utente.
pub async fn purge_user_data(state: &Arc<AppState>, confirmation: &str) -> AppResult<Vec<String>> {
    if confirmation.trim() != "VanzaKart" {
        return Err(AppError::BadRequest(
            "Type VanzaKart to confirm the deletion.".into(),
        ));
    }

    let mut removed = Vec::new();
    for target in [
        state.paths.cache_dir(),
        state.paths.downloads_dir(),
        state.paths.logs_dir(),
    ] {
        if target.is_dir() && std::fs::remove_dir_all(&target).is_ok() {
            removed.push(redact_path(&target.to_string_lossy()));
        }
    }

    state.paths.ensure()?;
    tracing::warn!(
        removed = removed.len(),
        "dati del launcher cancellati su richiesta"
    );
    Ok(removed)
}

fn entry(label: &str, value: &str, ok: Option<bool>) -> DiagnosticEntry {
    DiagnosticEntry {
        label: label.to_string(),
        value: if value.trim().is_empty() {
            "not configured".to_string()
        } else {
            value.to_string()
        },
        ok,
    }
}

fn redact_path(path: &str) -> String {
    vk_core::redact::redact(path)
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    async fn state_with(dir: &std::path::Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn diagnostics_cover_the_essentials() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let entries = collect(&state).await.unwrap();
        let labels: Vec<&str> = entries.iter().map(|item| item.label.as_str()).collect();

        for expected in [
            "Launcher",
            "Platform",
            "Dolphin",
            "ROM",
            "User folder",
            "Modpack installed",
            "Beta token",
        ] {
            assert!(labels.contains(&expected), "manca {expected}");
        }
    }

    #[tokio::test]
    async fn unconfigured_paths_are_labelled() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let entries = collect(&state).await.unwrap();
        let dolphin = entries.iter().find(|item| item.label == "Dolphin").unwrap();
        assert_eq!(dolphin.value, "not configured");
        assert_eq!(dolphin.ok, Some(false));
    }

    #[tokio::test]
    async fn the_token_value_is_never_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        state.secrets.write().await.beta_access_token = "SUPERSEGRETO".into();

        let entries = collect(&state).await.unwrap();
        let token = entries
            .iter()
            .find(|item| item.label == "Beta token")
            .unwrap();
        assert_eq!(token.value, "present");

        let json = serde_json::to_string(&entries).unwrap();
        assert!(!json.contains("SUPERSEGRETO"));
    }

    #[tokio::test]
    async fn the_log_tail_handles_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let tail = tail_log(&state).await.unwrap();
        assert!(tail.contains("No log file"));
    }

    #[tokio::test]
    async fn the_log_tail_is_bounded_and_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let mut content = String::new();
        for index in 0..(LOG_TAIL_LINES + 100) {
            content.push_str(&format!("riga {index}\n"));
        }
        content.push_str("scaricato da https://a.example/x.zip?token=segreto\n");
        std::fs::write(state.paths.logs_dir().join("app.log"), content).unwrap();

        let tail = tail_log(&state).await.unwrap();
        assert!(tail.lines().count() <= LOG_TAIL_LINES);
        assert!(!tail.contains("segreto"), "{tail}");
        assert!(tail.contains("https://a.example/x.zip"));
    }

    #[tokio::test]
    async fn purging_requires_the_exact_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        std::fs::write(state.paths.logs_dir().join("app.log"), "x").unwrap();

        assert!(purge_user_data(&state, "sì").await.is_err());
        assert!(state.paths.logs_dir().join("app.log").is_file());

        let removed = purge_user_data(&state, " VanzaKart ").await.unwrap();
        assert!(!removed.is_empty());
        // Le directory vengono ricreate vuote.
        assert!(state.paths.logs_dir().is_dir());
        assert!(!state.paths.logs_dir().join("app.log").exists());
    }

    #[tokio::test]
    async fn purging_never_touches_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;
        state.persist_settings().await.unwrap();

        purge_user_data(&state, "VanzaKart").await.unwrap();
        assert!(state.paths.settings_file().is_file());
    }

    #[test]
    fn the_host_is_extracted_from_a_url() {
        assert_eq!(host_of("https://sitodaking.it:8443/"), "sitodaking.it");
        assert_eq!(host_of("non un url"), "");
    }
}
