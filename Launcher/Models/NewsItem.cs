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
    public string? ResolvedMediaPath => ResolveMediaPath(MediaPath);
    public bool HasMedia => !string.IsNullOrEmpty(ResolvedMediaPath);
    public bool HasImage => ResolvedMediaPath is { } path && IsImageFile(path);
    public bool HasVideo => ResolvedMediaPath is { } path && IsVideoFile(path);
    public string? ImageMediaPath => HasImage ? ResolvedMediaPath : null;
    public string? VideoMediaPath => HasVideo ? ResolvedMediaPath : null;

    private static string? ResolveMediaPath(string? path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return null;
        }

        if (!Uri.TryCreate(path, UriKind.Absolute, out var uri) ||
            !uri.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase) ||
            !uri.Host.Equals("sitodaking.it", StringComparison.OrdinalIgnoreCase) ||
            !uri.IsDefaultPort)
        {
            return path;
        }

        // The VanzaKart HTTPS media endpoint is served on 8443. The default
        // HTTPS endpoint currently presents a certificate that Windows Media
        // Player rejects with a modal warning for every news card.
        return new UriBuilder(uri) { Port = 8443 }.Uri.AbsoluteUri;
    }

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
