using System.IO;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class ModConflictService
{
    private static readonly string[] AddonExtensions = { ".szs", ".brres", ".tpl", ".png", ".json", ".xml", ".ini" };

    public IReadOnlyList<ModConflictInfo> ScanAddonConflicts(string addonFolder)
    {
        if (string.IsNullOrWhiteSpace(addonFolder) || !Directory.Exists(addonFolder))
        {
            return Array.Empty<ModConflictInfo>();
        }

        return Directory.EnumerateFiles(addonFolder, "*.*", SearchOption.AllDirectories)
            .Where(path => AddonExtensions.Contains(Path.GetExtension(path), StringComparer.OrdinalIgnoreCase))
            .GroupBy(path => Path.GetFileName(path), StringComparer.OrdinalIgnoreCase)
            .Where(group => group.Count() > 1)
            .Select(group => new ModConflictInfo
            {
                FileName = group.Key,
                Count = group.Count(),
                Locations = string.Join(" | ", group.Select(path => Path.GetRelativePath(addonFolder, path)))
            })
            .OrderByDescending(info => info.Count)
            .ThenBy(info => info.FileName)
            .ToArray();
    }
}
