using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class ModManifest
{
    [JsonPropertyName("mod_version")]
    public string ModVersion { get; set; } = string.Empty;

    [JsonPropertyName("files")]
    public List<ModManifestFile> Files { get; set; } = new();
}

public sealed class ModManifestFile
{
    [JsonPropertyName("path")]
    public string Path { get; set; } = string.Empty;

    [JsonPropertyName("sha256")]
    public string Sha256 { get; set; } = string.Empty;

    [JsonPropertyName("size")]
    public long Size { get; set; }
}
