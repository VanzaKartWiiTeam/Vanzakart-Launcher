using System.IO;

namespace VanzaKartLauncher.Services;

public static class ShortcutHelper
{
    public static string DesktopShortcutPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory),
        "VanzaKart Launcher.lnk");
}