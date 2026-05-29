using System.Collections.Concurrent;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Security.Cryptography;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class MiiAvatarRenderService
{
    private const string StudioImageEndpoint = "https://studio.mii.nintendo.com/miis/image.png";
    private const int MaxAttempts = 3;
    private static readonly TimeSpan AttemptTimeout = TimeSpan.FromSeconds(14);
    private static readonly byte[] PngSignature = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    private static readonly ConcurrentDictionary<string, Task<MiiAvatarRenderResult>> InFlightRenders = new(StringComparer.OrdinalIgnoreCase);
    private static readonly SemaphoreSlim LogLock = new(1, 1);
    private readonly HttpClient _httpClient;

    public MiiAvatarRenderService()
    {
        _httpClient = new HttpClient { Timeout = TimeSpan.FromSeconds(25) };
        _httpClient.DefaultRequestHeaders.UserAgent.ParseAdd($"{LauncherConfig.ProductName}/3.0");
        _httpClient.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("image/png"));
    }

    public string GetAvatarCacheFolder()
    {
        return Path.Combine(AppContext.BaseDirectory, "Cache", "MiiAvatars");
    }

    public string GetAvatarCachePath(string cacheKey)
    {
        return Path.Combine(GetAvatarCacheFolder(), $"{cacheKey}.png");
    }

    public string GetRenderLogPath()
    {
        return Path.Combine(AppContext.BaseDirectory, "Logs", "mii-renderer.log");
    }

    public string TryGetCachedAvatar(WiiMiiData mii)
    {
        var key = GetRenderCacheKey(mii);
        var path = string.IsNullOrWhiteSpace(key) ? string.Empty : GetAvatarCachePath(key);
        return File.Exists(path) ? path : string.Empty;
    }

    public async Task<string> EnsureAvatarAsync(WiiMiiData mii, CancellationToken cancellationToken = default)
    {
        var result = await EnsureAvatarRenderAsync(mii, cancellationToken);
        return result.IsReady ? result.AvatarPath : string.Empty;
    }

    public Task<MiiAvatarRenderResult> EnsureAvatarRenderAsync(WiiMiiData mii, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(mii.StudioData) || string.IsNullOrWhiteSpace(mii.Sha256))
        {
            return Task.FromResult(MiiAvatarRenderResult.FromState(
                MiiAvatarRenderState.InvalidMiiData,
                "Mii data is missing the render payload."));
        }

        var cached = TryGetCachedAvatar(mii);
        if (!string.IsNullOrWhiteSpace(cached))
        {
            return Task.FromResult(MiiAvatarRenderResult.Ready(cached, 0, "Loaded from cache"));
        }

        var cacheKey = GetRenderCacheKey(mii);
        if (string.IsNullOrWhiteSpace(cacheKey))
        {
            return Task.FromResult(MiiAvatarRenderResult.FromState(
                MiiAvatarRenderState.InvalidMiiData,
                "Mii render cache key could not be created."));
        }

        return InFlightRenders.GetOrAdd(cacheKey, _ => RenderAndCacheAsync(mii, cacheKey, cancellationToken));
    }

    private async Task<MiiAvatarRenderResult> RenderAndCacheAsync(WiiMiiData mii, string cacheKey, CancellationToken cancellationToken)
    {
        try
        {
            Directory.CreateDirectory(GetAvatarCacheFolder());
            var targetPath = GetAvatarCachePath(cacheKey);
            var tempPath = targetPath + ".partial";
            var lastMessage = "Render did not start.";

            for (var attempt = 1; attempt <= MaxAttempts; attempt++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                using var attemptCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                attemptCts.CancelAfter(AttemptTimeout);

                try
                {
                    await WriteLogAsync($"render start name=\"{mii.Name}\" key={cacheKey} attempt={attempt}", cancellationToken);
                    var url = BuildStudioImageUrl(mii.StudioData);
                    using var response = await _httpClient.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, attemptCts.Token);

                    if (!response.IsSuccessStatusCode)
                    {
                        lastMessage = $"Nintendo Studio returned {(int)response.StatusCode} {response.ReasonPhrase}.";
                        await WriteLogAsync($"render http-error key={cacheKey} attempt={attempt} status={(int)response.StatusCode} reason=\"{response.ReasonPhrase}\"", cancellationToken);
                        await DelayBeforeRetryAsync(attempt, cancellationToken);
                        continue;
                    }

                    await using var input = await response.Content.ReadAsStreamAsync(attemptCts.Token);
                    await using (var output = new FileStream(tempPath, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true))
                    {
                        await input.CopyToAsync(output, attemptCts.Token);
                        await output.FlushAsync(attemptCts.Token);
                    }

                    var validation = ValidatePng(tempPath);
                    if (validation != null)
                    {
                        lastMessage = validation;
                        await WriteLogAsync($"render invalid-png key={cacheKey} attempt={attempt} message=\"{validation}\"", cancellationToken);
                        TryDelete(tempPath);
                        await DelayBeforeRetryAsync(attempt, cancellationToken);
                        continue;
                    }

                    File.Move(tempPath, targetPath, overwrite: true);
                    await WriteLogAsync($"render ready key={cacheKey} attempt={attempt} path=\"{targetPath}\"", cancellationToken);
                    return MiiAvatarRenderResult.Ready(targetPath, attempt, "Rendered with Nintendo Studio");
                }
                catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                {
                    lastMessage = $"Render attempt timed out after {AttemptTimeout.TotalSeconds:0}s.";
                    await WriteLogAsync($"render timeout key={cacheKey} attempt={attempt}", CancellationToken.None);
                    TryDelete(tempPath);
                    await DelayBeforeRetryAsync(attempt, cancellationToken);
                }
                catch (HttpRequestException ex)
                {
                    lastMessage = $"Network error: {ex.Message}";
                    await WriteLogAsync($"render network-error key={cacheKey} attempt={attempt} message=\"{ex.Message}\"", CancellationToken.None);
                    TryDelete(tempPath);
                    await DelayBeforeRetryAsync(attempt, cancellationToken);
                }
                catch (IOException ex)
                {
                    lastMessage = $"Cache write error: {ex.Message}";
                    await WriteLogAsync($"render io-error key={cacheKey} attempt={attempt} message=\"{ex.Message}\"", CancellationToken.None);
                    TryDelete(tempPath);
                    await DelayBeforeRetryAsync(attempt, cancellationToken);
                }
            }

            return MiiAvatarRenderResult.FromState(MiiAvatarRenderState.Failed, lastMessage, MaxAttempts);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            await WriteLogAsync($"render cancelled key={cacheKey}", CancellationToken.None);
            return MiiAvatarRenderResult.FromState(MiiAvatarRenderState.Cancelled, "Render cancelled.");
        }
        catch
        {
            await WriteLogAsync($"render failed key={cacheKey} unexpected", CancellationToken.None);
            return MiiAvatarRenderResult.FromState(MiiAvatarRenderState.Failed, "Unexpected renderer failure.", MaxAttempts);
        }
        finally
        {
            InFlightRenders.TryRemove(cacheKey, out _);
        }
    }

    private static string GetRenderCacheKey(WiiMiiData mii)
    {
        if (!string.IsNullOrWhiteSpace(mii.StudioData))
        {
            return Convert.ToHexString(SHA256.HashData(System.Text.Encoding.ASCII.GetBytes(mii.StudioData))).ToLowerInvariant();
        }

        return mii.Sha256;
    }

    private static string BuildStudioImageUrl(string studioData)
    {
        var query = new Dictionary<string, string>
        {
            ["data"] = studioData,
            ["type"] = "face",
            ["expression"] = "normal",
            ["width"] = "512",
            ["bgColor"] = "FFFFFF00",
            ["clothesColor"] = "default",
            ["cameraXRotate"] = "0",
            ["cameraYRotate"] = "0",
            ["cameraZRotate"] = "0",
            ["characterXRotate"] = "0",
            ["characterYRotate"] = "0",
            ["characterZRotate"] = "0",
            ["lightXDirection"] = "0",
            ["lightYDirection"] = "0",
            ["lightZDirection"] = "1",
            ["instanceCount"] = "1"
        };

        return $"{StudioImageEndpoint}?{string.Join("&", query.Select(pair => $"{Uri.EscapeDataString(pair.Key)}={Uri.EscapeDataString(pair.Value)}"))}";
    }

    private static string? ValidatePng(string path)
    {
        if (!File.Exists(path))
        {
            return "Renderer did not create an image file.";
        }

        var info = new FileInfo(path);
        if (info.Length < 512)
        {
            return $"Renderer returned an unexpectedly small image ({info.Length} bytes).";
        }

        using var stream = File.OpenRead(path);
        var signature = new byte[PngSignature.Length];
        var read = stream.Read(signature, 0, signature.Length);
        if (read != PngSignature.Length || !signature.SequenceEqual(PngSignature))
        {
            return "Renderer output is not a PNG image.";
        }

        return null;
    }

    private static async Task DelayBeforeRetryAsync(int attempt, CancellationToken cancellationToken)
    {
        if (attempt >= MaxAttempts)
        {
            return;
        }

        await Task.Delay(TimeSpan.FromMilliseconds(250 * attempt), cancellationToken);
    }

    private static async Task WriteLogAsync(string message, CancellationToken cancellationToken)
    {
        try
        {
            var path = Path.Combine(AppContext.BaseDirectory, "Logs", "mii-renderer.log");
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await LogLock.WaitAsync(cancellationToken);
            try
            {
                await File.AppendAllTextAsync(path, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] {message}{Environment.NewLine}", cancellationToken);
            }
            finally
            {
                LogLock.Release();
            }
        }
        catch
        {
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch
        {
        }
    }
}
