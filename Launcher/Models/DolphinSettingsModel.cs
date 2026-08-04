namespace VanzaKartLauncher.Models;

public sealed class DolphinSettingsModel
{
    // --- VIDEO ---
    public string GfxBackend { get; set; } = "Vulkan"; // Vulkan, D3D11, D3D12, OpenGL, Null
    public int InternalResolution { get; set; } = 3; // 0=Native, 1=1x (720p/480p), 2=2x (720p), 3=3x (1080p), 4=4x (1440p 2K), 5=5x, 6=6x (4K)
    public bool Fullscreen { get; set; } = true;
    public int AspectRatio { get; set; } = 1; // 0=Auto, 1=Force 16:9, 2=Force 4:3, 3=Stretch
    public bool VSync { get; set; } = false;
    public int AntiAliasing { get; set; } = 0; // 0=Off, 2=2x MSAA, 4=4x MSAA, 8=8x MSAA, 16=SSAA
    public int AnisotropicFiltering { get; set; } = 4; // 0=1x, 1=2x, 2=4x, 3=8x, 4=16x
    public int ShaderCompilationMode { get; set; } = 2; // 0=Synchronous, 1=Async (Skip Drawing), 2=Async (Ubershaders / Hybrid), 3=Sync (Ubershaders)
    public bool Force169 { get; set; } = true;
    public bool WidescreenHack { get; set; } = false;
    public bool RemoveBlur { get; set; } = true; // Disable Copy Filter
    public bool ShowFPS { get; set; } = false;
    public bool Ubershaders { get; set; } = true;
    public int TextureCacheAccuracy { get; set; } = 0; // 0=Safe, 1=Medium, 2=Fast
    public int FrameLimit { get; set; } = 0; // 0=Auto, 60=60 FPS, etc.
    public int RefreshRate { get; set; } = 0; // 0=Auto

    // --- AUDIO ---
    public int AudioVolume { get; set; } = 100; // 0-100%
    public string AudioBackend { get; set; } = "Cubeb"; // Cubeb, WASAPI, OpenAL, XAudio2, Null
    public bool DspLle { get; set; } = false; // False = HLE (Fast), True = LLE (Accurate)
    public bool AudioStretching { get; set; } = true;
    public int AudioLatency { get; set; } = 20; // Buffer size ms

    // --- CONTROLLER ---
    public int SelectedPort { get; set; } = 1; // 1-4
    public string DeviceTypePort1 { get; set; } = "Standard Controller"; // Standard Controller, GameCube Controller, Wiimote, Disabled
    public string DeviceTypePort2 { get; set; } = "Disabled";
    public string DeviceTypePort3 { get; set; } = "Disabled";
    public string DeviceTypePort4 { get; set; } = "Disabled";
    public int AnalogSensitivity { get; set; } = 100; // 50-150%
    public int AnalogDeadzone { get; set; } = 10; // 0-50%
    public bool Vibration { get; set; } = true;
    public string ControllerPreset { get; set; } = "Default GamePad";

    // --- WII ---
    public int WiiLanguage { get; set; } = 1; // 1=English, 2=German, 3=French, 4=Spanish, 5=Italian, etc.
    public int WiiRegion { get; set; } = 2; // 0=NTSC-J, 1=NTSC-U, 2=PAL, 3=NTSC-K
    public bool SystemTimeSync { get; set; } = true;
    public bool EnableSdCard { get; set; } = true;
    public bool ForceDisableWiimote { get; set; } = true;
    public bool LaunchInWindow { get; set; } = false;
    public bool RetroRewind { get; set; } = true;
    public bool EnableCheats { get; set; } = true;
    public bool EnableRiivolution { get; set; } = true;

    // --- PERFORMANCE ---
    public bool CpuOverride { get; set; } = false;
    public float CpuClockRatio { get; set; } = 1.0f; // 0.5f to 3.0f
    public bool DualCore { get; set; } = true;
    public string SyncGpu { get; set; } = "Auto"; // Auto, None, Fake
    public bool SkipIdle { get; set; } = true;
    public bool FastDiscSpeed { get; set; } = true;
    public string PerformancePreset { get; set; } = "Balanced"; // Low-End, Balanced, High-Performance, VanzaKart Recommended

    // --- GRAPHICS ENHANCEMENTS ---
    public bool LoadCustomTextures { get; set; } = true;
    public bool PrefetchCustomTextures { get; set; } = true;
    public string PostProcessingShader { get; set; } = "Off";
    public bool EnableBloom { get; set; } = false;
    public bool EnableAmbientOcclusion { get; set; } = false;
    public bool EnableColorCorrection { get; set; } = false;
    public float Gamma { get; set; } = 1.0f; // 0.5 to 2.5
    public int Brightness { get; set; } = 100; // 0 to 100%

    // --- PATHS ---
    public string DolphinExecutablePath { get; set; } = "";
    public string UserFolderPath { get; set; } = "";
    public string ModpackPath { get; set; } = "";
    public string TexturePackPath { get; set; } = "";
    public string ScreenshotPath { get; set; } = "";
    public string CachePath { get; set; } = "";

    // --- ADVANCED ---
    public string LogLevel { get; set; } = "Notice"; // Notice, Error, Warning, Info, Debug
    public bool LogToFile { get; set; } = true;
    public bool WaitForShadersBeforeStarting { get; set; } = false;
    public bool BackendMultithreading { get; set; } = true;
    public bool DebugMode { get; set; } = false;
    public bool PortableMode { get; set; } = false;
}
