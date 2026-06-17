using System.Diagnostics;
using System.IO;

namespace VanzaKartSetup.Services;

public sealed class ShortcutService
{
    public void CreateDesktopShortcut(string targetExePath, string workingDirectory)
        => CreateShortcut(GetDesktopShortcutPath(), targetExePath, workingDirectory);

    public void CreateStartMenuShortcut(string targetExePath, string workingDirectory)
    {
        var programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
        var folder = Path.Combine(programs, "VanzaKart");
        Directory.CreateDirectory(folder);
        CreateShortcut(Path.Combine(folder, "VanzaKart Launcher.lnk"), targetExePath, workingDirectory);
    }

    public void CreateQuickLaunchShortcut(string targetExePath, string workingDirectory)
    {
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        var folder = Path.Combine(appData, @"Microsoft\Internet Explorer\Quick Launch");
        Directory.CreateDirectory(folder);
        CreateShortcut(Path.Combine(folder, "VanzaKart Launcher.lnk"), targetExePath, workingDirectory);
    }

    public void RemoveAllShortcuts()
    {
        TryDelete(GetDesktopShortcutPath());

        var programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
        var startFolder = Path.Combine(programs, "VanzaKart");
        TryDelete(Path.Combine(startFolder, "VanzaKart Launcher.lnk"));
        TryDelete(startFolder);

        var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        TryDelete(Path.Combine(appData, @"Microsoft\Internet Explorer\Quick Launch\VanzaKart Launcher.lnk"));
    }

    private static string GetDesktopShortcutPath()
    {
        var desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        return Path.Combine(desktop, "VanzaKart Launcher.lnk");
    }

    private static void CreateShortcut(string shortcutPath, string targetExePath, string workingDirectory)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(shortcutPath)!);
        var scriptPath = Path.Combine(Path.GetTempPath(), "vanzakart_shortcut.vbs");

        var script = string.Join(Environment.NewLine, new[]
        {
            "Set shell = CreateObject(\"WScript.Shell\")",
            $"Set shortcut = shell.CreateShortcut(\"{shortcutPath}\")",
            $"shortcut.TargetPath = \"{targetExePath}\"",
            $"shortcut.WorkingDirectory = \"{workingDirectory}\"",
            $"shortcut.IconLocation = \"{targetExePath},0\"",
            "shortcut.Description = \"VanzaKart Modpack Launcher\"",
            "shortcut.Save"
        });

        File.WriteAllText(scriptPath, script);
        try
        {
            using var process = Process.Start(new ProcessStartInfo
            {
                FileName = "wscript.exe",
                Arguments = $"\"{scriptPath}\"",
                WindowStyle = ProcessWindowStyle.Hidden,
                CreateNoWindow = true
            });
            process?.WaitForExit();
        }
        finally
        {
            if (File.Exists(scriptPath)) File.Delete(scriptPath);
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
            else if (Directory.Exists(path) && !Directory.EnumerateFileSystemEntries(path).Any())
            {
                Directory.Delete(path, false);
            }
        }
        catch
        {
            // Shortcut cleanup is best-effort and should not block installation.
        }
    }
}
