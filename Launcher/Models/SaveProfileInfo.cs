using VanzaKartLauncher.ViewModels;

namespace VanzaKartLauncher.Models;

public sealed class SaveProfileInfo : BaseViewModel
{
    private bool _isActive;

    public string DisplayName { get; init; } = string.Empty;
    public string Subtitle { get; init; } = string.Empty;
    public string FilePath { get; init; } = string.Empty;
    public string SourceLabel { get; init; } = string.Empty;
    public string MiiName { get; init; } = string.Empty;
    public string AvatarInitial { get; init; } = "M";
    public string AvatarImagePath { get; init; } = string.Empty;
    public string AvatarStatus { get; init; } = "Render queued";
    public string AccentColor { get; init; } = "#39E7FF";
    public string FriendCode { get; init; } = string.Empty;
    public uint ProfileId { get; init; }
    public uint MiiId { get; init; }
    public uint Vr { get; init; }
    public uint Br { get; init; }
    public uint Races { get; init; }
    public uint Wins { get; init; }
    public DateTime LastModifiedUtc { get; init; }
    public long SizeBytes { get; init; }
    public bool IsLauncherManaged { get; init; }
    public int SlotIndex { get; init; }
    public bool IsEmpty { get; init; }

    public bool IsActive
    {
        get => _isActive;
        set => SetProperty(ref _isActive, value);
    }

    public bool HasAvatarImage => !string.IsNullOrWhiteSpace(AvatarImagePath);
    public double WinRate => Races == 0 ? 0 : Wins / (double)Races;
    public string StatsLine => Races > 0 || Wins > 0
        ? $"VR {Vr}   BR {Br}   Wins {Wins}   Races {Races}   Win {WinRate:P0}"
        : string.Empty;
}
