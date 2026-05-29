namespace VanzaKartLauncher.Models;

public sealed class LauncherMiiProfile
{
    public string Id { get; set; } = Guid.NewGuid().ToString("N");
    public string Name { get; set; } = "Vanza Mii";
    public string FavoriteColor { get; set; } = "#39E7FF";
    public DateTime CreatedUtc { get; set; } = DateTime.UtcNow;
    public string SourceLabel { get; set; } = "Launcher";
    public string ImportedFilePath { get; set; } = string.Empty;
    public string RawMiiBase64 { get; set; } = string.Empty;
    public string StudioData { get; set; } = string.Empty;
    public string AvatarImagePath { get; set; } = string.Empty;
    public string RenderState { get; set; } = "Queued";
    public string RenderMessage { get; set; } = "Waiting for render";
    public DateTime? LastRenderedUtc { get; set; }
    public string CreatorName { get; set; } = "VanzaKart";
    public uint MiiId { get; set; }
    public int FavoriteColorIndex { get; set; } = 3;
    public bool IsFemale { get; set; }
    public bool IsFavorite { get; set; }
    public bool HasAvatarImage => !string.IsNullOrWhiteSpace(AvatarImagePath);
    public bool IsRealMii => !string.IsNullOrWhiteSpace(RawMiiBase64);
    public string TechnicalLabel => IsRealMii ? "Wii Mii data" : "Not synced";
    public string RenderStatusText => HasAvatarImage
        ? "Rendered"
        : string.IsNullOrWhiteSpace(RenderMessage) ? RenderState : RenderMessage;
    public string AvatarInitial => string.IsNullOrWhiteSpace(Name)
        ? "M"
        : Name.Trim()[0].ToString().ToUpperInvariant();
}
