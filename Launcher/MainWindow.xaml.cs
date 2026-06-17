using Microsoft.Win32;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;
using VanzaKartLauncher.ViewModels;
using WpfBrush = System.Windows.Media.Brush;
using WpfBrushes = System.Windows.Media.Brushes;
using WpfButton = System.Windows.Controls.Button;
using WpfColor = System.Windows.Media.Color;
using WpfDragEventArgs = System.Windows.DragEventArgs;
using WpfKeyEventArgs = System.Windows.Input.KeyEventArgs;
using WpfOpenFolderDialog = Microsoft.Win32.OpenFolderDialog;
using WpfOpenFileDialog = Microsoft.Win32.OpenFileDialog;
using WpfSaveFileDialog = Microsoft.Win32.SaveFileDialog;

namespace VanzaKartLauncher;

public partial class MainWindow : Window
{
    private readonly SettingsService _settingsService = new();
    private readonly PreferencesService _preferencesService = new();
    private readonly NetworkService _networkService = new();
    private readonly ArchiveService _archiveService = new();
    private readonly SaveManagerService _saveManagerService = new();
    private readonly ModConflictService _modConflictService = new();
    private readonly LauncherNavigationService _navigationService = new();
    private readonly MiiRuntimeSetupService _miiRuntimeSetupService = new();
    private readonly ShellViewModel _shellViewModel = new();
    private readonly RoomsViewModel _roomsViewModel;
    private readonly LeaderboardViewModel _leaderboardViewModel;
    private readonly FriendsViewModel _friendsViewModel;
    private readonly ObservableCollection<NewsItem> _visibleNews = new();
    private readonly List<NewsItem> _allNews = new();
    private readonly ObservableCollection<SaveProfileInfo> _licenseCards = new();
    private readonly List<SaveProfileInfo> _allLicenseCards = new();
    private readonly ObservableCollection<LauncherMiiProfile> _miiProfiles = new();
    private readonly ObservableCollection<SaveBackupInfo> _backupRestoreItems = new();
    private readonly Stopwatch _downloadStopwatch = new();
    private readonly ModUpdateSafetyService _modUpdateSafetyService = new();
    private bool _isRefreshingMiis;
    private bool _isRenderingLicenseAvatars;
    private bool _isRenderingLauncherMiiAvatars;
    private bool _isInstallingMiiRuntime;
    private FileSystemWatcher? _dolphinFileWatcher;
    private FileSystemWatcher? _profileFileWatcher;
    private CancellationTokenSource? _filesystemRefreshCts;

    private UserPreferences _userPreferences;

    private readonly string _tempZipPath = Path.Combine(AppContext.BaseDirectory, "mod_temp.zip");
    private readonly string _localModVersionFile = Path.Combine(AppContext.BaseDirectory, "mod_version.txt");

    private string _latestModVersion = string.Empty;
    private string _latestModUrl = LauncherConfig.ModUrl;
    private string[] _latestModMirrors = Array.Empty<string>();
    private string _latestModSha256 = string.Empty;
    private string _latestModManifestUrl = LauncherConfig.ModManifestUrl;
    private string _latestModFilesUrl = LauncherConfig.ModFilesUrl;
    private string[] _latestModFilesMirrors = Array.Empty<string>();
    private string _latestLauncherUrl = LauncherConfig.LauncherZipUrl;
    private string[] _latestLauncherMirrors = Array.Empty<string>();
    private string[] _latestChangelog = Array.Empty<string>();
    private DateTime? _lastUpdateCheckUtc;
    private string _lastUpdateError = string.Empty;
    private string _newsFilter = "All";
    private string _currentTab = "Home";
    private bool _isBusy;
    private bool _isModUpdateRequired;
    private bool _isGameRunning;
    private long _downloadBaselineBytes = -1;

    public MainWindow()
    {
        _userPreferences = _preferencesService.Load();

        _roomsViewModel = new RoomsViewModel(_networkService);
        _leaderboardViewModel = new LeaderboardViewModel(_networkService);
        _friendsViewModel = new FriendsViewModel(_networkService);

        // Verify if a mod update is required based on last known version from check
        var localVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : "0.0";
        if (!string.IsNullOrWhiteSpace(_userPreferences.LastKnownLatestModVersion) && _userPreferences.LastKnownLatestModVersion != localVersion)
        {
            _isModUpdateRequired = true;
            _latestModVersion = _userPreferences.LastKnownLatestModVersion;
        }

        InitializeComponent();

        RoomsView.DataContext = _roomsViewModel;
        LeaderboardView.DataContext = _leaderboardViewModel;
        FriendsView.DataContext = _friendsViewModel;

        VersionBadgeTextBlock.Text = $"Launcher v{LauncherConfig.CurrentLauncherVersion}";
        DebugNavButton.Visibility = Debugger.IsAttached ? Visibility.Visible : Visibility.Collapsed;

        SeedNews();
        NewsItemsControl.ItemsSource = _visibleNews;
        LicenseCardsItemsControl.ItemsSource = _licenseCards;
        MiiCardsListBox.ItemsSource = _miiProfiles;
        BackupRestoreComboBox.ItemsSource = _backupRestoreItems;
        BackupRestoreComboBox.DisplayMemberPath = nameof(SaveBackupInfo.DisplayName);
        _navigationService.Navigated += tab => NavigateTo(tab);

        LoadSettingsIntoUi();
        ApplyWindowBounds();

        RefreshAllState();
        NavigateTo("Home", animate: false);

        Loaded += async (_, _) =>
        {
            AnimateEntrance();
            StartAmbientMotion();
            RefreshMiiRuntimeStatus();
            ConfigureFilesystemWatchers();
            if (_userPreferences.AutoCheckUpdates)
            {
                await CheckForUpdatesAsync(showMessages: false);
            }
            else
            {
                await FetchNewsFromServerAsync();
            }
        };
    }

    private void ApplyWindowBounds()
    {
        if (_userPreferences.WindowMaximized)
        {
            WindowState = WindowState.Maximized;
        }
        else
        {
            Width = Math.Max(MinWidth, _userPreferences.WindowWidth);
            Height = Math.Max(MinHeight, _userPreferences.WindowHeight);
        }
    }

    private void SaveWindowBounds()
    {
        if (WindowState == WindowState.Maximized)
        {
            _userPreferences.WindowMaximized = true;
        }
        else
        {
            _userPreferences.WindowMaximized = false;
            _userPreferences.WindowWidth = Width;
            _userPreferences.WindowHeight = Height;
        }

        _preferencesService.Save(_userPreferences);
    }

    private void MinimizeButton_Click(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void MaximizeButton_Click(object sender, RoutedEventArgs e)
    {
        WindowState = WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized;
        SaveWindowBounds();
    }

    private void CloseButton_Click(object sender, RoutedEventArgs e) => Close();

    protected override void OnClosed(EventArgs e)
    {
        _filesystemRefreshCts?.Cancel();
        _dolphinFileWatcher?.Dispose();
        _profileFileWatcher?.Dispose();
        base.OnClosed(e);
    }

    private void TitleBar_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ButtonState == MouseButtonState.Pressed)
        {
            DragMove();
        }
    }

    private void HomeNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Home");
    private void RoomsNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Rooms");
    private void LeaderboardNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Leaderboard");
    private void NewsNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("News");
    private void ModsNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Mods");
    private void LicensesNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Licenses");
    private void FriendsNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Friends");
    private void SettingsNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Settings");
    private void DebugNavButton_Click(object sender, RoutedEventArgs e) => _navigationService.Navigate("Debug");

    private async void RoomsRefresh_Click(object sender, RoutedEventArgs e)
    {
        if (_roomsViewModel != null)
        {
            await _roomsViewModel.RefreshAsync();
        }
    }

    private async void LeaderboardRefresh_Click(object sender, RoutedEventArgs e)
    {
        if (_leaderboardViewModel != null)
        {
            _leaderboardViewModel.UpdateLocalFriendCodes(_allLicenseCards.Select(c => c.FriendCode));
            await _leaderboardViewModel.RefreshAsync();
        }
    }

    private void NavigateTo(string tab, bool animate = true)
    {
        _currentTab = tab;
        _shellViewModel.CurrentTab = tab;
        _navigationService.CurrentTab = tab;

        if (tab != "Rooms")
        {
            _roomsViewModel?.StopAutoRefresh();
        }

        PlayView.Visibility = Visibility.Collapsed;
        RoomsView.Visibility = Visibility.Collapsed;
        LeaderboardView.Visibility = Visibility.Collapsed;
        NewsView.Visibility = Visibility.Collapsed;
        ModsView.Visibility = Visibility.Collapsed;
        LicensesView.Visibility = Visibility.Collapsed;
        FriendsView.Visibility = Visibility.Collapsed;
        SettingsView.Visibility = Visibility.Collapsed;
        DebugView.Visibility = Visibility.Collapsed;

        FrameworkElement view;
        switch (tab)
        {
            case "News":
                view = NewsView;
                PageTitleTextBlock.Text = "News";
                PageSubtitleTextBlock.Text = "Updates and changelog.";
                ApplyNewsFilter();
                break;
            case "Rooms":
                view = RoomsView;
                PageTitleTextBlock.Text = "Rooms";
                PageSubtitleTextBlock.Text = "Live Room List";
                _roomsViewModel?.StartAutoRefresh();
                _ = _roomsViewModel?.RefreshAsync();
                break;
            case "Leaderboard":
                view = LeaderboardView;
                PageTitleTextBlock.Text = "Leaderboard";
                PageSubtitleTextBlock.Text = "Global player ranking";
                _leaderboardViewModel?.UpdateLocalFriendCodes(_allLicenseCards.Select(c => c.FriendCode));
                _ = _leaderboardViewModel?.RefreshAsync();
                break;
            case "Mods":
                view = ModsView;
                PageTitleTextBlock.Text = "Mods";
                PageSubtitleTextBlock.Text = "Install, repair, and add custom textures.";
                RefreshModsView();
                break;
            case "Licenses":
                view = LicensesView;
                PageTitleTextBlock.Text = "Mii & Licenses";
                PageSubtitleTextBlock.Text = "Back up or import your saves and customize your miis.";
                RefreshLicenseView();
                break;
            case "Friends":
                view = FriendsView;
                PageTitleTextBlock.Text = "Friends";
                PageSubtitleTextBlock.Text = "Manage your Dolphin friend list locally.";
                _friendsViewModel?.LoadFriends();
                break;
            case "Settings":
                view = SettingsView;
                PageTitleTextBlock.Text = "Settings";
                PageSubtitleTextBlock.Text = "Paths and preferences.";
                break;
            case "Debug":
                view = DebugView;
                PageTitleTextBlock.Text = "Debug";
                PageSubtitleTextBlock.Text = "Developer-only local diagnostics.";
                RefreshDebugInfo();
                break;
            default:
                view = PlayView;
                PageTitleTextBlock.Text = "VanzaKart";
                PageSubtitleTextBlock.Text = "Ready to race.";
                RefreshDerivedState();
                break;
        }

        if (view != null)
        {
            view.Visibility = Visibility.Visible;
            if (animate)
            {
                AnimateViewTransition(view);
            }
        }

        SetActiveTab(tab);
    }

    private void SetActiveTab(string tab)
    {
        var buttons = new Dictionary<string, WpfButton>
        {
            ["Home"] = HomeNavButton,
            ["Rooms"] = RoomsNavButton,
            ["Leaderboard"] = LeaderboardNavButton,
            ["News"] = NewsNavButton,
            ["Mods"] = ModsNavButton,
            ["Licenses"] = LicensesNavButton,
            ["Friends"] = FriendsNavButton,
            ["Settings"] = SettingsNavButton,
            ["Debug"] = DebugNavButton
        };

        foreach (var (key, button) in buttons)
        {
            if (button == null) continue;
            button.Foreground = (WpfBrush)FindResource(key == tab ? "TextPrimary" : "TextSecondary");
            button.Background = (WpfBrush)FindResource(key == tab ? "ActiveTabBackgroundBrush" : "TransparentBrush");
        }
    }

    private static void AnimateViewTransition(FrameworkElement newView)
    {
        var slideIn = new TranslateTransform { X = 28 };
        newView.RenderTransform = slideIn;
        newView.Opacity = 0;

        var easing = new CubicEase { EasingMode = EasingMode.EaseOut };
        var animX = new DoubleAnimation(28, 0, TimeSpan.FromMilliseconds(260)) { EasingFunction = easing };
        var animOpacity = new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(220)) { EasingFunction = easing };

        slideIn.BeginAnimation(TranslateTransform.XProperty, animX);
        newView.BeginAnimation(OpacityProperty, animOpacity);
    }

    private void RefreshAllState()
    {
        RefreshDerivedState();
        RefreshModsView();
        RefreshLicenseView();
        RefreshPlayStats();
        RefreshDebugInfo();
        ApplyNewsFilter();
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
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            var detectedUserFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            if (!string.IsNullOrWhiteSpace(detectedUserFolder))
            {
                settings.UserFolderPath = detectedUserFolder;
                _settingsService.Save(settings);
            }
        }

        DolphinPathTextBox.Text = settings.DolphinPath;
        UserFolderTextBox.Text = settings.UserFolderPath;
        RomPathTextBox.Text = settings.RomPath;

        AutoUpdateCheckBox.IsChecked = _userPreferences.AutoCheckUpdates;
        SeparateSaveDefaultCheckBox.IsChecked = _userPreferences.SeparateSavegame;
        SeparateSaveCheckBox.IsChecked = _userPreferences.SeparateSavegame;
        GraphicsTexturesCheckBox.IsChecked = _userPreferences.ModOptionChoice == 2;
    }

    private void SaveSettingsFromUi()
    {
        _settingsService.Save(BuildSettingsFromUi());
        ConfigureFilesystemWatchers();
        RefreshAllState();
    }

    private void ConfigureFilesystemWatchers()
    {
        _dolphinFileWatcher?.Dispose();
        _profileFileWatcher?.Dispose();
        _dolphinFileWatcher = null;
        _profileFileWatcher = null;

        var settings = BuildSettingsFromUi();
        if (!string.IsNullOrWhiteSpace(settings.UserFolderPath) && Directory.Exists(settings.UserFolderPath))
        {
            _dolphinFileWatcher = CreateWatcher(settings.UserFolderPath, includeSubdirectories: true);
        }

        var profilesFolder = _saveManagerService.GetLauncherProfilesFolder();
        Directory.CreateDirectory(profilesFolder);
        _profileFileWatcher = CreateWatcher(profilesFolder, includeSubdirectories: true);
    }

    private FileSystemWatcher CreateWatcher(string folder, bool includeSubdirectories)
    {
        var watcher = new FileSystemWatcher(folder)
        {
            IncludeSubdirectories = includeSubdirectories,
            NotifyFilter = NotifyFilters.FileName | NotifyFilters.DirectoryName | NotifyFilters.LastWrite | NotifyFilters.Size,
            EnableRaisingEvents = true
        };

        watcher.Changed += (_, _) => ScheduleFilesystemRefresh();
        watcher.Created += (_, _) => ScheduleFilesystemRefresh();
        watcher.Deleted += (_, _) => ScheduleFilesystemRefresh();
        watcher.Renamed += (_, _) => ScheduleFilesystemRefresh();
        return watcher;
    }

    private void ScheduleFilesystemRefresh()
    {
        _filesystemRefreshCts?.Cancel();
        _filesystemRefreshCts = new CancellationTokenSource();
        var token = _filesystemRefreshCts.Token;

        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(650, token);
                await Dispatcher.InvokeAsync(() =>
                {
                    if (_currentTab == "Licenses")
                    {
                        RefreshLicenseView();
                    }
                    else
                    {
                        RefreshDebugInfo();
                    }
                });
            }
            catch (OperationCanceledException)
            {
            }
        }, token);
    }

    private void RefreshDerivedState()
    {
        var settings = BuildSettingsFromUi();
        var modFolder = settings.GetModFolder();
        ModFolderTextBlock.Text = modFolder;
        RefreshPlayStats();
        RefreshHomeUpdateCard();

        if (_isModUpdateRequired)
        {
            SetStatus($"Mod update available (v{_latestModVersion})", (WpfBrush)FindResource("WarningBrush"));
            return;
        }

        if (IsModInstalled(settings))
        {
            SetStatus("Mod installed and ready", (WpfBrush)FindResource("SuccessBrush"));
        }
        else
        {
            SetStatus("Setup required: install the mod", (WpfBrush)FindResource("WarningBrush"));
        }
    }

    private static bool IsModInstalled(LauncherSettings settings)
    {
        var xmlPath = Path.Combine(settings.GetModFolder(), "VanzaKart", "Riivolution", "VanzaKart.xml");
        return File.Exists(xmlPath);
    }

    private void RefreshModsView()
    {
        var settings = BuildSettingsFromUi();
        var localVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : "Not installed";
        var installed = IsModInstalled(settings);
        var myStuffFolder = Path.Combine(settings.GetModFolder(), "VanzaKart", "VanzaKart", "My Stuff");
        var conflicts = _modConflictService.ScanAddonConflicts(myStuffFolder);

        InstalledVersionText.Text = localVersion;
        LatestVersionText.Text = string.IsNullOrEmpty(_latestModVersion) ? "Unknown" : _latestModVersion;
        CoreModStatusTextBlock.Text = installed
            ? $"Installed version: {localVersion}"
            : "Core modpack is not installed yet.";
        AddonFolderTextBlock.Text = Directory.Exists(myStuffFolder)
            ? myStuffFolder
            : "My Stuff folder will be created after install or first import.";
        CompatibilityTextBlock.Text = installed
            ? "Core files detected. Addons can be staged locally."
            : "Install the core mod before importing addons.";
        VersioningTextBlock.Text = string.IsNullOrEmpty(_latestModVersion)
            ? "Waiting for manifest"
            : $"Manifest latest: v{_latestModVersion}";
        ModConflictTextBlock.Text = conflicts.Count == 0
            ? "No addon conflicts detected."
            : $"{conflicts.Count} conflict(s): {string.Join(", ", conflicts.Take(3).Select(conflict => conflict.FileName))}";
        ModConflictTextBlock.Foreground = conflicts.Count == 0
            ? (WpfBrush)FindResource("TextFaint")
            : (WpfBrush)FindResource("WarningBrush");
        RefreshHomeUpdateCard();
    }

    private void RefreshHomeUpdateCard()
    {
        if (HomeUpdateTitleTextBlock == null)
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        var installed = IsModInstalled(settings);
        var localVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : "Not installed";
        var latest = string.IsNullOrWhiteSpace(_latestModVersion) ? "Unknown" : _latestModVersion;

        HomeInstalledVersionTextBlock.Text = localVersion;
        HomeLatestVersionTextBlock.Text = latest;

        if (!string.IsNullOrWhiteSpace(_lastUpdateError))
        {
            SetHomeUpdateBadge("Error", "Update check failed", "#4A1825", "#FF6B82", "Read the error below, then retry check.");
            return;
        }

        if (_isBusy)
        {
            var detail = string.IsNullOrWhiteSpace(HomeUpdateCheckTextBlock.Text)
                ? "Update operation is running."
                : HomeUpdateCheckTextBlock.Text;
            SetHomeUpdateBadge(UpdatePhaseTextBlock.Text, "Working", "#233151", "#39E7FF", detail);
            return;
        }

        if (!installed)
        {
            SetHomeUpdateBadge("Setup", "Mod not installed", "#3C2D12", "#FFD166", "Install the modpack to start racing.");
            return;
        }

        if (_isModUpdateRequired)
        {
            SetHomeUpdateBadge("Update", "Update available", "#3C2D12", "#FFD166", $"Installed {localVersion}, latest {latest}.");
            return;
        }

        SetHomeUpdateBadge("Ready", "Up to date", "#153827", "#4DFFB0", "Installed mod is ready.");
    }

    private void SetHomeUpdateBadge(string badge, string title, string background, string border, string detail)
    {
        HomeUpdateBadgeTextBlock.Text = badge;
        HomeUpdateTitleTextBlock.Text = title;
        HomeUpdateCheckTextBlock.Text = detail;
        HomeUpdateBadgeBorder.Background = new SolidColorBrush((WpfColor)ColorConverter.ConvertFromString(background));
        HomeUpdateBadgeBorder.BorderBrush = new SolidColorBrush((WpfColor)ColorConverter.ConvertFromString(border));
    }

    private void RefreshLicenseView()
    {
        var settings = BuildSettingsFromUi();
        var profiles = _saveManagerService.GetSaveProfiles(settings);
        var activeMii = _saveManagerService.LoadMiiProfile();
        var miiDb = _saveManagerService.GetMiiDatabasePath(settings);
        var backupCount = _saveManagerService.GetBackupCount();

        RefreshMiiRuntimeStatus();
        RefreshMiiProfiles(activeMii.Id);
        RefreshBackupRestoreItems();

        _allLicenseCards.Clear();
        _allLicenseCards.AddRange(profiles);
        ApplyLicenseFilters();

        LicensesCountTextBlock.Text = _allLicenseCards.Count.ToString(CultureInfo.InvariantCulture);
        BackupCountTextBlock.Text = backupCount.ToString(CultureInfo.InvariantCulture);
        LatestBackupTextBlock.Text = _saveManagerService.GetLatestBackupLabel();
        MiiStateTextBlock.Text = File.Exists(miiDb)
            ? "Dolphin"
            : "Not found";

        if (_allLicenseCards.Count == 0)
        {
            LicenseSummaryTextBlock.Text = "No VanzaKart modpack license save was detected in the selected Dolphin user folder.";
            PrimaryLicenseTextBlock.Text = "No local license detected yet.";
            PrimaryLicensePathTextBlock.Text = string.Empty;
            QueueLicenseAvatarRender(settings);
            if (_friendsViewModel != null)
            {
                _friendsViewModel.ActiveLicense = null;
            }
            return;
        }

        LicenseSummaryTextBlock.Text = $"{profiles.Count} Dolphin license card(s) detected.";

        var previousActiveSlot = _friendsViewModel?.ActiveLicense?.SlotIndex;
        var previousActivePath = _friendsViewModel?.ActiveLicense?.FilePath;

        SaveProfileInfo? newActive = null;
        if (previousActiveSlot.HasValue && !string.IsNullOrEmpty(previousActivePath))
        {
            newActive = _allLicenseCards.FirstOrDefault(p => p.SlotIndex == previousActiveSlot.Value && p.FilePath == previousActivePath && !p.IsEmpty);
        }

        if (newActive == null)
        {
            newActive = _allLicenseCards.FirstOrDefault(p => !p.IsEmpty);
        }

        if (newActive != null)
        {
            SetActiveLicense(newActive);
        }
        else
        {
            if (_friendsViewModel != null)
            {
                _friendsViewModel.ActiveLicense = null;
            }
        }

        var selected = newActive ?? profiles.FirstOrDefault();
        PrimaryLicenseTextBlock.Text = selected == null
            ? string.Empty
            : $"{selected.DisplayName} - {FormatBytes(selected.SizeBytes)} - {selected.LastModifiedUtc.ToLocalTime():g}";
        PrimaryLicensePathTextBlock.Text = selected?.FilePath ?? string.Empty;
        QueueLicenseAvatarRender(settings);
    }

    private void ApplyLicenseFilters()
    {
        if (LicenseCardsItemsControl == null)
        {
            return;
        }

        var query = string.Empty;
        var filter = "All";
        var sort = "Modified";

        IEnumerable<SaveProfileInfo> cards = _allLicenseCards;
        if (!string.IsNullOrWhiteSpace(query))
        {
            cards = cards.Where(card =>
                card.DisplayName.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                card.MiiName.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                card.FilePath.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                card.SourceLabel.Contains(query, StringComparison.OrdinalIgnoreCase));
        }

        cards = filter switch
        {
            "Rendered" => cards.Where(card => card.HasAvatarImage),
            "MissingMii" => cards.Where(card => card.MiiId != 0 && card.MiiName.Contains("not found", StringComparison.OrdinalIgnoreCase)),
            "Active" => cards.Where(card => card.Races > 0 || card.Wins > 0 || card.Vr > 0 || card.Br > 0),
            _ => cards
        };

        cards = sort switch
        {
            "Name" => cards.OrderBy(card => card.DisplayName),
            "VR" => cards.OrderByDescending(card => card.Vr),
            "Wins" => cards.OrderByDescending(card => card.Wins),
            "Races" => cards.OrderByDescending(card => card.Races),
            _ => cards.OrderByDescending(card => card.LastModifiedUtc)
        };

        _licenseCards.Clear();
        foreach (var card in cards)
        {
            _licenseCards.Add(card);
        }
    }

    private async void FriendCode_OnMouseLeftButtonDown(object sender, System.Windows.Input.MouseButtonEventArgs e)
    {
        if (sender is not System.Windows.Controls.Border border)
        {
            return;
        }

        if (border.DataContext is not SaveProfileInfo profile || string.IsNullOrWhiteSpace(profile.FriendCode))
        {
            return;
        }

        try
        {
            System.Windows.Clipboard.SetText(profile.FriendCode);

            // Find the TextBlock displaying the friend code and briefly show "Copied!"
            if (border.Child is System.Windows.Controls.StackPanel panel && panel.Children.Count >= 2 &&
                panel.Children[1] is System.Windows.Controls.TextBlock fcText)
            {
                var original = fcText.Text;
                fcText.Text = "Copied!";
                await Task.Delay(1500);
                fcText.Text = original;
            }
        }
        catch
        {
        }
    }

    private void LicenseCard_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (sender is FrameworkElement element && element.DataContext is SaveProfileInfo profile)
        {
            if (profile.IsEmpty)
            {
                return;
            }
            SetActiveLicense(profile);
        }
    }

    private void SetActiveLicense(SaveProfileInfo profile)
    {
        foreach (var card in _allLicenseCards)
        {
            card.IsActive = (card == profile);
        }
        foreach (var card in _licenseCards)
        {
            card.IsActive = (card == profile);
        }
        if (_friendsViewModel != null)
        {
            _friendsViewModel.ActiveLicense = profile;
        }
    }

    private async void AddFriendButton_Click(object sender, RoutedEventArgs e)
    {
        if (_friendsViewModel != null)
        {
            await _friendsViewModel.AddFriendAsync();
        }
    }

    private async void RemoveFriendButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement element && element.DataContext is FriendPlayerInfo friend)
        {
            if (_friendsViewModel != null)
            {
                await _friendsViewModel.RemoveFriendAsync(friend);
            }
        }
    }

    private void SelectLicenseRedirect_Click(object sender, RoutedEventArgs e)
    {
        _navigationService.Navigate("Licenses");
    }

    private void OpenFriendsViewButton_OnClick(object sender, RoutedEventArgs e)
    {
        _navigationService.Navigate("Friends");
    }

    private async void QueueLicenseAvatarRender(LauncherSettings settings)
    {
        if (_isRenderingLicenseAvatars || string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            return;
        }

        _isRenderingLicenseAvatars = true;
        try
        {
            var rendered = await _saveManagerService.EnsureDolphinMiiAvatarCacheAsync(settings);
            if (rendered)
            {
                RefreshLicenseView();
            }
        }
        catch (Exception ex)
        {
            MiiRuntimeProgressTextBlock.Text = $"Renderer: {ex.Message}";
        }
        finally
        {
            _isRenderingLicenseAvatars = false;
        }
    }

    private async void QueueLauncherMiiAvatarRender()
    {
        if (_isRenderingLauncherMiiAvatars)
        {
            return;
        }

        _isRenderingLauncherMiiAvatars = true;
        try
        {
            var rendered = await _saveManagerService.EnsureLauncherMiiAvatarCacheAsync();
            if (rendered)
            {
                RefreshMiiProfiles();
            }
        }
        catch (Exception ex)
        {
            MiiRuntimeProgressTextBlock.Text = $"Renderer: {ex.Message}";
        }
        finally
        {
            _isRenderingLauncherMiiAvatars = false;
        }
    }

    private void RefreshMiiRuntimeStatus()
    {
        var status = _miiRuntimeSetupService.GetStatus();
        MiiRuntimeSetupCard.Visibility = status.IsInstalled ? Visibility.Collapsed : Visibility.Visible;
        MiiRuntimeStatusTextBlock.Text = status.IsInstalled
            ? "Renderer assets are installed."
            : "Install the Mii render asset cache for mii previews";
        MiiRuntimeProgressTextBlock.Text = status.IsInstalled
            ? $"Installed: {FormatBytes(status.SizeBytes)}"
            : "This downloads and verifies the render resource automatically.";
        MiiRuntimeProgressBar.Value = status.IsInstalled ? 100 : 0;
        InstallMiiRuntimeButton.IsEnabled = !_isInstallingMiiRuntime && !status.IsInstalled;
    }

    private void RefreshBackupRestoreItems()
    {
        var selectedPath = (BackupRestoreComboBox.SelectedItem as SaveBackupInfo)?.FilePath;
        _backupRestoreItems.Clear();
        foreach (var backup in _saveManagerService.GetBackups())
        {
            _backupRestoreItems.Add(backup);
        }

        BackupRestoreComboBox.SelectedItem = _backupRestoreItems.FirstOrDefault(item => item.FilePath == selectedPath)
                                             ?? _backupRestoreItems.FirstOrDefault();
    }

    private void RefreshMiiProfiles(string? activeMiiId = null)
    {
        _isRefreshingMiis = true;
        try
        {
            _miiProfiles.Clear();
            foreach (var profile in _saveManagerService.LoadMiiProfiles())
            {
                _miiProfiles.Add(profile);
            }

            if (_miiProfiles.Count == 0)
            {
                return;
            }

            var selected = _miiProfiles.FirstOrDefault(profile => profile.Id == activeMiiId)
                           ?? _miiProfiles.FirstOrDefault(profile => profile.Id == _saveManagerService.LoadMiiProfile().Id)
                           ?? _miiProfiles[0];
            MiiCardsListBox.SelectedItem = selected;

            if (_miiProfiles.Any(profile => profile.IsRealMii && !profile.HasAvatarImage))
            {
                QueueLauncherMiiAvatarRender();
            }
        }
        finally
        {
            _isRefreshingMiis = false;
        }
    }

    private void RefreshPlayStats()
    {
        LastPlayedTextBlock.Text = _userPreferences.LastPlayedUtc.HasValue
            ? _userPreferences.LastPlayedUtc.Value.ToLocalTime().ToString("g")
            : "Never";
        TimePlayedTextBlock.Text = FormatDuration(TimeSpan.FromMinutes(_userPreferences.TotalPlayTimeMinutes));
        LaunchCountTextBlock.Text = _userPreferences.LaunchCount.ToString(CultureInfo.InvariantCulture);
    }

    private void RefreshDebugInfo()
    {
        if (!Debugger.IsAttached)
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        DebugLogTextBlock.Text =
            $"Tab: {_currentTab}\n" +
            $"Busy: {_isBusy}\n" +
            $"Update required: {_isModUpdateRequired}\n" +
            $"Launcher version: {LauncherConfig.CurrentLauncherVersion}\n" +
            $"Latest mod version: {(_latestModVersion.Length == 0 ? "unknown" : _latestModVersion)}\n" +
            $"Dolphin: {settings.DolphinPath}\n" +
            $"User folder: {settings.UserFolderPath}\n" +
            $"ROM: {settings.RomPath}\n" +
            $"Mod folder: {settings.GetModFolder()}\n" +
            $"Settings file: {_settingsService.GetSettingsPath()}\n" +
            $"Preferences file: {_preferencesService.GetPreferencesPath()}";
    }

    private void SetBusy(bool value)
    {
        _isBusy = value;
        _shellViewModel.IsBusy = value;
        InstallButton.IsEnabled = !value;
        LaunchButton.IsEnabled = !value && !_isGameRunning;
        CheckUpdatesButton.IsEnabled = !value;
        RepairModButton.IsEnabled = !value;
        OpenModFolderButton.IsEnabled = !value;
    }

    private void SetStatus(string text, WpfBrush brush)
    {
        StatusTextBlock.Text = text;
        StatusTextBlock.Foreground = brush;
        ShellStatusTextBlock.Text = text;
        ShellStatusTextBlock.Foreground = brush;
        _shellViewModel.Status = text;
    }

    private void SetUpdateState(string phase, string detail, double? percent = null)
    {
        UpdatePhaseTextBlock.Text = phase;
        if (percent.HasValue)
        {
            DownloadProgressBar.Value = Math.Clamp(percent.Value, 0, 100);
            UpdatePercentTextBlock.Text = $"{Math.Clamp(percent.Value, 0, 100):F0}%";
        }

        if (HomeUpdateCheckTextBlock != null)
        {
            HomeUpdateCheckTextBlock.Text = detail;
            RefreshHomeUpdateCard();
        }
    }

    private MessageBoxResult ShowCustomDialog(string title, string message, MessageBoxButton buttons = MessageBoxButton.OK)
    {
        var dialog = new CustomDialog(title, message, buttons)
        {
            Owner = this
        };

        var result = dialog.ShowDialog();
        if (buttons == MessageBoxButton.OK)
        {
            return result == MessageBoxResult.OK ? MessageBoxResult.OK : MessageBoxResult.None;
        }

        if (buttons == MessageBoxButton.YesNo)
        {
            return result == MessageBoxResult.Yes ? MessageBoxResult.Yes : MessageBoxResult.No;
        }

        return result ?? MessageBoxResult.OK;
    }

    private async void InstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            ShowCustomDialog("Setup required", "Select the Dolphin User folder first in Settings.", MessageBoxButton.OK);
            _navigationService.Navigate("Settings");
            return;
        }

        SaveSettingsFromUi();

        if (IsModInstalled(settings) && !string.IsNullOrEmpty(_latestModVersion))
        {
            var localVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : "0.0";
            if (localVersion == _latestModVersion)
            {
                var result = ShowCustomDialog("Mod up to date", "The mod is already up to date. Reinstall anyway?", MessageBoxButton.YesNo);
                if (result != MessageBoxResult.Yes)
                {
                    return;
                }
            }
        }

        await PerformModInstallation();
    }

    private async Task PerformModInstallation()
    {
        SetBusy(true);
        ResetDownloadMetrics();
        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = false;
        DownloadProgressBar.Value = 0;
        UpdateSpeedTextBlock.Text = string.Empty;

        var settings = BuildSettingsFromUi();
        var modFolder = settings.GetModFolder();
        var modSubFolder = Path.Combine(modFolder, "VanzaKart");
        var isUpdate = IsModInstalled(settings);

        SetStatus("Connecting to update channel", (WpfBrush)FindResource("TextSecondary"));
        SetUpdateState("Connecting", "Preparing download...", 0);

        ModUpdateBackup? backup = null;

        try
        {
            // ── STEP 1: backup user data (only if the mod is already installed) ──
            if (isUpdate)
            {
                SetUpdateState("Backup", "Saving licenses, Mii and profiles...", 2);
                SetStatus("Backing up user data", (WpfBrush)FindResource("TextSecondary"));

                var backupProgress = new Progress<string>(msg =>
                    SetUpdateState("Backup", msg, 3));

                backup = await _modUpdateSafetyService.CreateBackupAsync(
                    settings, backupProgress);

                if (backup.Files.Count > 0)
                {
                    await WriteUpdateLogAsync(
                        $"Backup creato: {backup.BackupId} ({backup.Files.Count} file protetti)");
                }
            }

            ModUpdateResult result;
            ModManifest? manifest = null;

            if (isUpdate && !string.IsNullOrWhiteSpace(_latestModManifestUrl))
            {
                try
                {
                    SetUpdateState("Download", "Downloading update manifest...", 5);
                    SetStatus("Fetching update manifest", (WpfBrush)FindResource("TextSecondary"));

                    var manifestJson = await _networkService.DownloadStringAsync(_latestModManifestUrl);
                    manifest = JsonSerializer.Deserialize<ModManifest>(manifestJson);
                }
                catch (Exception ex)
                {
                    await WriteUpdateLogAsync($"Manifest download failed: {ex.Message}. Falling back to full ZIP.");
                }
            }

            if (isUpdate && manifest != null)
            {
                // ── STEP 2: differential update ──
                SetUpdateState("Verifying", "Scanning local files...", 8);
                SetStatus("Verifying local installation", (WpfBrush)FindResource("TextSecondary"));

                var localFiles = await _modUpdateSafetyService.ScanLocalFilesAsync(modSubFolder);

                // Diff files
                var filesToDownload = new List<ModManifestFile>();
                var filesToDelete = new List<string>();

                foreach (var serverFile in manifest.Files)
                {
                    var local = localFiles.FirstOrDefault(f => f.Path.Equals(serverFile.Path, StringComparison.OrdinalIgnoreCase));
                    if (local == null || local.Sha256 != serverFile.Sha256)
                    {
                        filesToDownload.Add(serverFile);
                    }
                }

                foreach (var localFile in localFiles)
                {
                    var serverHasIt = manifest.Files.Any(f => f.Path.Equals(localFile.Path, StringComparison.OrdinalIgnoreCase));
                    if (!serverHasIt)
                    {
                        filesToDelete.Add(localFile.Path);
                    }
                }

                long totalBytesToDownload = Math.Max(1, filesToDownload.Sum(f => f.Size));
                long downloadedBytes = 0;

                await WriteUpdateLogAsync($"Incremental update started: {filesToDownload.Count} files to download ({FormatBytes(totalBytesToDownload)}), {filesToDelete.Count} files to delete.");

                int fileIndex = 0;
                foreach (var file in filesToDownload)
                {
                    fileIndex++;

                    var localPath = Path.Combine(modSubFolder, file.Path.Replace('/', Path.DirectorySeparatorChar));

                    SetStatus($"Downloading {fileIndex}/{filesToDownload.Count}: {Path.GetFileName(file.Path)}", (WpfBrush)FindResource("TextSecondary"));
                    
                    var fileProgress = new Progress<(long current, long total)>(p =>
                    {
                        long currentTotalDownloaded = downloadedBytes + p.current;
                        UpdateDownloadProgress(currentTotalDownloaded, totalBytesToDownload);
                    });

                    var tempFile = localPath + ".tmp";
                    Directory.CreateDirectory(Path.GetDirectoryName(localPath)!);
                    
                    if (File.Exists(tempFile))
                        File.Delete(tempFile);

                    try
                    {
                        var mirrors = BuildModFileMirrorList(file.Path);
                        await _networkService.DownloadFileWithResumeAsync(mirrors, tempFile, fileProgress);
                        
                        var downloadedHash = await ModUpdateSafetyService.ComputeSha256Async(tempFile, default);
                        if (downloadedHash != file.Sha256)
                        {
                            throw new InvalidDataException($"Hash mismatch for downloaded file: {file.Path}. Expected {file.Sha256}, got {downloadedHash}");
                        }

                        if (File.Exists(localPath))
                            File.Delete(localPath);

                        File.Move(tempFile, localPath);
                    }
                    catch (Exception ex)
                    {
                        var primaryUrl = $"{_latestModFilesUrl.TrimEnd('/')}/{file.Path.Replace('\\', '/')}";
                        await WriteUpdateLogAsync($"Failed to download file '{file.Path}' from URL '{primaryUrl}'. Error: {ex.Message}");
                        throw;
                    }
                    finally
                    {
                        if (File.Exists(tempFile))
                            File.Delete(tempFile);
                    }

                    downloadedBytes += file.Size;
                }

                // Delete obsolete files
                int deletedCount = 0;
                foreach (var fileToDelete in filesToDelete)
                {
                    var localPath = Path.Combine(modSubFolder, fileToDelete.Replace('/', Path.DirectorySeparatorChar));
                    if (File.Exists(localPath))
                    {
                        File.Delete(localPath);
                        deletedCount++;
                        await WriteUpdateLogAsync($"pruned (obsolete): {fileToDelete}");
                    }
                }

                // Clean up empty directories
                _modUpdateSafetyService.RemoveEmptyDirectories(
                    modSubFolder,
                    modSubFolder,
                    _modUpdateSafetyService.BuildProtectedAbsolutePaths(settings, modSubFolder));

                result = new ModUpdateResult
                {
                    FilesWritten = filesToDownload.Count,
                    FilesSkipped = 0,
                    FilesPruned = deletedCount
                };
            }
            else
            {
                // ── STEP 2: download ──
                SetUpdateState("Download", "Downloading modpack...", 5);
                var downloadProgress = new Progress<(long current, long total)>(
                    p => UpdateDownloadProgress(p.current, p.total));

                await _networkService.DownloadFileWithResumeAsync(
                    BuildModMirrorList(), _tempZipPath, downloadProgress);

                // ── STEP 3: integrity check ──
                SetStatus("Verifying downloaded archive", (WpfBrush)FindResource("TextSecondary"));
                SetUpdateState("Verifying", "Checking archive integrity...", 96);
                await VerifyDownloadedArchiveAsync(_tempZipPath, _latestModSha256);

                // ── STEP 4: selective extraction ──
                DownloadProgressBar.IsIndeterminate = false;
                DownloadProgressBar.Value = 0;
                SetStatus("Updating modpack files", (WpfBrush)FindResource("WarningBrush"));
                SetUpdateState(
                    isUpdate ? "Updating" : "Installing",
                    isUpdate
                        ? "Replacing modpack files (user data is not affected)..."
                        : "Writing modpack files to Riivolution folder...",
                    0);

                var extractProgress = new Progress<int>(p =>
                    SetUpdateState(
                        isUpdate ? "Updating" : "Installing",
                        isUpdate
                            ? $"Updating modpack files... {p}%"
                            : $"Writing files... {p}%",
                        p));

                result = await _modUpdateSafetyService.ApplyZipUpdateAsync(
                    _tempZipPath,
                    modFolder,
                    modSubFolder,
                    settings,
                    extractProgress);

                // ── STEP 5: clean up temp ZIP ──
                if (File.Exists(_tempZipPath))
                    File.Delete(_tempZipPath);
            }

            // ── STEP 6: write version ────────────────────────────────────────────────────
            if (!string.IsNullOrWhiteSpace(_latestModVersion))
            {
                File.WriteAllText(_localModVersionFile, _latestModVersion);
                _userPreferences.LastKnownLatestModVersion = _latestModVersion;
                _preferencesService.Save(_userPreferences);
            }

            // ── Completato ────────────────────────────────────────────────────────
            DownloadProgressBar.Value = 100;
            _isModUpdateRequired = false;

            var summary = BuildSafeUpdateStatusMessage(isUpdate, result, backup);
            SetUpdateState("Completed", summary, 100);
            SetStatus(
                isUpdate ? "Update completed. Ready to race." : "Installation completed. Ready to race.",
                (WpfBrush)FindResource("SuccessBrush"));

            ShowToast(
                isUpdate ? "Update completed" : "Installation completed",
                summary);

            // Log del riepilogo
            await WriteUpdateLogAsync(
                $"Operation completed – {result}");

            RefreshAllState();
        }
        catch (Exception ex)
        {
            // ── Rollback automatico ───────────────────────────────────────────────
            if (backup != null && backup.Files.Count > 0)
            {
                try
                {
                    SetUpdateState("Rollback", "Restoring user data after error...", 0);
                    SetStatus("Error – restoring data", (WpfBrush)FindResource("DangerBrush"));

                    await _modUpdateSafetyService.RestoreBackupAsync(backup);

                    await WriteUpdateLogAsync(
                        $"Rollback completed (backup {backup.BackupId}): " +
                        $"{backup.Files.Count} user file(s) restored.");
                }
                catch (Exception rollbackEx)
                {
                    // Rollback itself failed: warn prominently
                    await WriteUpdateLogAsync(
                        $"WARNING – rollback failed: {rollbackEx.Message}. " +
                        $"Manual restore from Backups/{backup.BackupId}");

                    ShowCustomDialog(
                        "Warning – rollback failed",
                        $"The update failed AND the automatic rollback did not succeed.\n\n" +
                        $"Your data (licenses, Mii) is safe in the folder:\n" +
                        $"Backups\\ModUpdates\\{backup.BackupId}\n\n" +
                        $"Manually copy the files from there before relaunching.\n\n" +
                        $"Original error: {ex.Message}\n" +
                        $"Rollback error: {rollbackEx.Message}",
                        MessageBoxButton.OK);

                    goto Cleanup;
                }
            }

            // Standard error (no backup or rollback succeeded)
            DownloadProgressBar.IsIndeterminate = false;
            DownloadProgressBar.Visibility = Visibility.Collapsed;
            SetStatus("Installation failed", (WpfBrush)FindResource("DangerBrush"));
            SetUpdateState("Error", ex.Message, 0);
            ShowCustomDialog("Installation error", ex.Message, MessageBoxButton.OK);

            await WriteUpdateLogAsync($"Error: {ex.Message}");

        Cleanup:
            if (!isUpdate && Directory.Exists(modSubFolder))
            {
                try
                {
                    Directory.Delete(modSubFolder, true);
                }
                catch { }
            }
            RefreshAllState();
        }
        finally
        {
            // Clean up ZIP in any case
            try
            {
                if (File.Exists(_tempZipPath))
                    File.Delete(_tempZipPath);
            }
            catch { /* ignore */ }

            SetBusy(false);
        }
    }

    private static string BuildSafeUpdateStatusMessage(
        bool wasUpdate,
        ModUpdateResult result,
        ModUpdateBackup? backup)
    {
        if (!wasUpdate)
            return $"VanzaKart successfully installed ({result.FilesWritten} files).";

        var sb = new System.Text.StringBuilder();
        sb.Append($"VanzaKart updated: {result.FilesWritten} files replaced");

        if (result.FilesPruned > 0)
            sb.Append($", {result.FilesPruned} outdated files removed");

        if (result.FilesSkipped > 0)
            sb.Append($", {result.FilesSkipped} user files protected");

        if (backup?.Files.Count > 0)
            sb.Append($" (backup: {backup.BackupId})");

        sb.Append('.');

        if (result.HasErrors)
            sb.Append($" Warning: {result.Errors.Count} files failed to update (see log).");

        return sb.ToString();
    }
    private static Task WriteUpdateLogAsync(string message)
    {
        try
        {
            var path = Path.Combine(AppContext.BaseDirectory, "Logs", "mod-update.log");
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            return File.AppendAllTextAsync(
                path,
                $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] [Launcher] {message}{Environment.NewLine}");
        }
        catch
        {
            return Task.CompletedTask;
        }
    }

    private async void RepairModButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (ShowCustomDialog("Repair installation", "This will re-download and reinstall the mod. Continue?", MessageBoxButton.YesNo) == MessageBoxResult.Yes)
        {
            await PerformModInstallation();
        }
    }

    private async void LaunchButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        if (_isModUpdateRequired)
        {
            var result = ShowCustomDialog(
                "Update available",
                "The installed mod version is not the latest. Do you want to launch the game anyway?",
                MessageBoxButton.YesNo);
            if (result != MessageBoxResult.Yes)
            {
                _navigationService.Navigate("Mods");
                return;
            }
        }

        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.DolphinPath) ||  
            string.IsNullOrWhiteSpace(settings.RomPath) ||
            string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            ShowCustomDialog("Setup required", "Configure Dolphin, the User folder, and the Mario Kart Wii ROM in Settings.", MessageBoxButton.OK);
            _navigationService.Navigate("Settings");
            return;
        }
            
        var rootDir = Path.Combine(settings.GetModFolder(), "VanzaKart");
        var xmlPath = Path.Combine(rootDir, "Riivolution", "VanzaKart.xml");
        if (!File.Exists(xmlPath))
        {
            ShowCustomDialog("Mod not found", "Install the VanzaKart modpack before launching.", MessageBoxButton.OK);
            _navigationService.Navigate("Mods");
            return;
        }

        SaveSettingsFromUi();

        _userPreferences.SeparateSavegame = SeparateSaveCheckBox.IsChecked == true;
        _userPreferences.ModOptionChoice = GraphicsTexturesCheckBox.IsChecked == true ? 2 : 0;
        _userPreferences.LastPlayedUtc = DateTime.UtcNow;
        _userPreferences.LaunchCount++;
        _preferencesService.Save(_userPreferences);

        try
        {
            int optionChoice = _userPreferences.ModOptionChoice;
            int saveChoice = _userPreferences.SeparateSavegame ? 1 : 0;

            var jsonPath = Path.Combine(AppContext.BaseDirectory, "VanzaKart_launcher.json");
            var json = $@"{{
  ""base-file"": ""{EscapeJsonValue(settings.RomPath)}"",
  ""display-name"": ""VanzaKart Modpack"",
  ""riivolution"": {{
    ""patches"": [
      {{
        ""options"": [
          {{ ""choice"": 1, ""option-name"": ""Pack"", ""section-name"": ""VanzaKart"" }},
          {(optionChoice == 2 ? "{ \"choice\": 2, \"option-name\": \"MyStuff\", \"section-name\": \"VanzaKart\" }," : "")}
          {{ ""choice"": {saveChoice}, ""option-name"": ""Seperate Savegame"", ""section-name"": ""VanzaKart"" }}
        ],
        ""root"": ""{EscapeJsonValue(rootDir)}"",
        ""xml"": ""{EscapeJsonValue(xmlPath)}""
      }}
    ]
  }},
  ""type"": ""dolphin-game-mod-descriptor"",
  ""version"": 1
}}";

            File.WriteAllText(jsonPath, json);

            var process = Process.Start(new ProcessStartInfo
            {
                FileName = settings.DolphinPath,
                Arguments = $"-b \"{jsonPath}\"",
                UseShellExecute = true,
                WorkingDirectory = Path.GetDirectoryName(settings.DolphinPath)
            });

            _isGameRunning = true;
            SetBusy(_isBusy);

            TrackGameSession(process);
            SetStatus("Game launched. Enjoy VanzaKart.", (WpfBrush)FindResource("SuccessBrush"));
            ShowToast("Race started", "Vanzakart is launching.");
            RefreshPlayStats();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Launch error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void CheckUpdatesButton_OnClick(object sender, RoutedEventArgs e) => await CheckForUpdatesAsync(true);

    private async Task CheckForUpdatesAsync(bool showMessages)
    {
        try
        {
            if (showMessages)
            {
                SetStatus("Checking for updates", (WpfBrush)FindResource("TextSecondary"));
                SetUpdateState("Checking", "Reading VanzaKart update manifest...", 0);
            }

            await FetchNewsFromServerAsync();

            var noCacheUrl = $"{LauncherConfig.VersionJsonUrl}?t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
            var json = await _networkService.DownloadStringAsync(noCacheUrl);
            var info = JsonSerializer.Deserialize<VersionInfo>(json) ?? new VersionInfo();
            _lastUpdateCheckUtc = DateTime.UtcNow;
            _lastUpdateError = string.Empty;

            _latestModVersion = info.ModVersion;
            _userPreferences.LastKnownLatestModVersion = info.ModVersion;
            _preferencesService.Save(_userPreferences);
            _latestModUrl = string.IsNullOrWhiteSpace(info.ModUrl) ? LauncherConfig.ModUrl : info.ModUrl;
            _latestModMirrors = info.ModMirrors ?? Array.Empty<string>();
            _latestModSha256 = info.ModSha256;
            _latestModManifestUrl = string.IsNullOrWhiteSpace(info.ModManifestUrl) ? LauncherConfig.ModManifestUrl : info.ModManifestUrl;
            _latestModFilesUrl = string.IsNullOrWhiteSpace(info.ModFilesUrl) ? LauncherConfig.ModFilesUrl : info.ModFilesUrl;
            _latestModFilesMirrors = info.ModFilesMirrors ?? Array.Empty<string>();
            _latestLauncherUrl = string.IsNullOrWhiteSpace(info.LauncherUrl) ? LauncherConfig.LauncherZipUrl : info.LauncherUrl;
            _latestLauncherMirrors = info.LauncherMirrors ?? Array.Empty<string>();
            _latestChangelog = info.Changelog ?? Array.Empty<string>();

            if (!string.IsNullOrWhiteSpace(info.LauncherVersion) &&
                info.LauncherVersion != LauncherConfig.CurrentLauncherVersion)
            {
                var answer = ShowCustomDialog("Launcher update", $"New launcher v{info.LauncherVersion} is available. Update now?", MessageBoxButton.YesNo);
                if (answer == MessageBoxResult.Yes)
                {
                    await PerformLauncherUpdateAsync();
                    return;
                }
            }

            if (IsModInstalled(BuildSettingsFromUi()))
            {
                var localVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : "0.0";
                if (!string.IsNullOrWhiteSpace(info.ModVersion) && info.ModVersion != localVersion)
                {
                    _isModUpdateRequired = true;
                    SetStatus($"Mod update available (v{info.ModVersion})", (WpfBrush)FindResource("WarningBrush"));
                    SetUpdateState("Update available", $"Installed {localVersion}, latest {info.ModVersion}.", 0);
                    if (showMessages)
                    {
                        ShowToast("Update available", $"VanzaKart v{info.ModVersion} is ready to install.");
                    }
                }
                else
                {
                    _isModUpdateRequired = false;
                    SetStatus("Mod is up to date", (WpfBrush)FindResource("SuccessBrush"));
                    SetUpdateState("Up to date", "No mod update is required.", 100);
                    if (showMessages)
                    {
                        ShowToast("No updates", "VanzaKart is already up to date.");
                    }
                }
            }

            RefreshAllState();
        }
        catch (Exception ex)
        {
            _lastUpdateCheckUtc = DateTime.UtcNow;
            _lastUpdateError = ex.Message;
            SetStatus("Update check failed", (WpfBrush)FindResource("DangerBrush"));
            SetUpdateState("Network error", ex.Message, 0);
            if (showMessages)
            {
                ShowToast("Update check failed", ex.Message);
            }
            RefreshHomeUpdateCard();
        }
    }

    private async Task PerformLauncherUpdateAsync()
    {
        SetBusy(true);
        ResetDownloadMetrics();
        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = false;
        DownloadProgressBar.Value = 0;
        SetStatus("Downloading launcher update", (WpfBrush)FindResource("TextSecondary"));
        SetUpdateState("Launcher update", "Downloading new launcher package...", 0);

        var tempZip = Path.Combine(AppContext.BaseDirectory, "Launcher_Update.zip");
        var batchPath = Path.Combine(AppContext.BaseDirectory, "update.bat");

        try
        {
            var progress = new Progress<(long current, long total)>(p => UpdateDownloadProgress(p.current, p.total));
            await _networkService.DownloadFileWithResumeAsync(BuildLauncherMirrorList(), tempZip, progress);
            await _archiveService.ValidateZipAsync(tempZip);

            var exeName = Path.GetFileName(Environment.ProcessPath) ?? "VanzaKart Launcher.exe";
            var script = $@"@echo off
cd /d ""{AppContext.BaseDirectory}""
timeout /t 2 /nobreak >nul
powershell -NoProfile -ExecutionPolicy Bypass -Command ""Expand-Archive -Path '{tempZip}' -DestinationPath '{AppContext.BaseDirectory}' -Force""
del ""{tempZip}""
start """" ""{Path.Combine(AppContext.BaseDirectory, exeName)}""
del ""%~f0""";
            File.WriteAllText(batchPath, script);

            Process.Start(new ProcessStartInfo { FileName = batchPath, UseShellExecute = true, WindowStyle = ProcessWindowStyle.Hidden });
            System.Windows.Application.Current.Shutdown();
        }
        catch (Exception ex)
        {
            SetStatus("Launcher update failed", (WpfBrush)FindResource("DangerBrush"));
            SetUpdateState("Failed", ex.Message, 0);
            ShowCustomDialog("Launcher update error", ex.Message, MessageBoxButton.OK);
            SetBusy(false);
        }
    }

    private async Task VerifyDownloadedArchiveAsync(string zipPath, string expectedSha256)
    {
        await _archiveService.ValidateZipAsync(zipPath);
        if (string.IsNullOrWhiteSpace(expectedSha256))
        {
            return;
        }

        var actual = await ComputeSha256Async(zipPath);
        if (!actual.Equals(expectedSha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException($"The downloaded archive hash does not match the manifest.\nExpected: {expectedSha256}\nActual: {actual}");
        }
    }

    private static async Task<string> ComputeSha256Async(string path)
    {
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        using var sha = SHA256.Create();
        var hash = await sha.ComputeHashAsync(stream);
        return Convert.ToHexString(hash);
    }

    private IEnumerable<string> BuildModMirrorList()
    {
        yield return _latestModUrl;
        foreach (var mirror in _latestModMirrors)
        {
            yield return mirror;
        }
        yield return LauncherConfig.ModUrl;
    }

    private IEnumerable<string> BuildModFileMirrorList(string fileRelativePath)
    {
        var escapedPath = fileRelativePath.Replace('\\', '/');

        if (!string.IsNullOrWhiteSpace(_latestModFilesUrl))
        {
            yield return $"{_latestModFilesUrl.TrimEnd('/')}/{escapedPath}";
        }

        if (_latestModFilesMirrors != null)
        {
            foreach (var mirror in _latestModFilesMirrors)
            {
                if (!string.IsNullOrWhiteSpace(mirror))
                {
                    yield return $"{mirror.TrimEnd('/')}/{escapedPath}";
                }
            }
        }

        var defaultFilesUrl = LauncherConfig.ModFilesUrl;
        if (!string.IsNullOrWhiteSpace(defaultFilesUrl) && defaultFilesUrl != _latestModFilesUrl)
        {
            yield return $"{defaultFilesUrl.TrimEnd('/')}/{escapedPath}";
        }
    }

    private IEnumerable<string> BuildLauncherMirrorList()
    {
        yield return _latestLauncherUrl;
        foreach (var mirror in _latestLauncherMirrors)
        {
            yield return mirror;
        }
        yield return LauncherConfig.LauncherZipUrl;
    }

    private void ResetDownloadMetrics()
    {
        _downloadStopwatch.Reset();
        _downloadBaselineBytes = -1;
        UpdateSpeedTextBlock.Text = string.Empty;
        UpdatePercentTextBlock.Text = "0%";
    }

    private void UpdateDownloadProgress(long current, long total)
    {
        if (_downloadBaselineBytes < 0)
        {
            _downloadBaselineBytes = current;
            _downloadStopwatch.Restart();
        }

        var percent = total <= 0 ? 0 : (double)current / total * 100;
        var sessionBytes = Math.Max(0, current - _downloadBaselineBytes);
        var speed = _downloadStopwatch.Elapsed.TotalSeconds <= 0
            ? 0
            : sessionBytes / _downloadStopwatch.Elapsed.TotalSeconds;

        SetUpdateState("Downloading", $"{FormatBytes(current)} / {FormatBytes(total)}", percent);
        UpdateSpeedTextBlock.Text = speed <= 0 ? "Measuring speed..." : $"{FormatBytes((long)speed)}/s";
        SetStatus($"Downloading {percent:F0}%", (WpfBrush)FindResource("TextSecondary"));
    }

    private void OpenModFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        var folder = BuildSettingsFromUi().GetModFolder();
        if (Directory.Exists(folder))
        {
            OpenFolder(folder);
        }
        else
        {
            ShowCustomDialog("Folder not found", "The mod folder does not exist yet.", MessageBoxButton.OK);
        }
    }

    private void OpenAddonsFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog("Mod not installed", "Install VanzaKart before opening the addon folder.", MessageBoxButton.OK);
            return;
        }

        var folder = Path.Combine(settings.GetModFolder(), "VanzaKart", "VanzaKart", "My Stuff");
        Directory.CreateDirectory(folder);
        OpenFolder(folder);
        RefreshModsView();
    }

    private void BrowseDolphinButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfOpenFileDialog { Filter = "Executable (*.exe)|*.exe" };
        if (dialog.ShowDialog() != true)
        {
            return;
        }

        DolphinPathTextBox.Text = dialog.FileName;
        var settings = BuildSettingsFromUi();
        var possibleUser = _saveManagerService.TryAutoDetectUserFolder(settings);
        if (!string.IsNullOrWhiteSpace(possibleUser))
        {
            UserFolderTextBox.Text = possibleUser;
            ShowToast("Dolphin detected", "The User folder was detected automatically.");
        }

        SaveSettingsFromUi();
    }

    private void BrowseUserFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfOpenFolderDialog
        {
            Title = "Select Dolphin User folder",
            Multiselect = false
        };

        if (dialog.ShowDialog(this) == true)
        {
            UserFolderTextBox.Text = dialog.FolderName;
            SaveSettingsFromUi();
        }
    }

    private void BrowseRomButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfOpenFileDialog { Filter = "Wii ROM (*.wbfs;*.iso)|*.wbfs;*.iso" };
        if (dialog.ShowDialog() != true)
        {
            return;
        }

        RomPathTextBox.Text = dialog.FileName;
        SaveSettingsFromUi();
    }

    private async void InstallMiiRuntimeButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isInstallingMiiRuntime)
        {
            return;
        }

        _isInstallingMiiRuntime = true;
        InstallMiiRuntimeButton.IsEnabled = false;
        MiiRuntimeSetupCard.Visibility = Visibility.Visible;

        var progress = new Progress<MiiRuntimeSetupProgress>(item =>
        {
            MiiRuntimeStatusTextBlock.Text = item.Stage;
            MiiRuntimeProgressBar.Value = item.Percent;
            MiiRuntimeProgressTextBlock.Text = item.TotalBytes is > 0
                ? $"{FormatBytes(item.BytesReceived)} / {FormatBytes(item.TotalBytes.Value)}"
                : $"{FormatBytes(item.BytesReceived)} downloaded";
        });

        try
        {
            await _miiRuntimeSetupService.InstallAsync(progress);

            try
            {
                var settings = BuildSettingsFromUi();
                if (!string.IsNullOrWhiteSpace(settings.UserFolderPath) && Directory.Exists(settings.UserFolderPath))
                {
                    var faceLibDir = Path.Combine(settings.UserFolderPath, "Wii", "shared2", "menu", "FaceLib");
                    Directory.CreateDirectory(faceLibDir);
                    var status = _miiRuntimeSetupService.GetStatus();
                    if (status.IsInstalled)
                    {
                        File.Copy(status.ResourcePath, Path.Combine(faceLibDir, "FFLResHigh.dat"), overwrite: true);
                        File.Copy(status.ResourcePath, Path.Combine(faceLibDir, "FFLRes.dat"), overwrite: true);
                    }
                }
            }
            catch
            {
            }

            ShowToast("Mii setup ready", "Render assets installed successfully.");
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii setup error", ex.Message, MessageBoxButton.OK);
        }
        finally
        {
            _isInstallingMiiRuntime = false;
            RefreshMiiRuntimeStatus();
            QueueLauncherMiiAvatarRender();
            QueueLicenseAvatarRender(BuildSettingsFromUi());
        }
    }

    private async void BackupSaveButton_OnClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var backup = await _saveManagerService.BackupPrimarySaveAsync(BuildSettingsFromUi());
            ShowToast("Backup created", backup);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Backup error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void RestoreBackupButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (BackupRestoreComboBox.SelectedItem is not SaveBackupInfo backup)
        {
            ShowCustomDialog("No backup selected", "Create or select a backup before restoring.", MessageBoxButton.OK);
            return;
        }

        if (ShowCustomDialog("Restore backup", $"Restore {backup.DisplayName}? A safety backup will be created first.", MessageBoxButton.YesNo) != MessageBoxResult.Yes)
        {
            return;
        }

        try
        {
            await _saveManagerService.RestoreBackupAsync(BuildSettingsFromUi(), backup.FilePath);
            ShowToast("Backup restored", backup.DisplayName);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Restore error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void ImportSaveButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfOpenFileDialog { Filter = "Mario Kart Wii save (rksys.dat)|rksys.dat|All files (*.*)|*.*" };
        if (dialog.ShowDialog() != true)
        {
            return;
        }

        try
        {
            await _saveManagerService.ImportSaveFileAsync(BuildSettingsFromUi(), dialog.FileName);
            ShowToast("Save imported", "A backup was created before replacing the current save.");
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Import error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void ExportSaveButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfSaveFileDialog
        {
            Filter = "Mario Kart Wii save (rksys.dat)|rksys.dat|All files (*.*)|*.*",
            FileName = $"rksys_export_{DateTime.Now:yyyyMMdd_HHmmss}.dat"
        };

        if (dialog.ShowDialog() != true)
        {
            return;
        }

        try
        {
            await _saveManagerService.ExportPrimarySaveAsync(BuildSettingsFromUi(), dialog.FileName);
            ShowToast("Save exported", dialog.FileName);
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Export error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void CreateMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var state = new MiiEditorState
            {
                Name = BuildNewMiiName(),
                CreatorName = "VanzaKart",
                FavoriteColorIndex = 4,
                IsFavorite = _saveManagerService.LoadMiiProfiles().Count == 0
            };

            var profile = await _saveManagerService.CreateMiiProfileAsync(state);
            await TrySyncMiiToDolphinAsync(profile);
            RefreshLicenseView();
            OpenMiiEditor(profile.Id);
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii creation error", ex.Message, MessageBoxButton.OK);
        }
    }

    private void CreateLicenseButton_OnClick(object sender, RoutedEventArgs e)
    {
        var settings = BuildSettingsFromUi();
        var hasRealSave = _saveManagerService.GetSaveProfiles(settings).Count > 0;
        var message = hasRealSave
            ? "Real save-backed license editing is enabled through detected rksys.dat cards. Full from-scratch rksys generation is intentionally disabled until a verified save template is provided, so the launcher will not create a fake license."
            : "No real Mario Kart Wii save was found. Launch Mario Kart Wii once in Dolphin to create rksys.dat, then the launcher can manage real licenses without fake data.";
        ShowCustomDialog("Real license required", message, MessageBoxButton.OK);
    }

    private async void ImportMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new WpfOpenFileDialog
        {
            Filter = "Mii files (*.mii;*.miigx;*.mae;*.rcd;*.rsd;*.json;*.vk-mii)|*.mii;*.miigx;*.mae;*.rcd;*.rsd;*.json;*.vk-mii|All files (*.*)|*.*"
        };

        if (dialog.ShowDialog() != true)
        {
            return;
        }

        try
        {
            var profile = await _saveManagerService.ImportMiiProfileAsync(dialog.FileName);
            var synced = await TrySyncMiiToDolphinAsync(profile);
            ShowToast("Mii imported", synced ? $"{profile.Name} was synced to Dolphin." : profile.Name);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii import error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void ExportMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog("Select a Mii", "Select a Mii before exporting.", MessageBoxButton.OK);
            return;
        }

        var dialog = new WpfSaveFileDialog
        {
            Filter = "Wii Mii (*.mii)|*.mii|VanzaKart Mii profile (*.vk-mii)|*.vk-mii|JSON profile (*.json)|*.json",
            FileName = $"{SanitizeFileName(selected.Name)}.mii"
        };

        if (dialog.ShowDialog() != true)
        {
            return;
        }

        try
        {
            await _saveManagerService.ExportMiiProfileAsync(selected.Id, dialog.FileName);
            ShowToast("Mii exported", dialog.FileName);
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii export error", ex.Message, MessageBoxButton.OK);
        }
    }

    private async void DuplicateMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog("Select a Mii", "Select a Mii before duplicating.", MessageBoxButton.OK);
            return;
        }

        try
        {
            var duplicate = await _saveManagerService.DuplicateMiiProfileAsync(selected.Id);
            ShowToast("Mii duplicated", duplicate.Name);
            RefreshLicenseView();
            OpenMiiEditor(duplicate.Id);
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii duplicate error", ex.Message, MessageBoxButton.OK);
        }
    }

    private void DeleteMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog("Select a Mii", "Select a Mii before deleting.", MessageBoxButton.OK);
            return;
        }

        if (ShowCustomDialog("Delete Mii", $"Delete {selected.Name} from the library? this cannot be undone.", MessageBoxButton.YesNo) != MessageBoxResult.Yes)
        {
            return;
        }

        try
        {
            _saveManagerService.DeleteMiiProfile(selected.Id);
            ShowToast("Mii deleted", selected.Name);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii delete error", ex.Message, MessageBoxButton.OK);
        }
    }

    private void MiiCardsListBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isRefreshingMiis || MiiCardsListBox.SelectedItem is not LauncherMiiProfile selected)
        {
            return;
        }

        try
        {
            _saveManagerService.SetActiveMii(selected.Id);
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii selection error", ex.Message, MessageBoxButton.OK);
        }
    }

    private void EditMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog("Select a Mii", "Select a Mii before editing.", MessageBoxButton.OK);
            return;
        }

        OpenMiiEditor(selected.Id);
    }

    private void MiiCardsListBox_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (MiiCardsListBox.SelectedItem is LauncherMiiProfile selected)
        {
            OpenMiiEditor(selected.Id);
        }
    }

    private void MiiCardsListBox_PreviewMouseWheel(object sender, MouseWheelEventArgs e)
    {
        if (!e.Handled)
        {
            var scrollViewer = FindParent<ScrollViewer>(MiiCardsListBox);
            if (scrollViewer != null)
            {
                e.Handled = true;
                int lines = Math.Abs(e.Delta) / 40;
                if (lines == 0) lines = 1;
                for (int i = 0; i < lines; i++)
                {
                    if (e.Delta < 0)
                    {
                        scrollViewer.LineDown();
                    }
                    else
                    {
                        scrollViewer.LineUp();
                    }
                }
            }
        }
    }

    private static T? FindParent<T>(DependencyObject child) where T : DependencyObject
    {
        var parentDep = VisualTreeHelper.GetParent(child);
        if (parentDep == null)
        {
            return null;
        }

        if (parentDep is T parent)
        {
            return parent;
        }

        return FindParent<T>(parentDep);
    }

    private void OpenMiiEditor(string miiId)
    {
        try
        {
            var editor = new MiiEditorWindow(_saveManagerService, BuildSettingsFromUi(), miiId)
            {
                Owner = this
            };
            editor.ShowDialog();
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Mii editor error", ex.Message, MessageBoxButton.OK);
        }
    }

    private string BuildNewMiiName()
    {
        var index = _saveManagerService.LoadMiiProfiles().Count + 1;
        return index <= 1 ? "Vanza Mii" : $"Vanza Mii {index}";
    }

    private void OpenSavesFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        var settings = BuildSettingsFromUi();
        var wiiFolder = _saveManagerService.GetWiiRoot(settings);
        var folder = Directory.Exists(wiiFolder) ? wiiFolder : settings.UserFolderPath;
        if (Directory.Exists(folder))
        {
            OpenFolder(folder);
        }
        else
        {
            ShowCustomDialog("Folder not found", "The Dolphin Wii save folder was not found.", MessageBoxButton.OK);
        }
    }

    private void OpenMiiRendererLogButton_OnClick(object sender, RoutedEventArgs e)
    {
        var logPath = Path.Combine(AppContext.BaseDirectory, "Logs", "mii-renderer.log");
        if (File.Exists(logPath))
        {
            OpenFileLocation(logPath);
        }
        else
        {
            ShowCustomDialog("Renderer log", "No renderer log has been created yet.", MessageBoxButton.OK);
        }
    }

    private async Task<bool> TrySyncMiiToDolphinAsync(LauncherMiiProfile profile)
    {
        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath) || !Directory.Exists(settings.UserFolderPath))
        {
            return false;
        }

        try
        {
            await _saveManagerService.SyncMiiToDolphinAsync(settings, profile);
            return true;
        }
        catch (Exception ex)
        {
            ShowToast("Mii saved locally", ex.Message);
            return false;
        }
    }

    private void MiiDropZone_DragOver(object sender, WpfDragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(System.Windows.DataFormats.FileDrop)
            ? System.Windows.DragDropEffects.Copy
            : System.Windows.DragDropEffects.None;
        e.Handled = true;
    }

    private async void MiiDropZone_Drop(object sender, WpfDragEventArgs e)
    {
        if (!e.Data.GetDataPresent(System.Windows.DataFormats.FileDrop))
        {
            return;
        }

        var files = (string[]?)e.Data.GetData(System.Windows.DataFormats.FileDrop) ?? Array.Empty<string>();
        var imported = 0;
        foreach (var file in files.Where(MiiFileParserService.IsSupportedMiiFile))
        {
            try
            {
                var profile = await _saveManagerService.ImportMiiProfileAsync(file);
                await TrySyncMiiToDolphinAsync(profile);
                imported++;
            }
            catch (Exception ex)
            {
                ShowToast("Mii import skipped", ex.Message);
            }
        }

        if (imported > 0)
        {
            ShowToast("Mii import complete", $"{imported} Mii file(s) imported.");
            RefreshLicenseView();
        }
    }

    private void ModsDropZone_DragOver(object sender, WpfDragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(System.Windows.DataFormats.FileDrop) ? System.Windows.DragDropEffects.Copy : System.Windows.DragDropEffects.None;
        e.Handled = true;
    }

    private async void ModsDropZone_Drop(object sender, WpfDragEventArgs e)
    {
        if (!e.Data.GetDataPresent(System.Windows.DataFormats.FileDrop))
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog("Mod not installed", "Install VanzaKart before importing addons.", MessageBoxButton.OK);
            return;
        }

        var files = (string[])e.Data.GetData(System.Windows.DataFormats.FileDrop);
        var targetFolder = Path.Combine(settings.GetModFolder(), "VanzaKart", "VanzaKart", "My Stuff");
        Directory.CreateDirectory(targetFolder);

        try
        {
            foreach (var path in files)
            {
                if (Directory.Exists(path))
                {
                    CopyDirectory(path, Path.Combine(targetFolder, Path.GetFileName(path)), overwrite: true);
                }
                else if (File.Exists(path) && string.Equals(Path.GetExtension(path), ".zip", StringComparison.OrdinalIgnoreCase))
                {
                    await _archiveService.ValidateZipAsync(path);
                    await _archiveService.ExtractZipAsync(path, targetFolder);
                }
                else if (File.Exists(path))
                {
                    File.Copy(path, Path.Combine(targetFolder, Path.GetFileName(path)), overwrite: true);
                }
            }

            ShowToast("Addons imported", "Files were copied into the local My Stuff folder.");
            RefreshModsView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Import error", ex.Message, MessageBoxButton.OK);
        }
    }

    private void OpenWebsiteButton_OnClick(object sender, RoutedEventArgs e)
    {
        OpenUrl(LauncherConfig.DownloadPageUrl);
    }

    private void OpenDiscordBorder_OnClick(object sender, RoutedEventArgs e)
    {
        OpenUrl("https://discord.gg/4qPAQjt27j");
    }

    private void RefreshDebugButton_OnClick(object sender, RoutedEventArgs e)
    {
        RefreshAllState();
        ShowToast("Debug refreshed", "Local launcher state was refreshed.");
    }

    private void OpenSettingsFileButton_OnClick(object sender, RoutedEventArgs e)
    {
        OpenFileLocation(_settingsService.GetSettingsPath());
    }

    private void OpenPreferencesFileButton_OnClick(object sender, RoutedEventArgs e)
    {
        OpenFileLocation(_preferencesService.GetPreferencesPath());
    }



    private void AutoUpdateSetting_Changed(object sender, RoutedEventArgs e)
    {
        _userPreferences.AutoCheckUpdates = AutoUpdateCheckBox.IsChecked == true;
        _preferencesService.Save(_userPreferences);
    }

    private void SeparateSaveDefault_Changed(object sender, RoutedEventArgs e)
    {
        _userPreferences.SeparateSavegame = SeparateSaveDefaultCheckBox.IsChecked == true;
        _preferencesService.Save(_userPreferences);
        SeparateSaveCheckBox.IsChecked = _userPreferences.SeparateSavegame;
    }

    private void SeparateSave_Changed(object sender, RoutedEventArgs e)
    {
        _userPreferences.SeparateSavegame = SeparateSaveCheckBox.IsChecked == true;
        _preferencesService.Save(_userPreferences);
    }

    private void GraphicsTextures_Changed(object sender, RoutedEventArgs e)
    {
        _userPreferences.ModOptionChoice = GraphicsTexturesCheckBox.IsChecked == true ? 2 : 0;
        _preferencesService.Save(_userPreferences);
    }

    private void NewsSearchTextBox_TextChanged(object sender, TextChangedEventArgs e) => ApplyNewsFilter();

    private void LicenseSearchTextBox_TextChanged(object sender, TextChangedEventArgs e) => ApplyLicenseFilters();

    private void LicenseFilterComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e) => ApplyLicenseFilters();

    private void NewsFilterButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is WpfButton { Tag: string tag })
        {
            _newsFilter = tag;
            ApplyNewsFilter();
        }
    }

    private async void RefreshNewsButton_Click(object sender, RoutedEventArgs e)
    {
        var btn = sender as WpfButton;
        if (btn != null)
        {
            btn.IsEnabled = false;
        }

        try
        {
            await FetchNewsFromServerAsync();
            ShowToast("News updated", "The news feed has been refreshed successfully.");
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Update error", "Could not refresh news: " + ex.Message, MessageBoxButton.OK);
        }
        finally
        {
            if (btn != null)
            {
                btn.IsEnabled = true;
            }
        }
    }

    private void NewsVideo_Loaded(object sender, RoutedEventArgs e)
    {
        if (sender is MediaElement me)
        {
            me.Play();
        }
    }

    private void NewsVideo_MediaEnded(object sender, RoutedEventArgs e)
    {
        if (sender is MediaElement me)
        {
            me.Position = TimeSpan.Zero;
            me.Play();
        }
    }

    private void ApplyNewsFilter()
    {
        if (NewsItemsControl == null)
        {
            return;
        }

        var query = NewsSearchTextBox?.Text?.Trim() ?? string.Empty;
        var items = _allNews.Where(item =>
        {
            var filterMatch = _newsFilter switch
            {
                "Pinned" => item.IsPinned,
                "All" => true,
                _ => item.Category.Equals(_newsFilter, StringComparison.OrdinalIgnoreCase)
            };

            if (!filterMatch)
            {
                return false;
            }

            if (string.IsNullOrWhiteSpace(query))
            {
                return true;
            }

            return item.Title.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                   item.Summary.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                   item.Version.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                   item.Category.Contains(query, StringComparison.OrdinalIgnoreCase);
        });

        _visibleNews.Clear();
        foreach (var item in items)
        {
            _visibleNews.Add(item);
        }
    }

    private void SettingsSearchTextBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        var query = SettingsSearchTextBox.Text.Trim();
        SetCardVisibility(GamePathsSettingsCard, query, "paths dolphin rom user folder game");
        SetCardVisibility(LauncherSettingsCard, query, "launcher startup updates preferences");
        SetCardVisibility(GameSettingsCard, query, "game mod texture graphics grafiche save separate");
    }

    private static void SetCardVisibility(FrameworkElement card, string query, string keywords)
    {
        card.Visibility = string.IsNullOrWhiteSpace(query) ||
                          keywords.Contains(query, StringComparison.OrdinalIgnoreCase)
            ? Visibility.Visible
            : Visibility.Collapsed;
    }

    private void SeedNews()
    {
        _allNews.Clear();
        _allNews.AddRange(new[]
        {
            new NewsItem
            {
                Title = "VanzaKart Launcher UI/UX Revamp!",
                Category = "UPDATE",
                Version = $"Launcher v{LauncherConfig.CurrentLauncherVersion}",
                DateLabel = "Local",
                IsPinned = true,
                Summary = "# New UI/UX Revamp!\nWe are pleased to present the brand new look of the **VanzaKart** launcher.\n\n- **Smooth animations**: Modern transitions and backgrounds.\n- **Markdown & Media support**: You can now read formatted news and view gameplay videos or images directly in the feed!\n- *Go try the new features right now!*",
                MediaPath = "https://images.unsplash.com/photo-1551103782-8ab07afd45c1?w=800"
            },
            new NewsItem
            {
                Title = "Custom Tracks Gameplay Showcase",
                Category = "SHOWCASE",
                Version = "v6.7",
                DateLabel = "Live",
                IsPinned = false,
                Summary = "# Gameplay on the New Tracks\nHere is a brief video preview of one of the new tracks you will find in this version.\n\n- High-definition 3D models\n- Dynamic obstacles\n- Remastered soundtrack",
                MediaPath = "https://cripsum.com/vid/sossiogacha.mp4"
            },
            new NewsItem
            {
                Title = "Nauz BANNED for too many red shells",
                Category = "COMMUNITY",
                Version = "v1.0.0",
                DateLabel = "Local",
                IsPinned = false,
                Summary = "Nauz has been officially banned from the community for inappropriate use of red shells. **W sossio!**"
            }
        });
    }

    private async Task FetchNewsFromServerAsync()
    {
        try
        {
            var noCacheUrl = $"{LauncherConfig.NewsJsonUrl}?t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
            var json = await _networkService.DownloadStringAsync(noCacheUrl);
            var news = JsonSerializer.Deserialize<List<NewsItem>>(json);
            if (news != null && news.Count > 0)
            {
                var manifestNews = _allNews.Where(item => item.Category == "Manifest").ToList();
                _allNews.Clear();
                _allNews.AddRange(news);
                foreach (var item in manifestNews)
                {
                    _allNews.Insert(0, item);
                }
                ApplyNewsFilter();
            }
        }
        catch
        {
        }
    }

    private void MergeManifestNews(VersionInfo info)
    {
        var existingManifestItems = _allNews.Where(item => item.Category == "Manifest").ToArray();
        foreach (var item in existingManifestItems)
        {
            _allNews.Remove(item);
        }

        foreach (var change in info.Changelog.Take(4))
        {
            _allNews.Insert(0, new NewsItem
            {
                Title = change,
                Category = "Manifest",
                Version = string.IsNullOrWhiteSpace(info.ModVersion) ? "Mod" : $"Mod v{info.ModVersion}",
                DateLabel = "Live",
                IsPinned = false,
                Summary = "Loaded from the current update manifest."
            });
        }

        ApplyNewsFilter();
    }

    private void TrackGameSession(Process? process)
    {
        if (process == null)
        {
            _isGameRunning = false;
            SetBusy(_isBusy);
            return;
        }

        var sessionStart = DateTime.UtcNow;
        try
        {
            process.EnableRaisingEvents = true;
            process.Exited += (_, _) =>
            {
                Dispatcher.Invoke(() =>
                {
                    _isGameRunning = false;
                    var minutes = Math.Max(1, (DateTime.UtcNow - sessionStart).TotalMinutes);
                    _userPreferences.TotalPlayTimeMinutes += minutes;
                    _preferencesService.Save(_userPreferences);
                    RefreshPlayStats();
                    SetBusy(_isBusy);
                });
            };
        }
        catch
        {
            // Some shell-launched processes cannot be tracked reliably; launch still succeeds.
            _isGameRunning = false;
            SetBusy(_isBusy);
        }
    }

    private static void CopyDirectory(string sourceDir, string destinationDir, bool overwrite)
    {
        Directory.CreateDirectory(destinationDir);

        foreach (var file in Directory.EnumerateFiles(sourceDir))
        {
            File.Copy(file, Path.Combine(destinationDir, Path.GetFileName(file)), overwrite);
        }

        foreach (var directory in Directory.EnumerateDirectories(sourceDir))
        {
            CopyDirectory(directory, Path.Combine(destinationDir, Path.GetFileName(directory)), overwrite);
        }
    }

    private void ShowToast(string title, string message)
    {
        ToastTitleTextBlock.Text = title;
        ToastMessageTextBlock.Text = message;
        ToastBorder.Visibility = Visibility.Visible;
        ToastBorder.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(160)));

        _ = HideToastLaterAsync();
    }

    private async Task HideToastLaterAsync()
    {
        await Task.Delay(3200);
        var fade = new DoubleAnimation(1, 0, TimeSpan.FromMilliseconds(220));
        fade.Completed += (_, _) => ToastBorder.Visibility = Visibility.Collapsed;
        ToastBorder.BeginAnimation(OpacityProperty, fade);
    }

    private string? ShowTextInputDialog(string title, string label, string defaultValue)
    {
        var dialog = CreateSmallInputWindow(title, 390, 210);
        string? result = null;

        var stack = new StackPanel { Margin = new Thickness(22) };
        stack.Children.Add(CreateDialogTitle(title));
        stack.Children.Add(new TextBlock
        {
            Text = label,
            Foreground = new SolidColorBrush(WpfColor.FromRgb(0xA7, 0xB4, 0xCE)),
            Margin = new Thickness(0, 0, 0, 8)
        });

        var input = new System.Windows.Controls.TextBox
        {
            Text = defaultValue,
            Height = 36,
            Padding = new Thickness(10, 7, 10, 7),
            Background = new SolidColorBrush(WpfColor.FromRgb(0x0B, 0x10, 0x20)),
            Foreground = WpfBrushes.White,
            BorderBrush = new SolidColorBrush(WpfColor.FromRgb(0x35, 0x42, 0x62))
        };
        stack.Children.Add(input);
        stack.Children.Add(CreateDialogButtonRow(
            () =>
            {
                result = input.Text.Trim();
                dialog.Close();
            },
            dialog.Close));

        dialog.Content = stack;
        dialog.Loaded += (_, _) => input.Focus();
        dialog.ShowDialog();
        return result;
    }

    private (string name, string color)? ShowMiiProfileDialog()
    {
        var existing = _saveManagerService.LoadMiiProfile();
        var dialog = CreateSmallInputWindow("New Mii", 430, 280);
        (string name, string color)? result = null;

        var stack = new StackPanel { Margin = new Thickness(22) };
        stack.Children.Add(CreateDialogTitle("New Mii"));
        stack.Children.Add(new TextBlock
        {
            Text = "Mii name",
            Foreground = new SolidColorBrush(WpfColor.FromRgb(0xA7, 0xB4, 0xCE)),
            Margin = new Thickness(0, 0, 0, 8)
        });

        var nameBox = new System.Windows.Controls.TextBox
        {
            Text = existing.Name,
            Height = 36,
            Padding = new Thickness(10, 7, 10, 7),
            Background = new SolidColorBrush(WpfColor.FromRgb(0x0B, 0x10, 0x20)),
            Foreground = WpfBrushes.White,
            BorderBrush = new SolidColorBrush(WpfColor.FromRgb(0x35, 0x42, 0x62))
        };
        stack.Children.Add(nameBox);

        stack.Children.Add(new TextBlock
        {
            Text = "Favorite color",
            Foreground = new SolidColorBrush(WpfColor.FromRgb(0xA7, 0xB4, 0xCE)),
            Margin = new Thickness(0, 14, 0, 8)
        });

        var colorBox = new System.Windows.Controls.ComboBox
        {
            Height = 36,
            Background = new SolidColorBrush(WpfColor.FromRgb(0x0B, 0x10, 0x20)),
            Foreground = WpfBrushes.White,
            BorderBrush = new SolidColorBrush(WpfColor.FromRgb(0x35, 0x42, 0x62))
        };

        AddColorChoice(colorBox, "Cyan", "#39E7FF");
        AddColorChoice(colorBox, "Pink", "#FF3B7A");
        AddColorChoice(colorBox, "Purple", "#9D5CFF");
        AddColorChoice(colorBox, "Blue", "#5A6DFF");
        AddColorChoice(colorBox, "Green", "#4DFF8D");
        AddColorChoice(colorBox, "Yellow", "#FFD166");
        colorBox.SelectedIndex = Math.Max(0, colorBox.Items.Cast<System.Windows.Controls.ComboBoxItem>().ToList().FindIndex(item => Equals(item.Tag, existing.FavoriteColor)));
        stack.Children.Add(colorBox);

        stack.Children.Add(CreateDialogButtonRow(
            () =>
            {
                var selected = colorBox.SelectedItem as System.Windows.Controls.ComboBoxItem;
                result = (nameBox.Text.Trim(), selected?.Tag?.ToString() ?? "#39E7FF");
                dialog.Close();
            },
            dialog.Close));

        dialog.Content = stack;
        dialog.Loaded += (_, _) => nameBox.Focus();
        dialog.ShowDialog();
        return result;
    }

    private Window CreateSmallInputWindow(string title, double width, double height)
    {
        return new Window
        {
            Title = title,
            Owner = this,
            Width = width,
            Height = height,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            ResizeMode = ResizeMode.NoResize,
            WindowStyle = WindowStyle.None,
            Background = new SolidColorBrush(WpfColor.FromRgb(0x13, 0x1B, 0x2C)),
            Content = null
        };
    }

    private static TextBlock CreateDialogTitle(string title)
    {
        return new TextBlock
        {
            Text = title,
            FontSize = 20,
            FontWeight = FontWeights.Black,
            Foreground = WpfBrushes.White,
            Margin = new Thickness(0, 0, 0, 16)
        };
    }

    private static StackPanel CreateDialogButtonRow(Action confirm, Action cancel)
    {
        var row = new StackPanel
        {
            Orientation = System.Windows.Controls.Orientation.Horizontal,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Right,
            Margin = new Thickness(0, 18, 0, 0)
        };

        var ok = CreateSmallDialogButton("Create", true);
        ok.Click += (_, _) => confirm();
        var cancelButton = CreateSmallDialogButton("Cancel", false);
        cancelButton.Margin = new Thickness(10, 0, 0, 0);
        cancelButton.Click += (_, _) => cancel();

        row.Children.Add(ok);
        row.Children.Add(cancelButton);
        return row;
    }

    private static WpfButton CreateSmallDialogButton(string content, bool primary)
    {
        return new WpfButton
        {
            Content = content,
            MinWidth = 82,
            Height = 34,
            Padding = new Thickness(12, 0, 12, 0),
            FontWeight = FontWeights.Bold,
            Cursor = System.Windows.Input.Cursors.Hand,
            Foreground = WpfBrushes.White,
            Background = primary
                ? new LinearGradientBrush(WpfColor.FromRgb(0xFF, 0x3B, 0x7A), WpfColor.FromRgb(0x39, 0xE7, 0xFF), 0)
                : new SolidColorBrush(WpfColor.FromRgb(0x21, 0x2B, 0x43)),
            BorderBrush = new SolidColorBrush(WpfColor.FromRgb(0x43, 0x51, 0x70)),
            BorderThickness = new Thickness(1)
        };
    }

    private static void AddColorChoice(System.Windows.Controls.ComboBox comboBox, string label, string color)
    {
        comboBox.Items.Add(new System.Windows.Controls.ComboBoxItem
        {
            Content = label,
            Tag = color
        });
    }

    private static void OpenUrl(string url)
    {
        Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
    }

    private static void OpenFolder(string folder)
    {
        Process.Start(new ProcessStartInfo
        {
            FileName = "explorer.exe",
            Arguments = $"\"{folder}\"",
            UseShellExecute = true
        });
    }

    private static void OpenFileLocation(string file)
    {
        if (File.Exists(file))
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "explorer.exe",
                Arguments = $"/select,\"{file}\"",
                UseShellExecute = true
            });
            return;
        }

        var folder = Path.GetDirectoryName(file);
        if (!string.IsNullOrWhiteSpace(folder) && Directory.Exists(folder))
        {
            OpenFolder(folder);
        }
    }

    private static string EscapeJsonValue(string value) => value.Replace("\\", "\\\\").Replace("\"", "\\\"");

    private void AnimateEntrance()
    {
        Opacity = 0;
        var anim = new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(360))
        {
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
        };
        BeginAnimation(OpacityProperty, anim);
    }

    private void StartAmbientMotion()
    {
        var ease = new SineEase { EasingMode = EasingMode.EaseInOut };

        AmbientParticleTransform.BeginAnimation(TranslateTransform.XProperty, new DoubleAnimation(-18, 18, TimeSpan.FromSeconds(7.5))
        {
            AutoReverse = true,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = ease
        });
        AmbientParticleTransform.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(-8, 10, TimeSpan.FromSeconds(6.4))
        {
            AutoReverse = true,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = ease
        });

        HeroLogoTransform.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(-8, 8, TimeSpan.FromSeconds(3.6))
        {
            AutoReverse = true,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = ease
        });

        AmbientStreakTransformA.BeginAnimation(TranslateTransform.XProperty, CreateStreakAnimation(-80, 120, 4.8));
        AmbientStreakTransformB.BeginAnimation(TranslateTransform.XProperty, CreateStreakAnimation(70, -120, 5.7));
        AmbientStreakTransformC.BeginAnimation(TranslateTransform.XProperty, CreateStreakAnimation(-40, 90, 6.2));
    }

    private static DoubleAnimation CreateStreakAnimation(double from, double to, double seconds)
    {
        return new DoubleAnimation(from, to, TimeSpan.FromSeconds(seconds))
        {
            AutoReverse = true,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut }
        };
    }

    private static string FormatBytes(long bytes)
    {
        if (bytes <= 0)
        {
            return "0 B";
        }

        string[] units = { "B", "KB", "MB", "GB" };
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
        if (duration.TotalMinutes < 1)
        {
            return "0 min";
        }

        if (duration.TotalHours < 1)
        {
            return $"{duration.TotalMinutes:0} min";
        }

        return $"{duration.TotalHours:0.#} h";
    }

    private static string SanitizeFileName(string value)
    {
        var invalid = Path.GetInvalidFileNameChars();
        var cleaned = new string(value.Select(ch => invalid.Contains(ch) ? '_' : ch).ToArray());
        return string.IsNullOrWhiteSpace(cleaned) ? "mii" : cleaned;
    }

    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        SaveWindowBounds();
        base.OnClosing(e);
    }
}

public sealed class CustomDialog : Window
{
    private MessageBoxResult _result = MessageBoxResult.None;
    private readonly MessageBoxButton _buttons;
    private readonly Border root;

    public CustomDialog(string title, string message, MessageBoxButton buttons)
    {
        _buttons = buttons;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        Width = 560;
        Height = 330;
        ResizeMode = ResizeMode.NoResize;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        Background = WpfBrushes.Transparent;
        Topmost = true;
        Focusable = true;

        var rotateTransform = new RotateTransform(0, 0.5, 0.5);
        var borderBrush = new LinearGradientBrush
        {
            StartPoint = new System.Windows.Point(0, 0),
            EndPoint = new System.Windows.Point(1, 0),
            RelativeTransform = rotateTransform,
            GradientStops =
            {
                new GradientStop(WpfColor.FromRgb(0xFF, 0x00, 0x66), 0.00),
                new GradientStop(WpfColor.FromRgb(0xFF, 0x88, 0x00), 0.18),
                new GradientStop(WpfColor.FromRgb(0xFF, 0xEA, 0x00), 0.34),
                new GradientStop(WpfColor.FromRgb(0x00, 0xFF, 0x66), 0.50),
                new GradientStop(WpfColor.FromRgb(0x00, 0xF2, 0xFF), 0.67),
                new GradientStop(WpfColor.FromRgb(0x33, 0x00, 0xFF), 0.84),
                new GradientStop(WpfColor.FromRgb(0xB0, 0x00, 0xFF), 1.00)
            }
        };

        var rotateAnim = new DoubleAnimation(0, 360, TimeSpan.FromSeconds(6))
        {
            RepeatBehavior = RepeatBehavior.Forever
        };
        rotateTransform.BeginAnimation(RotateTransform.AngleProperty, rotateAnim);

        var translateTransform = new TranslateTransform(0, 20);

        root = new Border
        {
            Width = 460,
            Height = 230,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Center,
            VerticalAlignment = System.Windows.VerticalAlignment.Center,
            Opacity = 0,
            Padding = new Thickness(24),
            CornerRadius = new CornerRadius(16),
            Background = new SolidColorBrush(WpfColor.FromRgb(0x11, 0x18, 0x27)),
            BorderBrush = borderBrush,
            BorderThickness = new Thickness(1.8),
            RenderTransformOrigin = new System.Windows.Point(0.5, 0.5),
            RenderTransform = translateTransform,
            Effect = new System.Windows.Media.Effects.DropShadowEffect
            {
                BlurRadius = 36,
                ShadowDepth = 0,
                Opacity = 0.75,
                Color = WpfColor.FromRgb(0x00, 0xF2, 0xFF)
            }
        };

        var stack = new StackPanel();
        stack.Children.Add(new TextBlock
        {
            Text = title,
            FontSize = 20,
            FontWeight = FontWeights.Black,
            Foreground = WpfBrushes.White,
            Margin = new Thickness(0, 0, 0, 12),
            FontFamily = new FontFamily("Segoe UI")
        });
        stack.Children.Add(new TextBlock
        {
            Text = message,
            FontSize = 14,
            Foreground = new SolidColorBrush(WpfColor.FromRgb(0xA7, 0xB4, 0xCE)),
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(0, 0, 0, 24),
            FontFamily = new FontFamily("Segoe UI")
        });

        var buttonPanel = new StackPanel
        {
            Orientation = System.Windows.Controls.Orientation.Horizontal,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Right
        };

        var primaryButton = CreateDialogButton(buttons == MessageBoxButton.YesNo ? "Yes" : "OK", true);
        primaryButton.Click += (_, _) =>
        {
            _result = buttons == MessageBoxButton.YesNo ? MessageBoxResult.Yes : MessageBoxResult.OK;
            Close();
        };
        buttonPanel.Children.Add(primaryButton);

        if (buttons == MessageBoxButton.YesNo)
        {
            var secondaryButton = CreateDialogButton("No", false);
            secondaryButton.Margin = new Thickness(10, 0, 0, 0);
            secondaryButton.Click += (_, _) =>
            {
                _result = MessageBoxResult.No;
                Close();
            };
            buttonPanel.Children.Add(secondaryButton);
        }

        stack.Children.Add(buttonPanel);
        root.Child = stack;
        var container = new Grid { Background = WpfBrushes.Transparent };
        container.Children.Add(root);
        Content = container;
        
        Loaded += (_, _) =>
        {
            Focus();
            var duration = TimeSpan.FromMilliseconds(200);
            var ease = new CubicEase { EasingMode = EasingMode.EaseOut };

            var opacityAnim = new DoubleAnimation(0, 1, duration) { EasingFunction = ease };
            var translateYAnim = new DoubleAnimation(20, 0, duration) { EasingFunction = ease };

            root.BeginAnimation(UIElement.OpacityProperty, opacityAnim);
            translateTransform.BeginAnimation(TranslateTransform.YProperty, translateYAnim);
        };
    }

    private static WpfButton CreateDialogButton(string content, bool primary)
    {
        var btn = new WpfButton
        {
            Content = content,
            MinWidth = 96,
            Height = 38,
            Padding = new Thickness(16, 0, 16, 0),
            FontWeight = FontWeights.Bold,
            Cursor = System.Windows.Input.Cursors.Hand,
            Foreground = WpfBrushes.White,
            Background = primary
                ? new SolidColorBrush(WpfColor.FromRgb(0x15, 0x1E, 0x33))
                : new SolidColorBrush(WpfColor.FromRgb(0x1B, 0x26, 0x40)),
            BorderBrush = primary 
                ? new LinearGradientBrush
                {
                    StartPoint = new System.Windows.Point(0, 0),
                    EndPoint = new System.Windows.Point(1, 0),
                    GradientStops =
                    {
                        new GradientStop(WpfColor.FromRgb(0xFF, 0x00, 0x66), 0.00),
                        new GradientStop(WpfColor.FromRgb(0xFF, 0x88, 0x00), 0.18),
                        new GradientStop(WpfColor.FromRgb(0xFF, 0xEA, 0x00), 0.34),
                        new GradientStop(WpfColor.FromRgb(0x00, 0xFF, 0x66), 0.50),
                        new GradientStop(WpfColor.FromRgb(0x00, 0xF2, 0xFF), 0.67),
                        new GradientStop(WpfColor.FromRgb(0x33, 0x00, 0xFF), 0.84),
                        new GradientStop(WpfColor.FromRgb(0xB0, 0x00, 0xFF), 1.00)
                    }
                }
                : new SolidColorBrush(WpfColor.FromRgb(0x33, 0x40, 0x5D)),
            BorderThickness = new Thickness(primary ? 1.8 : 1),
            RenderTransformOrigin = new System.Windows.Point(0.5, 0.5),
            RenderTransform = new ScaleTransform(1, 1)
        };

        var template = new ControlTemplate(typeof(WpfButton));
        
        var gridFactory = new FrameworkElementFactory(typeof(Grid));
        gridFactory.SetValue(Grid.MarginProperty, new Thickness(2));

        if (primary)
        {
            var glowBorderFactory = new FrameworkElementFactory(typeof(Border));
            glowBorderFactory.Name = "GlowBorder";
            glowBorderFactory.SetValue(Border.CornerRadiusProperty, new CornerRadius(10));
            glowBorderFactory.SetValue(Border.MarginProperty, new Thickness(-4));
            glowBorderFactory.SetValue(Border.OpacityProperty, 0.25);
            
            var glowRainbow = new LinearGradientBrush
            {
                StartPoint = new System.Windows.Point(0, 0),
                EndPoint = new System.Windows.Point(1, 0),
                GradientStops =
                {
                    new GradientStop(WpfColor.FromRgb(0xFF, 0x00, 0x66), 0.00),
                    new GradientStop(WpfColor.FromRgb(0xFF, 0x88, 0x00), 0.18),
                    new GradientStop(WpfColor.FromRgb(0xFF, 0xEA, 0x00), 0.34),
                    new GradientStop(WpfColor.FromRgb(0x00, 0xFF, 0x66), 0.50),
                    new GradientStop(WpfColor.FromRgb(0x00, 0xF2, 0xFF), 0.67),
                    new GradientStop(WpfColor.FromRgb(0x33, 0x00, 0xFF), 0.84),
                    new GradientStop(WpfColor.FromRgb(0xB0, 0x00, 0xFF), 1.00)
                }
            };
            glowBorderFactory.SetValue(Border.BackgroundProperty, glowRainbow);
            
            var blur = new System.Windows.Media.Effects.BlurEffect { Radius = 10 };
            glowBorderFactory.SetValue(UIElement.EffectProperty, blur);
            
            gridFactory.AppendChild(glowBorderFactory);
        }

        var cardBorderFactory = new FrameworkElementFactory(typeof(Border));
        cardBorderFactory.Name = "CardBorder";
        cardBorderFactory.SetValue(Border.BackgroundProperty, new TemplateBindingExtension(WpfButton.BackgroundProperty));
        cardBorderFactory.SetValue(Border.BorderBrushProperty, new TemplateBindingExtension(WpfButton.BorderBrushProperty));
        cardBorderFactory.SetValue(Border.BorderThicknessProperty, new TemplateBindingExtension(WpfButton.BorderThicknessProperty));
        cardBorderFactory.SetValue(Border.CornerRadiusProperty, new CornerRadius(8));

        var presenterFactory = new FrameworkElementFactory(typeof(ContentPresenter));
        presenterFactory.SetValue(ContentPresenter.HorizontalAlignmentProperty, System.Windows.HorizontalAlignment.Center);
        presenterFactory.SetValue(ContentPresenter.VerticalAlignmentProperty, System.Windows.VerticalAlignment.Center);
        presenterFactory.SetValue(ContentPresenter.MarginProperty, new TemplateBindingExtension(WpfButton.PaddingProperty));
        
        if (primary)
        {
            var textShadow = new System.Windows.Media.Effects.DropShadowEffect
            {
                BlurRadius = 2,
                ShadowDepth = 1,
                Direction = 315,
                Opacity = 0.6,
                Color = WpfColor.FromRgb(0, 0, 0)
            };
            presenterFactory.SetValue(UIElement.EffectProperty, textShadow);
        }

        cardBorderFactory.AppendChild(presenterFactory);
        gridFactory.AppendChild(cardBorderFactory);
        
        template.VisualTree = gridFactory;
        btn.Template = template;

        btn.MouseEnter += (s, e) =>
        {
            var scale = btn.RenderTransform as ScaleTransform;
            if (scale != null)
            {
                var duration = TimeSpan.FromMilliseconds(120);
                var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
                scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(1.04, duration) { EasingFunction = ease });
                scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(1.04, duration) { EasingFunction = ease });
            }

            if (primary)
            {
                var bgBrush = btn.Background as SolidColorBrush;
                if (bgBrush != null && !bgBrush.IsFrozen)
                {
                    var bgAnim = new ColorAnimation(WpfColor.FromRgb(0x1F, 0x2C, 0x4C), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    };
                    bgBrush.BeginAnimation(SolidColorBrush.ColorProperty, bgAnim);
                }
                
                var glowBorder = btn.Template.FindName("GlowBorder", btn) as Border;
                if (glowBorder != null)
                {
                    var glowAnim = new DoubleAnimation(0.65, TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    };
                    glowBorder.BeginAnimation(UIElement.OpacityProperty, glowAnim);
                }
            }
            else
            {
                var bgBrush = btn.Background as SolidColorBrush;
                if (bgBrush != null && !bgBrush.IsFrozen)
                {
                    bgBrush.BeginAnimation(SolidColorBrush.ColorProperty, new ColorAnimation(WpfColor.FromRgb(0x25, 0x35, 0x5C), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    });
                }
                var borderBrush = btn.BorderBrush as SolidColorBrush;
                if (borderBrush != null && !borderBrush.IsFrozen)
                {
                    borderBrush.BeginAnimation(SolidColorBrush.ColorProperty, new ColorAnimation(WpfColor.FromRgb(0x4A, 0x5E, 0x8C), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    });
                }
            }
        };

        btn.MouseLeave += (s, e) =>
        {
            var scale = btn.RenderTransform as ScaleTransform;
            if (scale != null)
            {
                var duration = TimeSpan.FromMilliseconds(120);
                var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
                scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(1.0, duration) { EasingFunction = ease });
                scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(1.0, duration) { EasingFunction = ease });
            }

            if (primary)
            {
                var bgBrush = btn.Background as SolidColorBrush;
                if (bgBrush != null && !bgBrush.IsFrozen)
                {
                    var bgAnim = new ColorAnimation(WpfColor.FromRgb(0x15, 0x1E, 0x33), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    };
                    bgBrush.BeginAnimation(SolidColorBrush.ColorProperty, bgAnim);
                }
                
                var glowBorder = btn.Template.FindName("GlowBorder", btn) as Border;
                if (glowBorder != null)
                {
                    var glowAnim = new DoubleAnimation(0.25, TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    };
                    glowBorder.BeginAnimation(UIElement.OpacityProperty, glowAnim);
                }
            }
            else
            {
                var bgBrush = btn.Background as SolidColorBrush;
                if (bgBrush != null && !bgBrush.IsFrozen)
                {
                    bgBrush.BeginAnimation(SolidColorBrush.ColorProperty, new ColorAnimation(WpfColor.FromRgb(0x1B, 0x26, 0x40), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    });
                }
                var borderBrush = btn.BorderBrush as SolidColorBrush;
                if (borderBrush != null && !borderBrush.IsFrozen)
                {
                    borderBrush.BeginAnimation(SolidColorBrush.ColorProperty, new ColorAnimation(WpfColor.FromRgb(0x33, 0x40, 0x5D), TimeSpan.FromMilliseconds(150))
                    {
                        EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                    });
                }
            }
        };

        btn.PreviewMouseDown += (s, e) =>
        {
            var scale = btn.RenderTransform as ScaleTransform;
            if (scale != null)
            {
                scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(0.95, TimeSpan.FromMilliseconds(60)));
                scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(0.95, TimeSpan.FromMilliseconds(60)));
            }
        };

        btn.PreviewMouseUp += (s, e) =>
        {
            var scale = btn.RenderTransform as ScaleTransform;
            if (scale != null)
            {
                scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(1.0, TimeSpan.FromMilliseconds(80)));
                scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(1.0, TimeSpan.FromMilliseconds(80)));
            }
        };

        return btn;
    }

    private bool _isClosingAnimated = false;
    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        if (!_isClosingAnimated)
        {
            e.Cancel = true;
            _isClosingAnimated = true;
            
            var duration = TimeSpan.FromMilliseconds(150);
            var ease = new CubicEase { EasingMode = EasingMode.EaseIn };

            var opacityAnim = new DoubleAnimation(0, duration) { EasingFunction = ease };
            var translateYAnim = new DoubleAnimation(15, duration) { EasingFunction = ease };

            var translateTransform = root.RenderTransform as TranslateTransform;
            
            opacityAnim.Completed += (s, ev) => base.Close();
            
            root.BeginAnimation(UIElement.OpacityProperty, opacityAnim);
            if (translateTransform != null)
            {
                translateTransform.BeginAnimation(TranslateTransform.YProperty, translateYAnim);
            }
        }
        else
        {
            base.OnClosing(e);
        }
    }

    protected override void OnKeyDown(WpfKeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            _result = _buttons == MessageBoxButton.YesNo ? MessageBoxResult.Yes : MessageBoxResult.OK;
            Close();
        }
        else if (e.Key == Key.Escape && _result == MessageBoxResult.None)
        {
            _result = MessageBoxResult.No;
            Close();
        }

        base.OnKeyDown(e);
    }

    public new MessageBoxResult? ShowDialog()
    {
        base.ShowDialog();
        return _result;
    }
}
