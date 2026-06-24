using System.IO;
using System.Security.Cryptography;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class MusicPackService
{
    public const string FileName = "vanzakart_musicpack.zip";

    private readonly NetworkService _network;
    private readonly ArchiveService _archive;
    private readonly AddonManagerService _addonManager;

    public MusicPackService(NetworkService network, ArchiveService archive, AddonManagerService addonManager)
    {
        _network = network;
        _archive = archive;
        _addonManager = addonManager;
    }

    public Models.AddonInfo? GetInstalled(Models.LauncherSettings settings) =>
        _addonManager.Load(settings).FirstOrDefault(addon =>
            addon.IsManaged && addon.Id.Equals(AddonManagerService.OfficialMusicPackId, StringComparison.OrdinalIgnoreCase));

    public bool IsInstalled(Models.LauncherSettings settings) => GetInstalled(settings) != null;

    public async Task InstallAsync(
        Models.LauncherSettings settings,
        IEnumerable<string> downloadUrls,
        string expectedSha256,
        string manifestUrl,
        IEnumerable<string> filesBaseUrls,
        IProgress<(long current, long total)>? progress,
        IProgress<string>? stages,
        CancellationToken cancellationToken)
    {
        if (IsInstalled(settings))
        {
            if (string.IsNullOrWhiteSpace(manifestUrl))
                throw new InvalidDataException("The Music Pack differential manifest URL is missing.");
            await UpdateDifferentialAsync(settings, manifestUrl, filesBaseUrls, progress, stages, cancellationToken);
            return;
        }

        var tempRoot = Path.Combine(Path.GetTempPath(), "VanzaKart", "MusicPack", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempRoot);
        var temporary = Path.Combine(tempRoot, FileName + ".download");

        try
        {
            stages?.Report("Downloading official Music Pack...");
            await _network.DownloadFileWithResumeAsync(downloadUrls, temporary, progress, cancellationToken);

            stages?.Report("Verifying archive integrity...");
            await _archive.ValidateZipAsync(temporary);
            if (!string.IsNullOrWhiteSpace(expectedSha256))
            {
                await using var stream = new FileStream(temporary, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
                var actualHash = Convert.ToHexString(await SHA256.HashDataAsync(stream, cancellationToken));
                if (!actualHash.Equals(expectedSha256.Trim(), StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException($"Music Pack hash mismatch. Expected {expectedSha256}, received {actualHash}.");
            }

            stages?.Report("Extracting and installing into My Stuff...");
            await _addonManager.InstallOfficialMusicPackAsync(settings, temporary, cancellationToken);
        }
        catch
        {
            try { if (File.Exists(temporary)) File.Delete(temporary); } catch { }
            throw;
        }
        finally
        {
            try { if (File.Exists(temporary)) File.Delete(temporary); } catch { }
            try { if (Directory.Exists(tempRoot)) Directory.Delete(tempRoot, true); } catch { }
        }
    }

    private async Task UpdateDifferentialAsync(
        Models.LauncherSettings settings,
        string manifestUrl,
        IEnumerable<string> filesBaseUrls,
        IProgress<(long current, long total)>? progress,
        IProgress<string>? stages,
        CancellationToken cancellationToken)
    {
        stages?.Report("Downloading Music Pack update manifest...");
        var separator = manifestUrl.Contains('?') ? '&' : '?';
        var json = await _network.DownloadStringAsync($"{manifestUrl}{separator}t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}", cancellationToken);
        var manifest = JsonSerializer.Deserialize<ModManifest>(json.TrimStart('\uFEFF', '\u200B'))
            ?? throw new InvalidDataException("The Music Pack update manifest is invalid.");
        ValidateManifest(manifest);

        var installed = GetInstalled(settings) ?? throw new InvalidOperationException("The Music Pack is not installed.");
        var payload = _addonManager.GetManagedPayloadFolder(settings, installed.Id);
        stages?.Report("Comparing installed Music Pack files...");
        var changed = new List<ModManifestFile>();
        foreach (var file in manifest.Files)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var localPath = SafeCombine(payload, file.Path);
            if (!File.Exists(localPath) || new FileInfo(localPath).Length != file.Size ||
                !string.Equals(await ComputeSha256Async(localPath, cancellationToken), file.Sha256, StringComparison.OrdinalIgnoreCase))
                changed.Add(file);
        }

        var totalBytes = Math.Max(1, changed.Sum(file => file.Size));
        long completedBytes = 0;
        var stagingRoot = Path.Combine(Path.GetTempPath(), "VanzaKart", "MusicPackUpdate", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stagingRoot);
        try
        {
            for (var index = 0; index < changed.Count; index++)
            {
                var file = changed[index];
                stages?.Report($"Downloading changed file {index + 1}/{changed.Count}: {Path.GetFileName(file.Path)}");
                var target = SafeCombine(stagingRoot, file.Path);
                Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                var partial = target + ".download";
                var urls = filesBaseUrls.Where(url => !string.IsNullOrWhiteSpace(url))
                    .Select(url => $"{url.TrimEnd('/')}/{file.Path.Replace('\\', '/')}");
                var fileProgress = new Progress<(long current, long total)>(value =>
                    progress?.Report((completedBytes + value.current, totalBytes)));
                await _network.DownloadFileWithResumeAsync(urls, partial, fileProgress, cancellationToken);
                var actual = await ComputeSha256Async(partial, cancellationToken);
                if (!actual.Equals(file.Sha256, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException($"Hash mismatch for Music Pack file: {file.Path}");
                File.Move(partial, target, true);
                completedBytes += file.Size;
                progress?.Report((completedBytes, totalBytes));
            }

            stages?.Report(changed.Count == 0 ? "Removing obsolete files..." : "Applying differential Music Pack update...");
            await _addonManager.ApplyOfficialMusicPackUpdateAsync(settings, manifest.Files, stagingRoot, cancellationToken);
        }
        finally
        {
            try { if (Directory.Exists(stagingRoot)) Directory.Delete(stagingRoot, true); } catch { }
        }
    }

    private static void ValidateManifest(ModManifest manifest)
    {
        if (manifest.Files.Count == 0) throw new InvalidDataException("The Music Pack manifest contains no files.");
        var paths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var file in manifest.Files)
        {
            if (string.IsNullOrWhiteSpace(file.Path) || file.Path.StartsWith('/') || file.Path.StartsWith("..") ||
                Path.IsPathRooted(file.Path) || !paths.Add(file.Path) || file.Size < 0 || file.Sha256.Length != 64)
                throw new InvalidDataException($"Invalid Music Pack manifest entry: {file.Path}");
        }
    }

    private static string SafeCombine(string root, string relative)
    {
        var fullRoot = Path.GetFullPath(root) + Path.DirectorySeparatorChar;
        var result = Path.GetFullPath(Path.Combine(root, relative.Replace('/', Path.DirectorySeparatorChar)));
        if (!result.StartsWith(fullRoot, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("Unsafe Music Pack path.");
        return result;
    }

    private static async Task<string> ComputeSha256Async(string path, CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        return Convert.ToHexString(await SHA256.HashDataAsync(stream, cancellationToken));
    }

    public async Task SetEnabledAsync(Models.LauncherSettings settings, bool enabled, CancellationToken cancellationToken = default)
    {
        var addon = GetInstalled(settings) ?? throw new InvalidOperationException("The Music Pack is not installed.");
        await _addonManager.SetEnabledAsync(settings, addon, enabled, cancellationToken);
    }

    public async Task UninstallAsync(Models.LauncherSettings settings, CancellationToken cancellationToken = default)
    {
        var addon = GetInstalled(settings);
        if (addon != null) await _addonManager.DeleteAsync(settings, addon, cancellationToken);
    }
}
