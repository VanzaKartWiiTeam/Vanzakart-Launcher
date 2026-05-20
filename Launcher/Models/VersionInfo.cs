using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class VersionInfo
{
    [JsonPropertyName("mod_version")]
    public string ModVersion { get; set; } = string.Empty;

    [JsonPropertyName("launcher_version")]
    public string LauncherVersion { get; set; } = string.Empty;
}
