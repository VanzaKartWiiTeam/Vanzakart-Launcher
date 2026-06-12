// Models/UserPreferences.cs
namespace VanzaKartLauncher.Models;

public class UserPreferences
{
    public bool DiscordRpcEnabled { get; set; } = true;
    public bool AutoCheckUpdates { get; set; } = true;
    public bool SeparateSavegame { get; set; } = true;
    public int ModOptionChoice { get; set; } = 2;
    public double WindowWidth { get; set; } = 1280;
    public double WindowHeight { get; set; } = 820;
    public bool WindowMaximized { get; set; } = false;
    public DateTime? LastPlayedUtc { get; set; }
    public int LaunchCount { get; set; }
    public double TotalPlayTimeMinutes { get; set; }
    public string LastKnownLatestModVersion { get; set; } = string.Empty;
}
