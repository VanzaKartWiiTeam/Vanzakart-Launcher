using Microsoft.Win32;
using System.Globalization;
using System.IO;

namespace VanzaKartSetup.Services;

public sealed class WindowsInstallRegistryService
{
    public const string ProductName = "VanzaKart Launcher";
    public const string Publisher = "VanzaKart";
    public const string ProductVersion = "1.2.5";
    public const string UninstallKeyName = "VanzaKartLauncher";

    private const string UninstallRoot = @"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    private const string AppPathsRoot = @"Software\Microsoft\Windows\CurrentVersion\App Paths";

    public InstalledApplicationInfo? TryReadExistingInstall()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey($@"{UninstallRoot}\{UninstallKeyName}");
            if (key == null)
            {
                return null;
            }

            var installLocation = key.GetValue("InstallLocation") as string;
            if (string.IsNullOrWhiteSpace(installLocation))
            {
                return null;
            }

            return new InstalledApplicationInfo(
                installLocation,
                key.GetValue("DisplayVersion") as string ?? ProductVersion,
                key.GetValue("DisplayName") as string ?? ProductName);
        }
        catch
        {
            return null;
        }
    }

    public void Register(string installDir, string launcherExePath, string uninstallerPath, long estimatedSizeBytes)
    {
        Directory.CreateDirectory(installDir);

        using var key = Registry.CurrentUser.CreateSubKey($@"{UninstallRoot}\{UninstallKeyName}");
        if (key == null)
        {
            throw new InvalidOperationException("Unable to create Windows uninstall registry key.");
        }

        var uninstallCommand = $"\"{uninstallerPath}\"";
        var iconPath = File.Exists(launcherExePath) ? launcherExePath : uninstallerPath;

        key.SetValue("DisplayName", ProductName, RegistryValueKind.String);
        key.SetValue("DisplayVersion", ProductVersion, RegistryValueKind.String);
        key.SetValue("Publisher", Publisher, RegistryValueKind.String);
        key.SetValue("InstallLocation", installDir, RegistryValueKind.String);
        key.SetValue("InstallSource", AppContext.BaseDirectory, RegistryValueKind.String);
        key.SetValue("DisplayIcon", iconPath, RegistryValueKind.String);
        key.SetValue("UninstallString", uninstallCommand, RegistryValueKind.String);
        key.SetValue("QuietUninstallString", $"{uninstallCommand} /quiet", RegistryValueKind.String);
        key.SetValue("URLInfoAbout", "https://sitodaking.it/", RegistryValueKind.String);
        key.SetValue("NoModify", 1, RegistryValueKind.DWord);
        key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
        var estimatedSizeKb = (int)Math.Min(int.MaxValue, Math.Max(1, estimatedSizeBytes / 1024));
        key.SetValue("EstimatedSize", estimatedSizeKb, RegistryValueKind.DWord);
        key.SetValue("InstallDate", DateTime.Now.ToString("yyyyMMdd", CultureInfo.InvariantCulture), RegistryValueKind.String);

        using var appPathKey = Registry.CurrentUser.CreateSubKey($@"{AppPathsRoot}\{Path.GetFileName(launcherExePath)}");
        appPathKey?.SetValue(string.Empty, launcherExePath, RegistryValueKind.String);
        appPathKey?.SetValue("Path", installDir, RegistryValueKind.String);
    }
}

public sealed record InstalledApplicationInfo(string InstallLocation, string DisplayVersion, string DisplayName);
