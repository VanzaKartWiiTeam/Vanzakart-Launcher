using System.IO;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class ModInstallationStateService
{
    private readonly string _statePath = Path.Combine(AppContext.BaseDirectory, "mod_install_state.json");

    public ModInstallationState Load(string legacyVersion)
    {
        try
        {
            if (File.Exists(_statePath))
            {
                var json = File.ReadAllText(_statePath);
                var state = JsonSerializer.Deserialize<ModInstallationState>(json);
                if (state != null)
                {
                    state.Stable ??= new ModChannelInstallationState();
                    state.Beta ??= new ModChannelInstallationState();

                    // Migrate the original single-installation state without losing
                    // whichever channel the user had installed before this update.
                    using var document = JsonDocument.Parse(json);
                    var root = document.RootElement;
                    if (root.TryGetProperty("Version", out var versionElement))
                    {
                        var version = versionElement.GetString() ?? string.Empty;
                        var channel = ModReleaseChannel.Stable;
                        if (root.TryGetProperty("Channel", out var channelElement) &&
                            Enum.TryParse(channelElement.GetString(), true, out ModReleaseChannel parsedChannel))
                        {
                            channel = parsedChannel;
                        }

                        var installedAt = root.TryGetProperty("InstalledAtUtc", out var installedAtElement) &&
                                          installedAtElement.TryGetDateTime(out var parsedInstalledAt)
                            ? parsedInstalledAt
                            : (DateTime?)null;
                        var migrated = state.Get(channel);
                        if (string.IsNullOrWhiteSpace(migrated.Version))
                        {
                            migrated.Version = version;
                            migrated.InstalledAtUtc = installedAt;
                            Save(state);
                        }
                    }
                    return state;
                }
            }
        }
        catch
        {
            // A missing/corrupt state file is migrated as a legacy stable installation.
        }

        var legacyState = new ModInstallationState();
        legacyState.Stable.Version = legacyVersion;
        return legacyState;
    }

    public void Save(ModInstallationState state)
    {
        var json = JsonSerializer.Serialize(state, new JsonSerializerOptions { WriteIndented = true });
        var temporaryPath = _statePath + ".tmp";
        File.WriteAllText(temporaryPath, json);
        File.Move(temporaryPath, _statePath, overwrite: true);
    }

    public string GetStatePath() => _statePath;
}
