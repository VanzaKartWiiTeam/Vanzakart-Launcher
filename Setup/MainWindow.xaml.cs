using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Windows;
using VanzaKartSetup.Services;

namespace VanzaKartSetup;

public partial class MainWindow : Window
{
    private const string LauncherZipUrl = "https://sitodaking.it/Launcher/vanzakart_launcher.zip";

    private readonly NetworkService _networkService = new();
    private readonly ShortcutService _shortcutService = new();
    private readonly string _tempZipPath = Path.Combine(Path.GetTempPath(), "VanzaKart_SetupTemp.zip");
    private bool _isBusy;

    public MainWindow()
    {
        InitializeComponent();
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        InstallPathTextBox.Text = Path.Combine(localAppData, "VanzaKartLauncher");
    }

    private void SetBusy(bool value)
    {
        _isBusy = value;
        InstallButton.IsEnabled = !value;
        BrowseButton.IsEnabled = !value;
        InstallPathTextBox.IsEnabled = !value;
    }

    private void BrowseButton_OnClick(object sender, RoutedEventArgs e)
    {
        using var dialog = new System.Windows.Forms.FolderBrowserDialog
        {
            Description = "Select the installation folder"
        };

        if (dialog.ShowDialog() == System.Windows.Forms.DialogResult.OK)
        {
            InstallPathTextBox.Text = dialog.SelectedPath;
        }
    }

    private async void InstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy) return;

        var targetDir = InstallPathTextBox.Text.Trim();
        if (string.IsNullOrWhiteSpace(targetDir))
        {
            System.Windows.MessageBox.Show(
                "Select a valid folder.",
                "Warning",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            return;
        }

        SetBusy(true);
        ProgressBar.Visibility = Visibility.Visible;
        ProgressBar.IsIndeterminate = false;
        ProgressBar.Value = 0;
        StatusTextBlock.Text = "Downloading the launcher...";

        try
        {
            var progress = new Progress<(long current, long total)>(p =>
            {
                double percent = p.total > 0 ? (double)p.current / p.total * 100.0 : 0.0;
                ProgressBar.Value = percent;
                StatusTextBlock.Text = $"Downloading the launcher... {percent:0}%";
            });

            await _networkService.DownloadFileAsync(LauncherZipUrl, _tempZipPath, progress);

            StatusTextBlock.Text = "Extracting the file...";
            ProgressBar.IsIndeterminate = true;

            await Task.Run(() =>
            {
                Directory.CreateDirectory(targetDir);
                ZipFile.ExtractToDirectory(_tempZipPath, targetDir, overwriteFiles: true);
            });

            if (File.Exists(_tempZipPath))
            {
                File.Delete(_tempZipPath);
            }

            var exePath = Directory
                .GetFiles(targetDir, "*.exe", SearchOption.TopDirectoryOnly)
                .FirstOrDefault(path => Path.GetFileName(path)
                .Contains("Launcher", StringComparison.OrdinalIgnoreCase));

            if (!string.IsNullOrWhiteSpace(exePath))
            {
                _shortcutService.CreateDesktopShortcut(exePath, targetDir);
            }

            ProgressBar.IsIndeterminate = false;
            ProgressBar.Value = 100;
            StatusTextBlock.Text = "Installation complete.";
            SetBusy(false);
            InstallButton.Content = "Close";
            InstallButton.Click -= InstallButton_OnClick;
            InstallButton.Click += (_, _) => Close();
        }
        catch (Exception ex)
        {
            ProgressBar.Visibility = Visibility.Collapsed;
            StatusTextBlock.Text = "Installation failed.";
            System.Windows.MessageBox.Show(
                ex.Message,
                "Error",
                MessageBoxButton.OK,
                MessageBoxImage.Error);
            SetBusy(false);
        }
    }
}