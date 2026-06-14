using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class VersionInfo
{
    [JsonPropertyName("mod_version")]
    public string ModVersion { get; set; } = string.Empty;

    [JsonPropertyName("launcher_version")]
    public string LauncherVersion { get; set; } = string.Empty;

    [JsonPropertyName("mod_url")]
    public string ModUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_mirrors")]
    public string[] ModMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_sha256")]
    public string ModSha256 { get; set; } = string.Empty;

    [JsonPropertyName("launcher_url")]
    public string LauncherUrl { get; set; } = string.Empty;

    [JsonPropertyName("launcher_mirrors")]
    public string[] LauncherMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("changelog")]
    public string[] Changelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_manifest_url")]
    public string ModManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_url")]
    public string ModFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_mirrors")]
    public string[] ModFilesMirrors { get; set; } = Array.Empty<string>();
}
