// Services/PreferencesService.cs
using System.IO;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class PreferencesService
{
    private readonly string _preferencesPath;

    public PreferencesService()
    {
        _preferencesPath = Path.Combine(AppContext.BaseDirectory, "user_preferences.json");
    }

    public UserPreferences Load()
    {
        try
        {
            if (!File.Exists(_preferencesPath))
                return new UserPreferences();

            var json = File.ReadAllText(_preferencesPath);
            return JsonSerializer.Deserialize<UserPreferences>(json) ?? new UserPreferences();
        }
        catch
        {
            return new UserPreferences();
        }
    }

    public void Save(UserPreferences prefs)
    {
        var json = JsonSerializer.Serialize(prefs, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(_preferencesPath, json);
    }

    public string GetPreferencesPath() => _preferencesPath;
}
