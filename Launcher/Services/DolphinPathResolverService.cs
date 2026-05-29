using System.IO;
using Microsoft.Win32;

namespace VanzaKartLauncher.Services;

public sealed class DolphinPathResolverService
{
    public string TryFindUserFolderPath(string configuredDolphinPath = "")
    {
        return FindUserFolderCandidates(configuredDolphinPath)
            .FirstOrDefault(path => !string.IsNullOrWhiteSpace(path) && Directory.Exists(path))
            ?? string.Empty;
    }

    public IReadOnlyList<string> FindUserFolderCandidates(string configuredDolphinPath = "")
    {
        var candidates = new List<string>();
        var portable = TryFindPortableUserFolder(configuredDolphinPath);
        if (!string.IsNullOrWhiteSpace(portable))
        {
            candidates.Add(portable);
        }

        var registryUserPath = TryFindRegistryUserFolder();
        if (!string.IsNullOrWhiteSpace(registryUserPath))
        {
            candidates.Add(registryUserPath);
        }

        candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments), "Dolphin Emulator"));
        candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Dolphin Emulator"));
        candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Dolphin Emulator"));
        candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Documents", "Dolphin Emulator"));
        candidates.AddRange(FindPortableCandidatesNearCommonInstallRoots());

        return candidates
            .Select(NormalizePathSafe)
            .Where(path => !string.IsNullOrWhiteSpace(path))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static string TryFindPortableUserFolder(string configuredDolphinPath)
    {
        if (string.IsNullOrWhiteSpace(configuredDolphinPath))
        {
            return string.Empty;
        }

        try
        {
            var dolphinDirectory = Path.GetDirectoryName(Path.GetFullPath(configuredDolphinPath));
            if (string.IsNullOrWhiteSpace(dolphinDirectory))
            {
                return string.Empty;
            }

            var portableFlag = Path.Combine(dolphinDirectory, "portable.txt");
            var portableUser = Path.Combine(dolphinDirectory, "User");
            return File.Exists(portableFlag) && Directory.Exists(portableUser)
                ? portableUser
                : string.Empty;
        }
        catch
        {
            return string.Empty;
        }
    }

    private static string TryFindRegistryUserFolder()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Dolphin Emulator");
            var path = key?.GetValue("UserConfigPath") as string;
            if (!string.IsNullOrWhiteSpace(path))
            {
                return path.Replace(Path.AltDirectorySeparatorChar, Path.DirectorySeparatorChar);
            }
        }
        catch
        {
        }

        return string.Empty;
    }

    private static IEnumerable<string> FindPortableCandidatesNearCommonInstallRoots()
    {
        var roots = new[]
        {
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
            Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads")
        };

        foreach (var root in roots.Where(Directory.Exists))
        {
            IEnumerable<string> directories;
            try
            {
                directories = Directory.EnumerateDirectories(root, "*Dolphin*", SearchOption.TopDirectoryOnly).ToArray();
            }
            catch
            {
                continue;
            }

            foreach (var directory in directories)
            {
                var portable = Path.Combine(directory, "User");
                if (Directory.Exists(portable))
                {
                    yield return portable;
                }
            }
        }
    }

    private static string NormalizePathSafe(string path)
    {
        try
        {
            return string.IsNullOrWhiteSpace(path) ? string.Empty : Path.GetFullPath(path);
        }
        catch
        {
            return string.Empty;
        }
    }
}
