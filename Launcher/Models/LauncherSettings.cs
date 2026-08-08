using System.IO;

namespace VanzaKartLauncher.Models;

public sealed class LauncherSettings
{
    private string _dolphinPath = string.Empty;
    private string _romPath = string.Empty;
    private string _userFolderPath = string.Empty;

    public string DolphinPath
    {
        get => _dolphinPath;
        set => _dolphinPath = value?.Trim() ?? string.Empty;
    }

    public string RomPath
    {
        get => _romPath;
        set => _romPath = value?.Trim() ?? string.Empty;
    }

    public string UserFolderPath
    {
        get => _userFolderPath;
        set => _userFolderPath = value?.Trim().TrimEnd('\\', '/') ?? string.Empty;
    }

    public string ControllerConfigurationMode { get; set; } = string.Empty;

    public string GetModFolder()
    {
        if (string.IsNullOrWhiteSpace(UserFolderPath))
        {
            return Path.Combine(AppContext.BaseDirectory, "Modpack");
        }

        return Path.Combine(UserFolderPath, "Load", "Riivolution");
    }
}
