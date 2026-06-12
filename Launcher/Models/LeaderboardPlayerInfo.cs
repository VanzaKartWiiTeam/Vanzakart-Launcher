using System.ComponentModel;
using System.Text.Json.Serialization;
using VanzaKartLauncher.ViewModels;

namespace VanzaKartLauncher.Models;

public sealed class LeaderboardPlayerInfo : BaseViewModel
{
    public int Position { get; init; }
    public string Name { get; init; } = string.Empty;
    public int Points { get; init; }

    [JsonPropertyName("fc")]
    public string FriendCode { get; init; } = string.Empty;
    public int Wins { get; init; }
    public int Races { get; init; }
    public double WinRate { get; init; }
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

    public string WinRatePercent => $"{WinRate:F1}%";
    public string DisplayPoints => $"{Points:N0} EV";

    public string Rank
    {
        get
        {
            if (Points >= 9000) return "Master";
            if (Points >= 7500) return "Diamond";
            if (Points >= 6000) return "Platinum";
            if (Points >= 4500) return "Gold";
            if (Points >= 3000) return "Silver";
            return "Bronze";
        }
    }

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
}
