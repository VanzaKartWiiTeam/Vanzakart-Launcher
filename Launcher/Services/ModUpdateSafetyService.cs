using System.IO;
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
    {
        return Path.Combine(AppContext.BaseDirectory, "Backups", "ModUpdates");
    }

    public string GetOperationLogPath()
    {
        return Path.Combine(AppContext.BaseDirectory, "Logs", "mod-update.log");
    }

    public string GetModRoot(LauncherSettings settings)
    {
        return Path.Combine(settings.GetModFolder(), "VanzaKart");
    }

    public string GetUserDataRoot(LauncherSettings settings)
    {
        return Path.Combine(settings.GetModFolder(), "VanzaKart_UserData");
    }

    public bool IsProtectedUserDataPath(string path, LauncherSettings settings)
    {
        var modRoot = GetModRoot(settings);
        if (string.IsNullOrWhiteSpace(path) || string.IsNullOrWhiteSpace(modRoot))
        {
            return false;
        }

        string relative;
        try
        {
            relative = Path.GetRelativePath(modRoot, path);
        }
        catch
        {
            return false;
        }

        return IsProtectedRelativePath(relative);
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
            await WriteLogAsync($"backup {backupId}: mod root not found, nothing to preserve", cancellationToken);
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
            progress?.Report($"Preserving {relative}");

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
        await File.WriteAllTextAsync(manifestPath, JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }), cancellationToken);
        await WriteLogAsync($"backup {backupId}: preserved {files.Count} user file(s)", cancellationToken);
        return result;
    }

    public async Task RestoreBackupAsync(
        ModUpdateBackup backup,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        if (backup.Files.Count == 0)
        {
            await WriteLogAsync($"restore {backup.BackupId}: no files to restore", cancellationToken);
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
        await WriteLogAsync($"restore {backup.BackupId}: restored {backup.Files.Count} user file(s)", cancellationToken);
    }

    public async Task MigrateUserDataAsync(
        LauncherSettings settings,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot = GetModRoot(settings);
        var userDataRoot = GetUserDataRoot(settings);
        if (!Directory.Exists(modRoot))
        {
            return;
        }

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

        await WriteLogAsync($"migration: mirrored {migrated} user file(s) to {userDataRoot}", cancellationToken);
    }

    private IEnumerable<string> EnumerateUserDataFiles(string modRoot)
    {
        if (!Directory.Exists(modRoot))
        {
            yield break;
        }

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
            {
                yield return file;
            }
        }
    }

    private static bool IsProtectedRelativePath(string relative)
    {
        if (string.IsNullOrWhiteSpace(relative) || relative.StartsWith("..", StringComparison.Ordinal))
        {
            return false;
        }

        var segments = relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        if (segments.Any(segment => ProtectedDirectoryNames.Contains(segment, StringComparer.OrdinalIgnoreCase)))
        {
            return true;
        }

        var fileName = Path.GetFileName(relative);
        if (ProtectedFileNames.Contains(fileName, StringComparer.OrdinalIgnoreCase))
        {
            return true;
        }

        var extension = Path.GetExtension(relative);
        if (ProtectedExtensions.Contains(extension, StringComparer.OrdinalIgnoreCase))
        {
            return true;
        }

        return relative.Contains("save", StringComparison.OrdinalIgnoreCase)
               || relative.Contains("license", StringComparison.OrdinalIgnoreCase)
               || relative.Contains("patent", StringComparison.OrdinalIgnoreCase)
               || relative.Contains("mii", StringComparison.OrdinalIgnoreCase)
               || relative.Contains("profile", StringComparison.OrdinalIgnoreCase);
    }

    private static async Task VerifyBackupRestoreAsync(ModUpdateBackup backup, CancellationToken cancellationToken)
    {
        foreach (var file in backup.Files)
        {
            var destination = Path.Combine(backup.ModRoot, file.RelativePath);
            if (!File.Exists(destination))
            {
                throw new IOException($"User data restore failed: {file.RelativePath} is missing.");
            }

            var hash = await ComputeSha256Async(destination, cancellationToken);
            if (!hash.Equals(file.Sha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new IOException($"User data restore failed: {file.RelativePath} hash mismatch.");
            }
        }
    }

    private static async Task CopyFileAsync(string source, string destination, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        await using var input = new FileStream(source, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 81920, true);
        await using var output = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);
        await input.CopyToAsync(output, cancellationToken);
    }

    private static async Task<string> ComputeSha256Async(string path, CancellationToken cancellationToken)
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
            await File.AppendAllTextAsync(path, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] {message}{Environment.NewLine}", cancellationToken);
        }
        catch
        {
        }
    }
}
