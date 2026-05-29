using System.IO;
using System.IO.Compression;

namespace VanzaKartLauncher.Services;

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
}
