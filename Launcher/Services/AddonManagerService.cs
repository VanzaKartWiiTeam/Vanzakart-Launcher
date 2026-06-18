using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using SharpCompress.Archives;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class AddonManagerService
{
    private const string ManifestName = "addon.json";
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    public string GetMyStuffFolder(LauncherSettings settings) =>
        Path.Combine(settings.GetModFolder(), "VanzaKart", "VanzaKart", "My Stuff");

    private string GetLibraryFolder(LauncherSettings settings) =>
        Path.Combine(settings.GetModFolder(), "VanzaKart_UserData", "Addons");

    public IReadOnlyList<AddonInfo> Load(LauncherSettings settings)
    {
        var library = GetLibraryFolder(settings);
        var addons = new List<AddonInfo>();
        if (Directory.Exists(library))
        {
            foreach (var manifest in Directory.EnumerateFiles(library, ManifestName, SearchOption.AllDirectories))
            {
                try
                {
                    var addon = JsonSerializer.Deserialize<AddonInfo>(File.ReadAllText(manifest));
                    if (addon != null)
                    {
                        if (string.Equals(addon.Source, "Existing local addon", StringComparison.OrdinalIgnoreCase))
                            addon.Source = "My Stuff folder";
                        addons.Add(addon);
                    }
                }
                catch { }
            }
        }

        AddUnmanagedEntries(settings, addons);
        return addons.OrderByDescending(addon => addon.IsEnabled).ThenBy(addon => addon.Name, StringComparer.CurrentCultureIgnoreCase).ToArray();
    }

    public async Task<AddonInfo> InstallGameBananaAsync(
        LauncherSettings settings,
        GameBananaMod mod,
        NetworkService network,
        IProgress<(long current, long total)>? progress = null,
        IProgress<string>? stageProgress = null,
        CancellationToken cancellationToken = default)
    {
        var extension = Path.GetExtension(mod.FileName);

        var tempRoot = Path.Combine(Path.GetTempPath(), "VanzaKart", Guid.NewGuid().ToString("N"));
        var archivePath = Path.Combine(tempRoot, mod.FileName);
        var extracted = Path.Combine(tempRoot, "extracted");
        Directory.CreateDirectory(tempRoot);
        try
        {
            stageProgress?.Report("Downloading addon...");
            await network.DownloadFileWithResumeAsync(mod.DownloadUrl, archivePath, progress, cancellationToken);
            stageProgress?.Report("Extracting archive...");
            if (IsArchiveExtension(extension))
            {
                await ExtractArchiveSafeAsync(archivePath, extracted, cancellationToken);
            }
            else
            {
                Directory.CreateDirectory(extracted);
                await CopyFileAsync(archivePath, Path.Combine(extracted, mod.FileName), cancellationToken);
            }
            var payloadRoot = FindPayloadRoot(extracted);
            var addon = new AddonInfo
            {
                Id = $"gamebanana-{mod.Id}-{mod.FileId}",
                Name = mod.Name,
                Author = mod.Author,
                Source = "GameBanana",
                SourceUrl = mod.ProfileUrl,
                PreviewUrl = mod.PreviewUrl,
                IsEnabled = false,
                InstalledUtc = DateTime.UtcNow
            };
            stageProgress?.Report("Installing files...");
            await SavePayloadAsync(settings, addon, payloadRoot, cancellationToken);
            await SetEnabledAsync(settings, addon, true, cancellationToken);
            return addon;
        }
        finally
        {
            try { if (Directory.Exists(tempRoot)) Directory.Delete(tempRoot, true); } catch { }
        }
    }

    public async Task<AddonInfo> ImportAsync(LauncherSettings settings, string sourcePath, CancellationToken cancellationToken = default)
    {
        var tempRoot = Path.Combine(Path.GetTempPath(), "VanzaKart", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempRoot);
        try
        {
            string payloadRoot;
            if (Directory.Exists(sourcePath))
            {
                payloadRoot = sourcePath;
            }
            else if (IsArchiveExtension(Path.GetExtension(sourcePath)))
            {
                payloadRoot = Path.Combine(tempRoot, "extracted");
                await ExtractArchiveSafeAsync(sourcePath, payloadRoot, cancellationToken);
                payloadRoot = FindPayloadRoot(payloadRoot);
            }
            else
            {
                payloadRoot = Path.Combine(tempRoot, "single");
                Directory.CreateDirectory(payloadRoot);
                File.Copy(sourcePath, Path.Combine(payloadRoot, Path.GetFileName(sourcePath)), true);
            }

            var addon = new AddonInfo
            {
                Id = $"local-{Guid.NewGuid():N}",
                Name = Path.GetFileNameWithoutExtension(sourcePath),
                Source = "Local import",
                IsEnabled = false,
                InstalledUtc = DateTime.UtcNow
            };
            await SavePayloadAsync(settings, addon, payloadRoot, cancellationToken);
            await SetEnabledAsync(settings, addon, true, cancellationToken);
            return addon;
        }
        finally
        {
            try { if (Directory.Exists(tempRoot)) Directory.Delete(tempRoot, true); } catch { }
        }
    }

    public async Task SetEnabledAsync(LauncherSettings settings, AddonInfo addon, bool enabled, CancellationToken cancellationToken = default)
    {
        if (!addon.IsManaged)
        {
            if (enabled) return;
            await AdoptUnmanagedAsync(settings, addon, cancellationToken);
            return;
        }

        var myStuff = GetMyStuffFolder(settings);
        var addonFolder = Path.Combine(GetLibraryFolder(settings), addon.Id);
        var payload = Path.Combine(addonFolder, "payload");
        if (enabled)
        {
            var conflicts = Load(settings)
                .Where(other => other.IsManaged && other.IsEnabled && !string.Equals(other.Id, addon.Id, StringComparison.OrdinalIgnoreCase))
                .SelectMany(other => other.Files)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);
            var collision = addon.Files.FirstOrDefault(conflicts.Contains);
            if (collision != null)
                throw new IOException($"Cannot enable '{addon.Name}': '{collision}' is already supplied by another enabled addon.");

            collision = addon.Files.FirstOrDefault(relative =>
            {
                var target = SafeCombine(myStuff, relative);
                var source = SafeCombine(payload, relative);
                return File.Exists(target) && (!File.Exists(source) || !FilesMatch(target, source));
            });
            if (collision != null)
                throw new IOException($"Cannot enable '{addon.Name}': the existing local file '{collision}' would be overwritten. Disable the addon that owns it first.");

            foreach (var relative in addon.Files)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var source = SafeCombine(payload, relative);
                var destination = SafeCombine(myStuff, relative);
                Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                File.Copy(source, destination, true);
            }
        }
        else
        {
            foreach (var relative in addon.Files)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var target = SafeCombine(myStuff, relative);
                var source = SafeCombine(payload, relative);
                if (File.Exists(target) && File.Exists(source) && FilesMatch(target, source)) File.Delete(target);
            }
            RemoveEmptyDirectories(myStuff);
        }

        addon.IsEnabled = enabled;
        await WriteManifestAsync(addonFolder, addon, cancellationToken);
    }

    public async Task DeleteAsync(LauncherSettings settings, AddonInfo addon, CancellationToken cancellationToken = default)
    {
        if (!addon.IsManaged)
            await SetEnabledAsync(settings, addon, false, cancellationToken);
        else if (addon.IsEnabled)
            await SetEnabledAsync(settings, addon, false, cancellationToken);

        var folder = Path.Combine(GetLibraryFolder(settings), addon.Id);
        if (Directory.Exists(folder)) Directory.Delete(folder, true);
    }

    private async Task SavePayloadAsync(LauncherSettings settings, AddonInfo addon, string sourceRoot, CancellationToken cancellationToken)
    {
        var addonFolder = Path.Combine(GetLibraryFolder(settings), addon.Id);
        var payload = Path.Combine(addonFolder, "payload");
        var previous = Load(settings).FirstOrDefault(item => item.IsManaged && string.Equals(item.Id, addon.Id, StringComparison.OrdinalIgnoreCase));
        if (previous?.IsEnabled == true) await SetEnabledAsync(settings, previous, false, cancellationToken);
        if (Directory.Exists(addonFolder)) Directory.Delete(addonFolder, true);
        Directory.CreateDirectory(payload);

        foreach (var source in Directory.EnumerateFiles(sourceRoot, "*", SearchOption.AllDirectories))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative = Path.GetRelativePath(sourceRoot, source);
            if (ShouldSkip(relative)) continue;
            var destination = SafeCombine(payload, relative);
            Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
            await CopyFileAsync(source, destination, cancellationToken);
            addon.Files.Add(relative.Replace(Path.DirectorySeparatorChar, '/'));
        }
        if (addon.Files.Count == 0) throw new InvalidDataException("The archive does not contain installable files.");
        await WriteManifestAsync(addonFolder, addon, cancellationToken);
    }

    private async Task AdoptUnmanagedAsync(LauncherSettings settings, AddonInfo addon, CancellationToken cancellationToken)
    {
        addon.Id = $"local-{Guid.NewGuid():N}";
        addon.IsManaged = true;
        addon.IsEnabled = false;
        var addonFolder = Path.Combine(GetLibraryFolder(settings), addon.Id);
        var payload = Path.Combine(addonFolder, "payload");
        var myStuff = GetMyStuffFolder(settings);
        foreach (var relative in addon.Files)
        {
            var source = SafeCombine(myStuff, relative);
            if (!File.Exists(source)) continue;
            var destination = SafeCombine(payload, relative);
            Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
            await CopyFileAsync(source, destination, cancellationToken);
            File.Delete(source);
        }
        RemoveEmptyDirectories(myStuff);
        await WriteManifestAsync(addonFolder, addon, cancellationToken);
    }

    private void AddUnmanagedEntries(LauncherSettings settings, List<AddonInfo> addons)
    {
        var myStuff = GetMyStuffFolder(settings);
        if (!Directory.Exists(myStuff)) return;
        var owned = addons.Where(a => a.IsManaged && a.IsEnabled).SelectMany(a => a.Files).ToHashSet(StringComparer.OrdinalIgnoreCase);
        var looseFiles = Directory.EnumerateFiles(myStuff, "*", SearchOption.TopDirectoryOnly)
            .Select(path => Path.GetRelativePath(myStuff, path).Replace(Path.DirectorySeparatorChar, '/'))
            .Where(path => !owned.Contains(path)).ToList();
        if (looseFiles.Count > 0) addons.Add(CreateUnmanaged("Local files", looseFiles));

        foreach (var directory in Directory.EnumerateDirectories(myStuff))
        {
            var files = Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories)
                .Select(path => Path.GetRelativePath(myStuff, path).Replace(Path.DirectorySeparatorChar, '/'))
                .Where(path => !owned.Contains(path)).ToList();
            if (files.Count > 0) addons.Add(CreateUnmanaged(Path.GetFileName(directory), files));
        }
    }

    private static AddonInfo CreateUnmanaged(string name, List<string> files) => new()
    {
        Id = $"unmanaged-{Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(name))).Substring(0, 10)}",
        Name = name,
        Source = "My Stuff folder",
        IsEnabled = true,
        IsManaged = false,
        Files = files
    };

    private static bool IsArchiveExtension(string extension) => extension.ToLowerInvariant() is
        ".zip" or ".7z" or ".rar" or ".tar" or ".gz" or ".gzip" or ".bz2" or ".xz";

    private static Task ExtractArchiveSafeAsync(string archivePath, string destination, CancellationToken cancellationToken)
    {
        return Task.Run(() =>
        {
            Directory.CreateDirectory(destination);
            var root = Path.GetFullPath(destination) + Path.DirectorySeparatorChar;
            using var archive = ArchiveFactory.Open(archivePath);
            foreach (var entry in archive.Entries.Where(entry => !entry.IsDirectory))
            {
                cancellationToken.ThrowIfCancellationRequested();
                var key = entry.Key?.Replace('/', Path.DirectorySeparatorChar) ?? string.Empty;
                if (string.IsNullOrWhiteSpace(key)) continue;
                var target = Path.GetFullPath(Path.Combine(destination, key));
                if (!target.StartsWith(root, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("The archive contains an unsafe file path.");
                Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                using var input = entry.OpenEntryStream();
                using var output = new FileStream(target, FileMode.Create, FileAccess.Write, FileShare.None);
                input.CopyTo(output);
            }
        }, cancellationToken);
    }

    private static string FindPayloadRoot(string extracted)
    {
        var myStuff = Directory.EnumerateDirectories(extracted, "My Stuff", SearchOption.AllDirectories)
            .OrderBy(path => path.Count(ch => ch == Path.DirectorySeparatorChar)).FirstOrDefault();
        return myStuff ?? extracted;
    }

    private static bool ShouldSkip(string relative) =>
        relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Any(part => part.Equals("__MACOSX", StringComparison.OrdinalIgnoreCase)) ||
        Path.GetFileName(relative).Equals(".DS_Store", StringComparison.OrdinalIgnoreCase);

    private static string SafeCombine(string root, string relative)
    {
        var fullRoot = Path.GetFullPath(root) + Path.DirectorySeparatorChar;
        var fullPath = Path.GetFullPath(Path.Combine(root, relative.Replace('/', Path.DirectorySeparatorChar)));
        if (!fullPath.StartsWith(fullRoot, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("Unsafe addon path.");
        return fullPath;
    }

    private static async Task CopyFileAsync(string source, string destination, CancellationToken cancellationToken)
    {
        await using var input = new FileStream(source, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        await using var output = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);
        await input.CopyToAsync(output, cancellationToken);
    }

    private static bool FilesMatch(string first, string second)
    {
        if (new FileInfo(first).Length != new FileInfo(second).Length) return false;
        using var a = File.OpenRead(first);
        using var b = File.OpenRead(second);
        return SHA256.HashData(a).SequenceEqual(SHA256.HashData(b));
    }

    private static async Task WriteManifestAsync(string addonFolder, AddonInfo addon, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(addonFolder);
        await File.WriteAllTextAsync(Path.Combine(addonFolder, ManifestName), JsonSerializer.Serialize(addon, JsonOptions), cancellationToken);
    }

    private static void RemoveEmptyDirectories(string root)
    {
        if (!Directory.Exists(root)) return;
        foreach (var directory in Directory.EnumerateDirectories(root, "*", SearchOption.AllDirectories).OrderByDescending(path => path.Length))
        {
            if (!Directory.EnumerateFileSystemEntries(directory).Any()) Directory.Delete(directory);
        }
    }
}
