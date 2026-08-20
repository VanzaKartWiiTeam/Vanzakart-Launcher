using System.IO;
using System.Text.Json.Serialization;
using VanzaKartLauncher.Services;
using VanzaKartLauncher.ViewModels;

namespace VanzaKartLauncher.Models;

public sealed class LeaderboardPlayerInfo : BaseViewModel
{
    private int? _displayPosition;

    public int Position { get; init; }
    public string Name { get; init; } = string.Empty;
    public int Points { get; init; }

    [JsonPropertyName("fc")]
    public string FriendCode { get; init; } = string.Empty;

    [JsonPropertyName("prestigeRank")]
    public int PrestigeRank { get; init; }

    [JsonPropertyName("wins")]
    public int Wins { get; init; }

    [JsonPropertyName("races")]
    public int Races { get; init; }

    [JsonPropertyName("games")]
    public int Games { get; init; }

    public int TotalGames => Games > 0 ? Games : Races;

    [JsonPropertyName("winrate")]
    public double Winrate { get; init; }

    public string WinsLabel => $"{Wins:N0}";
    public string GamesLabel => $"{TotalGames:N0}";
    public string WinrateLabel => $"{Winrate:F1}%";
    public string GamesStatsLine => $"W: {Wins:N0} | G: {TotalGames:N0} ({Winrate:F1}%)";

    public DateTimeOffset? LastSeen { get; init; }
    public bool IsSuspicious { get; init; }
    public int VrLast24Hours { get; init; }
    public int VrLastWeek { get; init; }
    public int VrLastMonth { get; init; }
    private string? _rankImageUrl;
    public string? RankImageUrl
    {
        get
        {
            if (PrestigeRank < 1)
            {
                return null;
            }

            if (!string.IsNullOrWhiteSpace(_rankImageUrl))
            {
                if (File.Exists(_rankImageUrl))
                {
                    return _rankImageUrl;
                }
            }

            var cacheDir = Path.Combine(AppContext.BaseDirectory, "Cache", "RankImages");
            var localCache = Path.Combine(cacheDir, $"rank-{PrestigeRank}.png");
            if (File.Exists(localCache))
            {
                return localCache;
            }

            var fallbackCache = Path.Combine(cacheDir, "rank-1.png");
            if (File.Exists(fallbackCache))
            {
                return fallbackCache;
            }

            return !string.IsNullOrWhiteSpace(_rankImageUrl)
                ? _rankImageUrl
                : $"{LauncherConfig.RankImagesBaseUrl.TrimEnd('/')}/rank-{PrestigeRank}.png";
        }
        set
        {
            if (SetProperty(ref _rankImageUrl, value))
            {
                OnPropertyChanged(nameof(HasRankImage));
            }
        }
    }

    public bool IsSelf { get; set; }

    [JsonPropertyName("mii_data")]
    public string? MiiData { get; set; }

    [JsonPropertyName("mii_image")]
    public string? MiiImage { get; set; }

    private string? _avatarImagePath;
    public string? AvatarImagePath
    {
        get => _avatarImagePath;
        set
        {
            if (SetProperty(ref _avatarImagePath, value))
            {
                OnPropertyChanged(nameof(HasAvatarImage));
            }
        }
    }

    public bool HasAvatarImage => !string.IsNullOrWhiteSpace(AvatarImagePath);
    public bool HasRankImage => !string.IsNullOrWhiteSpace(RankImageUrl);

    public bool HasPrestigeRank => PrestigeRank >= 1;
    public string PrestigeRankLabel => HasPrestigeRank ? PrestigeRank.ToString() : "-";

    public string PrestigeBadgeColor1 => PrestigeRank switch
    {
        1 => "#FFD700", // Gold
        2 => "#C0C0C0", // Silver
        3 => "#CD7F32", // Bronze
        4 => "#00F2FF", // Cyan
        5 => "#B000FF", // Purple
        6 => "#FF6B7A", // Rose
        7 => "#5CE1A3", // Emerald
        8 => "#FFD166", // Amber
        > 8 => "#FFD166", // Higher ranks fallback color
        _ => "#3A4255"  // Default dark
    };

    public string PrestigeBadgeColor2 => PrestigeRank switch
    {
        1 => "#FF8C00", // Dark Gold
        2 => "#7A8899", // Steel
        3 => "#8B4513", // Saddle Brown
        4 => "#0088AA", // Deep Cyan
        5 => "#7000AA", // Deep Purple
        6 => "#CC3355", // Deep Rose
        7 => "#2A9D6E", // Deep Emerald
        8 => "#CC9933", // Deep Amber
        > 8 => "#CC9933", // Higher ranks fallback color
        _ => "#252A34"  // Default darker
    };

    public int DisplayPosition
    {
        get => _displayPosition ?? Position;
        set
        {
            if (_displayPosition == value) return;
            _displayPosition = value;
            OnPropertyChanged();
        }
    }

    public string DisplayPoints => $"{Points:N0} VR";
    public string GlobalRankLabel => $"#{Position:N0}";
    public string PrestigeLabel => PrestigeRank > 0 ? $"Prestige {PrestigeRank}" : "No prestige";
    public string CompactPrestigeLabel => $"P{PrestigeRank}";
    public string PodiumStatsLine => $"{GlobalRankLabel} | {PrestigeLabel}";
    public string VrLast24HoursLabel => FormatVrChange(VrLast24Hours);
    public string VrLastWeekLabel => FormatVrChange(VrLastWeek);
    public string VrLastMonthLabel => FormatVrChange(VrLastMonth);
    public string VrTrendToolTip => $"24 hours: {VrLast24HoursLabel} VR\n7 days: {VrLastWeekLabel} VR\n30 days: {VrLastMonthLabel} VR";
    public string VrTrendColor => VrLast24Hours > 0 ? "#5CE1A3" : VrLast24Hours < 0 ? "#FF6B7A" : "#8290A8";
    public string LastSeenLabel => LastSeen.HasValue
        ? LastSeen.Value.ToLocalTime().ToString("dd MMM, HH:mm")
        : "Unknown";

    public string AvatarInitial => string.IsNullOrWhiteSpace(Name) ? "?" : Name.Substring(0, 1).ToUpperInvariant();

    public string AvatarGradientStart
    {
        get
        {
            var gradients = GetGradientColors();
            return gradients.start;
        }
    }

    public string AvatarGradientEnd
    {
        get
        {
            var gradients = GetGradientColors();
            return gradients.end;
        }
    }

    private (string start, string end) GetGradientColors()
    {
        if (string.IsNullOrWhiteSpace(Name)) return ("#FF0066", "#FF7A00");
        
        var gradientPairs = new[]
        {
            ("#FF0066", "#FF7A00"), // Sunset Fire
            ("#00F2FF", "#B000FF"), // Neon Purple
            ("#00FF66", "#00F2FF"), // Mint Cyan
            ("#7A00FF", "#FF00FF"), // Phonic Pink
            ("#FFCC00", "#FF6600")  // Gold Orange
        };
        
        int hash = 0;
        foreach (char c in Name)
        {
            hash += c;
        }
        
        int index = Math.Abs(hash) % gradientPairs.Length;
        return gradientPairs[index];
    }

    private static string FormatVrChange(int value) => value switch
    {
        > 0 => $"+{value:N0}",
        < 0 => $"{value:N0}",
        _ => "-"
    };
}
