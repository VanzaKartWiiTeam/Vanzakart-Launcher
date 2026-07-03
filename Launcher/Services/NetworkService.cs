using System.IO;
using System.Net;
using System.Net.Http;

namespace VanzaKartLauncher.Services;

public sealed class NetworkService
{
    private readonly HttpClient _httpClient;
    private const int DefaultRetryCount = 3;

    public NetworkService()
    {
        ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls13;
        _httpClient = new HttpClient
        {
            Timeout = TimeSpan.FromSeconds(30)
        };
    }

    public async Task<string> DownloadStringAsync(string url, CancellationToken cancellationToken = default)
    {
        using var response = await _httpClient.GetAsync(url, cancellationToken);
        response.EnsureSuccessStatusCode();
        return await response.Content.ReadAsStringAsync(cancellationToken);
    }

    public async Task DownloadFileWithResumeAsync(
        string url,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Exception? lastError = null;

        for (var attempt = 1; attempt <= DefaultRetryCount; attempt++)
        {
            try
            {
                await DownloadFileWithResumeCoreAsync(url, destinationPath, progress, cancellationToken);
                return;
            }
            catch (Exception ex) when (attempt < DefaultRetryCount && ex is HttpRequestException or IOException or TaskCanceledException)
            {
                lastError = ex;
                TryDeletePartialOnRecoverableFailure(destinationPath);
                await Task.Delay(TimeSpan.FromMilliseconds(450 * attempt), cancellationToken);
            }
        }

        throw lastError ?? new HttpRequestException("Download failed.");
    }

    public async Task DownloadFileWithResumeAsync(
        IEnumerable<string> urls,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Exception? lastError = null;
        var errors = new List<string>();
        var candidates = urls
            .Where(url => !string.IsNullOrWhiteSpace(url))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();

        if (candidates.Length == 0)
        {
            throw new ArgumentException("At least one download URL is required.", nameof(urls));
        }

        foreach (var url in candidates)
        {
            try
            {
                await DownloadFileWithResumeAsync(url, destinationPath, progress, cancellationToken);
                return;
            }
            catch (Exception ex) when (ex is HttpRequestException or IOException or TaskCanceledException)
            {
                lastError = ex;
                errors.Add($"{url} -> {ex.Message}");
            }
        }

        throw new HttpRequestException(
            "All download mirrors failed." +
            (errors.Count > 0 ? $"{Environment.NewLine}{string.Join(Environment.NewLine, errors)}" : string.Empty),
            lastError);
    }

    private async Task DownloadFileWithResumeCoreAsync(
        string url,
        string destinationPath,
        IProgress<(long current, long total)>? progress,
        CancellationToken cancellationToken)
    {
        var directory = Path.GetDirectoryName(destinationPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        long existingLength = File.Exists(destinationPath) ? new FileInfo(destinationPath).Length : 0L;

        using var request = new HttpRequestMessage(HttpMethod.Get, url);
        if (existingLength > 0)
        {
            request.Headers.Range = new System.Net.Http.Headers.RangeHeaderValue(existingLength, null);
        }

        using var response = await _httpClient.SendAsync(
            request,
            HttpCompletionOption.ResponseHeadersRead,
            cancellationToken);

        if (existingLength > 0 && response.StatusCode != HttpStatusCode.PartialContent)
        {
            File.Delete(destinationPath);
            existingLength = 0;

            if (!response.IsSuccessStatusCode)
            {
                throw new IOException($"Resume request failed with HTTP {(int)response.StatusCode} ({response.ReasonPhrase}). Retrying from byte 0.");
            }
        }

        response.EnsureSuccessStatusCode();

        long total = (response.Content.Headers.ContentLength ?? 0L) + existingLength;

        await using var source = await response.Content.ReadAsStreamAsync(cancellationToken);
        await using var destination = new FileStream(
            destinationPath,
            existingLength > 0 ? FileMode.Append : FileMode.Create,
            FileAccess.Write,
            FileShare.None,
            81920,
            true);

        var buffer = new byte[81920];
        long current = existingLength;

        while (true)
        {
            int read = await source.ReadAsync(buffer.AsMemory(0, buffer.Length), cancellationToken);
            if (read <= 0)
            {
                break;
            }

            await destination.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
            current += read;
            progress?.Report((current, total));
        }
    }

    private static void TryDeletePartialOnRecoverableFailure(string destinationPath)
    {
        try
        {
            if (File.Exists(destinationPath))
            {
                File.Delete(destinationPath);
            }
        }
        catch
        {
        }
    }
}
