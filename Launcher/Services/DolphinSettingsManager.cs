using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Windows;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class DolphinSettingsManager
{
    private readonly DolphinIniService _iniService = new();

    public DolphinSettingsModel LoadSettings(string userFolderPath, LauncherSettings launcherSettings)
    {
        var model = new DolphinSettingsModel
        {
            DolphinExecutablePath = launcherSettings.DolphinPath ?? "",
            UserFolderPath = userFolderPath ?? "",
            ModpackPath = launcherSettings.RomPath ?? ""
        };

        if (string.IsNullOrWhiteSpace(userFolderPath) || !Directory.Exists(userFolderPath))
        {
            return model;
        }

        string dolphinIniPath = Path.Combine(userFolderPath, "Config", "Dolphin.ini");
        string gfxIniPath = Path.Combine(userFolderPath, "Config", "GFX.ini");
        string loggerIniPath = Path.Combine(userFolderPath, "Config", "Logger.ini");

        var dolphinIni = _iniService.ReadIni(dolphinIniPath);
        var gfxIni = _iniService.ReadIni(gfxIniPath);
        var loggerIni = _iniService.ReadIni(loggerIniPath);

        // --- Core / Video INI read ---
        if (dolphinIni.TryGetValue("Core", out var coreSection))
        {
            if (coreSection.TryGetValue("GFXBackend", out var gfxBackend)) model.GfxBackend = gfxBackend;
            if (coreSection.TryGetValue("CPUThread", out var cpuThread)) model.DualCore = parseBool(cpuThread, true);
            else if (coreSection.TryGetValue("DualCore", out var dc)) model.DualCore = parseBool(dc, true);

            if (coreSection.TryGetValue("SkipIdle", out var skipIdle)) model.SkipIdle = parseBool(skipIdle, true);
            if (coreSection.TryGetValue("FastDiscSpeed", out var fastDisc)) model.FastDiscSpeed = parseBool(fastDisc, true);
            if (coreSection.TryGetValue("OverclockEnable", out var ocEnable)) model.CpuOverride = parseBool(ocEnable, false);
            if (coreSection.TryGetValue("Overclock", out var ocVal) && float.TryParse(ocVal, System.Globalization.CultureInfo.InvariantCulture, out float oc)) model.CpuClockRatio = oc;
            if (coreSection.TryGetValue("AudioStretch", out var audioStretch)) model.AudioStretching = parseBool(audioStretch, true);
            if (coreSection.TryGetValue("AudioBufferSize", out var audioBuffer) && int.TryParse(audioBuffer, out int buf)) model.AudioLatency = buf;
            if (coreSection.TryGetValue("EnableCheats", out var cheats)) model.EnableCheats = parseBool(cheats, true);
            if (coreSection.TryGetValue("EnableRiivolution", out var riiv)) model.EnableRiivolution = parseBool(riiv, true);
            if (coreSection.TryGetValue("WiimoteEnableSpeaker", out var wiimoteSpeaker))
                model.ForceDisableWiimote = !parseBool(wiimoteSpeaker, false);
            if (coreSection.TryGetValue("FallbackRegion", out var region) &&
                int.TryParse(region, out var parsedRegion) &&
                parsedRegion is >= 0 and <= 3)
            {
                model.WiiRegion = parsedRegion;
            }
        }

        if (dolphinIni.TryGetValue("VanzaKartLauncher", out var launcherSection) &&
            launcherSection.TryGetValue("PerformancePreset", out var savedPreset))
        {
            model.PerformancePreset = NormalizePerformancePreset(savedPreset);
        }

        if (dolphinIni.TryGetValue("Display", out var displaySection))
        {
            if (displaySection.TryGetValue("Fullscreen", out var fs)) model.Fullscreen = parseBool(fs, true);
            if (displaySection.TryGetValue("RenderToMain", out var rtm)) model.LaunchInWindow = !parseBool(rtm, true);
        }

        if (dolphinIni.TryGetValue("DSP", out var dspSection))
        {
            if (dspSection.TryGetValue("Volume", out var vol) && int.TryParse(vol, out int v)) model.AudioVolume = v;
            if (dspSection.TryGetValue("Backend", out var backend)) model.AudioBackend = backend;
            if (dspSection.TryGetValue("DSPThread", out var dspThread)) model.DspLle = parseBool(dspThread, false);
            else if (dspSection.TryGetValue("LLE", out var lle)) model.DspLle = parseBool(lle, false);
        }

        if (dolphinIni.TryGetValue("Wii", out var wiiSection))
        {
            if (wiiSection.TryGetValue("WiiLanguage", out var lang) && int.TryParse(lang, out int l)) model.WiiLanguage = l;
            else if (wiiSection.TryGetValue("Language", out var lang2) && int.TryParse(lang2, out int l2)) model.WiiLanguage = l2;

            if (wiiSection.TryGetValue("WiiSDCard", out var sd)) model.EnableSdCard = parseBool(sd, true);
            else if (wiiSection.TryGetValue("SDCard", out var sd2)) model.EnableSdCard = parseBool(sd2, true);
        }

        // --- GFX.ini read ---
        if (gfxIni.TryGetValue("Settings", out var gfxSettings))
        {
            if (string.IsNullOrWhiteSpace(model.GfxBackend) && gfxSettings.TryGetValue("GFXBackend", out var gfxBackend2)) model.GfxBackend = gfxBackend2;
            if (gfxSettings.TryGetValue("InternalResolution", out var res) && int.TryParse(res, out int r)) model.InternalResolution = r;
            else if (gfxSettings.TryGetValue("EFBScale", out var efb) && int.TryParse(efb, out int e)) model.InternalResolution = e;

            if (gfxSettings.TryGetValue("AspectRatio", out var ar) && int.TryParse(ar, out int a)) model.AspectRatio = a;
            if (gfxSettings.TryGetValue("VSync", out var vs)) model.VSync = parseBool(vs, false);
            if (gfxSettings.TryGetValue("MSAA", out var msaa) && int.TryParse(msaa, out int m)) model.AntiAliasing = m;
            if (gfxSettings.TryGetValue("TexFiltMode", out var tf) && int.TryParse(tf, out int t)) model.AnisotropicFiltering = t;
            if (gfxSettings.TryGetValue("ShaderCompilationMode", out var scm) && int.TryParse(scm, out int s)) model.ShaderCompilationMode = s;
            if (gfxSettings.TryGetValue("DisableCopyFilter", out var dcf)) model.RemoveBlur = parseBool(dcf, true);
            if (gfxSettings.TryGetValue("ShowFPS", out var fps)) model.ShowFPS = parseBool(fps, false);
            if (gfxSettings.TryGetValue("HiresTextures", out var ht)) model.LoadCustomTextures = parseBool(ht, true);
            if (gfxSettings.TryGetValue("CacheHiresTextures", out var cht)) model.PrefetchCustomTextures = parseBool(cht, true);
            if (gfxSettings.TryGetValue("WidescreenHack", out var wsh)) model.WidescreenHack = parseBool(wsh, false);
            if (gfxSettings.TryGetValue("PostProcessingShader", out var pps)) model.PostProcessingShader = pps;
            if (gfxSettings.TryGetValue("WaitForShadersBeforeStarting", out var waitForShaders))
                model.WaitForShadersBeforeStarting = parseBool(waitForShaders, false);
            if (gfxSettings.TryGetValue("BackendMultithreading", out var backendMultithreading))
                model.BackendMultithreading = parseBool(backendMultithreading, true);
        }

        if (gfxIni.TryGetValue("Enhancements", out var gfxEnh))
        {
            if (model.InternalResolution <= 0)
            {
                if (gfxEnh.TryGetValue("InternalResolution", out var res2) && int.TryParse(res2, out int r2)) model.InternalResolution = r2;
                else if (gfxEnh.TryGetValue("EFBScale", out var efb2) && int.TryParse(efb2, out int e2)) model.InternalResolution = e2;
            }
            if (model.AnisotropicFiltering <= 0 && gfxEnh.TryGetValue("MaxAnisotropy", out var ma) && int.TryParse(ma, out int m2)) model.AnisotropicFiltering = m2;

            if (gfxEnh.TryGetValue("Bloom", out var bloom)) model.EnableBloom = parseBool(bloom, false);
            if (gfxEnh.TryGetValue("AmbientOcclusion", out var ao)) model.EnableAmbientOcclusion = parseBool(ao, false);
            if (gfxEnh.TryGetValue("ColorCorrection", out var cc)) model.EnableColorCorrection = parseBool(cc, false);
        }

        if (loggerIni.TryGetValue("Options", out var loggerOptions))
        {
            if (loggerOptions.TryGetValue("Verbosity", out var verbosity))
                model.LogLevel = LogLevelFromVerbosity(verbosity);
            if (loggerOptions.TryGetValue("WriteToFile", out var writeToFile))
                model.LogToFile = parseBool(writeToFile, true);
        }

        // Check Portable Mode
        if (!string.IsNullOrWhiteSpace(model.DolphinExecutablePath))
        {
            string dDir = Path.GetDirectoryName(model.DolphinExecutablePath) ?? "";
            model.PortableMode = File.Exists(Path.Combine(dDir, "portable.txt"));
        }

        // Derive paths
        model.TexturePackPath = Path.Combine(userFolderPath, "Load", "Textures");
        model.ScreenshotPath = Path.Combine(userFolderPath, "ScreenShots");
        model.CachePath = Path.Combine(userFolderPath, "Cache");

        return model;

        static bool parseBool(string val, bool fallback)
        {
            if (bool.TryParse(val, out bool b)) return b;
            if (val == "1" || val.Equals("True", StringComparison.OrdinalIgnoreCase)) return true;
            if (val == "0" || val.Equals("False", StringComparison.OrdinalIgnoreCase)) return false;
            return fallback;
        }
    }

    public void SaveSettings(string userFolderPath, DolphinSettingsModel model)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath)) return;

        string configDir = Path.Combine(userFolderPath, "Config");
        Directory.CreateDirectory(configDir);

        string dolphinIniPath = Path.Combine(configDir, "Dolphin.ini");
        string gfxIniPath = Path.Combine(configDir, "GFX.ini");
        string loggerIniPath = Path.Combine(configDir, "Logger.ini");

        var dolphinUpdates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Core"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["GFXBackend"] = model.GfxBackend,
                ["CPUThread"] = model.DualCore.ToString(),
                ["DualCore"] = model.DualCore.ToString(),
                ["SkipIdle"] = model.SkipIdle.ToString(),
                ["FastDiscSpeed"] = model.FastDiscSpeed.ToString(),
                ["OverclockEnable"] = model.CpuOverride.ToString(),
                ["Overclock"] = model.CpuClockRatio.ToString(System.Globalization.CultureInfo.InvariantCulture),
                ["AudioStretch"] = model.AudioStretching.ToString(),
                ["AudioBufferSize"] = model.AudioLatency.ToString(),
                ["EnableCheats"] = model.EnableCheats.ToString(),
                ["EnableRiivolution"] = model.EnableRiivolution.ToString(),
                ["WiimoteEnableSpeaker"] = (!model.ForceDisableWiimote).ToString(),
                ["FallbackRegion"] = Math.Clamp(model.WiiRegion, 0, 3).ToString()
            },
            ["Display"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Fullscreen"] = model.Fullscreen.ToString(),
                ["RenderToMain"] = (!model.LaunchInWindow).ToString()
            },
            ["DSP"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Volume"] = model.AudioVolume.ToString(),
                ["Backend"] = model.AudioBackend,
                ["DSPThread"] = model.DspLle.ToString(),
                ["LLE"] = model.DspLle.ToString()
            },
            ["Wii"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["WiiLanguage"] = model.WiiLanguage.ToString(),
                ["Language"] = model.WiiLanguage.ToString(),
                ["WiiSDCard"] = model.EnableSdCard.ToString(),
                ["SDCard"] = model.EnableSdCard.ToString()
            },
            ["VanzaKartLauncher"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["PerformancePreset"] = NormalizePerformancePreset(model.PerformancePreset)
            }
        };

        var gfxUpdates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Settings"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["InternalResolution"] = model.InternalResolution.ToString(),
                ["EFBScale"] = model.InternalResolution.ToString(),
                ["GFXBackend"] = model.GfxBackend,
                ["AspectRatio"] = model.AspectRatio.ToString(),
                ["VSync"] = model.VSync.ToString(),
                ["MSAA"] = model.AntiAliasing.ToString(),
                ["TexFiltMode"] = model.AnisotropicFiltering.ToString(),
                ["ShaderCompilationMode"] = model.ShaderCompilationMode.ToString(),
                ["DisableCopyFilter"] = model.RemoveBlur.ToString(),
                ["ShowFPS"] = model.ShowFPS.ToString(),
                ["HiresTextures"] = model.LoadCustomTextures.ToString(),
                ["CacheHiresTextures"] = model.PrefetchCustomTextures.ToString(),
                ["WidescreenHack"] = model.WidescreenHack.ToString(),
                ["WaitForShadersBeforeStarting"] = model.WaitForShadersBeforeStarting.ToString(),
                ["BackendMultithreading"] = model.BackendMultithreading.ToString()
            },
            ["Enhancements"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["InternalResolution"] = model.InternalResolution.ToString(),
                ["EFBScale"] = model.InternalResolution.ToString(),
                ["MaxAnisotropy"] = model.AnisotropicFiltering.ToString(),
                ["TexFiltMode"] = model.AnisotropicFiltering.ToString(),
                ["DisableCopyFilter"] = model.RemoveBlur.ToString(),
                ["WidescreenHack"] = model.WidescreenHack.ToString(),
                ["Bloom"] = model.EnableBloom.ToString(),
                ["AmbientOcclusion"] = model.EnableAmbientOcclusion.ToString(),
                ["ColorCorrection"] = model.EnableColorCorrection.ToString()
            },
            ["Hardware"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["VSync"] = model.VSync.ToString()
            }
        };

        var loggerUpdates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Options"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Verbosity"] = VerbosityFromLogLevel(model.LogLevel).ToString(),
                ["WriteToFile"] = model.LogToFile.ToString()
            }
        };

        _iniService.UpdateIniOrThrow(dolphinIniPath, dolphinUpdates);
        _iniService.UpdateIniOrThrow(gfxIniPath, gfxUpdates);
        _iniService.UpdateIniOrThrow(loggerIniPath, loggerUpdates);

        // portable.txt is deliberately read-only for the launcher. Existing
        // portable and standard Dolphin installations must never be converted.
    }

    public void OptimizeForVanzaKart(DolphinSettingsModel model, string userFolderPath)
    {
        // 1. Graphics Backend: Vulkan
        model.GfxBackend = "Vulkan";

        // 2. Fullscreen enabled
        model.Fullscreen = true;

        // 3. Resolution auto-detect screen or default to 2K (4x 1440p) / 3x 1080p
        double screenWidth = SystemParameters.PrimaryScreenWidth;
        if (screenWidth >= 3840) model.InternalResolution = 6; // 4K (6x)
        else if (screenWidth >= 2560) model.InternalResolution = 4; // 2K (4x 1440p)
        else if (screenWidth >= 1920) model.InternalResolution = 3; // 1080p (3x)
        else model.InternalResolution = 2; // 720p (2x)

        // 4. Aspect ratio: Force 16:9 + Widescreen Hack
        model.AspectRatio = 1; // Force 16:9
        model.Force169 = true;
        model.WidescreenHack = true;

        // 5. Shader Compilation Mode: Hybrid Ubershaders (2)
        model.ShaderCompilationMode = 2;
        model.Ubershaders = true;

        // 6. Enhancements: Load Custom Textures & Remove Blur
        model.LoadCustomTextures = true;
        model.PrefetchCustomTextures = true;
        model.RemoveBlur = true; // Disable Copy Filter
        model.VSync = false;
        model.AntiAliasing = 0; // Off for best perf or 2x
        model.AnisotropicFiltering = 4; // 16x

        // 7. Audio: Cubeb & Stretching enabled
        model.AudioBackend = "Cubeb";
        model.AudioStretching = true;
        model.AudioVolume = 100;
        model.AudioLatency = 20;
        model.DspLle = false; // HLE

        // 8. Performance & Wii System: Dual Core, Skip Idle, Fast Disc Speed, English Console Language
        model.DualCore = true;
        model.SkipIdle = true;
        model.FastDiscSpeed = true;
        model.CpuOverride = false;
        model.CpuClockRatio = 1.0f;
        model.WiiLanguage = 1; // English
        model.WiiRegion = 2; // PAL / Europe
        model.PerformancePreset = "VanzaKart Recommended";

        // Save immediately
        SaveSettings(userFolderPath, model);
    }

    public void ResetCategoryDefaults(DolphinSettingsModel model, string category, string userFolderPath)
    {
        switch (category?.ToLowerInvariant())
        {
            case "video":
                model.GfxBackend = "Vulkan";
                model.InternalResolution = 3; // 1080p
                model.Fullscreen = true;
                model.AspectRatio = 1;
                model.VSync = false;
                model.AntiAliasing = 0;
                model.AnisotropicFiltering = 4;
                model.ShaderCompilationMode = 2;
                model.Force169 = true;
                model.WidescreenHack = false;
                model.RemoveBlur = true;
                model.ShowFPS = false;
                model.Ubershaders = true;
                model.TextureCacheAccuracy = 0;
                model.FrameLimit = 0;
                model.RefreshRate = 0;
                break;
            case "audio":
                model.AudioVolume = 100;
                model.AudioBackend = "Cubeb";
                model.DspLle = false;
                model.AudioStretching = true;
                model.AudioLatency = 20;
                break;
            case "controller":
                model.AnalogSensitivity = 100;
                model.AnalogDeadzone = 10;
                model.Vibration = true;
                model.ControllerPreset = "Default GamePad";
                break;
            case "wii":
                model.WiiLanguage = 1;
                model.WiiRegion = 2;
                model.SystemTimeSync = true;
                model.EnableSdCard = true;
                model.ForceDisableWiimote = true;
                model.LaunchInWindow = false;
                model.RetroRewind = true;
                model.EnableCheats = true;
                model.EnableRiivolution = true;
                break;
            case "performance":
                model.CpuOverride = false;
                model.CpuClockRatio = 1.0f;
                model.DualCore = true;
                model.SyncGpu = "Auto";
                model.SkipIdle = true;
                model.FastDiscSpeed = true;
                model.PerformancePreset = "Balanced";
                break;
            case "enhancements":
                model.LoadCustomTextures = true;
                model.PrefetchCustomTextures = true;
                model.PostProcessingShader = "Off";
                model.EnableBloom = false;
                model.EnableAmbientOcclusion = false;
                model.EnableColorCorrection = false;
                model.Gamma = 1.0f;
                model.Brightness = 100;
                break;
            case "advanced":
                model.LogLevel = "Notice";
                model.LogToFile = true;
                model.WaitForShadersBeforeStarting = false;
                model.BackendMultithreading = true;
                model.DebugMode = false;
                break;
        }

        SaveSettings(userFolderPath, model);
    }

    public void ResetAllDefaults(DolphinSettingsModel model, string userFolderPath)
    {
        var defaultModel = new DolphinSettingsModel
        {
            DolphinExecutablePath = model.DolphinExecutablePath,
            UserFolderPath = model.UserFolderPath,
            ModpackPath = model.ModpackPath
        };

        // Copy default properties to model
        foreach (var prop in typeof(DolphinSettingsModel).GetProperties())
        {
            if (prop.CanWrite && prop.Name != nameof(DolphinSettingsModel.DolphinExecutablePath) &&
                prop.Name != nameof(DolphinSettingsModel.UserFolderPath) &&
                prop.Name != nameof(DolphinSettingsModel.ModpackPath))
            {
                prop.SetValue(model, prop.GetValue(defaultModel));
            }
        }

        SaveSettings(userFolderPath, model);
    }

    public string BackupConfiguration(string userFolderPath)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || !Directory.Exists(userFolderPath))
        {
            throw new DirectoryNotFoundException("User folder path is invalid or does not exist.");
        }

        string configDir = Path.Combine(userFolderPath, "Config");
        if (!Directory.Exists(configDir))
        {
            throw new DirectoryNotFoundException("Dolphin Config folder does not exist.");
        }

        string backupDir = Path.Combine(userFolderPath, "Backups");
        Directory.CreateDirectory(backupDir);

        string backupZipPath = Path.Combine(backupDir, $"Dolphin_Config_Backup_{DateTime.Now:yyyyMMdd_HHmmss}.zip");
        ZipFile.CreateFromDirectory(configDir, backupZipPath);
        return backupZipPath;
    }

    public void RestoreConfiguration(string backupZipPath, string userFolderPath)
    {
        if (!File.Exists(backupZipPath))
        {
            throw new FileNotFoundException("Backup zip file not found.");
        }

        string configDir = Path.Combine(userFolderPath, "Config");
        if (Directory.Exists(configDir))
        {
            Directory.Delete(configDir, recursive: true);
        }

        ZipFile.ExtractToDirectory(backupZipPath, configDir);
    }

    public static string NormalizePerformancePreset(string? value) =>
        value?.Trim() switch
        {
            "VanzaKart Recommended" => "VanzaKart Recommended",
            "High-Performance" => "High-Performance",
            "Low-End" => "Low-End",
            _ => "Balanced"
        };

    private static int VerbosityFromLogLevel(string? level) =>
        level?.Trim().ToLowerInvariant() switch
        {
            "error" => 2,
            "warning" => 3,
            "info" => 4,
            "debug" => 5,
            _ => 1
        };

    private static string LogLevelFromVerbosity(string? verbosity) =>
        int.TryParse(verbosity, out var level)
            ? level switch
            {
                2 => "Error",
                3 => "Warning",
                4 => "Info",
                5 => "Debug",
                _ => "Notice"
            }
            : "Notice";
}
