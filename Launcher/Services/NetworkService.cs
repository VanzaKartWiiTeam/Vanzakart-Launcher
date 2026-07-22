using System.IO;
using System.Net;
using System.Net.Http;
using System.Diagnostics;

namespace VanzaKartLauncher.Services;

public sealed record DownloadAttemptInfo(
    string Url,
    int Attempt,
    bool Success,
    int? StatusCode,
    string HttpVersion,
    long ExistingBytes,
    long BytesReceived,
    TimeSpan Duration,
    string Error);

public sealed record DownloadResult(
    string SourceUrl,
    long BytesReceived,
    long TotalBytes,
    TimeSpan Duration,
    IReadOnlyList<DownloadAttemptInfo> Attempts)
{
    public int RetryCount => Math.Max(0, Attempts.Count - 1);
}

public sealed class DownloadFailedException : HttpRequestException
{
    public DownloadFailedException(
        string message,
        Exception? innerException,
        IReadOnlyList<DownloadAttemptInfo> attempts)
        : base(message, innerException)
    {
        Attempts = attempts;
    }

    public IReadOnlyList<DownloadAttemptInfo> Attempts { get; }
}

public sealed class NetworkService
{
    private readonly HttpClient _httpClient;
    private const int DefaultRetryCount = 3;
    private const int DownloadBufferSize = 256 * 1024;
    private static readonly TimeSpan DownloadTimeout = TimeSpan.FromMinutes(10);

    public NetworkService()
    {
        ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls13;
        var handler = new SocketsHttpHandler
        {
            MaxConnectionsPerServer = 8,
            ConnectTimeout = TimeSpan.FromSeconds(20),
            PooledConnectionLifetime = TimeSpan.FromMinutes(15),
            AutomaticDecompression = DecompressionMethods.None
        };

        _httpClient = new HttpClient(handler)
        {
            Timeout = DownloadTimeout,
            DefaultRequestVersion = HttpVersion.Version20,
            DefaultVersionPolicy = HttpVersionPolicy.RequestVersionOrLower
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
        await DownloadFileWithResumeDetailedAsync(url, destinationPath, progress, cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task DownloadFileWithResumeAsync(
        IEnumerable<string> urls,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        await DownloadFileWithResumeDetailedAsync(urls, destinationPath, progress, cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task<DownloadResult> DownloadFileWithResumeDetailedAsync(
        string url,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Exception? lastError = null;
        var attempts = new List<DownloadAttemptInfo>();
        var overallStopwatch = Stopwatch.StartNew();

        for (var attempt = 1; attempt <= DefaultRetryCount; attempt++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var existingBytes = File.Exists(destinationPath) ? new FileInfo(destinationPath).Length : 0L;
            var attemptStopwatch = Stopwatch.StartNew();

            try
            {
                var transfer = await DownloadFileWithResumeCoreAsync(
                        url, destinationPath, progress, cancellationToken)
                    .ConfigureAwait(false);
                attemptStopwatch.Stop();
                attempts.Add(new DownloadAttemptInfo(
                    url,
                    attempt,
                    true,
                    transfer.StatusCode,
                    transfer.HttpVersion,
                    transfer.ExistingBytes,
                    transfer.BytesReceived,
                    attemptStopwatch.Elapsed,
                    string.Empty));

                overallStopwatch.Stop();
                return new DownloadResult(
                    url,
                    transfer.BytesReceived,
                    transfer.TotalBytes,
                    overallStopwatch.Elapsed,
                    attempts);
            }
            catch (Exception ex) when (IsRecoverableDownloadException(ex, cancellationToken))
            {
                attemptStopwatch.Stop();
                lastError = ex;
                var currentLength = File.Exists(destinationPath)
                    ? new FileInfo(destinationPath).Length
                    : 0L;
                attempts.Add(new DownloadAttemptInfo(
                    url,
                    attempt,
                    false,
                    (ex as HttpRequestException)?.StatusCode is { } status ? (int)status : null,
                    string.Empty,
                    existingBytes,
                    Math.Max(0, currentLength - existingBytes),
                    attemptStopwatch.Elapsed,
                    ex.Message));

                if (attempt < DefaultRetryCount && IsRetryableAttempt(ex))
                {
                    await Task.Delay(TimeSpan.FromMilliseconds(450 * attempt), cancellationToken)
                        .ConfigureAwait(false);
                }
                else
                {
                    break;
                }
            }
        }

        throw new DownloadFailedException(
            $"Download failed after {attempts.Count} attempt(s): {url}",
            lastError,
            attempts);
    }

    public async Task<DownloadResult> DownloadFileWithResumeDetailedAsync(
        IEnumerable<string> urls,
        string destinationPath,
        IProgress<(long current, long total)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Exception? lastError = null;
        var errors = new List<string>();
        var allAttempts = new List<DownloadAttemptInfo>();
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
                var result = await DownloadFileWithResumeDetailedAsync(
                        url, destinationPath, progress, cancellationToken)
                    .ConfigureAwait(false);
                allAttempts.AddRange(result.Attempts);
                return result with { Attempts = allAttempts };
            }
            catch (DownloadFailedException ex)
            {
                lastError = ex;
                allAttempts.AddRange(ex.Attempts);
                errors.Add($"{url} -> {ex.Message}");
            }
        }

        throw new DownloadFailedException(
            "All download mirrors failed." +
            (errors.Count > 0 ? $"{Environment.NewLine}{string.Join(Environment.NewLine, errors)}" : string.Empty),
            lastError,
            allAttempts);
    }

    private async Task<DownloadTransferResult> DownloadFileWithResumeCoreAsync(
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
            cancellationToken).ConfigureAwait(false);

        if (existingLength > 0 &&
            response.StatusCode == HttpStatusCode.RequestedRangeNotSatisfiable &&
            response.Content.Headers.ContentRange?.Length == existingLength)
        {
            progress?.Report((existingLength, existingLength));
            return new DownloadTransferResult(
                (int)response.StatusCode,
                response.Version.ToString(),
                existingLength,
                0,
                existingLength);
        }

        if (existingLength > 0 && response.StatusCode != HttpStatusCode.PartialContent)
        {
            // Keep the partial file when a mirror returns an error. A later retry
            // or mirror can still resume it because all candidates represent the
            // same hash-verified payload.
            response.EnsureSuccessStatusCode();
            File.Delete(destinationPath);
            existingLength = 0;
        }

        response.EnsureSuccessStatusCode();

        long total = (response.Content.Headers.ContentLength ?? 0L) + existingLength;

        await using var source = await response.Content.ReadAsStreamAsync(cancellationToken)
            .ConfigureAwait(false);
        await using var destination = new FileStream(
            destinationPath,
            existingLength > 0 ? FileMode.Append : FileMode.Create,
            FileAccess.Write,
            FileShare.None,
            DownloadBufferSize,
            FileOptions.Asynchronous | FileOptions.SequentialScan);

        var buffer = new byte[DownloadBufferSize];
        long current = existingLength;
        long bytesReceived = 0;

        while (true)
        {
            int read = await source.ReadAsync(buffer.AsMemory(0, buffer.Length), cancellationToken)
                .ConfigureAwait(false);
            if (read <= 0)
            {
                break;
            }

            await destination.WriteAsync(buffer.AsMemory(0, read), cancellationToken)
                .ConfigureAwait(false);
            current += read;
            bytesReceived += read;
            progress?.Report((current, total));
        }

        return new DownloadTransferResult(
            (int)response.StatusCode,
            response.Version.ToString(),
            existingLength,
            bytesReceived,
            total);
    }

    private static bool IsRecoverableDownloadException(
        Exception exception,
        CancellationToken cancellationToken)
    {
        if (cancellationToken.IsCancellationRequested)
        {
            return false;
        }

        return exception is HttpRequestException or IOException or TaskCanceledException;
    }

    private static bool IsRetryableAttempt(Exception exception)
    {
        if (exception is IOException or TaskCanceledException)
        {
            return true;
        }

        if (exception is not HttpRequestException httpException || httpException.StatusCode is null)
        {
            return true;
        }

        var statusCode = (int)httpException.StatusCode.Value;
        return statusCode is 408 or 425 or 429 or 500 or 502 or 503 or 504;
    }

    private sealed record DownloadTransferResult(
        int StatusCode,
        string HttpVersion,
        long ExistingBytes,
        long BytesReceived,
        long TotalBytes);
}
