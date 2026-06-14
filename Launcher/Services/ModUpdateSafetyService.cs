using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class ModUpdateSafetyService
{
    private static readonly string[] ProtectedDirectoryNames =
    [
        "My Stuff",
        "UserData",
        "userdata",
        "Saves",
        "Save",
        "Licenses",
        "License",
        "Patenti",
        "Profiles",
        "Miis",
        "Mii"
    ];

    private static readonly string[] ProtectedFileNames =
    [
        "rksys.dat",
        "RFL_DB.dat",
        "active_mii.txt",
        "mii_profile.json"
    ];

    private static readonly string[] ProtectedExtensions =
    [
        ".mii",
        ".miigx",
        ".mae",
        ".vk-mii"
    ];


    public string GetBackupRoot()
        => Path.Combine(AppContext.BaseDirectory, "Backups", "ModUpdates");

    public string GetOperationLogPath()
        => Path.Combine(AppContext.BaseDirectory, "Logs", "mod-update.log");

    public string GetModRoot(LauncherSettings settings)
        => Path.Combine(settings.GetModFolder(), "VanzaKart");

    public async Task<List<ModManifestFile>> ScanLocalFilesAsync(
        string modSubFolder,
        CancellationToken cancellationToken = default)
    {
        var result = new List<ModManifestFile>();
        if (!Directory.Exists(modSubFolder))
            return result;

        var files = Directory.EnumerateFiles(modSubFolder, "*", SearchOption.AllDirectories);
        foreach (var file in files)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative = Path.GetRelativePath(modSubFolder, file);

            if (IsProtectedRelativePath(relative))
                continue;

            var size = new FileInfo(file).Length;
            var sha256 = await ComputeSha256Async(file, cancellationToken);

            result.Add(new ModManifestFile
            {
                Path = relative.Replace(Path.DirectorySeparatorChar, '/'),
                Sha256 = sha256,
                Size = size
            });
        }

        return result;
    }

    public string GetUserDataRoot(LauncherSettings settings)
        => Path.Combine(settings.GetModFolder(), "VanzaKart_UserData");

    public string GetMyStuffPath(LauncherSettings settings)
        => Path.Combine(settings.GetModFolder(), "VanzaKart", "VanzaKart", "My Stuff");

    public bool IsProtectedUserDataPath(string path, LauncherSettings settings)
    {
        var modRoot = GetModRoot(settings);
        if (string.IsNullOrWhiteSpace(path) || string.IsNullOrWhiteSpace(modRoot))
            return false;

        try
        {
            var relative = Path.GetRelativePath(modRoot, path);
            return IsProtectedRelativePath(relative);
        }
        catch
        {
            return false;
        }
    }


    public async Task<ModUpdateBackup> CreateBackupAsync(
        LauncherSettings settings,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot = GetModRoot(settings);
        var userDataRoot = GetUserDataRoot(settings);
        var backupId = DateTime.Now.ToString("yyyyMMdd_HHmmss");
        var backupFolder = Path.Combine(GetBackupRoot(), backupId);
        var files = new List<ModUpdateBackupFile>();

        Directory.CreateDirectory(backupFolder);
        Directory.CreateDirectory(userDataRoot);

        if (!Directory.Exists(modRoot))
        {
            await WriteLogAsync($"backup {backupId}: mod not found", cancellationToken);
            return new ModUpdateBackup
            {
                BackupId = backupId,
                BackupFolder = backupFolder,
                ModRoot = modRoot,
                UserDataRoot = userDataRoot,
                Files = files
            };
        }

        foreach (var file in EnumerateUserDataFiles(modRoot))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative = Path.GetRelativePath(modRoot, file);
            progress?.Report($"Saving {relative}");

            var backupPath = Path.Combine(backupFolder, "files", relative);
            await CopyFileAsync(file, backupPath, cancellationToken);

            var mirrorPath = Path.Combine(userDataRoot, relative);
            await CopyFileAsync(file, mirrorPath, cancellationToken);

            files.Add(new ModUpdateBackupFile
            {
                RelativePath = relative,
                BackupPath = backupPath,
                Sha256 = await ComputeSha256Async(file, cancellationToken),
                SizeBytes = new FileInfo(file).Length
            });
        }

        var result = new ModUpdateBackup
        {
            BackupId = backupId,
            BackupFolder = backupFolder,
            ModRoot = modRoot,
            UserDataRoot = userDataRoot,
            Files = files
        };

        var manifestPath = Path.Combine(backupFolder, "manifest.json");
        await File.WriteAllTextAsync(
            manifestPath,
            JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }),
            cancellationToken);

        await WriteLogAsync($"backup {backupId}: saved {files.Count} user files", cancellationToken);
        return result;
    }

    public async Task<ModUpdateResult> ApplyZipUpdateAsync(
        string zipPath,
        string destinationRoot,
        string modSubFolder,
        LauncherSettings settings,
        IProgress<int>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot = GetModRoot(settings);

        var protectedAbsolutePaths = BuildProtectedAbsolutePaths(settings, modRoot);

        using var archive = ZipFile.OpenRead(zipPath);
        var entries = archive.Entries.Where(e => !string.IsNullOrEmpty(e.Name)).ToList();
        var totalEntries = Math.Max(1, entries.Count);
        var zipRelativePaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        var written = 0;
        var skipped = 0;
        var pruned = 0;
        var errors = new List<string>();
        var done = 0;

        foreach (var entry in entries)
        {
            cancellationToken.ThrowIfCancellationRequested();

            var entryRelative = entry.FullName.Replace('/', Path.DirectorySeparatorChar);
            var destPath = Path.GetFullPath(Path.Combine(destinationRoot, entryRelative));

            if (!destPath.StartsWith(
                    Path.GetFullPath(destinationRoot) + Path.DirectorySeparatorChar,
                    StringComparison.OrdinalIgnoreCase))
            {
                errors.Add($"Suspicious ZIP entry ignored: {entry.FullName}");
                done++;
                continue;
            }

            var relativeToModSub = Path.GetRelativePath(modSubFolder, destPath);
            zipRelativePaths.Add(relativeToModSub);

            if (IsAbsolutePathProtected(destPath, protectedAbsolutePaths))
            {
                skipped++;
                done++;
                progress?.Report(done * 100 / totalEntries);
                await WriteLogAsync($"skip (protected dir): {relativeToModSub}", cancellationToken);
                continue;
            }

            var relativeToModRoot = Path.GetRelativePath(modRoot, destPath);
            if (IsProtectedRelativePath(relativeToModRoot))
            {
                skipped++;
                done++;
                progress?.Report(done * 100 / totalEntries);
                await WriteLogAsync($"skip (protected path): {relativeToModRoot}", cancellationToken);
                continue;
            }

            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
                entry.ExtractToFile(destPath, overwrite: true);
                written++;
            }
            catch (Exception ex)
            {
                errors.Add($"{entryRelative}: {ex.Message}");
            }

            done++;
            progress?.Report(done * 100 / totalEntries);
        }

        if (Directory.Exists(modSubFolder))
        {
            foreach (var existingFile in Directory.EnumerateFiles(modSubFolder, "*", SearchOption.AllDirectories))
            {
                cancellationToken.ThrowIfCancellationRequested();

                if (IsAbsolutePathProtected(existingFile, protectedAbsolutePaths))
                    continue;

                var relToModRoot = Path.GetRelativePath(modRoot, existingFile);
                if (IsProtectedRelativePath(relToModRoot))
                    continue;

                var relToModSub = Path.GetRelativePath(modSubFolder, existingFile);
                if (!zipRelativePaths.Contains(relToModSub))
                {
                    try
                    {
                        File.Delete(existingFile);
                        pruned++;
                        await WriteLogAsync($"pruned (obsolete): {relToModSub}", cancellationToken);
                    }
                    catch (Exception ex)
                    {
                        errors.Add($"pruning {relToModSub}: {ex.Message}");
                    }
                }
            }

            RemoveEmptyDirectories(modSubFolder, modRoot, protectedAbsolutePaths);
        }

        var result = new ModUpdateResult
        {
            FilesWritten = written,
            FilesSkipped = skipped,
            FilesPruned = pruned,
            Errors = errors
        };

        await WriteLogAsync(
            $"Update applied: {written} updated, {skipped} skipped (protected), {pruned} deleted (obsolete)" +
            (errors.Count > 0 ? $", {errors.Count} errors" : string.Empty),
            cancellationToken);

        return result;
    }


    public async Task RestoreBackupAsync(
        ModUpdateBackup backup,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        if (backup.Files.Count == 0)
        {
            await WriteLogAsync($"restore {backup.BackupId}: nothing to restore", cancellationToken);
            return;
        }

        Directory.CreateDirectory(backup.ModRoot);
        foreach (var file in backup.Files)
        {
            cancellationToken.ThrowIfCancellationRequested();
            progress?.Report($"Restoring {file.RelativePath}");
            var destination = Path.Combine(backup.ModRoot, file.RelativePath);
            await CopyFileAsync(file.BackupPath, destination, cancellationToken);
        }

        await VerifyBackupRestoreAsync(backup, cancellationToken);
        await WriteLogAsync($"restore {backup.BackupId}: restored {backup.Files.Count} user files", cancellationToken);
    }


    public async Task MigrateUserDataAsync(
        LauncherSettings settings,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot = GetModRoot(settings);
        var userDataRoot = GetUserDataRoot(settings);
        if (!Directory.Exists(modRoot))
            return;

        Directory.CreateDirectory(userDataRoot);
        var migrated = 0;
        foreach (var file in EnumerateUserDataFiles(modRoot))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative = Path.GetRelativePath(modRoot, file);
            var destination = Path.Combine(userDataRoot, relative);
            progress?.Report($"Migrating {relative}");
            await CopyFileAsync(file, destination, cancellationToken);
            migrated++;
        }

        await WriteLogAsync($"Migration: {migrated} user files copied to {userDataRoot}", cancellationToken);
    }

    internal IReadOnlyList<string> BuildProtectedAbsolutePaths(LauncherSettings settings, string modRoot)
    {
        var list = new List<string>
        {

            GetMyStuffPath(settings),

            GetUserDataRoot(settings)
        };

        if (Directory.Exists(modRoot))
        {
            try
            {
                foreach (var dir in Directory.EnumerateDirectories(modRoot, "*", SearchOption.TopDirectoryOnly))
                {
                    var name = Path.GetFileName(dir);
                    if (ProtectedDirectoryNames.Contains(name, StringComparer.OrdinalIgnoreCase))
                        list.Add(dir);
                }
            }
            catch {}
        }


        return list
            .Select(p => Path.GetFullPath(p).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private static bool IsAbsolutePathProtected(
        string absolutePath,
        IReadOnlyList<string> protectedAbsolutePaths)
    {
        var normalized = Path.GetFullPath(absolutePath);
        foreach (var protectedRoot in protectedAbsolutePaths)
        {
            if (normalized.Equals(protectedRoot, StringComparison.OrdinalIgnoreCase) ||
                normalized.StartsWith(protectedRoot + Path.DirectorySeparatorChar,
                    StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    private IEnumerable<string> EnumerateUserDataFiles(string modRoot)
    {
        if (!Directory.Exists(modRoot))
            yield break;

        IEnumerable<string> files;
        try
        {
            files = Directory.EnumerateFiles(modRoot, "*", SearchOption.AllDirectories).ToArray();
        }
        catch
        {
            yield break;
        }

        foreach (var file in files)
        {
            var relative = Path.GetRelativePath(modRoot, file);
            if (IsProtectedRelativePath(relative))
                yield return file;
        }
    }

    private static bool IsProtectedRelativePath(string relative)
    {
        if (string.IsNullOrWhiteSpace(relative) || relative.StartsWith("..", StringComparison.Ordinal))
            return false;

        var segments = relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);

        if (segments.Any(s => ProtectedDirectoryNames.Contains(s, StringComparer.OrdinalIgnoreCase)))
            return true;

        var fileName = Path.GetFileName(relative);
        if (ProtectedFileNames.Contains(fileName, StringComparer.OrdinalIgnoreCase))
            return true;

        var extension = Path.GetExtension(relative);
        if (ProtectedExtensions.Contains(extension, StringComparer.OrdinalIgnoreCase))
            return true;

        return relative.Contains("save", StringComparison.OrdinalIgnoreCase)
            || relative.Contains("license", StringComparison.OrdinalIgnoreCase)
            || relative.Contains("patent", StringComparison.OrdinalIgnoreCase)
            || relative.Contains("mii", StringComparison.OrdinalIgnoreCase)
            || relative.Contains("profile", StringComparison.OrdinalIgnoreCase);
    }

    internal void RemoveEmptyDirectories(
        string root,
        string modRoot,
        IReadOnlyList<string> protectedAbsolutePaths)
    {

        foreach (var dir in Directory.EnumerateDirectories(root, "*", SearchOption.AllDirectories)
                                     .OrderByDescending(d => d.Length))
        {
            if (IsAbsolutePathProtected(dir, protectedAbsolutePaths))
                continue;

            var relToModRoot = Path.GetRelativePath(modRoot, dir);
            if (IsProtectedRelativePath(relToModRoot))
                continue;

            try
            {
                if (!Directory.EnumerateFileSystemEntries(dir).Any())
                    Directory.Delete(dir);
            }
            catch
            {
            }
        }
    }

    private static async Task VerifyBackupRestoreAsync(ModUpdateBackup backup, CancellationToken cancellationToken)
    {
        foreach (var file in backup.Files)
        {
            var destination = Path.Combine(backup.ModRoot, file.RelativePath);
            if (!File.Exists(destination))
                throw new IOException($"Restore failed: {file.RelativePath} is missing.");

            var hash = await ComputeSha256Async(destination, cancellationToken);
            if (!hash.Equals(file.Sha256, StringComparison.OrdinalIgnoreCase))
                throw new IOException($"Restore failed: {file.RelativePath} hash does not match.");
        }
    }

    private static async Task CopyFileAsync(string source, string destination, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        await using var input = new FileStream(source, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 81920, true);
        await using var output = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);
        await input.CopyToAsync(output, cancellationToken);
    }

    internal static async Task<string> ComputeSha256Async(string path, CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 81920, true);
        return Convert.ToHexString(await SHA256.HashDataAsync(stream, cancellationToken)).ToLowerInvariant();
    }

    private async Task WriteLogAsync(string message, CancellationToken cancellationToken)
    {
        try
        {
            var path = GetOperationLogPath();
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await File.AppendAllTextAsync(
                path,
                $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] {message}{Environment.NewLine}",
                cancellationToken);
        }
        catch
        {

        }
    }
}


public sealed class ModUpdateResult
{

    public int FilesWritten { get; init; }

    public int FilesSkipped { get; init; }

    public int FilesPruned { get; init; }

    public List<string> Errors { get; init; } = [];

    public bool HasErrors => Errors.Count > 0;

    public override string ToString() =>
        $"{FilesWritten} updated, {FilesSkipped} skipped (protected), {FilesPruned} deleted (obsolete)" +
        (HasErrors ? $", {Errors.Count} errors" : string.Empty);
}