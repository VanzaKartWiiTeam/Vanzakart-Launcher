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
                var state = JsonSerializer.Deserialize<ModInstallationState>(File.ReadAllText(_statePath));
                if (state != null)
                {
                    if (!Enum.IsDefined(state.Channel))
                    {
                        state.Channel = ModReleaseChannel.Stable;
                    }
                    return state;
                }
            }
        }
        catch
        {
            // A missing/corrupt state file is migrated as a legacy stable installation.
        }

        return new ModInstallationState
        {
            Version = legacyVersion,
            Channel = ModReleaseChannel.Stable
        };
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
