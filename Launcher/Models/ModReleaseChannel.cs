using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

[JsonConverter(typeof(JsonStringEnumConverter))]
public enum ModReleaseChannel
{
    Stable,
    Beta
}

public sealed class ModInstallationState
{
    public string Version { get; set; } = string.Empty;
    public ModReleaseChannel Channel { get; set; } = ModReleaseChannel.Stable;
    public DateTime InstalledAtUtc { get; set; }
}
