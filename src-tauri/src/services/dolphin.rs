//! Rilevamento di Dolphin, impostazioni e backup della configurazione.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vk_dolphin::settings::{ConfigPaths, DolphinSettings};

use crate::domain::SettingsView;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Vista delle impostazioni per la UI, con la validazione già risolta.
pub async fn settings_view(state: &Arc<AppState>) -> AppResult<SettingsView> {
    let settings = state.settings.read().await.clone();
    let preferences = state.preferences.read().await.clone();

    let dolphin = settings.dolphin();
    let rom = settings.rom();
    let user_folder = settings.user_folder();

    Ok(SettingsView {
        dolphin_valid: !settings.dolphin_path.is_empty() && dolphin.exists(),
        rom_valid: !settings.rom_path.is_empty()
            && rom.is_file()
            && vk_dolphin::riivolution::has_rom_extension(&rom),
        user_folder_valid: !settings.user_folder_path.is_empty() && user_folder.is_dir(),
        mod_folder: settings
            .mod_folder(&state.paths)
            .to_string_lossy()
            .to_string(),
        dolphin_path: settings.dolphin_path,
        rom_path: settings.rom_path,
        user_folder_path: settings.user_folder_path,
        controller_mode: settings.controller_mode,
        detected_user_folders: detect_user_folders(&dolphin)
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        separate_savegame: preferences.separate_savegame,
        my_stuff_enabled: preferences.mod_option_choice == 2,
        auto_check_updates: preferences.auto_check_updates,
        download_concurrency: preferences.effective_concurrency(),
    })
}

/// Cartelle User candidate realmente esistenti.
pub fn detect_user_folders(configured_dolphin: &Path) -> Vec<PathBuf> {
    let probe = crate::platform::path_probe();
    let configured = (!configured_dolphin.as_os_str().is_empty()).then_some(configured_dolphin);

    vk_dolphin::paths::user_folder_candidates(&probe, configured)
        .into_iter()
        .filter(|path| vk_dolphin::paths::looks_like_user_folder(path))
        .take(8)
        .collect()
}

/// Eseguibili di Dolphin trovati sul sistema.
pub fn detect_executables() -> Vec<PathBuf> {
    vk_dolphin::paths::executable_candidates(&crate::platform::path_probe())
        .into_iter()
        .take(8)
        .collect()
}

/// Applica e valida i percorsi scelti dall'utente.
///
/// Ogni percorso arriva da un dialogo nativo, ma viene comunque rivalidato:
/// il frontend non è una fonte attendibile.
pub async fn update_paths(
    state: &Arc<AppState>,
    dolphin_path: Option<String>,
    rom_path: Option<String>,
    user_folder_path: Option<String>,
) -> AppResult<SettingsView> {
    {
        let mut settings = state.settings.write().await;

        if let Some(value) = dolphin_path {
            let path = PathBuf::from(value.trim());
            if !value.trim().is_empty() && !path.exists() {
                return Err(AppError::BadRequest(
                    "il percorso di Dolphin indicato non esiste".into(),
                ));
            }
            settings.dolphin_path = value;
        }

        if let Some(value) = rom_path {
            let path = PathBuf::from(value.trim());
            if !value.trim().is_empty() {
                if !path.is_file() {
                    return Err(AppError::BadRequest("il file della ROM non esiste".into()));
                }
                if !vk_dolphin::riivolution::has_rom_extension(&path) {
                    return Err(AppError::BadRequest(
                        "estensione della ROM non supportata".into(),
                    ));
                }
            }
            settings.rom_path = value;
        }

        if let Some(value) = user_folder_path {
            let path = PathBuf::from(value.trim());
            if !value.trim().is_empty() && !path.is_dir() {
                return Err(AppError::BadRequest(
                    "la cartella User indicata non esiste".into(),
                ));
            }
            settings.user_folder_path = value;
        }

        *settings = settings.clone().normalized();
    }

    state.persist_settings().await?;
    settings_view(state).await
}

/// Legge le impostazioni di Dolphin dagli INI.
pub async fn load_dolphin_settings(state: &Arc<AppState>) -> AppResult<DolphinSettings> {
    let user_folder = state.settings.read().await.user_folder();
    let mut model = DolphinSettings::load(&user_folder);
    model.dolphin_executable_path = state.settings.read().await.dolphin_path.clone();
    model.modpack_path = state.settings.read().await.rom_path.clone();
    Ok(model)
}

/// Scrive le impostazioni di Dolphin negli INI.
pub async fn save_dolphin_settings(
    state: &Arc<AppState>,
    model: &DolphinSettings,
) -> AppResult<()> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;
    model.save(&user_folder)?;
    tracing::info!("impostazioni di Dolphin salvate");
    Ok(())
}

/// Applica il preset "VanzaKart Recommended" e salva.
pub async fn optimize(state: &Arc<AppState>, screen_width: u32) -> AppResult<DolphinSettings> {
    let mut model = load_dolphin_settings(state).await?;
    model.optimize_for_vanzakart(screen_width);
    save_dolphin_settings(state, &model).await?;
    Ok(model)
}

/// Ripristina i default di una categoria e salva.
pub async fn reset_category(state: &Arc<AppState>, category: &str) -> AppResult<DolphinSettings> {
    let mut model = load_dolphin_settings(state).await?;
    model.reset_category(category);
    save_dolphin_settings(state, &model).await?;
    Ok(model)
}

/// Crea un backup ZIP della cartella `Config` di Dolphin.
///
/// Nel launcher legacy questo pulsante era un segnaposto: qui è implementato.
pub async fn backup_config(state: &Arc<AppState>) -> AppResult<String> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    let config = ConfigPaths::from_user_folder(&user_folder).config_dir;
    if !config.is_dir() {
        return Err(AppError::Configuration(
            "la cartella Config di Dolphin non esiste".into(),
        ));
    }

    let destination = state.paths.backups_dir().join(format!(
        "dolphin-config-{}.zip",
        vk_core::fsx::backup_timestamp()
    ));
    tokio::fs::create_dir_all(state.paths.backups_dir())
        .await
        .map_err(|error| AppError::io(state.paths.backups_dir(), error))?;

    let target = destination.clone();
    tokio::task::spawn_blocking(move || zip_directory(&config, &target))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;

    tracing::info!("configurazione di Dolphin salvata in un backup");
    Ok(destination.to_string_lossy().to_string())
}

/// Ripristina un backup della configurazione.
///
/// La `Config` esistente viene prima spostata di lato, non cancellata: se il
/// ripristino fallisce l'utente non resta senza configurazione.
pub async fn restore_config(state: &Arc<AppState>, archive: &Path) -> AppResult<()> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    if !archive.is_file() {
        return Err(AppError::BadRequest(
            "il file di backup indicato non esiste".into(),
        ));
    }

    let config = ConfigPaths::from_user_folder(&user_folder).config_dir;
    if config.is_dir() {
        let aside = config.with_file_name(format!(
            "Config.pre-restore-{}",
            vk_core::fsx::backup_timestamp()
        ));
        tokio::fs::rename(&config, &aside)
            .await
            .map_err(|error| AppError::io(&config, error))?;
        tracing::info!("configurazione precedente messa da parte prima del ripristino");
    }

    let archive = archive.to_path_buf();
    let destination = config.clone();
    tokio::task::spawn_blocking(move || {
        vk_core::zipx::extract_safe(
            &archive,
            &destination,
            &vk_core::zipx::ExtractOptions::default(),
            &vk_core::progress::noop_sink(),
            &vk_core::progress::CancelToken::new(),
        )
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))??;

    Ok(())
}

/// Elimina i file `GameSettings/RMC*.ini`, che possono sovrascrivere le
/// impostazioni scelte nel launcher.
pub async fn delete_game_settings(state: &Arc<AppState>) -> AppResult<Vec<String>> {
    let user_folder = state.settings.read().await.user_folder();
    require_user_folder(&user_folder)?;

    let directory = user_folder.join("GameSettings");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(Vec::new());
    };

    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_mario_kart = name.to_ascii_uppercase().starts_with("RMC")
            && name.to_ascii_lowercase().ends_with(".ini");
        if is_mario_kart && std::fs::remove_file(entry.path()).is_ok() {
            removed.push(name);
        }
    }

    Ok(removed)
}

fn require_user_folder(user_folder: &Path) -> AppResult<()> {
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Err(AppError::Configuration(
            "Seleziona prima la cartella User di Dolphin.".into(),
        ));
    }
    Ok(())
}

/// Comprime una directory in un archivio ZIP, preservando i percorsi relativi.
fn zip_directory(source: &Path, destination: &Path) -> AppResult<()> {
    use std::io::Write;

    let file =
        std::fs::File::create(destination).map_err(|error| AppError::io(destination, error))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    for path in vk_core::fsx::list_files_recursive(source) {
        let relative = vk_core::fsx::relative_slash(source, &path);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        writer
            .start_file(relative, options)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        writer
            .write_all(&bytes)
            .map_err(|error| AppError::io(destination, error))?;
    }

    writer
        .finish()
        .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;

    async fn state_with(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_settings_view_reports_invalid_paths() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let view = settings_view(&state).await.unwrap();
        assert!(!view.dolphin_valid);
        assert!(!view.rom_valid);
        assert!(!view.user_folder_valid);
        assert!(view.mod_folder.ends_with("Modpack"));
    }

    #[tokio::test]
    async fn updating_paths_validates_each_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let missing = update_paths(&state, Some("/non/esiste".into()), None, None)
            .await
            .unwrap_err();
        assert_eq!(missing.code(), "bad-request");

        let dolphin = dir.path().join("Dolphin.exe");
        std::fs::write(&dolphin, b"").unwrap();
        let view = update_paths(
            &state,
            Some(dolphin.to_string_lossy().to_string()),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(view.dolphin_valid);
    }

    #[tokio::test]
    async fn a_rom_with_a_wrong_extension_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let rom = dir.path().join("appunti.txt");
        std::fs::write(&rom, b"").unwrap();

        let error = update_paths(&state, None, Some(rom.to_string_lossy().to_string()), None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("estensione"));
    }

    #[tokio::test]
    async fn clearing_a_path_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let view = update_paths(&state, Some(String::new()), None, None)
            .await
            .unwrap();
        assert!(view.dolphin_path.is_empty());
    }

    #[tokio::test]
    async fn dolphin_settings_require_a_user_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let error = save_dolphin_settings(&state, &DolphinSettings::default())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "configuration");

        assert_eq!(
            backup_config(&state).await.unwrap_err().code(),
            "configuration"
        );
    }

    #[tokio::test]
    async fn dolphin_settings_round_trip_through_the_ini() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let mut model = load_dolphin_settings(&state).await.unwrap();
        model.audio_volume = 33;
        model.internal_resolution = 5;
        save_dolphin_settings(&state, &model).await.unwrap();

        let reloaded = load_dolphin_settings(&state).await.unwrap();
        assert_eq!(reloaded.audio_volume, 33);
        assert_eq!(reloaded.internal_resolution, 5);
    }

    #[tokio::test]
    async fn optimizing_writes_the_preset() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let model = optimize(&state, 2560).await.unwrap();
        assert_eq!(model.internal_resolution, 4);
        assert_eq!(
            load_dolphin_settings(&state)
                .await
                .unwrap()
                .performance_preset,
            "VanzaKart Recommended"
        );
    }

    #[tokio::test]
    async fn config_backup_and_restore_preserve_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        let config = user.join("Config");
        std::fs::create_dir_all(config.join("Profiles/GCPad")).unwrap();
        std::fs::write(config.join("Dolphin.ini"), "[Core]\nA = 1\n").unwrap();
        std::fs::write(
            config.join("Profiles/GCPad/Corsa.ini"),
            "[Profile]\nB = 2\n",
        )
        .unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let archive = backup_config(&state).await.unwrap();
        assert!(Path::new(&archive).is_file());

        // Il ripristino non deve cancellare: sposta di lato e riscrive.
        std::fs::write(config.join("Dolphin.ini"), "[Core]\nA = 999\n").unwrap();
        restore_config(&state, Path::new(&archive)).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(config.join("Dolphin.ini")).unwrap(),
            "[Core]\nA = 1\n"
        );
        assert!(config.join("Profiles/GCPad/Corsa.ini").is_file());
        // La copia precedente esiste ancora.
        assert!(std::fs::read_dir(&user)
            .unwrap()
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("Config.pre-restore-")));
    }

    #[tokio::test]
    async fn only_mario_kart_game_settings_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        let game_settings = user.join("GameSettings");
        std::fs::create_dir_all(&game_settings).unwrap();
        std::fs::write(game_settings.join("RMCP01.ini"), b"").unwrap();
        std::fs::write(game_settings.join("RMCE01.ini"), b"").unwrap();
        std::fs::write(game_settings.join("SOUE01.ini"), b"").unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        let removed = delete_game_settings(&state).await.unwrap();

        assert_eq!(removed.len(), 2);
        assert!(game_settings.join("SOUE01.ini").is_file());
    }
}
