using DiscordRPC;

namespace VanzaKartLauncher.Services;

public sealed class DiscordPresenceService : IDisposable
{
    private DiscordRpcClient? _client;

    public void Initialize()
    {
        try
        {
            _client = new DiscordRpcClient(LauncherConfig.DiscordAppId);
            _client.Initialize();
            SetLauncherIdle();
        }
        catch
        {
        }
    }

    public void SetLauncherIdle()
    {
        SetPresence("In the launcher", "Ready to race");
    }

    public void SetDownloading()
    {
        SetPresence("Downloading the mod", "Fetching files...");
    }

    public void SetExtracting()
    {
        SetPresence("Extracting files", "Almost ready to race");
    }

    public void SetPlaying()
    {
        if (_client is not { IsInitialized: true }) return;

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
        if (_client is not { IsInitialized: true }) return;

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

    public void Dispose()
    {
        try
        {
            _client?.Dispose();
        }
        catch
        {
        }
    }
}