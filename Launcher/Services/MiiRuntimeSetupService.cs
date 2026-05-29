using System.IO;
using System.IO.Compression;
using System.Net.Http;

namespace VanzaKartLauncher.Services;

public sealed record MiiRuntimeSetupProgress(string Stage, long BytesReceived, long? TotalBytes)
{
    public double Percent => TotalBytes is > 0
        ? Math.Clamp(BytesReceived / (double)TotalBytes.Value * 100.0, 0.0, 100.0)
        : 0.0;
}

public sealed record MiiRuntimeStatus(bool IsInstalled, string ResourcePath, long SizeBytes);

public sealed class MiiRuntimeSetupService
{
    private const string ArchiveEntryPath = "asset/model/character/mii/AFLResHigh_2_3.dat";
    private const string ResourceFileName = "FFLResHigh.dat";
    private const long MinimumExpectedSizeBytes = 1024 * 1024;
    private readonly HttpClient _httpClient = new() { Timeout = TimeSpan.FromMinutes(3) };

    public string GetRuntimeFolder()
    {
        return Path.Combine(AppContext.BaseDirectory, "Runtime", "MiiRendering");
    }

    public string GetResourcePath()
    {
        return Path.Combine(GetRuntimeFolder(), ResourceFileName);
    }

    public MiiRuntimeStatus GetStatus()
    {
        var path = GetResourcePath();
        if (!File.Exists(path))
        {
            return new MiiRuntimeStatus(false, path, 0);
        }

        var size = new FileInfo(path).Length;
        return new MiiRuntimeStatus(size >= MinimumExpectedSizeBytes, path, size);
    }

    public async Task<MiiRuntimeStatus> InstallAsync(
        IProgress<MiiRuntimeSetupProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(GetRuntimeFolder());
        var archivePath = Path.Combine(GetRuntimeFolder(), "mii-render-assets.zip.partial");
        var extractedPath = GetResourcePath() + ".partial";

        try
        {
            progress?.Report(new MiiRuntimeSetupProgress("Downloading render assets", 0, null));
            await DownloadArchiveAsync(archivePath, progress, cancellationToken);

            progress?.Report(new MiiRuntimeSetupProgress("Extracting render assets", 1, 1));
            await ExtractResourceAsync(archivePath, extractedPath, cancellationToken);

            var extractedSize = new FileInfo(extractedPath).Length;
            if (extractedSize < MinimumExpectedSizeBytes)
            {
                throw new InvalidDataException("Downloaded render asset is incomplete.");
            }

            File.Move(extractedPath, GetResourcePath(), overwrite: true);
            progress?.Report(new MiiRuntimeSetupProgress("Ready", extractedSize, extractedSize));
            return GetStatus();
        }
        finally
        {
            TryDelete(archivePath);
            TryDelete(extractedPath);
        }
    }

    private async Task DownloadArchiveAsync(
        string archivePath,
        IProgress<MiiRuntimeSetupProgress>? progress,
        CancellationToken cancellationToken)
    {
        using var response = await _httpClient.GetAsync(
            LauncherConfig.MiiRenderingArchiveUrl,
            HttpCompletionOption.ResponseHeadersRead,
            cancellationToken);
        response.EnsureSuccessStatusCode();

        var total = response.Content.Headers.ContentLength;
        await using var input = await response.Content.ReadAsStreamAsync(cancellationToken);
        await using var output = new FileStream(archivePath, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);

        var buffer = new byte[81920];
        long received = 0;
        while (true)
        {
            var read = await input.ReadAsync(buffer, cancellationToken);
            if (read == 0)
            {
                break;
            }

            await output.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
            received += read;
            progress?.Report(new MiiRuntimeSetupProgress("Downloading render assets", received, total));
        }
    }

    private static async Task ExtractResourceAsync(string archivePath, string extractedPath, CancellationToken cancellationToken)
    {
        await using var archiveStream = new FileStream(archivePath, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        using var archive = new ZipArchive(archiveStream, ZipArchiveMode.Read);
        var entry = archive.GetEntry(ArchiveEntryPath)
            ?? throw new InvalidDataException("Render asset archive is missing the required resource.");

        await using var input = entry.Open();
        await using var output = new FileStream(extractedPath, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);
        await input.CopyToAsync(output, cancellationToken);
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
