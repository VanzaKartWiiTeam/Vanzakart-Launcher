using System.IO;
using System.IO.Compression;

namespace VanzaKartLauncher.Services;

public sealed class ArchiveService
{
    public Task ExtractZipAsync(string zipPath, string destinationFolder, string? folderToReplace = null)
    {
        return Task.Run(() =>
        {
            if (!string.IsNullOrWhiteSpace(folderToReplace) && Directory.Exists(folderToReplace))
            {
                Directory.Delete(folderToReplace, true);
            }

            Directory.CreateDirectory(destinationFolder);
            ZipFile.ExtractToDirectory(zipPath, destinationFolder, overwriteFiles: true);
        });
    }
}
