using System;

namespace VanzaKartLauncher.Models;

public sealed class NewsItem
{
    public string Title { get; init; } = string.Empty;
    public string Category { get; init; } = string.Empty;
    public string Version { get; init; } = string.Empty;
    public string Summary { get; init; } = string.Empty;
    public string DateLabel { get; init; } = string.Empty;
    public bool IsPinned { get; init; }

    // Media support
    public string? MediaPath { get; init; }
    public bool HasMedia => !string.IsNullOrEmpty(MediaPath);
    public bool HasImage => HasMedia && IsImageFile(MediaPath!);
    public bool HasVideo => HasMedia && IsVideoFile(MediaPath!);

    private static bool IsImageFile(string path)
    {
        if (string.IsNullOrEmpty(path)) return false;
        var lower = path.ToLowerInvariant();
        return lower.EndsWith(".png") || 
               lower.EndsWith(".jpg") || 
               lower.EndsWith(".jpeg") || 
               lower.EndsWith(".gif") || 
               lower.EndsWith(".webp") || 
               lower.Contains("photo-") || 
               lower.Contains("image");
    }

    private static bool IsVideoFile(string path)
    {
        if (string.IsNullOrEmpty(path)) return false;
        var lower = path.ToLowerInvariant();
        return lower.EndsWith(".mp4") || 
               lower.EndsWith(".webm") || 
               lower.EndsWith(".wmv") || 
               lower.EndsWith(".avi") || 
               lower.EndsWith(".mov") || 
               lower.Contains("video");
    }
}
