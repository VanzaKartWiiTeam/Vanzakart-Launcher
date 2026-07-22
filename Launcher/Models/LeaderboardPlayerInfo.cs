using System.Text.Json.Serialization;
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
            if (PrestigeRank >= 1 && PrestigeRank <= 8)
            {
                return $"https://sitodaking.it:8443/FOOTAGE/ranks/rank-{PrestigeRank}.png";
            }
            return _rankImageUrl;
        }
        init => _rankImageUrl = value;
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
