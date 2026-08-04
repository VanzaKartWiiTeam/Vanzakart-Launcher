using System.IO;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class SettingsService
{
    private readonly string _settingsPath;
    private readonly string _legacySettingsPath;
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        WriteIndented = true
    };

    public SettingsService()
        : this(
            GetPersistentSettingsPath(),
            Path.Combine(AppContext.BaseDirectory, "launcher_settings.json"))
    {
    }

    internal SettingsService(string settingsPath, string legacySettingsPath)
    {
        _settingsPath = settingsPath;
        _legacySettingsPath = legacySettingsPath;
    }

    public LauncherSettings Load()
    {
        if (TryLoad(_settingsPath, out var settings))
        {
            SynchronizeLegacyCopy(settings);
            return settings;
        }

        if (TryLoad(_legacySettingsPath, out settings))
        {
            // Previous versions stored these paths beside the executable. Migrate
            // them before a launcher update can replace the installation folder.
            WriteAtomically(_settingsPath, settings);
            return settings;
        }

        return new LauncherSettings();
    }

    public void Save(LauncherSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);

        WriteAtomically(_settingsPath, settings);
        SynchronizeLegacyCopy(settings);
    }

    public string GetSettingsPath() => _settingsPath;

    public static string GetPersistentSettingsPath()
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return Path.Combine(localAppData, "VanzaKart", "Launcher", "launcher_settings.json");
    }

    private static bool TryLoad(string path, out LauncherSettings settings)
    {
        settings = new LauncherSettings();

        try
        {
            if (!File.Exists(path))
            {
                return false;
            }

            var json = File.ReadAllText(path);
            var loaded = JsonSerializer.Deserialize<LauncherSettings>(json);
            if (loaded is null)
            {
                return false;
            }

            settings = loaded;
            return true;
        }
        catch
        {
            return false;
        }
    }

    private void SynchronizeLegacyCopy(LauncherSettings settings)
    {
        if (string.Equals(_settingsPath, _legacySettingsPath, StringComparison.OrdinalIgnoreCase) ||
            !File.Exists(_legacySettingsPath))
        {
            return;
        }

        try
        {
            WriteAtomically(_legacySettingsPath, settings);
        }
        catch
        {
            // The installed application directory can be read-only. The durable
            // copy in LocalAppData remains authoritative.
        }
    }

    private static void WriteAtomically(string path, LauncherSettings settings)
    {
        var directory = Path.GetDirectoryName(path)
            ?? throw new InvalidOperationException("The launcher settings path has no parent directory.");
        Directory.CreateDirectory(directory);

        var temporaryPath = Path.Combine(directory, $".{Path.GetFileName(path)}.{Guid.NewGuid():N}.tmp");
        try
        {
            var json = JsonSerializer.Serialize(settings, SerializerOptions);
            File.WriteAllText(temporaryPath, json);
            File.Move(temporaryPath, path, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }
}
