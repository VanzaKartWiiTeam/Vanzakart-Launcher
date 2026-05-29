namespace VanzaKartLauncher.Models;

public sealed class NewsItem
{
    public string Title { get; init; } = string.Empty;
    public string Category { get; init; } = string.Empty;
    public string Version { get; init; } = string.Empty;
    public string Summary { get; init; } = string.Empty;
    public string DateLabel { get; init; } = string.Empty;
    public bool IsPinned { get; init; }
}
