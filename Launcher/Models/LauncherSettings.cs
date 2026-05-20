using System.IO;

namespace VanzaKartLauncher.Models;

public sealed class LauncherSettings
{
    public string DolphinPath { get; set; } = string.Empty;
    public string RomPath { get; set; } = string.Empty;
    public string UserFolderPath { get; set; } = string.Empty;

    public string GetModFolder()
    {
        if (string.IsNullOrWhiteSpace(UserFolderPath))
        {
            return Path.Combine(AppContext.BaseDirectory, "Modpack");
        }

        return Path.Combine(UserFolderPath, "Load", "Riivolution");
    }
}