using Microsoft.Win32;
using System.Globalization;
using System.IO;

namespace VanzaKartLauncher.Services;

public static class WindowsInstallRegistryService
{
    private const string ProductName = "VanzaKart Launcher";
    private const string Publisher = "VanzaKart";
    private const string UninstallerFileName = "VanzaKart Uninstaller.exe";
    private const string UninstallKeyPath =
        @"Software\Microsoft\Windows\CurrentVersion\Uninstall\VanzaKartLauncher";
    private const string AppPathsRoot =
        @"Software\Microsoft\Windows\CurrentVersion\App Paths";

    public static void SynchronizeRegistration(string version)
    {
        if (string.IsNullOrWhiteSpace(version))
        {
            return;
        }

        try
        {
            var installDirectory = NormalizeDirectory(AppContext.BaseDirectory);
            var launcherPath = Environment.ProcessPath;
            var uninstallerPath = Path.Combine(installDirectory, UninstallerFileName);

            if (string.IsNullOrWhiteSpace(launcherPath) ||
                !File.Exists(launcherPath))
            {
                return;
            }

            using (var existingKey = Registry.CurrentUser.OpenSubKey(UninstallKeyPath))
            {
                var registeredLocation = existingKey?.GetValue("InstallLocation") as string;
                if (!string.IsNullOrWhiteSpace(registeredLocation) &&
                    !PathsEqual(registeredLocation, installDirectory))
                {
                    return;
                }
            }

            using var key = Registry.CurrentUser.CreateSubKey(UninstallKeyPath);
            if (key == null)
            {
                return;
            }

            var uninstallCommand = $"\"{uninstallerPath}\"";
            key.SetValue("DisplayName", ProductName, RegistryValueKind.String);
            key.SetValue("DisplayVersion", version, RegistryValueKind.String);
            key.SetValue("Publisher", Publisher, RegistryValueKind.String);
            key.SetValue("InstallLocation", installDirectory, RegistryValueKind.String);
            key.SetValue("InstallSource", installDirectory, RegistryValueKind.String);
            key.SetValue("DisplayIcon", launcherPath, RegistryValueKind.String);
            key.SetValue("UninstallString", uninstallCommand, RegistryValueKind.String);
            key.SetValue("QuietUninstallString", $"{uninstallCommand} /quiet", RegistryValueKind.String);
            key.SetValue("URLInfoAbout", "https://sitodaking.it/", RegistryValueKind.String);
            key.SetValue("NoModify", 1, RegistryValueKind.DWord);
            key.SetValue("NoRepair", 1, RegistryValueKind.DWord);

            if (key.GetValue("EstimatedSize") == null)
            {
                key.SetValue("EstimatedSize", GetEstimatedSizeKb(installDirectory), RegistryValueKind.DWord);
            }

            if (key.GetValue("InstallDate") == null)
            {
                key.SetValue(
                    "InstallDate",
                    DateTime.Now.ToString("yyyyMMdd", CultureInfo.InvariantCulture),
                    RegistryValueKind.String);
            }

            using var appPathKey = Registry.CurrentUser.CreateSubKey(
                $@"{AppPathsRoot}\{Path.GetFileName(launcherPath)}");
            appPathKey?.SetValue(string.Empty, launcherPath, RegistryValueKind.String);
            appPathKey?.SetValue("Path", installDirectory, RegistryValueKind.String);
        }
        catch
        {
            // Registry synchronization must never prevent the launcher from starting.
        }
    }

    private static bool PathsEqual(string firstPath, string secondPath)
    {
        try
        {
            return string.Equals(
                NormalizeDirectory(firstPath),
                NormalizeDirectory(secondPath),
                StringComparison.OrdinalIgnoreCase);
        }
        catch
        {
            return false;
        }
    }

    private static string NormalizeDirectory(string path) =>
        Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);

    private static int GetEstimatedSizeKb(string installDirectory)
    {
        try
        {
            var totalBytes = Directory.EnumerateFiles(installDirectory, "*", SearchOption.AllDirectories)
                .Sum(path => new FileInfo(path).Length);
            return (int)Math.Min(int.MaxValue, Math.Max(1, totalBytes / 1024));
        }
        catch
        {
            return 1;
        }
    }
}
