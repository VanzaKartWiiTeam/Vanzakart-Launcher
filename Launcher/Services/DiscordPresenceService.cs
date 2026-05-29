// Services/DiscordPresenceService.cs (aggiornato con toggle)
using DiscordRPC;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class DiscordPresenceService : IDisposable
{
    private DiscordRpcClient? _client;
    private UserPreferences _preferences;

    public DiscordPresenceService(UserPreferences preferences)
    {
        _preferences = preferences;
    }

    public void Initialize()
    {
        if (!_preferences.DiscordRpcEnabled) return;
        try
        {
            _client = new DiscordRpcClient(LauncherConfig.DiscordAppId);
            _client.Initialize();
            SetLauncherIdle();
        }
        catch { }
    }

    public void SetLauncherIdle()
    {
        if (!_preferences.DiscordRpcEnabled || _client == null) return;
        SetPresence("In the launcher", "Ready to race");
    }

    public void SetDownloading()
    {
        if (!_preferences.DiscordRpcEnabled || _client == null) return;
        SetPresence("Downloading the mod", "Fetching files...");
    }

    public void SetExtracting()
    {
        if (!_preferences.DiscordRpcEnabled || _client == null) return;
        SetPresence("Extracting files", "Almost ready to race");
    }

    public void SetPlaying()
    {
        if (!_preferences.DiscordRpcEnabled || _client == null) return;
        _client.SetPresence(new RichPresence
        {
            Details = "On the track!",
            State = "Playing VanzaKart",
            Timestamps = Timestamps.Now,
            Assets = new Assets
            {
                LargeImageKey = "vklogo",
                LargeImageText = "VanzaKart Modpack"
            }
        });
    }

    private void SetPresence(string details, string state)
    {
        if (_client == null) return;
        _client.SetPresence(new RichPresence
        {
            Details = details,
            State = state,
            Assets = new Assets
            {
                LargeImageKey = "vklogo",
                LargeImageText = "VanzaKart Modpack"
            }
        });
    }

    public void UpdatePreferences(UserPreferences preferences)
    {
        _preferences = preferences;
        if (_preferences.DiscordRpcEnabled)
        {
            if (_client == null || !_client.IsInitialized)
            {
                _client?.Dispose();
                _client = new DiscordRpcClient(LauncherConfig.DiscordAppId);
                _client.Initialize();
                SetLauncherIdle();
            }
        }
        else
        {
            _client?.Dispose();
            _client = null;
        }
    }

    public void Dispose()
    {
        try { _client?.Dispose(); }
        catch { }
    }
}