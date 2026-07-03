using Microsoft.Win32;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace VanzaKartUninstaller;

public partial class MainWindow : Window
{
    private const string ProductName = "VanzaKart Launcher";
    private const string UninstallKeyName = "VanzaKartLauncher";
    private const string UninstallRoot = @"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    private const string AppPathsRoot = @"Software\Microsoft\Windows\CurrentVersion\App Paths";

    private readonly StringBuilder _log = new();
    private string _installDir = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
    private string _version = "Unknown";
    private bool _isBusy;

    public MainWindow()
    {
        InitializeComponent();

        Loaded += async (_, _) =>
        {
            AnimateEntrance();
            StartAmbientMotion();
            LoadInstallInfo();

            if (Environment.GetCommandLineArgs().Any(arg => string.Equals(arg, "/quiet", StringComparison.OrdinalIgnoreCase)))
            {
                await RunUninstallAsync(quiet: true);
            }
        };
    }

    private void LoadInstallInfo()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey($@"{UninstallRoot}\{UninstallKeyName}");
            if (key != null)
            {
                _installDir = key.GetValue("InstallLocation") as string ?? _installDir;
                _version = key.GetValue("DisplayVersion") as string ?? _version;
            }
            else
            {
                var launcherExe = Directory.EnumerateFiles(_installDir, "*.exe", SearchOption.TopDirectoryOnly)
                    .FirstOrDefault(path => Path.GetFileName(path).Contains("Launcher", StringComparison.OrdinalIgnoreCase));
                if (!string.IsNullOrWhiteSpace(launcherExe))
                {
                    _version = FileVersionInfo.GetVersionInfo(launcherExe).ProductVersion ?? _version;
                }
            }
        }
        catch
        {
            // Fall back to the executable directory.
        }

        VersionTextBlock.Text = _version;
        PathTextBlock.Text = _installDir;
        SizeTextBlock.Text = Directory.Exists(_installDir) ? FormatBytes(GetDirectorySize(_installDir)) : "Not found";
        AppendLog($"Install location: {_installDir}");
        AppendLog($"Installed version: {_version}");
    }

    private async void UninstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        await RunUninstallAsync(quiet: false);
    }

    private async Task RunUninstallAsync(bool quiet)
    {
        SetBusy(true);
        UninstallProgressBar.Visibility = Visibility.Visible;
        UninstallProgressBar.Value = 0;
        StatusTextBlock.Text = "Preparing removal...";
        FooterStatusTextBlock.Text = "Uninstall in progress...";

        try
        {
            var removeMod = DeleteModCheckBox.IsChecked == true;
            var deleteUserData = DeleteAllUserDataRadioButton.IsChecked == true;

            AppendLog("Uninstall started.");

            await Task.Run(() =>
            {
                DeleteCache();

                Dispatcher.Invoke(() => UninstallProgressBar.Value = 18);

                DeleteTemporaryFiles();

                Dispatcher.Invoke(() => UninstallProgressBar.Value = 34);

                if (removeMod)
                {
                    DeleteInstalledMod();
                }

                Dispatcher.Invoke(() => UninstallProgressBar.Value = 48);

                DeleteLogs();

                Dispatcher.Invoke(() => UninstallProgressBar.Value = 62);

                if (deleteUserData)
                {
                    DeleteExternalUserData();
                }

                DeleteSettings();

                Dispatcher.Invoke(() => UninstallProgressBar.Value = 76);

                RemoveShortcuts();
                RemoveRegistryEntries();
            });

            UninstallProgressBar.Value = 88;
            if (Directory.Exists(_installDir))
            {
                AppendLog("Scheduling launcher folder removal after exit.");
                ScheduleDirectoryRemoval(_installDir);
            }

            UninstallProgressBar.Value = 100;
            StatusTextBlock.Text = "Uninstallation complete.";
            FooterStatusTextBlock.Text = "VanzaKart removed.";
            AppendLog("Uninstall completed successfully.");

            if (quiet || Directory.Exists(_installDir))
            {
                await Task.Delay(650);
                Close();
            }
            else
            {
                UninstallButton.Content = "Close";
                UninstallButton.Click -= UninstallButton_OnClick;
                UninstallButton.Click += (_, _) => Close();
            }
        }
        catch (Exception ex)
        {
            AppendLog($"ERROR: {ex.Message}");
            StatusTextBlock.Text = "Uninstallation failed.";
            FooterStatusTextBlock.Text = ex.Message;
            FooterStatusTextBlock.Foreground = (System.Windows.Media.Brush)FindResource("DangerBrush");
            SetBusy(false);
        }
    }

    private void DeleteCache()
    {
        AppendLog("Deleting cache.");
        DeleteKnownDirectories("Cache", "cache", ".cache");
    }

    private void DeleteTemporaryFiles()
    {
        AppendLog("Deleting temporary files.");
        DeleteKnownDirectories("Temp", "temp", "Temporary", "tmp");

        foreach (var file in Directory.EnumerateFiles(Path.GetTempPath(), "VanzaKart*", SearchOption.TopDirectoryOnly))
        {
            TryDeleteFile(file);
        }
    }

    private void DeleteSettings()
    {
        AppendLog("Deleting launcher settings.");
        foreach (var file in new[]
        {
            "launcher_settings.json",
            "user_preferences.json",
            "VanzaKart_launcher.json",
            "VKBeta_launcher.json",
            "mod_version.txt",
            "mod_beta_version.txt",
            "musicpack_version.txt",
            "musicpack_beta_version.txt",
            "mod_install_state.json"
        })
        {
            TryDeleteFile(Path.Combine(_installDir, file));
        }
    }

    private void DeleteLogs()
    {
        AppendLog("Deleting logs.");
        DeleteKnownDirectories("Logs", "logs", "Log", "log");

        if (Directory.Exists(_installDir))
        {
            foreach (var file in Directory.EnumerateFiles(_installDir, "*.log", SearchOption.TopDirectoryOnly))
            {
                TryDeleteFile(file);
            }
        }
    }

    private void DeleteInstalledMod()
    {
        AppendLog("Deleting installed Stable and Beta modpacks.");
        var userFolder = TryReadDolphinUserFolder();
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            AppendLog("No Dolphin user folder found for modpack removal.");
            return;
        }

        DeleteKnownPath(Path.Combine(userFolder, "Load", "Riivolution", "VanzaKart"));
        DeleteKnownPath(Path.Combine(userFolder, "Load", "Riivolution", "VKBeta"));
    }

    private void DeleteExternalUserData()
    {
        AppendLog("Deleting external user data.");
        var userFolder = TryReadDolphinUserFolder();
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            AppendLog("No external Dolphin user folder found.");
            return;
        }

        DeleteKnownPath(Path.Combine(userFolder, "Load", "Riivolution", "VanzaKart_UserData"));
        DeleteKnownPath(Path.Combine(userFolder, "Load", "Riivolution", "VKBeta_UserData"));
    }

    private string? TryReadDolphinUserFolder()
    {
        var settingsPath = Path.Combine(_installDir, "launcher_settings.json");
        if (!File.Exists(settingsPath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(settingsPath);
            using var doc = JsonDocument.Parse(stream);
            if (doc.RootElement.TryGetProperty("UserFolderPath", out var userFolderElement))
            {
                return userFolderElement.GetString();
            }
        }
        catch
        {
            AppendLog("Unable to read launcher_settings.json.");
        }

        return null;
    }

    private void DeleteKnownDirectories(params string[] names)
    {
        foreach (var name in names)
        {
            DeleteKnownPath(Path.Combine(_installDir, name));
        }
    }

    private void DeleteKnownPath(string path)
    {
        try
        {
            if (Directory.Exists(path))
            {
                Directory.Delete(path, recursive: true);
                AppendLog($"Deleted directory: {path}");
            }
            else if (File.Exists(path))
            {
                File.Delete(path);
                AppendLog($"Deleted file: {path}");
            }
        }
        catch (Exception ex)
        {
            AppendLog($"Skipped {path}: {ex.Message}");
        }
    }

    private void TryDeleteFile(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
                AppendLog($"Deleted file: {path}");
            }
        }
        catch (Exception ex)
        {
            AppendLog($"Skipped {path}: {ex.Message}");
        }
    }

    private void RemoveShortcuts()
    {
        AppendLog("Removing shortcuts.");
        var desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        TryDeleteFile(Path.Combine(desktop, "VanzaKart Launcher.lnk"));

        var programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
        var startFolder = Path.Combine(programs, "VanzaKart");
        TryDeleteFile(Path.Combine(startFolder, "VanzaKart Launcher.lnk"));
        TryDeleteEmptyDirectory(startFolder);

        var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        TryDeleteFile(Path.Combine(appData, @"Microsoft\Internet Explorer\Quick Launch\VanzaKart Launcher.lnk"));
    }

    private static void TryDeleteEmptyDirectory(string path)
    {
        try
        {
            if (Directory.Exists(path) && !Directory.EnumerateFileSystemEntries(path).Any())
            {
                Directory.Delete(path, false);
            }
        }
        catch
        {
        }
    }

    private void RemoveRegistryEntries()
    {
        AppendLog("Removing Windows registry entries.");
        try
        {
            Registry.CurrentUser.DeleteSubKeyTree($@"{UninstallRoot}\{UninstallKeyName}", throwOnMissingSubKey: false);
        }
        catch (Exception ex)
        {
            AppendLog($"Unable to remove uninstall registry key: {ex.Message}");
        }

        try
        {
            var launcherExe = Directory.Exists(_installDir)
                ? Directory.EnumerateFiles(_installDir, "*.exe", SearchOption.TopDirectoryOnly)
                    .FirstOrDefault(path => Path.GetFileName(path).Contains("Launcher", StringComparison.OrdinalIgnoreCase))
                : null;

            if (!string.IsNullOrWhiteSpace(launcherExe))
            {
                Registry.CurrentUser.DeleteSubKeyTree($@"{AppPathsRoot}\{Path.GetFileName(launcherExe)}", throwOnMissingSubKey: false);
            }
            Registry.CurrentUser.DeleteSubKeyTree($@"{AppPathsRoot}\VanzaKart Launcher.exe", throwOnMissingSubKey: false);
        }
        catch (Exception ex)
        {
            AppendLog($"Unable to remove app path registry key: {ex.Message}");
        }
    }

    private void ScheduleDirectoryRemoval(string installDir)
    {
        var fullPath = Path.GetFullPath(installDir);
        if (fullPath.Length < 8 || string.Equals(Path.GetPathRoot(fullPath), fullPath, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Refusing to remove an unsafe installation path.");
        }

        var batchPath = Path.Combine(Path.GetTempPath(), $"vanzakart_uninstall_{Guid.NewGuid():N}.bat");
        var script = string.Join(Environment.NewLine, new[]
        {
            "@echo off",
            "timeout /t 2 /nobreak >nul",
            $"rd /s /q \"{fullPath}\"",
            "del \"%~f0\""
        });

        File.WriteAllText(batchPath, script, Encoding.ASCII);
        Process.Start(new ProcessStartInfo
        {
            FileName = batchPath,
            UseShellExecute = true,
            WindowStyle = ProcessWindowStyle.Hidden,
            CreateNoWindow = true
        });
    }

    private void CancelButton_Click(object sender, RoutedEventArgs e) => Close();

    private void MinimizeButton_Click(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void CloseButton_Click(object sender, RoutedEventArgs e) => Close();

    private void TitleBar_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ButtonState == MouseButtonState.Pressed)
        {
            DragMove();
        }
    }

    private void SetBusy(bool value)
    {
        _isBusy = value;
        UninstallButton.IsEnabled = !value;
        CancelButton.IsEnabled = !value;
        DeleteModCheckBox.IsEnabled = !value;
        KeepUserDataRadioButton.IsEnabled = !value;
        DeleteAllUserDataRadioButton.IsEnabled = !value;
    }

    private void AnimateEntrance()
    {
        RootShell.Opacity = 0;
        RootShell.RenderTransform = new TranslateTransform(0, 18);

        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        RootShell.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(240)) { EasingFunction = ease });
        ((TranslateTransform)RootShell.RenderTransform).BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(18, 0, TimeSpan.FromMilliseconds(260)) { EasingFunction = ease });
    }

    private void StartAmbientMotion()
    {
        AmbientOne.RenderTransform = new TranslateTransform();
        AmbientTwo.RenderTransform = new TranslateTransform();
        ((TranslateTransform)AmbientOne.RenderTransform).BeginAnimation(TranslateTransform.XProperty, FloatingAnimation(0, -18, 5.8));
        ((TranslateTransform)AmbientOne.RenderTransform).BeginAnimation(TranslateTransform.YProperty, FloatingAnimation(0, 14, 6.6));
        ((TranslateTransform)AmbientTwo.RenderTransform).BeginAnimation(TranslateTransform.XProperty, FloatingAnimation(0, 16, 7.2));
        ((TranslateTransform)AmbientTwo.RenderTransform).BeginAnimation(TranslateTransform.YProperty, FloatingAnimation(0, -14, 6.4));
    }

    private static DoubleAnimation FloatingAnimation(double from, double to, double seconds) => new(from, to, TimeSpan.FromSeconds(seconds))
    {
        AutoReverse = true,
        RepeatBehavior = RepeatBehavior.Forever,
        EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut }
    };

    private void AppendLog(string message)
    {
        Dispatcher.Invoke(() =>
        {
            _log.AppendLine($"[{DateTime.Now:HH:mm:ss}] {message}");
            LogTextBlock.Text = _log.ToString();
        });
    }

    private static long GetDirectorySize(string folder)
    {
        try
        {
            return Directory.EnumerateFiles(folder, "*", SearchOption.AllDirectories)
                .Sum(path =>
                {
                    try { return new FileInfo(path).Length; }
                    catch { return 0L; }
                });
        }
        catch
        {
            return 0;
        }
    }

    private static string FormatBytes(long bytes)
    {
        if (bytes <= 0)
        {
            return "0 B";
        }

        string[] units = ["B", "KB", "MB", "GB"];
        var size = (double)bytes;
        var unit = 0;
        while (size >= 1024 && unit < units.Length - 1)
        {
            size /= 1024;
            unit++;
        }

        return $"{size:0.#} {units[unit]}";
    }
}
