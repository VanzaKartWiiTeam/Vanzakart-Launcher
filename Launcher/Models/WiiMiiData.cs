namespace VanzaKartLauncher.Models;

public sealed class WiiMiiData
{
    public string Name { get; init; } = "Mii";
    public string CreatorName { get; init; } = string.Empty;
    public string FormatName { get; init; } = "Wii Mii";
    public string SourceFilePath { get; init; } = string.Empty;
    public string RawMiiBase64 { get; init; } = string.Empty;
    public string StudioData { get; init; } = string.Empty;
    public string Sha256 { get; init; } = string.Empty;
    public string FavoriteColor { get; init; } = "#39E7FF";
    public string AvatarImagePath { get; init; } = string.Empty;
    public uint MiiId { get; init; }
    public int FavoriteColorIndex { get; init; }
    public int Height { get; init; }
    public int Weight { get; init; }
    public bool IsFemale { get; init; }
    public bool IsFavorite { get; init; }
    public DateTime CreatedDate { get; init; }

    public byte[] GetRawBytes()
    {
        return string.IsNullOrWhiteSpace(RawMiiBase64)
            ? Array.Empty<byte>()
            : Convert.FromBase64String(RawMiiBase64);
    }
}
