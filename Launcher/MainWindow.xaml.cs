using Microsoft.Win32;
using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Navigation;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher;

public partial class MainWindow : Window
{
    private readonly SettingsService _settingsService = new();
    private readonly NetworkService _networkService = new();
    private readonly ArchiveService _archiveService = new();

    private readonly string _tempZipPath = Path.Combine(AppContext.BaseDirectory, "mod_temp.zip");
    private readonly string _localModVersionFile = Path.Combine(AppContext.BaseDirectory, "mod_version.txt");

    private string _latestModVersion = string.Empty;
    private bool _isBusy;
    private bool _isModUpdateRequired;

    public MainWindow()
    {
        InitializeComponent();
        VersionBadgeText.Text = $"Launcher v{LauncherConfig.CurrentLauncherVersion}";
        LoadSettingsIntoUi();
        RefreshDerivedState();

        Loaded += async (_, _) =>
        {
            AnimateEntrance();
            await CheckForUpdatesAsync(showMessages: false);
        };
    }

    private LauncherSettings BuildSettingsFromUi() => new()
    {
        DolphinPath = DolphinPathTextBox.Text.Trim(),
        UserFolderPath = UserFolderTextBox.Text.Trim(),
        RomPath = RomPathTextBox.Text.Trim()
    };

    private void LoadSettingsIntoUi()
    {
        var settings = _settingsService.Load();
        DolphinPathTextBox.Text = settings.DolphinPath;
        UserFolderTextBox.Text = settings.UserFolderPath;
        RomPathTextBox.Text = settings.RomPath;
    }

    private void SaveSettingsFromUi()
    {
        _settingsService.Save(BuildSettingsFromUi());
        RefreshDerivedState();
    }

    private void RefreshDerivedState()
    {
        var settings = BuildSettingsFromUi();
        ModFolderTextBlock.Text = settings.GetModFolder();

        if (IsModInstalled(settings))
        {
            InstallStateBadgeText.Text = "Mod installed";
            SetStatus("Mod already installed and ready. Play now.", System.Windows.Media.Brushes.LimeGreen);
        }
        else
        {
            InstallStateBadgeText.Text = "Setup required";
            SetStatus("Select the paths and install the mod.", System.Windows.Media.Brushes.LightGray);
        }
    }

    private static bool IsModInstalled(LauncherSettings settings)
    {
        var xmlPath = Path.Combine(settings.GetModFolder(), "VanzaKart", "Riivolution", "VanzaKart.xml");
        return File.Exists(xmlPath);
    }

    private void SetBusy(bool value)
    {
        _isBusy = value;
        InstallButton.IsEnabled = !value;
        LaunchButton.IsEnabled = !value && !_isModUpdateRequired;
        CheckUpdatesButton.IsEnabled = !value;
        BrowseDolphinButton.IsEnabled = !value;
        BrowseUserFolderButton.IsEnabled = !value;
        BrowseRomButton.IsEnabled = !value;
    }

    private void ApplyModUpdateRequiredState()
    {
        LaunchButton.IsEnabled = !_isBusy && !_isModUpdateRequired;
    }

    private void SetStatus(string text, System.Windows.Media.Brush brush)
    {
        StatusTextBlock.Text = text;
        StatusTextBlock.Foreground = brush;
    }

    private async void InstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy) return;

        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            System.Windows.MessageBox.Show("Select the Dolphin User folder first.", "Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        SaveSettingsFromUi();

        if (IsModInstalled(settings))
        {
            var result = System.Windows.MessageBox.Show(
                "The mod appears to already be installed. Do you want to re-download it to update or repair it?",
                "Mod already installed",
                MessageBoxButton.YesNo,
                MessageBoxImage.Question);

            if (result != MessageBoxResult.Yes)
            {
                RefreshDerivedState();
                return;
            }
        }

        SetBusy(true);
        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = false;
        DownloadProgressBar.Value = 0;
        SetStatus("Connecting to the server...", System.Windows.Media.Brushes.LightSkyBlue);

        try
        {
            var progress = new Progress<(long current, long total)>(p =>
            {
                if (p.total <= 0) return;
                DownloadProgressBar.Value = (double)p.current / p.total * 100d;
                var currentGb = p.current / 1024d / 1024d / 1024d;
                var totalGb = p.total / 1024d / 1024d / 1024d;
                SetStatus($"Download: {currentGb:F2} GB of {totalGb:F2} GB ({DownloadProgressBar.Value:F0}%)", System.Windows.Media.Brushes.LightSkyBlue);
            });

            await _networkService.DownloadFileWithResumeAsync(LauncherConfig.ModUrl, _tempZipPath, progress);

            DownloadProgressBar.IsIndeterminate = true;
            SetStatus("Extracting files...", System.Windows.Media.Brushes.Orange);

            var modFolder = settings.GetModFolder();
            var specificModFolder = Path.Combine(modFolder, "VanzaKart");
            await _archiveService.ExtractZipAsync(_tempZipPath, modFolder, specificModFolder);

            if (File.Exists(_tempZipPath))
            {
                File.Delete(_tempZipPath);
            }

            if (!string.IsNullOrWhiteSpace(_latestModVersion))
            {
                File.WriteAllText(_localModVersionFile, _latestModVersion);
            }

            DownloadProgressBar.IsIndeterminate = false;
            DownloadProgressBar.Value = 100;
            _isModUpdateRequired = false;
            ApplyModUpdateRequiredState();
            SetStatus("Installation complete. You can now launch the game.", System.Windows.Media.Brushes.LimeGreen);
            RefreshDerivedState();
        }
        catch (Exception ex)
        {
            DownloadProgressBar.IsIndeterminate = false;
            DownloadProgressBar.Visibility = Visibility.Collapsed;
            SetStatus("Error during installation or update.", System.Windows.Media.Brushes.IndianRed);
            System.Windows.MessageBox.Show(ex.Message, "Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void LaunchButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy) return;

        if (_isModUpdateRequired)
        {
            System.Windows.MessageBox.Show(
                "A mandatory mod update is available. You must install the update before playing.",
                "Update required",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            return;
        }

        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.DolphinPath) || string.IsNullOrWhiteSpace(settings.RomPath) || string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            System.Windows.MessageBox.Show("Select Dolphin, the ROM, and the User folder before launching the game.", "Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        var rootDir = Path.Combine(settings.GetModFolder(), "VanzaKart");
        var xmlPath = Path.Combine(rootDir, "Riivolution", "VanzaKart.xml");
        if (!File.Exists(xmlPath))
        {
            System.Windows.MessageBox.Show("Mod XML file not found. Install the mod first.", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            return;
        }

        SaveSettingsFromUi();

        try
        {
            var jsonPath = Path.Combine(AppContext.BaseDirectory, "VanzaKart_launcher.json");
            var json = string.Join(Environment.NewLine, new[]
{
                "{",
                $"  \"base-file\": \"{EscapeJsonValue(settings.RomPath)}\",",
                "  \"display-name\": \"VanzaKart Modpack\",",
                "  \"riivolution\": {",
                "    \"patches\": [",
                "      {",
                "        \"options\": [",
                "          { \"choice\": 1, \"option-name\": \"Pack\", \"section-name\": \"VanzaKart\" },",
                "          { \"choice\": 2, \"option-name\": \"My Stuff\", \"section-name\": \"VanzaKart\" },",
                "          { \"choice\": 1, \"option-name\": \"Seperate Savegame\", \"section-name\": \"VanzaKart\" }",
                "        ],",
                $"        \"root\": \"{EscapeJsonValue(rootDir)}\",",
                $"        \"xml\": \"{EscapeJsonValue(xmlPath)}\"",
                "      }",
                "    ]",
                "  },",
                "  \"type\": \"dolphin-game-mod-descriptor\",",
                "  \"version\": 1",
                "}"
            });

            File.WriteAllText(jsonPath, json);

            Process.Start(new ProcessStartInfo
            {
                FileName = settings.DolphinPath,
                Arguments = $"\"{jsonPath}\"",
                UseShellExecute = true,
                WorkingDirectory = Path.GetDirectoryName(settings.DolphinPath)
            });

            SetStatus("Game launched.", System.Windows.Media.Brushes.LimeGreen);
        }
        catch (Exception ex)
        {
            System.Windows.MessageBox.Show(ex.Message, "Launch error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private async void CheckUpdatesButton_OnClick(object sender, RoutedEventArgs e)
    {
        await CheckForUpdatesAsync(showMessages: true);
    }

    private async Task CheckForUpdatesAsync(bool showMessages)
    {
        try
        {
            var noCacheUrl = $"{LauncherConfig.VersionJsonUrl}?t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
            var json = await _networkService.DownloadStringAsync(noCacheUrl);
            var info = JsonSerializer.Deserialize<VersionInfo>(json) ?? new VersionInfo();
            _latestModVersion = info.ModVersion;

            var notes = new List<string>();

            if (!string.IsNullOrWhiteSpace(info.LauncherVersion) && info.LauncherVersion != LauncherConfig.CurrentLauncherVersion)
            {
                notes.Add($"New launcher available: v{info.LauncherVersion}.");
                var answer = System.Windows.MessageBox.Show(
                    $"A launcher update is available (v{info.LauncherVersion}). Do you want to download it now?",
                    "Launcher update",
                    MessageBoxButton.YesNo,
                    MessageBoxImage.Information);

                if (answer == MessageBoxResult.Yes)
                {
                    await PerformLauncherUpdateAsync();
                    return;
                }
            }

            if (IsModInstalled(BuildSettingsFromUi()))
            {
                var localVersion = File.Exists(_localModVersionFile)
                    ? File.ReadAllText(_localModVersionFile).Trim()
                    : "0.0";

                if (!string.IsNullOrWhiteSpace(info.ModVersion) && info.ModVersion != localVersion)
                {
                    notes.Add($"New mod available: v{info.ModVersion}. Update required to play.");
                    _isModUpdateRequired = true;
                    ApplyModUpdateRequiredState();
                    SetStatus(
                        $"⚠ Mod update required (v{info.ModVersion}). Install the update to play.",
                        System.Windows.Media.Brushes.Orange);
                }
                else
                {
                    _isModUpdateRequired = false;
                    ApplyModUpdateRequiredState();
                }
            }

            if (notes.Count == 0)
            {
                if (showMessages)
                {
                    System.Windows.MessageBox.Show("No updates found.", "Updates", MessageBoxButton.OK, MessageBoxImage.Information);
                }
            }
            else if (showMessages)
            {
                System.Windows.MessageBox.Show(string.Join(Environment.NewLine, notes), "Updates", MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }
        catch
        {
            if (showMessages)
            {
                System.Windows.MessageBox.Show("Unable to check for updates now.", "Network", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
        }
    }

    private async Task PerformLauncherUpdateAsync()
    {
        SetBusy(true);
        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = true;
        SetStatus("Downloading the new launcher...", System.Windows.Media.Brushes.LightSkyBlue);

        var tempZip = Path.Combine(AppContext.BaseDirectory, "Launcher_Update.zip");
        var batchPath = Path.Combine(AppContext.BaseDirectory, "update.bat");

        try
        {
            await _networkService.DownloadFileWithResumeAsync(LauncherConfig.LauncherZipUrl, tempZip);

            var exeName = Path.GetFileName(Environment.ProcessPath) ?? "VanzaKart Launcher.exe";
            var script = string.Join(Environment.NewLine, new[]
            {
                "@echo off",
                $"cd /d \"{AppContext.BaseDirectory}\"",
                "timeout /t 2 /nobreak >nul",
                $"powershell -NoProfile -ExecutionPolicy Bypass -Command \"Expand-Archive -Path '{tempZip}' -DestinationPath '{AppContext.BaseDirectory}' -Force\"",
                $"del \"{tempZip}\"",
                $"start \"\" \"{Path.Combine(AppContext.BaseDirectory, exeName)}\"",
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

            System.Windows.Application.Current.Shutdown();
        }
        catch (Exception ex)
        {
            SetStatus("Error during launcher update.", System.Windows.Media.Brushes.IndianRed);
            System.Windows.MessageBox.Show(ex.Message, "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            SetBusy(false);
        }
    }

    private void BrowseDolphinButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new Microsoft.Win32.OpenFileDialog { Filter = "Executable (*.exe)|*.exe" };
        if (dialog.ShowDialog() != true) return;

        DolphinPathTextBox.Text = dialog.FileName;
        var dolphinDir = Path.GetDirectoryName(dialog.FileName) ?? string.Empty;
        var possibleUser = Path.Combine(dolphinDir, "User");

        if (Directory.Exists(possibleUser))
        {
            UserFolderTextBox.Text = possibleUser;
            SetStatus("Dolphin and the User folder detected automatically.", System.Windows.Media.Brushes.LimeGreen);
        }
        else
        {
            SetStatus("Dolphin found. The User folder must be set manually.", System.Windows.Media.Brushes.Orange);
        }

        SaveSettingsFromUi();
    }

    private void BrowseUserFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        using var dialog = new System.Windows.Forms.FolderBrowserDialog
        {
            Description = "Select the Dolphin User folder"
        };

        if (dialog.ShowDialog() == System.Windows.Forms.DialogResult.OK)
        {
            UserFolderTextBox.Text = dialog.SelectedPath;
            SaveSettingsFromUi();
        }
    }

    private void BrowseRomButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new Microsoft.Win32.OpenFileDialog { Filter = "Wii ROM (*.wbfs;*.iso)|*.wbfs;*.iso" };
        if (dialog.ShowDialog() != true) return;

        RomPathTextBox.Text = dialog.FileName;
        SaveSettingsFromUi();
    }

    private static string EscapeJsonValue(string value)
    {
        return value.Replace("\\", "\\\\").Replace("\"", "\\\"");
    }

    private void AnimateEntrance()
    {
        Opacity = 0;
        var animation = new System.Windows.Media.Animation.DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(380));
        BeginAnimation(OpacityProperty, animation);
    }
    private void Link_RequestNavigate(object sender, RequestNavigateEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = e.Uri.AbsoluteUri,
                UseShellExecute = true
            });
        }
        catch
        {
            System.Windows.MessageBox.Show("Impossibile aprire il link.");
        }
    }
    private void OpenDiscordBorder_OnClick(object sender, MouseButtonEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "https://discord.gg/4qPAQjt27j",
                UseShellExecute = true
            });
        }
        catch
        {
            System.Windows.MessageBox.Show("Impossibile aprire Discord.");
        }
    }

    private void OpenWebsiteBorder_OnClick(object sender, MouseButtonEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "https://web.sitodaking.it/",
                UseShellExecute = true
            });
        }
        catch
        {
            System.Windows.MessageBox.Show("Impossibile aprire il sito.");
        }
    }
}