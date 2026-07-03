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
    public ModChannelInstallationState Stable { get; set; } = new();
    public ModChannelInstallationState Beta { get; set; } = new();

    public ModChannelInstallationState Get(ModReleaseChannel channel) =>
        channel == ModReleaseChannel.Beta ? Beta : Stable;
}

public sealed class ModChannelInstallationState
{
    public string Version { get; set; } = string.Empty;
    public DateTime? InstalledAtUtc { get; set; }
}
