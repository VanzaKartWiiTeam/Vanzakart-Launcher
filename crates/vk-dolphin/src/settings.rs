//! Impostazioni di Dolphin e loro mappatura sugli INI.
//!
//! Porta `Launcher/Models/DolphinSettingsModel.cs` e
//! `Launcher/Services/DolphinSettingsManager.cs`. La scrittura passa da
//! [`crate::ini::update_ini`], quindi non distrugge le chiavi che il launcher
//! non gestisce.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::DolphinResult;
use crate::ini::{self, IniData, IniUpdates};

/// Preset di performance riconosciuti (il valore è persistito in
/// `[VanzaKartLauncher] PerformancePreset` dentro `Dolphin.ini`).
pub const PERFORMANCE_PRESETS: &[&str] = &[
    "Low-End",
    "Balanced",
    "High-Performance",
    "VanzaKart Recommended",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DolphinSettings {
    // --- VIDEO ---
    pub gfx_backend: String,
    pub internal_resolution: i64,
    pub fullscreen: bool,
    pub aspect_ratio: i64,
    pub vsync: bool,
    pub anti_aliasing: i64,
    pub anisotropic_filtering: i64,
    pub shader_compilation_mode: i64,
    pub force_16_9: bool,
    pub widescreen_hack: bool,
    pub remove_blur: bool,
    pub show_fps: bool,
    pub ubershaders: bool,
    pub texture_cache_accuracy: i64,
    pub frame_limit: i64,
    pub refresh_rate: i64,

    // --- AUDIO ---
    pub audio_volume: i64,
    pub audio_backend: String,
    pub dsp_lle: bool,
    pub audio_stretching: bool,
    pub audio_latency: i64,

    // --- CONTROLLER ---
    pub selected_port: i64,
    pub device_type_port1: String,
    pub device_type_port2: String,
    pub device_type_port3: String,
    pub device_type_port4: String,
    pub analog_sensitivity: i64,
    pub analog_deadzone: i64,
    pub vibration: bool,
    pub controller_preset: String,

    // --- WII ---
    pub wii_language: i64,
    pub wii_region: i64,
    pub system_time_sync: bool,
    pub enable_sd_card: bool,
    pub force_disable_wiimote: bool,
    pub launch_in_window: bool,
    pub retro_rewind: bool,
    pub enable_cheats: bool,
    pub enable_riivolution: bool,

    // --- PERFORMANCE ---
    pub cpu_override: bool,
    pub cpu_clock_ratio: f32,
    pub dual_core: bool,
    pub sync_gpu: String,
    pub skip_idle: bool,
    pub fast_disc_speed: bool,
    pub performance_preset: String,

    // --- MIGLIORAMENTI GRAFICI ---
    pub load_custom_textures: bool,
    pub prefetch_custom_textures: bool,
    pub post_processing_shader: String,
    pub enable_bloom: bool,
    pub enable_ambient_occlusion: bool,
    pub enable_color_correction: bool,
    pub gamma: f32,
    pub brightness: i64,

    // --- PERCORSI ---
    pub dolphin_executable_path: String,
    pub user_folder_path: String,
    pub modpack_path: String,

    // --- AVANZATE ---
    pub log_level: String,
    pub log_to_file: bool,
    pub wait_for_shaders_before_starting: bool,
    pub backend_multithreading: bool,
    pub debug_mode: bool,
    pub portable_mode: bool,
}

impl Default for DolphinSettings {
    fn default() -> Self {
        // Stessi default del `DolphinSettingsModel` legacy.
        Self {
            gfx_backend: "Vulkan".into(),
            internal_resolution: 3,
            fullscreen: true,
            aspect_ratio: 1,
            vsync: false,
            anti_aliasing: 0,
            anisotropic_filtering: 4,
            shader_compilation_mode: 2,
            force_16_9: true,
            widescreen_hack: false,
            remove_blur: true,
            show_fps: false,
            ubershaders: true,
            texture_cache_accuracy: 0,
            frame_limit: 0,
            refresh_rate: 0,

            audio_volume: 100,
            audio_backend: "Cubeb".into(),
            dsp_lle: false,
            audio_stretching: true,
            audio_latency: 20,

            selected_port: 1,
            device_type_port1: "Standard Controller".into(),
            device_type_port2: "Disabled".into(),
            device_type_port3: "Disabled".into(),
            device_type_port4: "Disabled".into(),
            analog_sensitivity: 100,
            analog_deadzone: 10,
            vibration: true,
            controller_preset: "Default GamePad".into(),

            wii_language: 1,
            wii_region: 2,
            system_time_sync: true,
            enable_sd_card: true,
            force_disable_wiimote: true,
            launch_in_window: false,
            retro_rewind: true,
            enable_cheats: true,
            enable_riivolution: true,

            cpu_override: false,
            cpu_clock_ratio: 1.0,
            dual_core: true,
            sync_gpu: "Auto".into(),
            skip_idle: true,
            fast_disc_speed: true,
            performance_preset: "Balanced".into(),

            load_custom_textures: true,
            prefetch_custom_textures: true,
            post_processing_shader: "Off".into(),
            enable_bloom: false,
            enable_ambient_occlusion: false,
            enable_color_correction: false,
            gamma: 1.0,
            brightness: 100,

            dolphin_executable_path: String::new(),
            user_folder_path: String::new(),
            modpack_path: String::new(),

            log_level: "Notice".into(),
            log_to_file: true,
            wait_for_shaders_before_starting: false,
            backend_multithreading: true,
            debug_mode: false,
            portable_mode: false,
        }
    }
}

/// Percorsi dei tre INI gestiti.
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub dolphin_ini: PathBuf,
    pub gfx_ini: PathBuf,
    pub logger_ini: PathBuf,
    pub config_dir: PathBuf,
}

impl ConfigPaths {
    pub fn from_user_folder(user_folder: &Path) -> Self {
        let config_dir = user_folder.join("Config");
        Self {
            dolphin_ini: config_dir.join("Dolphin.ini"),
            gfx_ini: config_dir.join("GFX.ini"),
            logger_ini: config_dir.join("Logger.ini"),
            config_dir,
        }
    }
}

impl DolphinSettings {
    /// Legge le impostazioni dagli INI della cartella User.
    ///
    /// I valori assenti restano ai default; l'ordine dei fallback (per esempio
    /// `InternalResolution` poi `EFBScale`) è quello del launcher legacy.
    pub fn load(user_folder: &Path) -> Self {
        let mut model = Self::default();
        if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
            return model;
        }
        model.user_folder_path = user_folder.to_string_lossy().to_string();

        let paths = ConfigPaths::from_user_folder(user_folder);
        model.apply_ini(
            &ini::read_ini(&paths.dolphin_ini),
            &ini::read_ini(&paths.gfx_ini),
            &ini::read_ini(&paths.logger_ini),
        );
        model
    }

    /// Nucleo puro di [`Self::load`], testabile senza filesystem.
    pub fn apply_ini(&mut self, dolphin: &IniData, gfx: &IniData, logger: &IniData) {
        // --- Dolphin.ini / [Core] ---
        if let Some(value) = ini::get(dolphin, "Core", "GFXBackend") {
            self.gfx_backend = value.to_string();
        }
        self.dual_core = ini::get_bool(dolphin, "Core", "CPUThread")
            .or_else(|| ini::get_bool(dolphin, "Core", "DualCore"))
            .unwrap_or(self.dual_core);
        set_bool(&mut self.skip_idle, dolphin, "Core", "SkipIdle");
        set_bool(&mut self.fast_disc_speed, dolphin, "Core", "FastDiscSpeed");
        set_bool(&mut self.cpu_override, dolphin, "Core", "OverclockEnable");
        if let Some(value) = ini::get_float(dolphin, "Core", "Overclock") {
            self.cpu_clock_ratio = value;
        }
        set_bool(&mut self.audio_stretching, dolphin, "Core", "AudioStretch");
        set_int(&mut self.audio_latency, dolphin, "Core", "AudioBufferSize");
        set_bool(&mut self.enable_cheats, dolphin, "Core", "EnableCheats");
        set_bool(
            &mut self.enable_riivolution,
            dolphin,
            "Core",
            "EnableRiivolution",
        );
        if let Some(value) = ini::get_bool(dolphin, "Core", "WiimoteEnableSpeaker") {
            self.force_disable_wiimote = !value;
        }
        if let Some(region) = ini::get_int(dolphin, "Core", "FallbackRegion") {
            if (0..=3).contains(&region) {
                self.wii_region = region;
            }
        }

        if let Some(preset) = ini::get(dolphin, "VanzaKartLauncher", "PerformancePreset") {
            self.performance_preset = normalize_performance_preset(preset);
        }

        // --- Dolphin.ini / [Display] ---
        set_bool(&mut self.fullscreen, dolphin, "Display", "Fullscreen");
        if let Some(render_to_main) = ini::get_bool(dolphin, "Display", "RenderToMain") {
            self.launch_in_window = !render_to_main;
        }

        // --- Dolphin.ini / [DSP] ---
        set_int(&mut self.audio_volume, dolphin, "DSP", "Volume");
        if let Some(value) = ini::get(dolphin, "DSP", "Backend") {
            self.audio_backend = value.to_string();
        }
        self.dsp_lle = ini::get_bool(dolphin, "DSP", "DSPThread")
            .or_else(|| ini::get_bool(dolphin, "DSP", "LLE"))
            .unwrap_or(self.dsp_lle);

        // --- Dolphin.ini / [Wii] ---
        self.wii_language = ini::get_int(dolphin, "Wii", "WiiLanguage")
            .or_else(|| ini::get_int(dolphin, "Wii", "Language"))
            .unwrap_or(self.wii_language);
        self.enable_sd_card = ini::get_bool(dolphin, "Wii", "WiiSDCard")
            .or_else(|| ini::get_bool(dolphin, "Wii", "SDCard"))
            .unwrap_or(self.enable_sd_card);

        // --- GFX.ini / [Settings] ---
        if self.gfx_backend.trim().is_empty() {
            if let Some(value) = ini::get(gfx, "Settings", "GFXBackend") {
                self.gfx_backend = value.to_string();
            }
        }
        self.internal_resolution = ini::get_int(gfx, "Settings", "InternalResolution")
            .or_else(|| ini::get_int(gfx, "Settings", "EFBScale"))
            .unwrap_or(self.internal_resolution);
        set_int(&mut self.aspect_ratio, gfx, "Settings", "AspectRatio");
        set_bool(&mut self.vsync, gfx, "Settings", "VSync");
        set_int(&mut self.anti_aliasing, gfx, "Settings", "MSAA");
        set_int(
            &mut self.anisotropic_filtering,
            gfx,
            "Settings",
            "TexFiltMode",
        );
        set_int(
            &mut self.shader_compilation_mode,
            gfx,
            "Settings",
            "ShaderCompilationMode",
        );
        set_bool(&mut self.remove_blur, gfx, "Settings", "DisableCopyFilter");
        set_bool(&mut self.show_fps, gfx, "Settings", "ShowFPS");
        set_bool(
            &mut self.load_custom_textures,
            gfx,
            "Settings",
            "HiresTextures",
        );
        set_bool(
            &mut self.prefetch_custom_textures,
            gfx,
            "Settings",
            "CacheHiresTextures",
        );
        set_bool(&mut self.widescreen_hack, gfx, "Settings", "WidescreenHack");
        if let Some(value) = ini::get(gfx, "Settings", "PostProcessingShader") {
            self.post_processing_shader = value.to_string();
        }
        set_bool(
            &mut self.wait_for_shaders_before_starting,
            gfx,
            "Settings",
            "WaitForShadersBeforeStarting",
        );
        set_bool(
            &mut self.backend_multithreading,
            gfx,
            "Settings",
            "BackendMultithreading",
        );

        // --- GFX.ini / [Enhancements] ---
        if self.internal_resolution <= 0 {
            self.internal_resolution = ini::get_int(gfx, "Enhancements", "InternalResolution")
                .or_else(|| ini::get_int(gfx, "Enhancements", "EFBScale"))
                .unwrap_or(self.internal_resolution);
        }
        set_int(
            &mut self.anisotropic_filtering,
            gfx,
            "Enhancements",
            "MaxAnisotropy",
        );
        set_bool(&mut self.enable_bloom, gfx, "Enhancements", "Bloom");
        set_bool(
            &mut self.enable_ambient_occlusion,
            gfx,
            "Enhancements",
            "AmbientOcclusion",
        );
        set_bool(
            &mut self.enable_color_correction,
            gfx,
            "Enhancements",
            "ColorCorrection",
        );

        // --- Logger.ini ---
        if let Some(verbosity) = ini::get_int(logger, "Options", "Verbosity") {
            self.log_level = log_level_from_verbosity(verbosity).to_string();
        }
        set_bool(&mut self.log_to_file, logger, "Options", "WriteToFile");
    }

    /// Aggiornamenti da applicare a `Dolphin.ini`.
    pub fn dolphin_ini_updates(&self) -> IniData {
        IniUpdates::new()
            .set("Core", "GFXBackend", self.gfx_backend.clone())
            .set_bool("Core", "CPUThread", self.dual_core)
            .set_bool("Core", "DualCore", self.dual_core)
            .set_bool("Core", "SkipIdle", self.skip_idle)
            .set_bool("Core", "FastDiscSpeed", self.fast_disc_speed)
            .set_bool("Core", "OverclockEnable", self.cpu_override)
            .set("Core", "Overclock", format_float(self.cpu_clock_ratio))
            .set_bool("Core", "AudioStretch", self.audio_stretching)
            .set_int("Core", "AudioBufferSize", self.audio_latency)
            .set_bool("Core", "EnableCheats", self.enable_cheats)
            .set_bool("Core", "EnableRiivolution", self.enable_riivolution)
            .set_bool("Core", "WiimoteEnableSpeaker", !self.force_disable_wiimote)
            .set_int("Core", "FallbackRegion", self.wii_region.clamp(0, 3))
            .set_bool("Display", "Fullscreen", self.fullscreen)
            .set_bool("Display", "RenderToMain", !self.launch_in_window)
            .set_int("DSP", "Volume", self.audio_volume)
            .set("DSP", "Backend", self.audio_backend.clone())
            .set_bool("DSP", "DSPThread", self.dsp_lle)
            .set_bool("DSP", "LLE", self.dsp_lle)
            .set_int("Wii", "WiiLanguage", self.wii_language)
            .set_int("Wii", "Language", self.wii_language)
            .set_bool("Wii", "WiiSDCard", self.enable_sd_card)
            .set_bool("Wii", "SDCard", self.enable_sd_card)
            .set(
                "VanzaKartLauncher",
                "PerformancePreset",
                normalize_performance_preset(&self.performance_preset),
            )
            .into_data()
    }

    /// Aggiornamenti da applicare a `GFX.ini`.
    pub fn gfx_ini_updates(&self) -> IniData {
        IniUpdates::new()
            .set_int("Settings", "InternalResolution", self.internal_resolution)
            .set_int("Settings", "EFBScale", self.internal_resolution)
            .set("Settings", "GFXBackend", self.gfx_backend.clone())
            .set_int("Settings", "AspectRatio", self.aspect_ratio)
            .set_bool("Settings", "VSync", self.vsync)
            .set_int("Settings", "MSAA", self.anti_aliasing)
            .set_int("Settings", "TexFiltMode", self.anisotropic_filtering)
            .set_int(
                "Settings",
                "ShaderCompilationMode",
                self.shader_compilation_mode,
            )
            .set_bool("Settings", "DisableCopyFilter", self.remove_blur)
            .set_bool("Settings", "ShowFPS", self.show_fps)
            .set_bool("Settings", "HiresTextures", self.load_custom_textures)
            .set_bool(
                "Settings",
                "CacheHiresTextures",
                self.prefetch_custom_textures,
            )
            .set_bool("Settings", "WidescreenHack", self.widescreen_hack)
            .set_bool(
                "Settings",
                "WaitForShadersBeforeStarting",
                self.wait_for_shaders_before_starting,
            )
            .set_bool(
                "Settings",
                "BackendMultithreading",
                self.backend_multithreading,
            )
            .set_int(
                "Enhancements",
                "InternalResolution",
                self.internal_resolution,
            )
            .set_int("Enhancements", "EFBScale", self.internal_resolution)
            .set_int("Enhancements", "MaxAnisotropy", self.anisotropic_filtering)
            .set_int("Enhancements", "TexFiltMode", self.anisotropic_filtering)
            .set_bool("Enhancements", "DisableCopyFilter", self.remove_blur)
            .set_bool("Enhancements", "WidescreenHack", self.widescreen_hack)
            .set_bool("Enhancements", "Bloom", self.enable_bloom)
            .set_bool(
                "Enhancements",
                "AmbientOcclusion",
                self.enable_ambient_occlusion,
            )
            .set_bool(
                "Enhancements",
                "ColorCorrection",
                self.enable_color_correction,
            )
            .set_bool("Hardware", "VSync", self.vsync)
            .into_data()
    }

    /// Aggiornamenti da applicare a `Logger.ini`.
    pub fn logger_ini_updates(&self) -> IniData {
        IniUpdates::new()
            .set_int(
                "Options",
                "Verbosity",
                verbosity_from_log_level(&self.log_level),
            )
            .set_bool("Options", "WriteToFile", self.log_to_file)
            .into_data()
    }

    /// Scrive le impostazioni nei tre INI.
    ///
    /// `portable.txt` non viene mai creato né rimosso: convertire
    /// un'installazione esistente sarebbe distruttivo.
    pub fn save(&self, user_folder: &Path) -> DolphinResult<()> {
        if user_folder.as_os_str().is_empty() {
            return Ok(());
        }
        let paths = ConfigPaths::from_user_folder(user_folder);
        std::fs::create_dir_all(&paths.config_dir)
            .map_err(|e| crate::error::DolphinError::io(&paths.config_dir, e))?;

        ini::update_ini(&paths.dolphin_ini, &self.dolphin_ini_updates())?;
        ini::update_ini(&paths.gfx_ini, &self.gfx_ini_updates())?;
        ini::update_ini(&paths.logger_ini, &self.logger_ini_updates())?;
        Ok(())
    }

    /// Applica il preset "VanzaKart Recommended".
    ///
    /// `screen_width` serve a scegliere la risoluzione interna; il legacy la
    /// leggeva da `SystemParameters.PrimaryScreenWidth`.
    pub fn optimize_for_vanzakart(&mut self, screen_width: u32) {
        self.gfx_backend = "Vulkan".into();
        self.fullscreen = true;
        self.internal_resolution = match screen_width {
            width if width >= 3840 => 6,
            width if width >= 2560 => 4,
            width if width >= 1920 => 3,
            _ => 2,
        };
        self.aspect_ratio = 1;
        self.force_16_9 = true;
        self.widescreen_hack = true;
        self.shader_compilation_mode = 2;
        self.ubershaders = true;
        self.load_custom_textures = true;
        self.prefetch_custom_textures = true;
        self.remove_blur = true;
        self.vsync = false;
        self.anti_aliasing = 0;
        self.anisotropic_filtering = 4;
        self.audio_backend = "Cubeb".into();
        self.audio_stretching = true;
        self.audio_volume = 100;
        self.audio_latency = 20;
        self.dsp_lle = false;
        self.dual_core = true;
        self.skip_idle = true;
        self.fast_disc_speed = true;
        self.cpu_override = false;
        self.cpu_clock_ratio = 1.0;
        self.wii_language = 1;
        self.wii_region = 2;
        self.performance_preset = "VanzaKart Recommended".into();
    }

    /// Ripristina i default di una categoria, lasciando intatte le altre.
    pub fn reset_category(&mut self, category: &str) {
        let defaults = Self::default();
        match category.trim().to_ascii_lowercase().as_str() {
            "video" => {
                self.gfx_backend = defaults.gfx_backend;
                self.internal_resolution = defaults.internal_resolution;
                self.fullscreen = defaults.fullscreen;
                self.aspect_ratio = defaults.aspect_ratio;
                self.vsync = defaults.vsync;
                self.anti_aliasing = defaults.anti_aliasing;
                self.anisotropic_filtering = defaults.anisotropic_filtering;
                self.shader_compilation_mode = defaults.shader_compilation_mode;
                self.force_16_9 = defaults.force_16_9;
                self.widescreen_hack = defaults.widescreen_hack;
                self.remove_blur = defaults.remove_blur;
                self.show_fps = defaults.show_fps;
                self.ubershaders = defaults.ubershaders;
                self.texture_cache_accuracy = defaults.texture_cache_accuracy;
            }
            "audio" => {
                self.audio_volume = defaults.audio_volume;
                self.audio_backend = defaults.audio_backend;
                self.dsp_lle = defaults.dsp_lle;
                self.audio_stretching = defaults.audio_stretching;
                self.audio_latency = defaults.audio_latency;
            }
            "wii" => {
                self.wii_language = defaults.wii_language;
                self.wii_region = defaults.wii_region;
                self.system_time_sync = defaults.system_time_sync;
                self.enable_sd_card = defaults.enable_sd_card;
                self.force_disable_wiimote = defaults.force_disable_wiimote;
                self.enable_cheats = defaults.enable_cheats;
                self.enable_riivolution = defaults.enable_riivolution;
            }
            "performance" => {
                self.cpu_override = defaults.cpu_override;
                self.cpu_clock_ratio = defaults.cpu_clock_ratio;
                self.dual_core = defaults.dual_core;
                self.sync_gpu = defaults.sync_gpu;
                self.skip_idle = defaults.skip_idle;
                self.fast_disc_speed = defaults.fast_disc_speed;
                self.performance_preset = defaults.performance_preset;
            }
            _ => {
                // Reset completo, preservando i percorsi come nel legacy.
                let paths = (
                    self.dolphin_executable_path.clone(),
                    self.user_folder_path.clone(),
                    self.modpack_path.clone(),
                );
                *self = defaults;
                self.dolphin_executable_path = paths.0;
                self.user_folder_path = paths.1;
                self.modpack_path = paths.2;
            }
        }
    }
}

fn set_bool(target: &mut bool, data: &IniData, section: &str, key: &str) {
    if let Some(value) = ini::get_bool(data, section, key) {
        *target = value;
    }
}

fn set_int(target: &mut i64, data: &IniData, section: &str, key: &str) {
    if let Some(value) = ini::get_int(data, section, key) {
        *target = value;
    }
}

/// Formatta un float con punto decimale, indipendentemente dalla localizzazione.
fn format_float(value: f32) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Replica di `NormalizePerformancePreset`: qualunque valore non riconosciuto
/// diventa `Balanced`.
pub fn normalize_performance_preset(value: &str) -> String {
    match value.trim() {
        "VanzaKart Recommended" => "VanzaKart Recommended",
        "High-Performance" => "High-Performance",
        "Low-End" => "Low-End",
        _ => "Balanced",
    }
    .to_string()
}

/// Replica di `VerbosityFromLogLevel`.
pub fn verbosity_from_log_level(level: &str) -> i64 {
    match level.trim().to_ascii_lowercase().as_str() {
        "error" => 2,
        "warning" => 3,
        "info" => 4,
        "debug" => 5,
        _ => 1,
    }
}

/// Replica di `LogLevelFromVerbosity`.
pub fn log_level_from_verbosity(verbosity: i64) -> &'static str {
    match verbosity {
        2 => "Error",
        3 => "Warning",
        4 => "Info",
        5 => "Debug",
        _ => "Notice",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::parse_ini;

    #[test]
    fn defaults_match_the_legacy_model() {
        let defaults = DolphinSettings::default();
        assert_eq!(defaults.gfx_backend, "Vulkan");
        assert_eq!(defaults.internal_resolution, 3);
        assert_eq!(defaults.aspect_ratio, 1);
        assert_eq!(defaults.audio_backend, "Cubeb");
        assert_eq!(defaults.wii_region, 2);
        assert!(defaults.enable_riivolution);
        assert_eq!(defaults.performance_preset, "Balanced");
    }

    #[test]
    fn reads_core_display_dsp_and_wii() {
        let dolphin = parse_ini(
            "[Core]\nGFXBackend = D3D12\nCPUThread = False\nEnableCheats = False\nFallbackRegion = 1\nWiimoteEnableSpeaker = True\nOverclock = 1.75\n\
             [Display]\nFullscreen = False\nRenderToMain = False\n\
             [DSP]\nVolume = 42\nBackend = WASAPI\nLLE = True\n\
             [Wii]\nLanguage = 5\nSDCard = False\n",
        );

        let mut model = DolphinSettings::default();
        model.apply_ini(&dolphin, &IniData::new(), &IniData::new());

        assert_eq!(model.gfx_backend, "D3D12");
        assert!(!model.dual_core);
        assert!(!model.enable_cheats);
        assert_eq!(model.wii_region, 1);
        assert!(!model.force_disable_wiimote);
        assert_eq!(model.cpu_clock_ratio, 1.75);
        assert!(!model.fullscreen);
        assert!(model.launch_in_window);
        assert_eq!(model.audio_volume, 42);
        assert_eq!(model.audio_backend, "WASAPI");
        assert!(model.dsp_lle);
        assert_eq!(model.wii_language, 5);
        assert!(!model.enable_sd_card);
    }

    #[test]
    fn efb_scale_is_a_fallback_for_internal_resolution() {
        let mut model = DolphinSettings::default();
        model.apply_ini(
            &IniData::new(),
            &parse_ini("[Settings]\nEFBScale = 5\n"),
            &IniData::new(),
        );
        assert_eq!(model.internal_resolution, 5);
    }

    #[test]
    fn an_out_of_range_region_is_ignored() {
        let mut model = DolphinSettings::default();
        model.apply_ini(
            &parse_ini("[Core]\nFallbackRegion = 9\n"),
            &IniData::new(),
            &IniData::new(),
        );
        assert_eq!(model.wii_region, 2);
    }

    #[test]
    fn reads_the_log_level_from_verbosity() {
        let mut model = DolphinSettings::default();
        model.apply_ini(
            &IniData::new(),
            &IniData::new(),
            &parse_ini("[Options]\nVerbosity = 4\nWriteToFile = False\n"),
        );
        assert_eq!(model.log_level, "Info");
        assert!(!model.log_to_file);
    }

    #[test]
    fn writes_both_aliases_of_every_dual_key() {
        let updates = DolphinSettings::default().dolphin_ini_updates();
        assert_eq!(updates["Core"]["CPUThread"], "True");
        assert_eq!(updates["Core"]["DualCore"], "True");
        assert_eq!(updates["Wii"]["WiiLanguage"], "1");
        assert_eq!(updates["Wii"]["Language"], "1");
        assert_eq!(updates["DSP"]["DSPThread"], "False");
        assert_eq!(updates["DSP"]["LLE"], "False");
    }

    #[test]
    fn wiimote_speaker_is_the_inverse_of_force_disable() {
        let mut model = DolphinSettings {
            force_disable_wiimote: true,
            ..Default::default()
        };
        assert_eq!(
            model.dolphin_ini_updates()["Core"]["WiimoteEnableSpeaker"],
            "False"
        );
        model.force_disable_wiimote = false;
        assert_eq!(
            model.dolphin_ini_updates()["Core"]["WiimoteEnableSpeaker"],
            "True"
        );
    }

    #[test]
    fn the_overclock_ratio_always_uses_a_dot() {
        let mut model = DolphinSettings {
            cpu_clock_ratio: 1.0,
            ..Default::default()
        };
        assert_eq!(model.dolphin_ini_updates()["Core"]["Overclock"], "1.0");
        model.cpu_clock_ratio = 1.25;
        assert_eq!(model.dolphin_ini_updates()["Core"]["Overclock"], "1.25");
    }

    #[test]
    fn round_trips_through_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        std::fs::write(
            user.join("Config/Dolphin.ini"),
            "; commento da preservare\n[Core]\nChiaveSconosciuta = 7\n",
        )
        .unwrap();

        let model = DolphinSettings {
            audio_volume: 55,
            internal_resolution: 6,
            log_level: "Debug".into(),
            ..Default::default()
        };
        model.save(&user).unwrap();

        let reloaded = DolphinSettings::load(&user);
        assert_eq!(reloaded.audio_volume, 55);
        assert_eq!(reloaded.internal_resolution, 6);
        assert_eq!(reloaded.log_level, "Debug");

        // Le chiavi non gestite dal launcher sopravvivono.
        let raw = std::fs::read_to_string(user.join("Config/Dolphin.ini")).unwrap();
        assert!(raw.contains("; commento da preservare"));
        assert!(raw.contains("ChiaveSconosciuta = 7"));
    }

    #[test]
    fn loading_from_a_missing_folder_returns_defaults() {
        let model = DolphinSettings::load(Path::new("/percorso/inesistente"));
        assert_eq!(model, DolphinSettings::default());
    }

    #[test]
    fn the_vanzakart_preset_scales_with_the_screen() {
        for (width, expected) in [(1366u32, 2i64), (1920, 3), (2560, 4), (3840, 6)] {
            let mut model = DolphinSettings::default();
            model.optimize_for_vanzakart(width);
            assert_eq!(model.internal_resolution, expected, "larghezza {width}");
            assert_eq!(model.performance_preset, "VanzaKart Recommended");
            assert!(model.widescreen_hack);
            assert_eq!(model.gfx_backend, "Vulkan");
        }
    }

    #[test]
    fn resetting_a_category_leaves_the_others_alone() {
        let mut model = DolphinSettings {
            audio_volume: 10,
            internal_resolution: 6,
            ..Default::default()
        };

        model.reset_category("video");

        assert_eq!(model.internal_resolution, 3);
        assert_eq!(model.audio_volume, 10, "l'audio non doveva cambiare");
    }

    #[test]
    fn a_full_reset_preserves_the_paths() {
        let mut model = DolphinSettings {
            dolphin_executable_path: "/opt/dolphin/Dolphin".into(),
            user_folder_path: "/home/a/User".into(),
            audio_volume: 3,
            ..Default::default()
        };

        model.reset_category("tutto");

        assert_eq!(model.audio_volume, 100);
        assert_eq!(model.dolphin_executable_path, "/opt/dolphin/Dolphin");
        assert_eq!(model.user_folder_path, "/home/a/User");
    }

    #[test]
    fn performance_presets_fall_back_to_balanced() {
        assert_eq!(normalize_performance_preset("Low-End"), "Low-End");
        assert_eq!(
            normalize_performance_preset(" VanzaKart Recommended "),
            "VanzaKart Recommended"
        );
        assert_eq!(normalize_performance_preset("Turbo"), "Balanced");
    }

    #[test]
    fn log_level_and_verbosity_round_trip() {
        for level in ["Notice", "Error", "Warning", "Info", "Debug"] {
            let verbosity = verbosity_from_log_level(level);
            assert_eq!(log_level_from_verbosity(verbosity), level, "{level}");
        }
        assert_eq!(verbosity_from_log_level("sconosciuto"), 1);
    }
}
