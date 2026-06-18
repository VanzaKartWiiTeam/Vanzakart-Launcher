using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using VanzaKartSetup.Services;
using WpfBrush = System.Windows.Media.Brush;
using WpfColor = System.Windows.Media.Color;

namespace VanzaKartSetup;

public partial class MainWindow : Window
{
    private const string LauncherZipUrl = "https://sitodaking.it/Launcher/vanzakart_launcher.zip";
    private const long FallbackDownloadSizeBytes = 350L * 1024L * 1024L;
    private const long MinimumRequiredBytes = 900L * 1024L * 1024L;

    private readonly NetworkService _networkService = new();
    private readonly ShortcutService _shortcutService = new();
    private readonly WindowsInstallRegistryService _registryService = new();
    private readonly string _tempZipPath = Path.Combine(Path.GetTempPath(), "VanzaKart_SetupTemp.zip");
    private readonly Stopwatch _downloadStopwatch = new();
    private readonly StringBuilder _installLog = new();
    private readonly List<Border> _stepCards;

    private SetupStep _currentStep = SetupStep.Welcome;
    private InstalledApplicationInfo? _existingInstall;
    private long _downloadSizeBytes;
    private long _requiredSpaceBytes = MinimumRequiredBytes;
    private bool _isBusy;
    private string? _launcherExePath;

    public MainWindow()
    {
        InitializeComponent();
        _stepCards =
        [
            StepWelcome,
            StepFolder,
            StepSpace,
            StepDownload,
            StepInstall,
            StepComplete
        ];

        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        InstallPathTextBox.Text = Path.Combine(localAppData, "VanzaKartLauncher");
        BackupPathTextBox.Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments), "VanzaKart_Backups");

        Loaded += async (_, _) =>
        {
            AnimateEntrance();
            StartAmbientMotion();
            DetectExistingInstallation();
            SetStep(SetupStep.Welcome, animate: false);
            await RefreshNetworkPreviewAsync();
        };
    }

    private void DetectExistingInstallation()
    {
        _existingInstall = _registryService.TryReadExistingInstall();
        if (_existingInstall == null)
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            var defaultDir = Path.Combine(localAppData, "VanzaKartLauncher");
            if (Directory.Exists(defaultDir))
            {
                _existingInstall = new InstalledApplicationInfo(defaultDir, WindowsInstallRegistryService.ProductVersion, WindowsInstallRegistryService.ProductName);
            }
        }

        if (_existingInstall == null)
        {
            ExistingInstallCard.Visibility = Visibility.Collapsed;
            FooterStatusTextBlock.Text = "No existing installation detected.";
            return;
        }

        ExistingInstallCard.Visibility = Visibility.Visible;
        ExistingInstallTextBlock.Text =
            $"An existing launcher directory was found at:\n{_existingInstall.InstallLocation}\nChoose whether you want to update it or perform a clean reinstall.";
        InstallPathTextBox.Text = _existingInstall.InstallLocation;
        FooterStatusTextBlock.Text = $"Existing installation detected: v{_existingInstall.DisplayVersion}.";
    }

    private async Task RefreshNetworkPreviewAsync()
    {
        InternetStatusTextBlock.Text = "Checking...";
        EstimatedSizeTextBlock.Text = "Calculating...";

        var online = await _networkService.CheckInternetAsync();
        _downloadSizeBytes = await _networkService.GetContentLengthAsync(LauncherZipUrl);

        InternetStatusTextBlock.Text = online ? "Online" : "Not verified";
        InternetStatusTextBlock.Foreground = (WpfBrush)FindResource(online ? "SuccessBrush" : "WarningBrush");
        EstimatedSizeTextBlock.Text = _downloadSizeBytes > 0 ? FormatBytes(_downloadSizeBytes) : "Variable";
    }

    private async void NextButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        switch (_currentStep)
        {
            case SetupStep.Welcome:
                SetStep(SetupStep.Folder);
                break;
            case SetupStep.Folder:
                if (ValidateFolderStep())
                {
                    await RefreshPreflightAsync();
                    SetStep(SetupStep.Space);
                }
                break;
            case SetupStep.Space:
                await RunInstallAsync();
                break;
            case SetupStep.Complete:
                LaunchAndClose();
                break;
        }
    }

    private void BackButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        if (_currentStep == SetupStep.Folder)
        {
            SetStep(SetupStep.Welcome);
        }
        else if (_currentStep == SetupStep.Space)
        {
            SetStep(SetupStep.Folder);
        }
    }

    private bool ValidateFolderStep()
    {
        var targetDir = InstallPathTextBox.Text.Trim();
        if (string.IsNullOrWhiteSpace(targetDir))
        {
            ShowInlineError("Select a valid installation folder.");
            return false;
        }

        try
        {
            _ = Path.GetFullPath(targetDir);
            return true;
        }
        catch
        {
            ShowInlineError("The installation path is not valid.");
            return false;
        }
    }

    private async Task RefreshPreflightAsync()
    {
        FooterStatusTextBlock.Text = "Running checks...";
        NextButton.IsEnabled = false;

        try
        {
            var targetDir = Path.GetFullPath(InstallPathTextBox.Text.Trim());
            var root = Path.GetPathRoot(targetDir);
            var drive = DriveInfo.GetDrives()
                .FirstOrDefault(d => string.Equals(d.Name, root, StringComparison.OrdinalIgnoreCase));

            var online = await _networkService.CheckInternetAsync();
            if (_downloadSizeBytes <= 0)
            {
                _downloadSizeBytes = await _networkService.GetContentLengthAsync(LauncherZipUrl);
            }

            _requiredSpaceBytes = Math.Max(MinimumRequiredBytes, (_downloadSizeBytes > 0 ? _downloadSizeBytes : FallbackDownloadSizeBytes) * 3);
            var available = drive?.AvailableFreeSpace ?? 0;
            var hasSpace = available > _requiredSpaceBytes;

            RequiredSpaceTextBlock.Text = FormatBytes(_requiredSpaceBytes);
            AvailableSpaceTextBlock.Text = available > 0 ? FormatBytes(available) : "Not detected";
            DownloadSizeTextBlock.Text = _downloadSizeBytes > 0 ? FormatBytes(_downloadSizeBytes) : "Not declared";
            EstimatedTimeTextBlock.Text = _downloadSizeBytes > 0 ? EstimateDownloadTime(_downloadSizeBytes) : "During download";
            InternetCheckTextBlock.Text = online ? "Connection: online" : "Connection: not verified, download may fail";
            InternetCheckTextBlock.Foreground = (WpfBrush)FindResource(online ? "SuccessBrush" : "WarningBrush");
            SpaceStatusTextBlock.Text = hasSpace
                ? "Ready. Press Install to download and register VanzaKart in Windows."
                : "Not enough free space on the selected drive. Free up space or choose another folder.";
            SpaceStatusTextBlock.Foreground = (WpfBrush)FindResource(hasSpace ? "TextSecondary" : "DangerBrush");
            NextButton.IsEnabled = hasSpace;
        }
        finally
        {
            FooterStatusTextBlock.Text = "Checks complete.";
            if (_currentStep != SetupStep.Space)
            {
                NextButton.IsEnabled = true;
            }
        }
    }

    private async Task RunInstallAsync()
    {
        SetBusy(true);
        SetStep(SetupStep.Download);

        try
        {
            var targetDir = Path.GetFullPath(InstallPathTextBox.Text.Trim());
            var backupDir = Path.GetFullPath(BackupPathTextBox.Text.Trim());
            var cleanReinstall = CleanReinstallRadioButton.IsChecked == true && Directory.Exists(targetDir);
            var createBackup = BackupCheckBox.IsChecked == true;

            AppendLog("Installation started.");
            AppendLog($"Target: {targetDir}");
            AppendLog(cleanReinstall ? "Mode: clean reinstall." : "Mode: update/install.");

            await DownloadLauncherAsync();

            SetStep(SetupStep.Install);
            InstallProgressBar.Value = 8;
            InstallStatusTextBlock.Text = "Preparing folder...";

            await Task.Run(() =>
            {
                Directory.CreateDirectory(targetDir);

                if (createBackup && Directory.Exists(targetDir))
                {
                    BackupUserData(targetDir, backupDir);
                }

                if (cleanReinstall)
                {
                    CleanInstallDirectory(targetDir);
                }
            });

            InstallProgressBar.Value = 28;
            InstallStatusTextBlock.Text = "Extracting launcher...";
            AppendLog("Extracting launcher package.");

            await Task.Run(() =>
            {
                ZipFile.ExtractToDirectory(_tempZipPath, targetDir, overwriteFiles: true);
            });

            InstallProgressBar.Value = 58;
            _launcherExePath = FindLauncherExecutable(targetDir);
            if (string.IsNullOrWhiteSpace(_launcherExePath))
            {
                throw new FileNotFoundException("Launcher executable not found after extraction.");
            }

            AppendLog($"Launcher executable: {_launcherExePath}");
            var uninstallerPath = EnsureUninstallerAvailable(targetDir);

            InstallProgressBar.Value = 72;
            InstallStatusTextBlock.Text = "Creating shortcuts...";
            CreateSelectedShortcuts(_launcherExePath, targetDir);

            InstallProgressBar.Value = 84;
            InstallStatusTextBlock.Text = "Registering in Windows...";
            var estimatedSize = GetDirectorySize(targetDir);
            _registryService.Register(targetDir, _launcherExePath, uninstallerPath, estimatedSize);
            AppendLog("Windows uninstall registry entry created.");

            if (File.Exists(_tempZipPath))
            {
                File.Delete(_tempZipPath);
            }

            InstallProgressBar.Value = 100;
            InstallStatusTextBlock.Text = "Installation complete.";
            CompleteSummaryTextBlock.Text =
                $"Installed in {targetDir}\nRegistered as {WindowsInstallRegistryService.ProductName} v{WindowsInstallRegistryService.ProductVersion}.";
            AppendLog("Installation completed successfully.");
            SetStep(SetupStep.Complete);
        }
        catch (Exception ex)
        {
            AppendLog($"ERROR: {ex.Message}");
            InstallStatusTextBlock.Text = "Installation failed.";
            FooterStatusTextBlock.Text = ex.Message;
            ShowInlineError(ex.Message);
            SetStep(SetupStep.Install);
        }
        finally
        {
            SetBusy(false);
            if (_currentStep == SetupStep.Complete)
            {
                BackButton.Visibility = Visibility.Collapsed;
                NextButton.Content = "Finish";
            }
        }
    }

    private async Task DownloadLauncherAsync()
    {
        if (File.Exists(_tempZipPath))
        {
            File.Delete(_tempZipPath);
        }

        DownloadProgressBar.Value = 0;
        _downloadStopwatch.Restart();
        var speedSamples = new Queue<(double seconds, long bytes)>();
        double displayedSpeed = 0;

        var progress = new Progress<(long current, long total)>(p =>
        {
            var total = p.total > 0 ? p.total : _downloadSizeBytes;
            var percent = total > 0 ? (double)p.current / total * 100.0 : 0.0;
            DownloadProgressBar.Value = Math.Min(100, percent);

            var elapsedSeconds = _downloadStopwatch.Elapsed.TotalSeconds;
            if (speedSamples.Count == 0 || p.current > speedSamples.Last().bytes)
            {
                speedSamples.Enqueue((elapsedSeconds, p.current));
            }

            while (speedSamples.Count > 2 && elapsedSeconds - speedSamples.Peek().seconds > 3)
            {
                speedSamples.Dequeue();
            }

            double measuredSpeed = 0;
            if (speedSamples.Count >= 2)
            {
                var oldest = speedSamples.Peek();
                var newest = speedSamples.Last();
                var sampleDuration = newest.seconds - oldest.seconds;
                if (sampleDuration > 0)
                {
                    measuredSpeed = (newest.bytes - oldest.bytes) / sampleDuration;
                }
            }
            else if (elapsedSeconds > 0.1)
            {
                measuredSpeed = p.current / elapsedSeconds;
            }

            if (measuredSpeed > 0)
            {
                displayedSpeed = displayedSpeed <= 0
                    ? measuredSpeed
                    : (displayedSpeed * 0.65) + (measuredSpeed * 0.35);
            }

            var remaining = displayedSpeed > 0 && total > p.current
                ? TimeSpan.FromSeconds((total - p.current) / displayedSpeed)
                : TimeSpan.Zero;

            DownloadStatusTextBlock.Text = total > 0
                ? $"Downloading launcher package... {percent:0}%"
                : "Downloading launcher package...";
            DownloadSpeedTextBlock.Text = displayedSpeed > 0
                ? $"{FormatBytes((long)displayedSpeed)}/s"
                : "Measuring...";
            DownloadedTextBlock.Text = total > 0
                ? $"{FormatBytes(p.current)} / {FormatBytes(total)}"
                : FormatBytes(p.current);
            DownloadEtaTextBlock.Text = remaining > TimeSpan.Zero ? FormatDuration(remaining) : "-";
        });

        AppendLog("Downloading launcher package.");
        await _networkService.DownloadFileAsync(LauncherZipUrl, _tempZipPath, progress);
        _downloadStopwatch.Stop();
        DownloadProgressBar.Value = 100;
        DownloadStatusTextBlock.Text = $"Download complete in {FormatDuration(_downloadStopwatch.Elapsed)}.";
        AppendLog($"Download complete: {FormatBytes(new FileInfo(_tempZipPath).Length)}.");
    }

    private void BackupUserData(string targetDir, string backupRoot)
    {
        var backupDir = Path.Combine(backupRoot, $"VanzaKart_Backup_{DateTime.Now:yyyyMMdd_HHmmss}");
        Directory.CreateDirectory(backupDir);
        AppendLog($"Creating safety backup: {backupDir}");

        var filesToBackup = new[]
        {
            "launcher_settings.json",
            "user_preferences.json",
            "mod_version.txt",
            "VanzaKart_launcher.json"
        };

        foreach (var file in filesToBackup)
        {
            var source = Path.Combine(targetDir, file);
            if (File.Exists(source))
            {
                File.Copy(source, Path.Combine(backupDir, file), overwrite: true);
            }
        }

        var userDataDir = Path.Combine(targetDir, "VanzaKart_UserData");
        if (Directory.Exists(userDataDir))
        {
            CopyDirectory(userDataDir, Path.Combine(backupDir, "VanzaKart_UserData"));
        }
    }

    private void CleanInstallDirectory(string targetDir)
    {
        var fullPath = Path.GetFullPath(targetDir);
        if (fullPath.Length < 8 || string.Equals(Path.GetPathRoot(fullPath), fullPath, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Refusing to clean an unsafe installation path.");
        }

        AppendLog("Cleaning previous program files.");
        foreach (var entry in Directory.EnumerateFileSystemEntries(fullPath))
        {
            if (Directory.Exists(entry))
            {
                Directory.Delete(entry, recursive: true);
            }
            else
            {
                File.Delete(entry);
            }
        }
    }

    private string EnsureUninstallerAvailable(string targetDir)
    {
        var destination = Path.Combine(targetDir, "VanzaKart Uninstaller.exe");
        if (File.Exists(destination))
        {
            AppendLog("Uninstaller found in installed package.");
            return destination;
        }

        var source = Directory.EnumerateFiles(AppContext.BaseDirectory, "*Uninstaller*.exe", SearchOption.TopDirectoryOnly)
            .FirstOrDefault(path => !string.Equals(path, destination, StringComparison.OrdinalIgnoreCase));

        if (!string.IsNullOrWhiteSpace(source) && File.Exists(source))
        {
            File.Copy(source, destination, overwrite: true);
            AppendLog("Bundled uninstaller copied to installation folder.");
            return destination;
        }

        AppendLog("Warning: uninstaller executable was not found next to setup. Registry entry points to the expected installed uninstaller path.");
        return destination;
    }

    private void CreateSelectedShortcuts(string launcherExePath, string targetDir)
    {
        _shortcutService.RemoveAllShortcuts();

        if (DesktopShortcutCheckBox.IsChecked == true)
        {
            _shortcutService.CreateDesktopShortcut(launcherExePath, targetDir);
            AppendLog("Desktop shortcut created.");
        }

        if (StartMenuShortcutCheckBox.IsChecked == true)
        {
            _shortcutService.CreateStartMenuShortcut(launcherExePath, targetDir);
            AppendLog("Start Menu shortcut created.");
        }

        if (QuickLaunchShortcutCheckBox.IsChecked == true)
        {
            _shortcutService.CreateQuickLaunchShortcut(launcherExePath, targetDir);
            AppendLog("Quick Launch shortcut created.");
        }
    }

    private void SetStep(SetupStep step, bool animate = true)
    {
        _currentStep = step;

        WelcomeView.Visibility = step == SetupStep.Welcome ? Visibility.Visible : Visibility.Collapsed;
        FolderView.Visibility = step == SetupStep.Folder ? Visibility.Visible : Visibility.Collapsed;
        SpaceView.Visibility = step == SetupStep.Space ? Visibility.Visible : Visibility.Collapsed;
        DownloadView.Visibility = step == SetupStep.Download ? Visibility.Visible : Visibility.Collapsed;
        InstallView.Visibility = step == SetupStep.Install ? Visibility.Visible : Visibility.Collapsed;
        CompleteView.Visibility = step == SetupStep.Complete ? Visibility.Visible : Visibility.Collapsed;

        BackButton.Visibility = step is SetupStep.Welcome or SetupStep.Download or SetupStep.Install or SetupStep.Complete
            ? Visibility.Collapsed
            : Visibility.Visible;
        NextButton.Visibility = step is SetupStep.Download or SetupStep.Install ? Visibility.Collapsed : Visibility.Visible;
        NextButton.Content = step switch
        {
            SetupStep.Space => "Install",
            SetupStep.Complete => "Finish",
            _ => "Next"
        };

        foreach (var card in _stepCards)
        {
            var index = int.Parse(card.Tag?.ToString() ?? "0");
            if (index == (int)step)
            {
                card.BorderBrush = (WpfBrush)FindResource("RainbowGradient");
                card.BorderThickness = new Thickness(1.6);
                card.Background = new SolidColorBrush(WpfColor.FromArgb(235, 28, 39, 64));
                card.Effect = (System.Windows.Media.Effects.Effect)FindResource("NeonGlow");
            }
            else if (index < (int)step)
            {
                card.BorderBrush = new SolidColorBrush(WpfColor.FromRgb(58, 86, 84));
                card.BorderThickness = new Thickness(1);
                card.Background = new SolidColorBrush(WpfColor.FromArgb(180, 17, 32, 39));
                card.Effect = null;
            }
            else
            {
                card.BorderBrush = (WpfBrush)FindResource("StrokeBrush");
                card.BorderThickness = new Thickness(1);
                card.Background = (WpfBrush)FindResource("PanelBrush");
                card.Effect = null;
            }
        }

        var activeView = step switch
        {
            SetupStep.Welcome => WelcomeView,
            SetupStep.Folder => FolderView,
            SetupStep.Space => SpaceView,
            SetupStep.Download => DownloadView,
            SetupStep.Install => InstallView,
            _ => CompleteView
        };

        if (animate)
        {
            AnimateViewTransition(activeView);
        }
    }

    private void SetBusy(bool value)
    {
        _isBusy = value;
        BrowseInstallPathButton.IsEnabled = !value;
        BrowseBackupPathButton.IsEnabled = !value;
        InstallPathTextBox.IsEnabled = !value;
        BackupPathTextBox.IsEnabled = !value;
        BackButton.IsEnabled = !value;
        NextButton.IsEnabled = !value || _currentStep == SetupStep.Complete;
    }

    private void BrowseInstallPathButton_OnClick(object sender, RoutedEventArgs e)
    {
        BrowseFolder("Select the installation folder", InstallPathTextBox);
    }

    private void BrowseBackupPathButton_OnClick(object sender, RoutedEventArgs e)
    {
        BrowseFolder("Select the backup folder", BackupPathTextBox);
    }

    private static void BrowseFolder(string description, System.Windows.Controls.TextBox target)
    {
        using var dialog = new System.Windows.Forms.FolderBrowserDialog
        {
            Description = description,
            SelectedPath = Directory.Exists(target.Text) ? target.Text : Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            UseDescriptionForTitle = true
        };

        if (dialog.ShowDialog() == System.Windows.Forms.DialogResult.OK)
        {
            target.Text = dialog.SelectedPath;
        }
    }

    private void LaunchAndClose()
    {
        if (LaunchAfterInstallCheckBox.IsChecked == true && !string.IsNullOrWhiteSpace(_launcherExePath) && File.Exists(_launcherExePath))
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = _launcherExePath,
                WorkingDirectory = Path.GetDirectoryName(_launcherExePath)!,
                UseShellExecute = true
            });
        }

        Close();
    }

    private void MinimizeButton_Click(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void CloseButton_Click(object sender, RoutedEventArgs e) => Close();

    private void TitleBar_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ButtonState == MouseButtonState.Pressed)
        {
            DragMove();
        }
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

    private static void AnimateViewTransition(FrameworkElement view)
    {
        view.Opacity = 0;
        view.RenderTransform = new TranslateTransform(24, 0);
        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        view.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(220)) { EasingFunction = ease });
        ((TranslateTransform)view.RenderTransform).BeginAnimation(TranslateTransform.XProperty, new DoubleAnimation(24, 0, TimeSpan.FromMilliseconds(250)) { EasingFunction = ease });
    }

    private void AppendLog(string message)
    {
        Dispatcher.Invoke(() =>
        {
            _installLog.AppendLine($"[{DateTime.Now:HH:mm:ss}] {message}");
            InstallLogTextBlock.Text = _installLog.ToString();
        });
    }

    private void ShowInlineError(string message)
    {
        FooterStatusTextBlock.Text = message;
        FooterStatusTextBlock.Foreground = (WpfBrush)FindResource("DangerBrush");
    }

    private static string? FindLauncherExecutable(string targetDir)
        => Directory.GetFiles(targetDir, "*.exe", SearchOption.TopDirectoryOnly)
            .FirstOrDefault(path =>
                Path.GetFileName(path).Contains("Launcher", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(path).Contains("Setup", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(path).Contains("Uninstaller", StringComparison.OrdinalIgnoreCase));

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

    private static void CopyDirectory(string sourceDir, string destinationDir)
    {
        Directory.CreateDirectory(destinationDir);
        foreach (var file in Directory.EnumerateFiles(sourceDir))
        {
            File.Copy(file, Path.Combine(destinationDir, Path.GetFileName(file)), overwrite: true);
        }

        foreach (var directory in Directory.EnumerateDirectories(sourceDir))
        {
            CopyDirectory(directory, Path.Combine(destinationDir, Path.GetFileName(directory)));
        }
    }

    private static string EstimateDownloadTime(long bytes)
    {
        const double assumedBytesPerSecond = 8 * 1024 * 1024;
        return FormatDuration(TimeSpan.FromSeconds(bytes / assumedBytesPerSecond));
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

    private static string FormatDuration(TimeSpan duration)
    {
        if (duration.TotalSeconds < 1)
        {
            return "-";
        }

        if (duration.TotalMinutes < 1)
        {
            return $"{duration.TotalSeconds:0}s";
        }

        if (duration.TotalHours < 1)
        {
            return $"{duration.TotalMinutes:0} min";
        }

        return $"{duration.TotalHours:0.#} h";
    }
}

public enum SetupStep
{
    Welcome = 0,
    Folder = 1,
    Space = 2,
    Download = 3,
    Install = 4,
    Complete = 5
}
