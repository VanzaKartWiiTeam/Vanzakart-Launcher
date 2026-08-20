using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;

namespace VanzaKartSetup.Services;

public sealed class NetworkService
{
    private readonly HttpClient _httpClient;

    public NetworkService()
    {
        ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls13;
        _httpClient = new HttpClient
        {
            Timeout = TimeSpan.FromSeconds(30)
        };
    }

    public async Task<bool> CheckInternetAsync(CancellationToken cancellationToken = default)
    {
        string[] testEndpoints = [
            "https://sitodaking.it:8443/Launcher/endpoints.json",
            "https://1.1.1.1",
            "https://www.google.com"
        ];

        foreach (var endpoint in testEndpoints)
        {
            try
            {
                using var request = new HttpRequestMessage(HttpMethod.Head, endpoint);
                using var response = await _httpClient.SendAsync(request, cancellationToken);
                if (response.IsSuccessStatusCode || (int)response.StatusCode < 500)
                {
                    return true;
                }
            }
            catch
            {
                // Try next endpoint
            }
        }

        return false;
    }

    public async Task<long> GetContentLengthAsync(string url, CancellationToken cancellationToken = default)
    {
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Head, url);
            using var response = await _httpClient.SendAsync(request, cancellationToken);
            response.EnsureSuccessStatusCode();
            return response.Content.Headers.ContentLength ?? 0L;
        }
        catch
        {
            return 0L;
        }
    }

    public async Task DownloadFileAsync(
        string url,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var directory = Path.GetDirectoryName(destinationPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var response = await _httpClient.GetAsync(
            url,
            HttpCompletionOption.ResponseHeadersRead,
            cancellationToken);

        response.EnsureSuccessStatusCode();

        var total = response.Content.Headers.ContentLength ?? 0L;

        await using var source = await response.Content.ReadAsStreamAsync(cancellationToken);
        await using var destination = new FileStream(
            destinationPath,
            FileMode.Create,
            FileAccess.Write,
            FileShare.None,
            81920,
            true);

        var buffer = new byte[128 * 1024];
        long current = 0;
        long lastReportedBytes = -1;
        var progressTimer = Stopwatch.StartNew();
        var lastProgressReport = TimeSpan.Zero;

        progress?.Report((0, total));
        lastReportedBytes = 0;

        while (true)
        {
            int read = await source.ReadAsync(buffer.AsMemory(0, buffer.Length), cancellationToken);
            if (read <= 0)
            {
                break;
            }

            await destination.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
            current += read;

            var now = progressTimer.Elapsed;
            if (now - lastProgressReport >= TimeSpan.FromMilliseconds(200))
            {
                progress?.Report((current, total));
                lastReportedBytes = current;
                lastProgressReport = now;
            }
        }

        if (current != lastReportedBytes)
        {
            progress?.Report((current, total));
        }
    }

    public async Task<string> DownloadStringAsync(string url, CancellationToken cancellationToken = default)
    {
        using var response = await _httpClient.GetAsync(url, cancellationToken);
        response.EnsureSuccessStatusCode();
        return await response.Content.ReadAsStringAsync(cancellationToken);
    }
}
