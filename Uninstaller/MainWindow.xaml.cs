using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Text.Json;

namespace VanzaKartUninstaller;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
    }

    private void UninstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        var answer = System.Windows.MessageBox.Show(
            "This will delete the launcher and the mod. Do you want to continue?",
            "Confirm",
            MessageBoxButton.YesNo,
            MessageBoxImage.Warning);

        if (answer != MessageBoxResult.Yes)
        {
            return;
        }

        try
        {
            var installDir = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar);
            var desktopPath = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
            var shortcutPath = Path.Combine(desktopPath, "VanzaKart Launcher.lnk");
            var settingsPath = Path.Combine(installDir, "launcher_settings.json");
            string? modFolder = null;

            if (File.Exists(settingsPath))
            {
                using var stream = File.OpenRead(settingsPath);
                using var doc = JsonDocument.Parse(stream);
                if (doc.RootElement.TryGetProperty("UserFolderPath", out var userFolderElement))
                {
                    var userFolder = userFolderElement.GetString();
                    if (!string.IsNullOrWhiteSpace(userFolder))
                    {
                        modFolder = Path.Combine(userFolder, "Load", "Riivolution", "VanzaKart");
                    }
                }
            }

            if (!string.IsNullOrWhiteSpace(modFolder) && Directory.Exists(modFolder))
            {
                Directory.Delete(modFolder, true);
            }

            if (File.Exists(shortcutPath))
            {
                File.Delete(shortcutPath);
            }

            var keepSettings = System.Windows.MessageBox.Show(
                "Do you want to keep the saved paths?",
                "Settings",
                MessageBoxButton.YesNo,
                MessageBoxImage.Question);

            if (keepSettings == MessageBoxResult.No && File.Exists(settingsPath))
            {
                File.Delete(settingsPath);
            }

            var batchPath = Path.Combine(Path.GetTempPath(), "vanzakart_uninstall.bat");
            var script = string.Join(Environment.NewLine, new[]
            {
                "@echo off",
                "timeout /t 2 /nobreak >nul",
                $"rd /s /q \"{installDir}\"",
                "del \"%~f0\""
            });
            File.WriteAllText(batchPath, script);

            Process.Start(new ProcessStartInfo
            {
                FileName = batchPath,
                UseShellExecute = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                CreateNoWindow = true
            });

            StatusTextBlock.Text = "Uninstallation complete.";
            Close();
        }
        catch (Exception ex)
        {
            System.Windows.MessageBox.Show(ex.Message, "Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
}
