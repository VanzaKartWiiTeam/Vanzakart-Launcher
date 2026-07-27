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

        var dolphinIni = _iniService.ReadIni(dolphinIniPath);
        var gfxIni = _iniService.ReadIni(gfxIniPath);

        // --- Core / Video INI read ---
        if (dolphinIni.TryGetValue("Core", out var coreSection))
        {
            if (coreSection.TryGetValue("GFXBackend", out var gfxBackend)) model.GfxBackend = gfxBackend;
            if (coreSection.TryGetValue("CPUThread", out var cpuThread)) model.DualCore = parseBool(cpuThread, true);
            if (coreSection.TryGetValue("SkipIdle", out var skipIdle)) model.SkipIdle = parseBool(skipIdle, true);
            if (coreSection.TryGetValue("FastDiscSpeed", out var fastDisc)) model.FastDiscSpeed = parseBool(fastDisc, true);
            if (coreSection.TryGetValue("OverclockEnable", out var ocEnable)) model.CpuOverride = parseBool(ocEnable, false);
            if (coreSection.TryGetValue("Overclock", out var ocVal) && float.TryParse(ocVal, System.Globalization.CultureInfo.InvariantCulture, out float oc)) model.CpuClockRatio = oc;
            if (coreSection.TryGetValue("AudioStretch", out var audioStretch)) model.AudioStretching = parseBool(audioStretch, true);
            if (coreSection.TryGetValue("AudioBufferSize", out var audioBuffer) && int.TryParse(audioBuffer, out int buf)) model.AudioLatency = buf;
            if (coreSection.TryGetValue("EnableCheats", out var cheats)) model.EnableCheats = parseBool(cheats, true);
            if (coreSection.TryGetValue("EnableRiivolution", out var riiv)) model.EnableRiivolution = parseBool(riiv, true);
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
        }

        if (dolphinIni.TryGetValue("Wii", out var wiiSection))
        {
            if (wiiSection.TryGetValue("WiiLanguage", out var lang) && int.TryParse(lang, out int l)) model.WiiLanguage = l;
            if (wiiSection.TryGetValue("WiiSDCard", out var sd)) model.EnableSdCard = parseBool(sd, true);
        }

        // --- GFX.ini read ---
        if (gfxIni.TryGetValue("Settings", out var gfxSettings))
        {
            if (gfxSettings.TryGetValue("InternalResolution", out var res) && int.TryParse(res, out int r)) model.InternalResolution = r;
            if (gfxSettings.TryGetValue("AspectRatio", out var ar) && int.TryParse(ar, out int a)) model.AspectRatio = a;
            if (gfxSettings.TryGetValue("VSync", out var vs) && parseBool(vs, false)) model.VSync = true;
            if (gfxSettings.TryGetValue("MSAA", out var msaa) && int.TryParse(msaa, out int m)) model.AntiAliasing = m;
            if (gfxSettings.TryGetValue("TexFiltMode", out var tf) && int.TryParse(tf, out int t)) model.AnisotropicFiltering = t;
            if (gfxSettings.TryGetValue("ShaderCompilationMode", out var scm) && int.TryParse(scm, out int s)) model.ShaderCompilationMode = s;
            if (gfxSettings.TryGetValue("DisableCopyFilter", out var dcf)) model.RemoveBlur = parseBool(dcf, true);
            if (gfxSettings.TryGetValue("ShowFPS", out var fps)) model.ShowFPS = parseBool(fps, false);
            if (gfxSettings.TryGetValue("HiresTextures", out var ht)) model.LoadCustomTextures = parseBool(ht, true);
            if (gfxSettings.TryGetValue("CacheHiresTextures", out var cht)) model.PrefetchCustomTextures = parseBool(cht, true);
            if (gfxSettings.TryGetValue("WidescreenHack", out var wsh)) model.WidescreenHack = parseBool(wsh, false);
            if (gfxSettings.TryGetValue("PostProcessingShader", out var pps)) model.PostProcessingShader = pps;
        }

        if (gfxIni.TryGetValue("Enhancements", out var gfxEnh))
        {
            if (gfxEnh.TryGetValue("Bloom", out var bloom)) model.EnableBloom = parseBool(bloom, false);
            if (gfxEnh.TryGetValue("AmbientOcclusion", out var ao)) model.EnableAmbientOcclusion = parseBool(ao, false);
            if (gfxEnh.TryGetValue("ColorCorrection", out var cc)) model.EnableColorCorrection = parseBool(cc, false);
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

        var dolphinUpdates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Core"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["GFXBackend"] = model.GfxBackend,
                ["CPUThread"] = model.DualCore.ToString(),
                ["SkipIdle"] = model.SkipIdle.ToString(),
                ["FastDiscSpeed"] = model.FastDiscSpeed.ToString(),
                ["OverclockEnable"] = model.CpuOverride.ToString(),
                ["Overclock"] = model.CpuClockRatio.ToString(System.Globalization.CultureInfo.InvariantCulture),
                ["AudioStretch"] = model.AudioStretching.ToString(),
                ["AudioBufferSize"] = model.AudioLatency.ToString(),
                ["EnableCheats"] = model.EnableCheats.ToString(),
                ["EnableRiivolution"] = model.EnableRiivolution.ToString()
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
                ["DSPThread"] = model.DspLle.ToString()
            },
            ["Wii"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["WiiLanguage"] = model.WiiLanguage.ToString(),
                ["WiiSDCard"] = model.EnableSdCard.ToString()
            }
        };

        var gfxUpdates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Settings"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["InternalResolution"] = model.InternalResolution.ToString(),
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
                ["PostProcessingShader"] = model.PostProcessingShader ?? ""
            },
            ["Enhancements"] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Bloom"] = model.EnableBloom.ToString(),
                ["AmbientOcclusion"] = model.EnableAmbientOcclusion.ToString(),
                ["ColorCorrection"] = model.EnableColorCorrection.ToString()
            }
        };

        _iniService.UpdateIni(dolphinIniPath, dolphinUpdates);
        _iniService.UpdateIni(gfxIniPath, gfxUpdates);

        // Portable mode update
        if (!string.IsNullOrWhiteSpace(model.DolphinExecutablePath))
        {
            try
            {
                string dDir = Path.GetDirectoryName(model.DolphinExecutablePath) ?? "";
                if (!string.IsNullOrWhiteSpace(dDir) && Directory.Exists(dDir))
                {
                    string pFile = Path.Combine(dDir, "portable.txt");
                    if (model.PortableMode && !File.Exists(pFile))
                    {
                        File.WriteAllText(pFile, "User directory created by VanzaKart Launcher");
                    }
                    else if (!model.PortableMode && File.Exists(pFile))
                    {
                        File.Delete(pFile);
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"[DolphinSettingsManager] Portable mode error: {ex.Message}");
            }
        }
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
                model.WiiRegion = 1;
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
                model.DebugMode = false;
                model.PortableMode = false;
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
}
