using System.IO;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class AddonInfo : INotifyPropertyChanged
{
    private bool _isEnabled;

    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public string Author { get; set; } = string.Empty;
    public string Source { get; set; } = "Local";
    public string SourceUrl { get; set; } = string.Empty;
    public string PreviewUrl { get; set; } = string.Empty;
    public DateTime InstalledUtc { get; set; } = DateTime.UtcNow;
    public List<string> Files { get; set; } = new();
    public bool IsManaged { get; set; } = true;

    public bool IsEnabled
    {
        get => _isEnabled;
        set
        {
            if (_isEnabled == value) return;
            _isEnabled = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsEnabled)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(StatusText)));
        }
    }

    [JsonIgnore]
    public string StatusText => IsEnabled ? "Enabled" : "Disabled";

    [JsonIgnore]
    public string FileCountText => Files.Count == 1 ? "1 file" : $"{Files.Count} files";

    public event PropertyChangedEventHandler? PropertyChanged;
}

public sealed class GameBananaMod
{
    public int Id { get; set; }
    public string Name { get; set; } = string.Empty;
    public string Author { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
    public string ProfileUrl { get; set; } = string.Empty;
    public string PreviewUrl { get; set; } = string.Empty;
    public List<GameBananaFile> Files { get; set; } = new();
    public int Views { get; set; }
    public int Likes { get; set; }
    public int Downloads { get; set; }

    public string StatsText => $"{Views:N0} views  •  {Downloads:N0} downloads  •  {Likes:N0} likes";
    public string FileText => Files.Count == 1 ? Files[0].SizeText : $"{Files.Count} files available";
    public string InstallText => "Install";

    public GameBananaFile? DefaultFile => Files.FirstOrDefault();

    private static string FormatBytes(long bytes)
    {
        string[] units = { "B", "KB", "MB", "GB" };
        var value = (double)bytes;
        var unit = 0;
        while (value >= 1024 && unit < units.Length - 1) { value /= 1024; unit++; }
        return $"{value:0.#} {units[unit]}";
    }
}

public sealed class GameBananaFile
{
    public int FileId { get; set; }
    public string FileName { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
    public string DownloadUrl { get; set; } = string.Empty;
    public long FileSize { get; set; }
    public int DownloadCount { get; set; }
    public DateTime DateAddedUtc { get; set; }

    public string SizeText => FormatBytes(FileSize);
    public string VariantDescription => string.IsNullOrWhiteSpace(Description)
        ? "Standard package provided by the creator."
        : Description;
    public string MetadataText
    {
        get
        {
            var date = DateAddedUtc.Year > 1970
                ? $"  •  {DateAddedUtc.ToString("MMM d, yyyy", System.Globalization.CultureInfo.InvariantCulture)}"
                : string.Empty;
            return $"{SizeText}  •  {DownloadCount:N0} downloads{date}";
        }
    }
    public string DisplayText
    {
        get
        {
            var detail = string.IsNullOrWhiteSpace(Description) ? "GameBanana download" : Description;
            return $"{FileName}\n{detail}  •  {SizeText}  •  {DownloadCount:N0} downloads";
        }
    }

    private static string FormatBytes(long bytes)
    {
        string[] units = { "B", "KB", "MB", "GB" };
        var value = (double)Math.Max(0, bytes);
        var unit = 0;
        while (value >= 1024 && unit < units.Length - 1) { value /= 1024; unit++; }
        return $"{value:0.#} {units[unit]}";
    }
}

public sealed class GameBananaSearchResult
{
    public IReadOnlyList<GameBananaMod> Mods { get; init; } = Array.Empty<GameBananaMod>();
    public int TotalAvailable { get; init; }
    public bool HasMore { get; init; }
}
