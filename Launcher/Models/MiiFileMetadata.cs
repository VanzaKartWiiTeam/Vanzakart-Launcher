namespace VanzaKartLauncher.Models;

public sealed class MiiFileMetadata
{
    public string FormatName { get; init; } = "Mii";
    public string SuggestedName { get; init; } = "Imported Mii";
    public long SizeBytes { get; init; }
    public string Sha256 { get; init; } = string.Empty;
    public string RawMiiBase64 { get; init; } = string.Empty;
    public string StudioData { get; init; } = string.Empty;
    public string CreatorName { get; init; } = string.Empty;
    public string FavoriteColor { get; init; } = "#39E7FF";
    public int FavoriteColorIndex { get; init; }
    public uint MiiId { get; init; }
    public bool IsFemale { get; init; }
    public bool IsFavorite { get; init; }
    public bool IsRealMii => !string.IsNullOrWhiteSpace(RawMiiBase64);
}
