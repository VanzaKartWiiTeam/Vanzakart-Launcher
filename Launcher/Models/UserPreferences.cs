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
    public string LastKnownLatestBetaModVersion { get; set; } = string.Empty;
    public string BetaAccessToken { get; set; } = string.Empty;
    public ModReleaseChannel ModReleaseChannel { get; set; } = ModReleaseChannel.Stable;

    /// <summary>UI language code ("en"/"it"). Empty means "follow the Windows display language".</summary>
    public string Language { get; set; } = string.Empty;
}
