using System.Diagnostics;
using System.IO;

namespace VanzaKartSetup.Services;

public sealed class ShortcutService
{
    public void CreateDesktopShortcut(string targetExePath, string workingDirectory)
    {
        var desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        var shortcutPath = Path.Combine(desktop, "VanzaKart Launcher.lnk");
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
}
