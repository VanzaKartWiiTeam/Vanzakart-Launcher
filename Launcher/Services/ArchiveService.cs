using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;

namespace VanzaKartLauncher.Services;

public sealed record ArchiveExtractionProgress(int Percent, int WrittenFiles, int SkippedFiles, int PreservedUserFiles);

public sealed class ArchiveService
{
    public Task ValidateZipAsync(string zipPath, CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            cancellationToken.ThrowIfCancellationRequested();

            using var archive = ZipFile.OpenRead(zipPath);
            if (archive.Entries.Count == 0)
            {
                throw new InvalidDataException("The downloaded archive is empty.");
            }

            foreach (var entry in archive.Entries)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (string.IsNullOrEmpty(entry.Name))
                {
                    continue;
                }

                using var stream = entry.Open();
                stream.CopyTo(Stream.Null);
            }
        }, cancellationToken);
    }

    public async Task ExtractZipAsync(string zipPath, string destinationFolder, string? folderToReplace = null, IProgress<int>? progress = null)
    {
        await Task.Run(() =>
        {
            if (!string.IsNullOrWhiteSpace(folderToReplace) && Directory.Exists(folderToReplace))
                Directory.Delete(folderToReplace, true);

            Directory.CreateDirectory(destinationFolder);
            var destinationRoot = Path.GetFullPath(destinationFolder);
            if (!destinationRoot.EndsWith(Path.DirectorySeparatorChar))
            {
                destinationRoot += Path.DirectorySeparatorChar;
            }

            using var archive = ZipFile.OpenRead(zipPath);
            int totalEntries = archive.Entries.Count;
            int processed = 0;

            foreach (var entry in archive.Entries)
            {
                string destPath = Path.GetFullPath(Path.Combine(destinationFolder, entry.FullName));
                if (!destPath.StartsWith(destinationRoot, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException("The archive contains an unsafe file path.");
                }

                if (string.IsNullOrEmpty(entry.Name))
                {
                    Directory.CreateDirectory(destPath);
                }
                else
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
                    entry.ExtractToFile(destPath, true);
                }
                processed++;
                progress?.Report(totalEntries == 0 ? 100 : (int)((double)processed / totalEntries * 100));
            }
        });
    }

    public async Task ExtractZipIncrementalAsync(
        string zipPath,
        string destinationFolder,
        Func<string, bool> preserveExistingFile,
        IProgress<ArchiveExtractionProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        await Task.Run(() =>
        {
            Directory.CreateDirectory(destinationFolder);
            var destinationRoot = Path.GetFullPath(destinationFolder);
            if (!destinationRoot.EndsWith(Path.DirectorySeparatorChar))
            {
                destinationRoot += Path.DirectorySeparatorChar;
            }

            using var archive = ZipFile.OpenRead(zipPath);
            var entries = archive.Entries.ToArray();
            var totalEntries = Math.Max(entries.Length, 1);
            var processed = 0;
            var written = 0;
            var skipped = 0;
            var preserved = 0;

            foreach (var entry in entries)
            {
                cancellationToken.ThrowIfCancellationRequested();

                var destPath = Path.GetFullPath(Path.Combine(destinationFolder, entry.FullName));
                if (!destPath.StartsWith(destinationRoot, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException("The archive contains an unsafe file path.");
                }

                if (string.IsNullOrEmpty(entry.Name))
                {
                    Directory.CreateDirectory(destPath);
                }
                else if (File.Exists(destPath) && preserveExistingFile(destPath))
                {
                    preserved++;
                }
                else if (IsEntrySameAsExistingFile(entry, destPath))
                {
                    skipped++;
                }
                else
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
                    entry.ExtractToFile(destPath, overwrite: true);
                    written++;
                }

                processed++;
                progress?.Report(new ArchiveExtractionProgress(
                    (int)Math.Clamp(processed / (double)totalEntries * 100.0, 0.0, 100.0),
                    written,
                    skipped,
                    preserved));
            }
        }, cancellationToken);
    }

    private static bool IsEntrySameAsExistingFile(ZipArchiveEntry entry, string destPath)
    {
        if (!File.Exists(destPath))
        {
            return false;
        }

        var existing = new FileInfo(destPath);
        if (existing.Length != entry.Length)
        {
            return false;
        }

        using var entryStream = entry.Open();
        using var existingStream = File.OpenRead(destPath);
        var entryHash = SHA256.HashData(entryStream);
        var existingHash = SHA256.HashData(existingStream);
        return entryHash.SequenceEqual(existingHash);
    }
}
