using Microsoft.Win32;
using System.Collections.Concurrent;
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
    private readonly AddonManagerService _addonManagerService = new();
    private readonly MusicPackService _musicPackService;
    private readonly GameBananaService _gameBananaService;
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
    private readonly ObservableCollection<LauncherMiiProfile> _licenseMiiPickerItems = new();
    private readonly ObservableCollection<AddonInfo> _installedAddons = new();
    private readonly ObservableCollection<GameBananaMod> _gameBananaMods = new();
    private readonly Stopwatch _downloadStopwatch = new();
    private static readonly SemaphoreSlim UpdateLogSemaphore = new(1, 1);
    private const int DifferentialDownloadConcurrency = 4;
    private readonly ModUpdateSafetyService _modUpdateSafetyService = new();
    private readonly ModInstallationStateService _modInstallationStateService = new();
    private readonly BetaAccessService _betaAccessService = new();
    private readonly SemaphoreSlim _updateCheckLock = new(1, 1);
    private bool _isRefreshingMiis;
    private bool _isRenderingLicenseAvatars;
    private bool _isRenderingLauncherMiiAvatars;
    private bool _isInstallingMiiRuntime;
    private bool _isApplyingLicenseMii;
    private SaveProfileInfo? _pendingLicenseMiiTarget;
    private FileSystemWatcher? _dolphinFileWatcher;
    private FileSystemWatcher? _profileFileWatcher;
    private CancellationTokenSource? _filesystemRefreshCts;

    private UserPreferences _userPreferences;
    private ModInstallationState _installedModState;
    private bool _isLoadingReleaseChannel;
    private bool _isDownloadingLauncherUpdate = false;
    private string _latestLauncherVersion = string.Empty;

    private readonly string _tempZipPath = Path.Combine(AppContext.BaseDirectory, "mod_temp.zip");
    private readonly string _localModVersionFile = Path.Combine(AppContext.BaseDirectory, "mod_version.txt");
    private readonly string _localBetaModVersionFile = Path.Combine(AppContext.BaseDirectory, "mod_beta_version.txt");
    private readonly string _localMusicPackVersionFile = Path.Combine(AppContext.BaseDirectory, "musicpack_version.txt");
    private readonly string _localBetaMusicPackVersionFile = Path.Combine(AppContext.BaseDirectory, "musicpack_beta_version.txt");

    private string _latestModVersion = string.Empty;
    private string _latestModUrl = LauncherConfig.ModUrl;
    private string[] _latestModMirrors = Array.Empty<string>();
    private string _latestModSha256 = string.Empty;
    private string _latestModManifestUrl = LauncherConfig.ModManifestUrl;
    private string _latestModFilesUrl = LauncherConfig.ModFilesUrl;
    private string[] _latestModFilesMirrors = Array.Empty<string>();
    private string _latestModHashFilesUrl = LauncherConfig.ModHashFilesUrl;
    private string[] _latestModHashFilesMirrors = Array.Empty<string>();
    private string _latestMusicPackVersion = string.Empty;
    private string _latestMusicPackUrl = LauncherConfig.MusicPackUrl;
    private string[] _latestMusicPackMirrors = Array.Empty<string>();
    private string _latestMusicPackSha256 = string.Empty;
    private string[] _latestMusicPackChangelog = Array.Empty<string>();
    private string _latestMusicPackManifestUrl = LauncherConfig.MusicPackManifestUrl;
    private string _latestMusicPackFilesUrl = LauncherConfig.MusicPackFilesUrl;
    private string[] _latestMusicPackFilesMirrors = Array.Empty<string>();
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
    private bool _gameBananaLoaded;
    private int _gameBananaPage;
    private bool _gameBananaHasMore;
    private bool _isLoadingGameBanana;
    private CancellationTokenSource? _gameBananaSearchCts;
    private long _downloadBaselineBytes = -1;
    private long _lastDownloadSampleBytes;
    private TimeSpan _lastDownloadSampleTime;
    private double _smoothedDownloadBytesPerSecond;
    private int _releaseChannelRevision;
    private readonly DolphinSettingsManager _dolphinSettingsManager = new();


    private bool _isUpdatingLanguageUi;
    private string _currentHeaderTag = "Home";

    public MainWindow()
    {
        _userPreferences = _preferencesService.Load();
        ApplyStoredLanguage();
        if (!Enum.IsDefined(_userPreferences.ModReleaseChannel))
        {
            _userPreferences.ModReleaseChannel = ModReleaseChannel.Stable;
        }
        var legacyModVersion = File.Exists(_localModVersionFile) ? File.ReadAllText(_localModVersionFile).Trim() : string.Empty;
        _installedModState = _modInstallationStateService.Load(legacyModVersion);
        ConfigureModReleaseDefaults(SelectedModReleaseChannel);
        _gameBananaService = new GameBananaService(_networkService);
        _musicPackService = new MusicPackService(_networkService, _archiveService, _addonManagerService);

        _roomsViewModel = new RoomsViewModel(_networkService);
        _leaderboardViewModel = new LeaderboardViewModel(_networkService);
        _friendsViewModel = new FriendsViewModel(_networkService);

        // Verify if a mod update is required based on last known version from check
        var localVersion = GetInstalledModVersion();
        var lastKnownVersion = GetLastKnownVersionForSelectedChannel();
        var initialSettings = _settingsService.Load();
        if (!IsModInstalled(initialSettings, SelectedModReleaseChannel) ||
            (!string.IsNullOrWhiteSpace(lastKnownVersion) && lastKnownVersion != localVersion))
        {
            _isModUpdateRequired = true;
            _latestModVersion = lastKnownVersion;
        }

        InitializeComponent();

        MarioKartControllerPanel.UserFolderResolver = ResolveControllerUserFolder;
        MarioKartControllerPanel.ConfigurationChanged += (_, _) => _hasUnsavedChanges = true;
        MarioKartControllerPanel.StatusChanged += (_, message) => ShowSettingsStatusNotification(message);
        MarioKartControllerPanel.PlayRequested += (_, _) => LaunchButton_OnClick(MarioKartControllerPanel, new RoutedEventArgs());

        RoomsView.DataContext = _roomsViewModel;
        LeaderboardView.DataContext = _leaderboardViewModel;
        FriendsView.DataContext = _friendsViewModel;

        VersionBadgeTextBlock.Text = Loc.Format("Msg_LauncherV", LauncherConfig.CurrentLauncherVersion);
        PopulateLanguageComboBox();
        UpdateTeamVersionLabel();
        Loc.Service.LanguageChanged += (_, _) => OnLanguageChanged();
        DebugNavButton.Visibility = Debugger.IsAttached ? Visibility.Visible : Visibility.Collapsed;

        SeedNews();
        NewsItemsControl.ItemsSource = _visibleNews;
        LicenseCardsItemsControl.ItemsSource = _licenseCards;
        MiiCardsListBox.ItemsSource = _miiProfiles;
        LicenseMiiPickerListBox.ItemsSource = _licenseMiiPickerItems;
        InstalledAddonsItemsControl.ItemsSource = _installedAddons;
        GameBananaModsItemsControl.ItemsSource = _gameBananaMods;
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
            await ValidateSavedBetaTokenOnStartupAsync();
            await CheckForUpdatesAsync(showMessages: false);
        };

        _navigationService.Navigated += tab => NavigateTo(tab);
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
        _gameBananaSearchCts?.Cancel();
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
                ApplyPageHeader("News");
                ApplyNewsFilter();
                break;
            case "Rooms":
                view = RoomsView;
                ApplyPageHeader("Rooms");
                _roomsViewModel?.StartAutoRefresh();
                _ = _roomsViewModel?.RefreshAsync();
                break;
            case "Leaderboard":
                view = LeaderboardView;
                ApplyPageHeader("Leaderboard");
                _leaderboardViewModel?.UpdateLocalFriendCodes(_allLicenseCards.Select(c => c.FriendCode));
                _ = _leaderboardViewModel?.RefreshAsync();
                break;
            case "Mods":
                view = ModsView;
                ApplyPageHeader("Mods");
                RefreshModsView();
                break;
            case "Licenses":
                view = LicensesView;
                ApplyPageHeader("Licenses");
                RefreshLicenseView();
                break;
            case "Friends":
                view = FriendsView;
                ApplyPageHeader("Friends");
                _friendsViewModel?.LoadFriends();
                break;
            case "Settings":
                view = SettingsView;
                ApplyPageHeader("Settings");
                break;
            case "Debug":
                view = DebugView;
                ApplyPageHeader("Debug");
                RefreshDebugInfo();
                break;
            default:
                view = PlayView;
                ApplyPageHeader("Home");
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

    /// <summary>Writes the localized title/subtitle for a page into the header.</summary>
    private void ApplyPageHeader(string headerTag)
    {
        _currentHeaderTag = headerTag;
        PageTitleTextBlock.Text = L($"Nav_{headerTag}Title");
        PageSubtitleTextBlock.Text = L($"Nav_{headerTag}Subtitle");
    }

    private void UpdateNavigationLabels()
    {
        ApplyPageHeader(_currentHeaderTag);
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
        RefreshReleaseChannelUi();
        RefreshLicenseView();
        RefreshPlayStats();
        RefreshDebugInfo();
        ApplyNewsFilter();
    }

    private LauncherSettings BuildSettingsFromUi()
    {
        var settings = _settingsService.Load();
        settings.DolphinPath = DolphinPathTextBox.Text.Trim();
        settings.UserFolderPath = UserFolderTextBox.Text.Trim();
        settings.RomPath = RomPathTextBox.Text.Trim();
        return settings;
    }

    private void LoadSettingsIntoUi()
    {
        _isUpdatingDolphinUi = true;
        try
        {
            var settings = _settingsService.Load();
            var detectedUserFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            if (!string.IsNullOrWhiteSpace(detectedUserFolder) &&
                (!string.Equals(settings.UserFolderPath, detectedUserFolder, StringComparison.OrdinalIgnoreCase) || string.IsNullOrWhiteSpace(settings.UserFolderPath)))
            {
                settings.UserFolderPath = detectedUserFolder;
                _settingsService.Save(settings);
            }

            DolphinPathTextBox.Text = settings.DolphinPath;
            UserFolderTextBox.Text = settings.UserFolderPath;
            RomPathTextBox.Text = settings.RomPath;

            AutoUpdateCheckBox.IsChecked = _userPreferences.AutoCheckUpdates;
            SeparateSaveDefaultCheckBox.IsChecked = _userPreferences.SeparateSavegame;
            SeparateSaveCheckBox.IsChecked = _userPreferences.SeparateSavegame;
            GraphicsTexturesCheckBox.IsChecked = _userPreferences.ModOptionChoice == 2;
            _isLoadingReleaseChannel = true;
            ModReleaseChannelComboBox.SelectedIndex = SelectedModReleaseChannel == ModReleaseChannel.Beta ? 1 : 0;
            _isLoadingReleaseChannel = false;
            RefreshReleaseChannelUi();

            LoadDolphinSettingsIntoUi();
        }
        finally
        {
            _isUpdatingDolphinUi = false;
            _hasUnsavedChanges = false;
        }
    }

    private ModReleaseChannel SelectedModReleaseChannel => _userPreferences.ModReleaseChannel;

    private static string GetModDirectoryName(ModReleaseChannel channel) =>
        channel == ModReleaseChannel.Beta ? "VKBeta" : "VanzaKart";

    private static string GetModRoot(LauncherSettings settings, ModReleaseChannel channel) =>
        Path.Combine(settings.GetModFolder(), GetModDirectoryName(channel));

    private string GetInstalledModVersion() => GetInstalledModVersion(SelectedModReleaseChannel);

    private string GetInstalledModVersion(ModReleaseChannel channel)
    {
        var channelState = _installedModState.Get(channel);
        if (!string.IsNullOrWhiteSpace(channelState.Version))
        {
            return channelState.Version;
        }

        var versionFile = GetModVersionFile(channel);
        return File.Exists(versionFile) ? File.ReadAllText(versionFile).Trim() : "0.0";
    }

    private string GetModVersionFile(ModReleaseChannel channel) =>
        channel == ModReleaseChannel.Beta ? _localBetaModVersionFile : _localModVersionFile;

    private string GetMusicPackVersionFile(ModReleaseChannel channel) =>
        channel == ModReleaseChannel.Beta ? _localBetaMusicPackVersionFile : _localMusicPackVersionFile;

    private string GetLastKnownVersionForSelectedChannel() =>
        SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? _userPreferences.LastKnownLatestBetaModVersion
            : _userPreferences.LastKnownLatestModVersion;

    private bool IsChannelSwitchPending(LauncherSettings settings) =>
        !IsModInstalled(settings, SelectedModReleaseChannel) &&
        IsModInstalled(settings, SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? ModReleaseChannel.Stable
            : ModReleaseChannel.Beta);

    private static string GetChannelDisplayName(ModReleaseChannel channel) =>
        channel == ModReleaseChannel.Beta ? "Beta" : "Stable";

    private void ConfigureModReleaseDefaults(ModReleaseChannel channel)
    {
        if (channel == ModReleaseChannel.Beta)
        {
            _latestModUrl = LauncherConfig.BetaModUrl;
            _latestModManifestUrl = LauncherConfig.BetaModManifestUrl;
            _latestModFilesUrl = LauncherConfig.BetaModFilesUrl;
            _latestModHashFilesUrl = LauncherConfig.BetaModHashFilesUrl;
        }
        else
        {
            _latestModUrl = LauncherConfig.ModUrl;
            _latestModManifestUrl = LauncherConfig.ModManifestUrl;
            _latestModFilesUrl = LauncherConfig.ModFilesUrl;
            _latestModHashFilesUrl = LauncherConfig.ModHashFilesUrl;
        }

        _latestModMirrors = Array.Empty<string>();
        _latestModFilesMirrors = Array.Empty<string>();
        _latestModHashFilesMirrors = Array.Empty<string>();
        _latestModSha256 = string.Empty;
    }

    private void RefreshReleaseChannelUi()
    {
        if (ReleaseChannelTitleTextBlock == null)
        {
            return;
        }

        var selectedName = GetChannelDisplayName(SelectedModReleaseChannel);
        ReleaseChannelTitleTextBlock.Text = Loc.Format("Msg_Channel", selectedName);
        ReleaseChannelDescriptionTextBlock.Text = SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? L("Msg_PreviewBuildsInstalledSeparately")
            : L("Msg_RecommendedBuildsInstalledSeparately");
        var settings = BuildSettingsFromUi();
        var installedChannels = new[] { ModReleaseChannel.Stable, ModReleaseChannel.Beta }
            .Where(channel => IsModInstalled(settings, channel))
            .Select(channel => $"{GetChannelDisplayName(channel)} {GetInstalledModVersion(channel)}")
            .ToArray();
        InstalledReleaseChannelTextBlock.Text = installedChannels.Length > 0
            ? Loc.Format("Msg_Installed2", string.Join(" • ", installedChannels))
            : L("Msg_InstalledNone");
        ReleaseChannelSettingsCard.BorderBrush = new SolidColorBrush((WpfColor)ColorConverter.ConvertFromString(
            SelectedModReleaseChannel == ModReleaseChannel.Beta ? "#FF9F43" : "#397FB9"));
        ModReleaseChannelComboBox.IsEnabled = !_isBusy;

        if (ManageBetaTokenButton != null)
        {
            ManageBetaTokenButton.Visibility = SelectedModReleaseChannel == ModReleaseChannel.Beta ? Visibility.Visible : Visibility.Collapsed;
        }
    }

    private async Task ValidateSavedBetaTokenOnStartupAsync()
    {
        if (_userPreferences.ModReleaseChannel != ModReleaseChannel.Beta)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(_userPreferences.BetaAccessToken))
        {
            _userPreferences.ModReleaseChannel = ModReleaseChannel.Stable;
            _preferencesService.Save(_userPreferences);
            ConfigureModReleaseDefaults(ModReleaseChannel.Stable);
            RefreshReleaseChannelUi();
            RefreshAllState();
            return;
        }

        var result = await _betaAccessService.VerifyTokenAsync(_userPreferences.BetaAccessToken);
        if (!result.Success && !result.IsNetworkOrServerError)
        {
            _userPreferences.BetaAccessToken = string.Empty;
            _userPreferences.ModReleaseChannel = ModReleaseChannel.Stable;
            _preferencesService.Save(_userPreferences);
            ConfigureModReleaseDefaults(ModReleaseChannel.Stable);
            RefreshReleaseChannelUi();
            RefreshAllState();

            ShowCustomDialog(
                L("Msg_BetaAccessRevoked"),
                L("Msg_YourBetaAccessTokenIsNoLonger"),
                MessageBoxButton.OK);
        }
    }

    private Task<bool> PromptBetaTokenIfNeededAsync(bool forcePrompt = false)
    {
        if (!forcePrompt && !string.IsNullOrWhiteSpace(_userPreferences.BetaAccessToken))
        {
            return Task.FromResult(true);
        }

        var dialog = new BetaTokenDialog(_userPreferences.BetaAccessToken)
        {
            Owner = this
        };

        var result = dialog.ShowDialog();
        if (result == true && !string.IsNullOrWhiteSpace(dialog.VerifiedToken))
        {
            _userPreferences.BetaAccessToken = dialog.VerifiedToken;
            _preferencesService.Save(_userPreferences);
            return Task.FromResult(true);
        }

        return Task.FromResult(false);
    }

    private async void ManageBetaTokenButton_OnClick(object sender, RoutedEventArgs e)
    {
        var updated = await PromptBetaTokenIfNeededAsync(forcePrompt: true);
        if (updated)
        {
            ShowToast(L("Msg_BetaTokenUpdated"), L("Msg_YourAccessTokenHasBeenUpdated"));
        }
    }

    private async void ModReleaseChannelComboBox_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isLoadingReleaseChannel || ModReleaseChannelComboBox.SelectedItem is not ComboBoxItem selectedItem)
        {
            return;
        }

        if (!Enum.TryParse<ModReleaseChannel>(selectedItem.Tag?.ToString(), true, out var requestedChannel) ||
            requestedChannel == SelectedModReleaseChannel)
        {
            return;
        }

        if (_isBusy)
        {
            RestoreReleaseChannelSelection();
            return;
        }

        if (requestedChannel == ModReleaseChannel.Beta)
        {
            var betaUnlocked = await PromptBetaTokenIfNeededAsync();
            if (!betaUnlocked)
            {
                RestoreReleaseChannelSelection();
                return;
            }
        }

        var message = L(requestedChannel == ModReleaseChannel.Beta ? "Msg_JoinBetaChannel" : "Msg_ReturnToStableChannel");

        if (ShowCustomDialog(L("Msg_ChangeModpackChannel"), message, MessageBoxButton.YesNo) != MessageBoxResult.Yes)
        {
            RestoreReleaseChannelSelection();
            return;
        }

        _userPreferences.ModReleaseChannel = requestedChannel;
        _releaseChannelRevision++;
        _preferencesService.Save(_userPreferences);
        ConfigureModReleaseDefaults(requestedChannel);
        _latestModVersion = string.Empty;
        _latestModSha256 = string.Empty;
        _lastUpdateError = string.Empty;
        var settingsAfterSwitch = BuildSettingsFromUi();
        var requestedInstalled = IsModInstalled(settingsAfterSwitch, requestedChannel);
        var requestedVersion = GetInstalledModVersion(requestedChannel);
        var requestedLatest = GetLastKnownVersionForSelectedChannel();
        _isModUpdateRequired = !requestedInstalled ||
            (!string.IsNullOrWhiteSpace(requestedLatest) && requestedLatest != requestedVersion);
        RefreshReleaseChannelUi();
        RefreshAllState();

        await CheckForUpdatesAsync(showMessages: false);

        if (requestedChannel != SelectedModReleaseChannel)
        {
            return;
        }

        var ready = IsModInstalled(BuildSettingsFromUi(), requestedChannel) && !_isModUpdateRequired;
        ShowToast(
            Loc.Format("Msg_Selected", GetChannelDisplayName(requestedChannel)),
            ready
                ? Loc.Format("Msg_IsAlreadyInstalledAndReadyTo", GetModDirectoryName(requestedChannel))
                : Loc.Format("Msg_InstallOrUpdateFromModsBefore", GetModDirectoryName(requestedChannel)));
    }

    private void RestoreReleaseChannelSelection()
    {
        _isLoadingReleaseChannel = true;
        ModReleaseChannelComboBox.SelectedIndex = SelectedModReleaseChannel == ModReleaseChannel.Beta ? 1 : 0;
        _isLoadingReleaseChannel = false;
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
            var pendingText = IsChannelSwitchPending(settings)
                ? Loc.Format("Msg_SwitchToChannelRequired", GetChannelDisplayName(SelectedModReleaseChannel))
                : Loc.Format("Msg_ModUpdateAvailableV", _latestModVersion);
            SetStatus(pendingText, (WpfBrush)FindResource("WarningBrush"));
            return;
        }

        if (IsModInstalled(settings))
        {
            SetStatus(L("Msg_ModInstalledAndReady"), (WpfBrush)FindResource("SuccessBrush"));
        }
        else
        {
            SetStatus(L("Msg_SetupRequiredInstallTheMod"), (WpfBrush)FindResource("WarningBrush"));
        }
    }

    private bool IsModInstalled(LauncherSettings settings) => IsModInstalled(settings, SelectedModReleaseChannel);

    private static bool IsModInstalled(LauncherSettings settings, ModReleaseChannel channel)
    {
        var modDirectoryName = GetModDirectoryName(channel);
        var xmlPath = Path.Combine(settings.GetModFolder(), modDirectoryName, "Riivolution", $"{modDirectoryName}.xml");
        return File.Exists(xmlPath);
    }

    private void RefreshModsView()
    {
        var settings = BuildSettingsFromUi();
        var installed = IsModInstalled(settings);
        var localVersion = installed ? GetInstalledModVersion() : L("Mods_NotInstalled");
        var switchPending = IsChannelSwitchPending(settings);
        var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var myStuffFolder = Path.Combine(settings.GetModFolder(), modDirectoryName, modDirectoryName, "My Stuff");
        var conflicts = _modConflictService.ScanAddonConflicts(myStuffFolder);

        InstalledVersionText.Text = localVersion;
        LatestVersionText.Text = string.IsNullOrEmpty(_latestModVersion) ? L("Play_Unknown") : _latestModVersion;
        CoreModStatusTextBlock.Text = switchPending
            ? Loc.Format("Msg_IsNotInstalledYetTheOtherChannel", modDirectoryName)
            : installed
            ? Loc.Format("Msg_Installed", GetChannelDisplayName(SelectedModReleaseChannel), localVersion)
            : L("Msg_CoreModpackIsNotInstalledYet");
        AddonFolderTextBlock.Text = Directory.Exists(myStuffFolder)
            ? myStuffFolder
            : L("Msg_MyStuffFolderWillBeCreatedAfter");
        CompatibilityTextBlock.Text = installed
            ? L("Msg_CoreFilesDetectedAddonsCanBe")
            : L("Msg_InstallTheCoreModBeforeImporting");
        VersioningTextBlock.Text = string.IsNullOrEmpty(_latestModVersion)
            ? L("Msg_WaitingForManifest")
            : Loc.Format("Msg_ManifestLatestV", GetChannelDisplayName(SelectedModReleaseChannel), _latestModVersion);
        ModChannelBadgeTextBlock.Text = GetChannelDisplayName(SelectedModReleaseChannel).ToUpperInvariant();
        ModChannelBadgeBorder.Background = new SolidColorBrush((WpfColor)ColorConverter.ConvertFromString(
            SelectedModReleaseChannel == ModReleaseChannel.Beta ? "#4A2B18" : "#163754"));
        ModChannelBadgeBorder.BorderBrush = new SolidColorBrush((WpfColor)ColorConverter.ConvertFromString(
            SelectedModReleaseChannel == ModReleaseChannel.Beta ? "#FF9F43" : "#397FB9"));
        InstallButton.Content = switchPending
            ? Loc.Format("Msg_Install", modDirectoryName)
            : _isModUpdateRequired ? L("Btn_Update") : installed ? L("Btn_Reinstall") : L("Btn_Install");
        ModConflictTextBlock.Text = conflicts.Count == 0
            ? L("Msg_NoAddonConflictsDetected")
            : $"{conflicts.Count} conflict(s): {string.Join(", ", conflicts.Take(3).Select(conflict => conflict.FileName))}";
        ModConflictTextBlock.Foreground = conflicts.Count == 0
            ? (WpfBrush)FindResource("TextFaint")
            : (WpfBrush)FindResource("WarningBrush");
        RefreshMusicPackCard(settings, installed);
        RefreshInstalledAddons();
        RefreshHomeUpdateCard();
    }

    private void RefreshMusicPackCard(LauncherSettings settings, bool coreInstalled)
    {
        var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var installedPack = _musicPackService.GetInstalled(settings, modDirectoryName);
        var packInstalled = installedPack != null;
        var musicPackVersionFile = GetMusicPackVersionFile(SelectedModReleaseChannel);
        var localVersion = packInstalled && File.Exists(musicPackVersionFile)
            ? File.ReadAllText(musicPackVersionFile).Trim()
            : packInstalled ? L("Play_Unknown") : L("Mods_NotInstalled");
        var latestVersion = string.IsNullOrWhiteSpace(_latestMusicPackVersion) ? L("Play_Unknown") : _latestMusicPackVersion;
        var updateAvailable = packInstalled && !string.IsNullOrWhiteSpace(_latestMusicPackVersion) &&
                              !string.Equals(localVersion, _latestMusicPackVersion, StringComparison.OrdinalIgnoreCase);

        InstalledMusicPackVersionText.Text = localVersion;
        LatestMusicPackVersionText.Text = latestVersion;
        MusicPackInstallButton.Content = updateAvailable ? L("Btn_Update") : packInstalled ? L("Btn_Reinstall") : L("Btn_Install");
        MusicPackInstallButton.IsEnabled = !_isBusy && coreInstalled && !string.IsNullOrWhiteSpace(_latestMusicPackUrl);
        MusicPackRemoveButton.IsEnabled = !_isBusy && packInstalled;
        MusicPackEnabledCheckBox.IsChecked = installedPack?.IsEnabled == true;
        MusicPackEnabledCheckBox.IsEnabled = !_isBusy && packInstalled;
        MusicPackEnabledCheckBox.Content = installedPack?.IsEnabled == true ? L("Msg_Enabled") : L("Msg_Disabled");
        MusicPackStatusTextBlock.Text = !coreInstalled
            ? L("Msg_InstallTheVanzakartModpackBefore3")
            : updateAvailable
                ? Loc.Format("Msg_UpdateAvailable", localVersion, _latestMusicPackVersion)
                : packInstalled
                    ? installedPack?.IsEnabled == true
                        ? Loc.Format("Msg_OfficialMusicPackIsInstalled", localVersion)
                        : Loc.Format("Msg_OfficialMusicPackIsInstalled2", localVersion)
                    : L("Msg_OptionalOfficialPackageItIsInstalled");
        MusicPackStatusTextBlock.Foreground = updateAvailable
            ? (WpfBrush)FindResource("WarningBrush")
            : packInstalled ? (WpfBrush)FindResource("SuccessBrush") : (WpfBrush)FindResource("TextSecondary");
    }

    private void RefreshInstalledAddons()
    {
        _installedAddons.Clear();
        foreach (var addon in _addonManagerService.Load(BuildSettingsFromUi(), GetModDirectoryName(SelectedModReleaseChannel))
                     .Where(addon => !addon.Id.Equals(AddonManagerService.OfficialMusicPackId, StringComparison.OrdinalIgnoreCase)))
        {
            _installedAddons.Add(addon);
        }
        InstalledAddonsEmptyTextBlock.Visibility = _installedAddons.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void RefreshHomeUpdateCard()
    {
        if (HomeUpdateTitleTextBlock == null)
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        var installed = IsModInstalled(settings);
        var localVersion = installed ? GetInstalledModVersion() : L("Mods_NotInstalled");
        var latest = string.IsNullOrWhiteSpace(_latestModVersion) ? L("Play_Unknown") : _latestModVersion;

        if (_isDownloadingLauncherUpdate && !string.IsNullOrWhiteSpace(_latestLauncherVersion))
        {
            var instVer = LauncherConfig.CurrentLauncherVersion.StartsWith("v") ? LauncherConfig.CurrentLauncherVersion : $"v{LauncherConfig.CurrentLauncherVersion}";
            var latVer = _latestLauncherVersion.StartsWith("v") ? _latestLauncherVersion : $"v{_latestLauncherVersion}";

            HomeInstalledVersionTextBlock.Text = instVer;
            HomeLatestVersionTextBlock.Text = latVer;
        }
        else
        {
            HomeInstalledVersionTextBlock.Text = installed
                ? $"{GetChannelDisplayName(SelectedModReleaseChannel)} {localVersion}"
                : localVersion;
            HomeLatestVersionTextBlock.Text = $"{GetChannelDisplayName(SelectedModReleaseChannel)} {latest}";
        }

        if (HomeInstallButtonTextBlock != null)
        {
            HomeInstallButtonTextBlock.Text = installed ? L("Msg_UpdateMod") : L("Msg_InstallMod");
        }
        if (InstallButton != null)
        {
            InstallButton.Content = installed ? L("Msg_UpdateMod") : L("Msg_InstallMod");
        }

        if (HomeUpdateHeaderTextBlock != null)
        {
            HomeUpdateHeaderTextBlock.Text = L("Msg_ModUpdate");
        }

        if (!string.IsNullOrWhiteSpace(_lastUpdateError))
        {
            SetHomeUpdateBadge(L("Phase_Error"), L("Msg_UpdateCheckFailed"), "#4A1825", "#FF6B82", L("Msg_ReadTheErrorBelowThenRetryCheck"));
            return;
        }

        if (_isBusy)
        {
            var detail = string.IsNullOrWhiteSpace(HomeUpdateCheckTextBlock.Text)
                ? L("Msg_UpdateOperationRunning")
                : HomeUpdateCheckTextBlock.Text;

            if (_isDownloadingLauncherUpdate)
            {
                if (HomeUpdateHeaderTextBlock != null)
                {
                    HomeUpdateHeaderTextBlock.Text = L("Msg_LauncherUpdate");
                }
                if (HomeInstallButtonTextBlock != null)
                {
                    HomeInstallButtonTextBlock.Text = "Updating...";
                }
                SetHomeUpdateBadge(L("Phase_Launcher"), L("Msg_UpdatingLauncher"), "#3C2D12", "#FFD166", detail);
            }
            else
            {
                SetHomeUpdateBadge(UpdatePhaseTextBlock.Text, L("Phase_Working"), "#233151", "#39E7FF", detail);
            }
            return;
        }

        if (!installed)
        {
            SetHomeUpdateBadge(L("Phase_Setup"), L("Msg_ModNotInstalled"), "#3C2D12", "#FFD166", L("Msg_InstallTheModpackToStartRacing"));
            return;
        }

        if (_isModUpdateRequired)
        {
            if (IsChannelSwitchPending(settings))
            {
                SetHomeUpdateBadge(
                    L("Phase_Channel"),
                    Loc.Format("Msg_SwitchTo", GetChannelDisplayName(SelectedModReleaseChannel)),
                    "#3C2D12",
                    "#FFD166",
                    Loc.Format("Msg_MustBeInstalledOnceTheOtherModpack", GetModDirectoryName(SelectedModReleaseChannel)));
            }
            else
            {
                SetHomeUpdateBadge(L("Phase_Update"), L("Msg_UpdateAvailable2"), "#3C2D12", "#FFD166", Loc.Format("Msg_InstalledLatest", localVersion, latest));
            }
            return;
        }

        SetHomeUpdateBadge(L("Phase_Ready"), Loc.Format("Msg_IsUpToDate", GetChannelDisplayName(SelectedModReleaseChannel)), "#153827", "#4DFFB0", L("Msg_InstalledModIsReady"));
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
        var activeModDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var profiles = _saveManagerService.GetSaveProfiles(settings, activeModDirectoryName);
        var activeMii = _saveManagerService.LoadMiiProfile();
        var miiDb = _saveManagerService.GetMiiDatabasePath(settings);

        RefreshMiiRuntimeStatus();
        RefreshMiiProfiles(activeMii.Id);

        _allLicenseCards.Clear();
        _allLicenseCards.AddRange(profiles);
        ApplyLicenseFilters();

        LicensesCountTextBlock.Text = _allLicenseCards.Count.ToString(CultureInfo.InvariantCulture);
        MiiStateTextBlock.Text = File.Exists(miiDb)
            ? "Dolphin"
            : L("Msg_NotFound");

        if (_allLicenseCards.Count == 0)
        {
            LicenseSummaryTextBlock.Text = Loc.Format("Msg_NoLicenseSaveWasDetectedInThe", activeModDirectoryName);
            PrimaryLicenseTextBlock.Text = L("Msg_NoLocalLicenseDetectedYet");
            PrimaryLicensePathTextBlock.Text = string.Empty;
            QueueLicenseAvatarRender(settings);
            if (_friendsViewModel != null)
            {
                _friendsViewModel.ActiveLicense = null;
            }
            return;
        }

        LicenseSummaryTextBlock.Text = Loc.Format("Msg_LicenseCardSDetected", profiles.Count, activeModDirectoryName);

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
                fcText.Text = L("Msg_Copied");
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

    private void CopyFriendCodeButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement element || element.DataContext is not FriendPlayerInfo friend)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(friend.FriendCode))
        {
            return;
        }

        try
        {
            System.Windows.Clipboard.SetText(friend.FriendCode);
            ShowToast(L("Toast_FriendCodeCopiedTitle"), Loc.Format("Toast_FriendCodeCopiedBody", friend.FriendCode));
        }
        catch (Exception ex)
        {
            ShowToast(L("Toast_CopyFailedTitle"), ex.Message);
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

    private void SwitchLicenseMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var target = _friendsViewModel?.ActiveLicense
                     ?? _allLicenseCards.FirstOrDefault(card => card.IsActive && !card.IsEmpty)
                     ?? _allLicenseCards.FirstOrDefault(card => !card.IsEmpty);

        if (target == null || target.IsEmpty || string.IsNullOrWhiteSpace(target.FilePath))
        {
            ShowCustomDialog(L("Msg_SelectALicense"), L("Msg_SelectARealLicenseCardBefore"), MessageBoxButton.OK);
            return;
        }

        _licenseMiiPickerItems.Clear();
        foreach (var profile in _saveManagerService.LoadMiiProfiles(BuildSettingsFromUi()).Where(profile => profile.IsRealMii))
        {
            _licenseMiiPickerItems.Add(profile);
        }

        if (_licenseMiiPickerItems.Count == 0)
        {
            ShowCustomDialog(L("Msg_NoMiiAvailable"), L("Msg_CreateOrImportARealWiiMiiFirst"), MessageBoxButton.OK);
            return;
        }

        _pendingLicenseMiiTarget = target;
        LicenseMiiPickerSummaryTextBlock.Text = Loc.Format("Msg_AssignASavedMiiToCurrentMii", target.DisplayName, target.Subtitle, target.MiiName);
        LicenseMiiPickerStatusTextBlock.Text = L("Msg_SelectAMiiToContinue");
        ApplyLicenseMiiButton.Content = L("Msg_ApplyMii");
        ApplyLicenseMiiButton.IsEnabled = true;
        LicenseMiiPickerListBox.SelectedItem = _licenseMiiPickerItems.FirstOrDefault(profile => profile.MiiId == target.MiiId)
                                               ?? _licenseMiiPickerItems.FirstOrDefault();

        ShowLicenseMiiPicker();
    }

    private async void ApplyLicenseMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isApplyingLicenseMii)
        {
            return;
        }

        if (_pendingLicenseMiiTarget == null)
        {
            ShowCustomDialog(L("Msg_SelectALicense"), L("Msg_NoTargetLicenseIsSelected"), MessageBoxButton.OK);
            return;
        }

        if (LicenseMiiPickerListBox.SelectedItem is not LauncherMiiProfile selectedMii)
        {
            LicenseMiiPickerStatusTextBlock.Text = L("Msg_ChooseAMiiBeforeApplying");
            return;
        }

        _isApplyingLicenseMii = true;
        ApplyLicenseMiiButton.IsEnabled = false;
        ApplyLicenseMiiButton.Content = "Applying...";
        LicenseMiiPickerStatusTextBlock.Text = L("Msg_CreatingBackupSyncingMiiAndUpdating");

        try
        {
            var backupPath = await _saveManagerService.ApplyMiiToLicenseAsync(BuildSettingsFromUi(), _pendingLicenseMiiTarget, selectedMii);
            HideLicenseMiiPicker();
            ShowToast(L("Msg_LicenseMiiUpdated"), Loc.Format("Msg_AssignedBackup", selectedMii.Name, Path.GetFileName(backupPath)));
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            LicenseMiiPickerStatusTextBlock.Text = ex.Message;
            ShowCustomDialog(L("Msg_SwitchMiiError"), ex.Message, MessageBoxButton.OK);
        }
        finally
        {
            _isApplyingLicenseMii = false;
            ApplyLicenseMiiButton.IsEnabled = true;
            ApplyLicenseMiiButton.Content = L("Msg_ApplyMii");
        }
    }

    private void CancelLicenseMiiPickerButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isApplyingLicenseMii)
        {
            return;
        }

        HideLicenseMiiPicker();
    }

    private void ShowLicenseMiiPicker()
    {
        LicenseMiiPickerOverlay.Visibility = Visibility.Visible;
        LicenseMiiPickerOverlay.Opacity = 0;

        if (LicenseMiiPickerCard.RenderTransform is not ScaleTransform scale)
        {
            scale = new ScaleTransform(0.96, 0.96);
            LicenseMiiPickerCard.RenderTransform = scale;
        }

        scale.ScaleX = 0.96;
        scale.ScaleY = 0.96;

        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        LicenseMiiPickerOverlay.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(180)) { EasingFunction = ease });
        scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(0.96, 1, TimeSpan.FromMilliseconds(220)) { EasingFunction = ease });
        scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(0.96, 1, TimeSpan.FromMilliseconds(220)) { EasingFunction = ease });
    }

    private void HideLicenseMiiPicker()
    {
        if (LicenseMiiPickerOverlay.Visibility != Visibility.Visible)
        {
            return;
        }

        var ease = new CubicEase { EasingMode = EasingMode.EaseIn };
        var fade = new DoubleAnimation(0, TimeSpan.FromMilliseconds(140)) { EasingFunction = ease };
        fade.Completed += (_, _) =>
        {
            LicenseMiiPickerOverlay.Visibility = Visibility.Collapsed;
            _pendingLicenseMiiTarget = null;
        };

        LicenseMiiPickerOverlay.BeginAnimation(OpacityProperty, fade);
        if (LicenseMiiPickerCard.RenderTransform is ScaleTransform scale)
        {
            scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(0.97, TimeSpan.FromMilliseconds(140)) { EasingFunction = ease });
            scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(0.97, TimeSpan.FromMilliseconds(140)) { EasingFunction = ease });
        }
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
            MiiRuntimeProgressTextBlock.Text = Loc.Format("Msg_Renderer", ex.Message);
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
            MiiRuntimeProgressTextBlock.Text = Loc.Format("Msg_Renderer", ex.Message);
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
            ? L("Msg_RendererAssetsAreInstalled")
            : L("Msg_InstallTheMiiRenderAssetCache");
        MiiRuntimeProgressTextBlock.Text = status.IsInstalled
            ? Loc.Format("Msg_Installed2", FormatBytes(status.SizeBytes))
            : L("Msg_ThisDownloadsAndVerifiesTheRender");
        MiiRuntimeProgressBar.Value = status.IsInstalled ? 100 : 0;
        InstallMiiRuntimeButton.IsEnabled = !_isInstallingMiiRuntime && !status.IsInstalled;
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
            : L("Play_Never");
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
            Loc.Format("Msg_Tab", _currentTab) +
            Loc.Format("Msg_Busy", _isBusy) +
            Loc.Format("Msg_UpdateRequired", _isModUpdateRequired) +
            Loc.Format("Msg_LauncherVersion", LauncherConfig.CurrentLauncherVersion) +
            $"Latest mod version: {(_latestModVersion.Length == 0 ? "unknown" : _latestModVersion)}\n" +
            Loc.Format("Msg_SelectedChannel", GetChannelDisplayName(SelectedModReleaseChannel)) +
            Loc.Format("Msg_StableInstalled", IsModInstalled(settings, ModReleaseChannel.Stable), GetInstalledModVersion(ModReleaseChannel.Stable)) +
            Loc.Format("Msg_BetaInstalled", IsModInstalled(settings, ModReleaseChannel.Beta), GetInstalledModVersion(ModReleaseChannel.Beta)) +
            Loc.Format("Msg_Dolphin", settings.DolphinPath) +
            Loc.Format("Msg_UserFolder", settings.UserFolderPath) +
            Loc.Format("Msg_Rom", settings.RomPath) +
            Loc.Format("Msg_ModFolder", settings.GetModFolder()) +
            Loc.Format("Msg_SettingsFile", _settingsService.GetSettingsPath()) +
            Loc.Format("Msg_PreferencesFile", _preferencesService.GetPreferencesPath()) +
            Loc.Format("Msg_ModStateFile", _modInstallationStateService.GetStatePath());
    }

    /// <summary>Shorthand for a translated string, used all over the code-behind.</summary>
    private static string L(string key) => Loc.T(key);

    /// <summary>
    /// Applies the stored language before the first frame is built, so the window
    /// never flashes English while an Italian user is loading.
    /// </summary>
    private void ApplyStoredLanguage()
    {
        var stored = _userPreferences.Language;
        Loc.Service.SetLanguage(LocalizationService.IsSupported(stored)
            ? stored
            : LocalizationService.DetectSystemLanguage());
    }

    private void PopulateLanguageComboBox()
    {
        if (LanguageComboBox == null)
        {
            return;
        }

        _isUpdatingLanguageUi = true;
        try
        {
            LanguageComboBox.Items.Clear();
            foreach (var option in LocalizationService.AvailableLanguages)
            {
                LanguageComboBox.Items.Add(new ComboBoxItem
                {
                    Content = option.Label,
                    Tag = option.Code
                });
            }

            foreach (ComboBoxItem item in LanguageComboBox.Items)
            {
                if (string.Equals(item.Tag as string, Loc.Service.CurrentLanguage, StringComparison.OrdinalIgnoreCase))
                {
                    LanguageComboBox.SelectedItem = item;
                    break;
                }
            }

            if (LanguageComboBox.SelectedItem == null && LanguageComboBox.Items.Count > 0)
            {
                LanguageComboBox.SelectedIndex = 0;
            }
        }
        finally
        {
            _isUpdatingLanguageUi = false;
        }
    }

    private void LanguageComboBox_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isUpdatingLanguageUi)
        {
            return;
        }

        if ((LanguageComboBox?.SelectedItem as ComboBoxItem)?.Tag is not string code)
        {
            return;
        }

        if (string.Equals(code, Loc.Service.CurrentLanguage, StringComparison.OrdinalIgnoreCase))
        {
            return;
        }

        Loc.Service.SetLanguage(code);
        _userPreferences.Language = Loc.Service.CurrentLanguage;
        _preferencesService.Save(_userPreferences);
        ShowSettingsStatusNotification(L("Set_LanguageApplied"));
    }

    /// <summary>
    /// Text that code writes directly into a control replaces the XAML binding, so
    /// those places have to be recomputed whenever the language changes.
    /// </summary>
    private void OnLanguageChanged()
    {
        try
        {
            PopulateLanguageComboBox();
            UpdateNavigationLabels();
            RefreshAllState();
            RefreshMiiRuntimeStatus();
            UpdateTeamVersionLabel();
        }
        catch
        {
            // Never let a refresh failure take the window down mid-switch.
        }
    }

    private void UpdateTeamVersionLabel()
    {
        if (TeamVersionTextBlock != null)
        {
            TeamVersionTextBlock.Text = Loc.Format("Team_HeroVersion", LauncherConfig.CurrentLauncherVersion);
        }
    }


    private void SetBusy(bool value)
    {
        _isBusy = value;
        _shellViewModel.IsBusy = value;
        InstallButton.IsEnabled = !value;
        HomeVerifyButton.IsEnabled = !value;
        LaunchButton.IsEnabled = !value && !_isGameRunning;
        CheckUpdatesButton.IsEnabled = !value;
        RepairModButton.IsEnabled = !value;
        OpenModFolderButton.IsEnabled = !value;
        GameBananaSearchButton.IsEnabled = !value;
        MusicPackInstallButton.IsEnabled = !value && IsModInstalled(BuildSettingsFromUi()) && !string.IsNullOrWhiteSpace(_latestMusicPackUrl);
        var selectedModDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        MusicPackRemoveButton.IsEnabled = !value && _musicPackService.IsInstalled(BuildSettingsFromUi(), selectedModDirectoryName);
        MusicPackEnabledCheckBox.IsEnabled = !value && _musicPackService.IsInstalled(BuildSettingsFromUi(), selectedModDirectoryName);
        ModReleaseChannelComboBox.IsEnabled = !value;
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

        if (buttons == MessageBoxButton.YesNoCancel)
        {
            return dialog.Result;
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
            ShowCustomDialog(L("Msg_SetupRequired"), L("Msg_SelectTheDolphinUserFolderFirst"), MessageBoxButton.OK);
            _navigationService.Navigate("Settings");
            return;
        }

        SaveSettingsFromUi();

        if (!await EnsureSelectedModReleaseLoadedAsync())
        {
            ShowCustomDialog(
                L("Msg_ReleaseUnavailable"),
                Loc.Format("Msg_TheReleaseMetadataCouldNotBe", GetChannelDisplayName(SelectedModReleaseChannel)),
                MessageBoxButton.OK);
            return;
        }

        if (IsModInstalled(settings) && !string.IsNullOrEmpty(_latestModVersion) && !IsChannelSwitchPending(settings))
        {
            var localVersion = GetInstalledModVersion();
            if (localVersion == _latestModVersion)
            {
                var result = ShowCustomDialog(L("Msg_ModUpToDate"), L("Msg_TheModIsAlreadyUpToDateReinstall"), MessageBoxButton.YesNo);
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
        var targetChannel = SelectedModReleaseChannel;
        SetBusy(true);
        ResetDownloadMetrics();
        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = false;
        DownloadProgressBar.Value = 0;
        UpdateSpeedTextBlock.Text = string.Empty;

        var settings = BuildSettingsFromUi();
        var modFolder = settings.GetModFolder();
        var modDirectoryName = GetModDirectoryName(targetChannel);
        var modSubFolder = Path.Combine(modFolder, modDirectoryName);
        var isUpdate = IsModInstalled(settings, targetChannel);
        var operationId = $"{DateTime.Now:yyyyMMdd-HHmmss}-{Guid.NewGuid():N}"[..24];
        var operationStopwatch = Stopwatch.StartNew();

        Task LogOperationAsync(string message)
            => WriteUpdateLogAsync($"[operation {operationId}] {message}");

        SetStatus(Loc.Format("Msg_ConnectingToChannel", GetChannelDisplayName(targetChannel)), (WpfBrush)FindResource("TextSecondary"));
        SetUpdateState(L("Phase_Connecting"), Loc.Format("Msg_PreparingDownload", GetChannelDisplayName(targetChannel)), 0);

        await LogOperationAsync(
            $"Started: channel={targetChannel}, mode={(isUpdate ? "update" : "install")}, " +
            $"installedVersion={GetInstalledModVersion(targetChannel)}, targetVersion={_latestModVersion}, " +
            $"launcherVersion={LauncherConfig.CurrentLauncherVersion}, runtime={Environment.Version}, " +
            $"os={Environment.OSVersion.VersionString}, process64Bit={Environment.Is64BitProcess}, " +
            $"modFolder={modSubFolder}, manifest={_latestModManifestUrl}, filesBase={_latestModFilesUrl}");

        ModUpdateBackup? backup = null;

        async Task<ModUpdateResult> ApplyFullZipUpdateAsync(string fallbackReason)
        {
            if (!string.IsNullOrWhiteSpace(fallbackReason))
            {
                await LogOperationAsync($"Differential update failed, falling back to full ZIP: {fallbackReason}");
                SetUpdateState(L("Phase_Recovery"), L("Msg_DifferentialUpdateFailedDownloading"), 5);
                SetStatus(L("Msg_RepairingInstallationWithFull"), (WpfBrush)FindResource("WarningBrush"));
            }
            else
            {
                SetUpdateState(L("Phase_Download"), L("Msg_DownloadingModpack"), 5);
            }

            ResetDownloadMetrics();
            var downloadProgress = new Progress<(long current, long total)>(
                p => UpdateDownloadProgress(p.current, p.total));

            var fullDownloadResult = await _networkService.DownloadFileWithResumeDetailedAsync(
                BuildModMirrorList(), _tempZipPath, downloadProgress);
            await LogOperationAsync(FormatDownloadResult("Full ZIP downloaded", fullDownloadResult));

            SetStatus(L("Msg_VerifyingDownloadedArchive"), (WpfBrush)FindResource("TextSecondary"));
            SetUpdateState(L("Phase_Verifying"), L("Msg_CheckingArchiveIntegrity"), 96);
            await VerifyDownloadedArchiveAsync(_tempZipPath, _latestModSha256);

            DownloadProgressBar.IsIndeterminate = false;
            DownloadProgressBar.Value = 0;
            SetStatus(L("Msg_UpdatingModpackFiles"), (WpfBrush)FindResource("WarningBrush"));
            SetUpdateState(
                isUpdate ? L("Phase_Updating") : L("Phase_Installing"),
                isUpdate
                    ? L("Msg_ReplacingModpackFilesUserData")
                    : L("Msg_WritingModpackFilesToRiivolution"),
                0);

            var extractProgress = new Progress<int>(p =>
                SetUpdateState(
                    isUpdate ? L("Phase_Updating") : L("Phase_Installing"),
                    isUpdate
                        ? Loc.Format("Msg_UpdatingModpackFiles2", p)
                        : Loc.Format("Msg_WritingFiles", p),
                    p));

            var fullResult = await _modUpdateSafetyService.ApplyZipUpdateAsync(
                _tempZipPath,
                modFolder,
                modSubFolder,
                settings,
                modDirectoryName,
                extractProgress);

            if (File.Exists(_tempZipPath))
                File.Delete(_tempZipPath);

            return fullResult;
        }

        try
        {
            // ── STEP 1: backup user data (only if the mod is already installed) ──
            if (isUpdate)
            {
                SetUpdateState(L("Phase_Backup"), L("Msg_SavingLicensesMiiAndProfiles"), 2);
                SetStatus(L("Msg_BackingUpUserData"), (WpfBrush)FindResource("TextSecondary"));

                var backupProgress = new Progress<string>(msg =>
                    SetUpdateState(L("Phase_Backup"), msg, 3));

                backup = await _modUpdateSafetyService.CreateBackupAsync(
                    settings, modDirectoryName, backupProgress);

                if (backup.Files.Count > 0)
                {
                    await LogOperationAsync(
                        $"Backup created: id={backup.BackupId}, protectedFiles={backup.Files.Count}, " +
                        $"folder={backup.BackupFolder}");
                }
            }

            ModUpdateResult result;
            ModManifest? manifest = null;

            if (isUpdate && !string.IsNullOrWhiteSpace(_latestModManifestUrl))
            {
                try
                {
                    SetUpdateState(L("Phase_Download"), L("Msg_DownloadingUpdateManifest"), 5);
                    SetStatus(L("Msg_FetchingUpdateManifest"), (WpfBrush)FindResource("TextSecondary"));

                    var manifestJson = await _networkService.DownloadStringAsync(AddNoCacheQuery(_latestModManifestUrl));
                    manifest = JsonSerializer.Deserialize<ModManifest>(manifestJson.TrimStart('\uFEFF', '\u200B'));
                    ValidateModManifest(manifest);
                    _latestModVersion = manifest!.ModVersion;
                    if (!string.IsNullOrWhiteSpace(manifest.ArchiveSha256))
                    {
                        _latestModSha256 = manifest.ArchiveSha256;
                    }

                    await LogOperationAsync(
                        $"Manifest loaded: version={manifest.ModVersion}, files={manifest.Files.Count}, " +
                        $"archiveSha256={manifest.ArchiveSha256}");
                }
                catch (Exception ex)
                {
                    manifest = null;
                    await LogOperationAsync(
                        $"Manifest download failed; full ZIP will be used. Error={ex}");
                }
            }

            if (isUpdate && manifest != null)
            {
                try
                {
                    // ── STEP 2: differential update ──
                    SetUpdateState(L("Phase_Verifying"), L("Msg_ScanningLocalFiles"), 8);
                    SetStatus(L("Msg_VerifyingLocalInstallation"), (WpfBrush)FindResource("TextSecondary"));

                    var localFiles = await _modUpdateSafetyService.ScanLocalFilesAsync(modSubFolder);

                    // Diff files
                    var filesToDownload = new List<ModManifestFile>();
                    var filesToDelete = new List<string>();

                    foreach (var serverFile in manifest.Files)
                    {
                        var local = localFiles.FirstOrDefault(f => f.Path.Equals(serverFile.Path, StringComparison.OrdinalIgnoreCase));
                        if (local == null || !local.Sha256.Equals(serverFile.Sha256, StringComparison.OrdinalIgnoreCase))
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

                    long downloadPayloadBytes = filesToDownload.Sum(file => file.Size);
                    long totalBytesToDownload = Math.Max(1, downloadPayloadBytes);
                    await LogOperationAsync(
                        $"Differential plan: manifestFiles={manifest.Files.Count}, localFiles={localFiles.Count}, " +
                        $"downloadFiles={filesToDownload.Count}, downloadBytes={downloadPayloadBytes}, " +
                        $"deleteFiles={filesToDelete.Count}, concurrency={DifferentialDownloadConcurrency}");

                    var stagingRoot = Path.Combine(Path.GetTempPath(), $"vanzakart_mod_update_{Guid.NewGuid():N}");
                    Directory.CreateDirectory(stagingRoot);

                    try
                    {
                        var fullModSubFolder = Path.GetFullPath(modSubFolder)
                            .TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
                        var progressSync = new object();
                        var activeFileBytes = new Dictionary<string, long>(StringComparer.OrdinalIgnoreCase);
                        var completedProgressPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                        var downloadResults = new ConcurrentBag<DownloadResult>();
                        var differentialStopwatch = Stopwatch.StartNew();
                        long completedBytes = 0;
                        var completedFiles = 0;
                        Exception? firstDownloadFailure = null;

                        IProgress<(string path, long current)> aggregateProgress = new Progress<(string path, long current)>(update =>
                        {
                            long aggregateBytes;
                            lock (progressSync)
                            {
                                if (completedProgressPaths.Contains(update.path))
                                {
                                    return;
                                }

                                activeFileBytes[update.path] = Math.Max(0, update.current);
                                aggregateBytes = completedBytes + activeFileBytes.Values.Sum();
                            }

                            UpdateDownloadProgress(Math.Min(aggregateBytes, totalBytesToDownload), totalBytesToDownload);
                        });

                        using var downloadGate = new SemaphoreSlim(DifferentialDownloadConcurrency);
                        using var downloadCancellation = new CancellationTokenSource();

                        async Task DownloadAndVerifyFileAsync(ModManifestFile file, int fileNumber)
                        {
                            var fileProgress = new Progress<(long current, long total)>(progress =>
                                aggregateProgress.Report((file.Path, Math.Min(progress.current, file.Size))));

                            await downloadGate.WaitAsync(downloadCancellation.Token);
                            try
                            {
                                var relativePath = file.Path.Replace('/', Path.DirectorySeparatorChar);
                                var localPath = Path.GetFullPath(Path.Combine(modSubFolder, relativePath));
                                if (!localPath.StartsWith(fullModSubFolder + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) &&
                                    !localPath.Equals(fullModSubFolder, StringComparison.OrdinalIgnoreCase))
                                {
                                    throw new InvalidDataException($"Invalid update manifest path: {file.Path}");
                                }

                                SetStatus(
                                    Loc.Format("Msg_Downloading3", Math.Min(fileNumber, filesToDownload.Count), filesToDownload.Count, Path.GetFileName(file.Path)),
                                    (WpfBrush)FindResource("TextSecondary"));

                                var tempFile = Path.Combine(stagingRoot, relativePath);
                                Directory.CreateDirectory(Path.GetDirectoryName(tempFile)!);
                                if (File.Exists(tempFile))
                                {
                                    File.Delete(tempFile);
                                }

                                var mirrors = BuildModFileMirrorList(file).ToArray();
                                var downloadResult = await _networkService.DownloadFileWithResumeDetailedAsync(
                                    mirrors,
                                    tempFile,
                                    fileProgress,
                                    downloadCancellation.Token);

                                var downloadedHash = await ModUpdateSafetyService.ComputeSha256Async(
                                    tempFile,
                                    downloadCancellation.Token);
                                if (!downloadedHash.Equals(file.Sha256, StringComparison.OrdinalIgnoreCase))
                                {
                                    throw new InvalidDataException(
                                        $"Hash mismatch for downloaded file: {file.Path}. Expected {file.Sha256}, got {downloadedHash}");
                                }

                                downloadResults.Add(downloadResult);
                                long aggregateBytes;
                                lock (progressSync)
                                {
                                    completedProgressPaths.Add(file.Path);
                                    activeFileBytes.Remove(file.Path);
                                    completedBytes += file.Size;
                                    aggregateBytes = completedBytes + activeFileBytes.Values.Sum();
                                }

                                UpdateDownloadProgress(Math.Min(aggregateBytes, totalBytesToDownload), totalBytesToDownload);
                                var completed = Interlocked.Increment(ref completedFiles);

                                if (downloadResult.RetryCount > 0 || downloadResult.Duration >= TimeSpan.FromSeconds(3))
                                {
                                    await LogOperationAsync(FormatDownloadResult($"File '{file.Path}'", downloadResult));
                                }

                                if (completed % 25 == 0 || completed == filesToDownload.Count)
                                {
                                    var elapsedSeconds = Math.Max(0.001, differentialStopwatch.Elapsed.TotalSeconds);
                                    await LogOperationAsync(
                                        $"Differential progress: files={completed}/{filesToDownload.Count}, " +
                                        $"bytes={completedBytes}/{totalBytesToDownload}, " +
                                        $"effectiveAverage={FormatBytes((long)(completedBytes / elapsedSeconds))}/s");
                                }
                            }
                            catch (OperationCanceledException) when (downloadCancellation.IsCancellationRequested && firstDownloadFailure != null)
                            {
                                throw;
                            }
                            catch (Exception ex)
                            {
                                lock (progressSync)
                                {
                                    firstDownloadFailure ??= new FileNotFoundException(
                                        $"The update manifest references '{file.Path}', but the file could not be downloaded or verified.",
                                        ex);
                                }

                                await LogOperationAsync(
                                    $"Differential file failed: path={file.Path}, size={file.Size}.{Environment.NewLine}" +
                                    FormatDownloadFailure(ex));
                                downloadCancellation.Cancel();
                                throw;
                            }
                            finally
                            {
                                downloadGate.Release();
                            }
                        }

                        var downloadTasks = filesToDownload
                            .Select((file, index) => DownloadAndVerifyFileAsync(file, index + 1))
                            .ToArray();

                        try
                        {
                            await Task.WhenAll(downloadTasks);
                        }
                        catch
                        {
                            if (firstDownloadFailure != null)
                            {
                                throw firstDownloadFailure;
                            }

                            throw;
                        }

                        differentialStopwatch.Stop();
                        var differentialSeconds = Math.Max(0.001, differentialStopwatch.Elapsed.TotalSeconds);
                        await LogOperationAsync(
                            $"Differential download completed: files={filesToDownload.Count}, payload={downloadPayloadBytes}, " +
                            $"elapsed={differentialStopwatch.Elapsed.TotalSeconds:F2}s, " +
                            $"effectiveAverage={FormatBytes((long)(downloadPayloadBytes / differentialSeconds))}/s, " +
                            $"httpAttempts={downloadResults.Sum(item => item.Attempts.Count)}, " +
                            $"retries={downloadResults.Sum(item => item.RetryCount)}");

                        // Apply downloaded files only after every required file was downloaded
                        // and verified. This prevents half-updated installs when the server
                        // manifest references a file that has not been uploaded yet.
                        foreach (var file in filesToDownload)
                        {
                            var relativePath = file.Path.Replace('/', Path.DirectorySeparatorChar);
                            var sourcePath = Path.Combine(stagingRoot, relativePath);
                            var destinationPath = Path.GetFullPath(Path.Combine(modSubFolder, relativePath));
                            Directory.CreateDirectory(Path.GetDirectoryName(destinationPath)!);
                            File.Move(sourcePath, destinationPath, overwrite: true);
                        }

                        // Delete obsolete files after new files are safely staged and applied.
                        int deletedCount = 0;
                        foreach (var fileToDelete in filesToDelete)
                        {
                            var localPath = Path.Combine(modSubFolder, fileToDelete.Replace('/', Path.DirectorySeparatorChar));
                            if (File.Exists(localPath))
                            {
                                File.Delete(localPath);
                                deletedCount++;
                                await LogOperationAsync($"Pruned obsolete file: {fileToDelete}");
                            }
                        }

                        // Clean up empty directories
                        _modUpdateSafetyService.RemoveEmptyDirectories(
                            modSubFolder,
                            modSubFolder,
                            _modUpdateSafetyService.BuildProtectedAbsolutePaths(settings, modSubFolder, modDirectoryName));

                        result = new ModUpdateResult
                        {
                            FilesWritten = filesToDownload.Count,
                            FilesSkipped = 0,
                            FilesPruned = deletedCount
                        };
                    }
                    finally
                    {
                        try
                        {
                            if (Directory.Exists(stagingRoot))
                                Directory.Delete(stagingRoot, recursive: true);
                        }
                        catch
                        {
                        }
                    }
                }
                catch (Exception diffEx)
                {
                    result = await ApplyFullZipUpdateAsync(diffEx.Message);
                }
            }
            else
            {
                result = await ApplyFullZipUpdateAsync(string.Empty);
            }

            // The release intentionally contains an empty My Stuff directory.
            // Ensure it also exists after extraction and differential updates,
            // since ZIP readers and web servers may omit empty directories.
            Directory.CreateDirectory(Path.Combine(modSubFolder, modDirectoryName, "My Stuff"));

            // ── STEP 6: write version ────────────────────────────────────────────────────
            if (!string.IsNullOrWhiteSpace(_latestModVersion))
            {
                File.WriteAllText(GetModVersionFile(targetChannel), _latestModVersion);
                var installedChannel = _installedModState.Get(targetChannel);
                installedChannel.Version = _latestModVersion;
                installedChannel.InstalledAtUtc = DateTime.UtcNow;
                _modInstallationStateService.Save(_installedModState);
                if (targetChannel == ModReleaseChannel.Beta)
                {
                    _userPreferences.LastKnownLatestBetaModVersion = _latestModVersion;
                }
                else
                {
                    _userPreferences.LastKnownLatestModVersion = _latestModVersion;
                }
                _preferencesService.Save(_userPreferences);
            }

            // ── Completato ────────────────────────────────────────────────────────
            DownloadProgressBar.Value = 100;
            _isModUpdateRequired = false;

            var summary = BuildSafeUpdateStatusMessage(isUpdate, result, backup);
            SetUpdateState(L("Phase_Completed"), summary, 100);
            SetStatus(
                Loc.Format(isUpdate ? "Msg_ChannelUpdateCompleted" : "Msg_ChannelInstallCompleted", GetChannelDisplayName(targetChannel)),
                (WpfBrush)FindResource("SuccessBrush"));

            ShowToast(
                isUpdate ? L("Msg_UpdateCompleted") : L("Msg_InstallationCompleted"),
                summary);

            operationStopwatch.Stop();
            await LogOperationAsync(
                $"Completed: elapsed={operationStopwatch.Elapsed.TotalSeconds:F2}s, result={result}, " +
                $"backup={(backup?.BackupId ?? "none")}");

            RefreshAllState();
        }
        catch (Exception ex)
        {
            // ── Rollback automatico ───────────────────────────────────────────────
            if (backup != null && backup.Files.Count > 0)
            {
                try
                {
                    SetUpdateState(L("Phase_Rollback"), L("Msg_RestoringUserDataAfterError"), 0);
                    SetStatus(L("Msg_ErrorRestoringData"), (WpfBrush)FindResource("DangerBrush"));

                    await _modUpdateSafetyService.RestoreBackupAsync(backup);

                    await LogOperationAsync(
                        $"Rollback completed (backup {backup.BackupId}): " +
                        $"{backup.Files.Count} user file(s) restored.");
                }
                catch (Exception rollbackEx)
                {
                    // Rollback itself failed: warn prominently
                    await LogOperationAsync(
                        $"WARNING – rollback failed: {rollbackEx.Message}. " +
                        $"Manual restore from Backups/{backup.BackupId}");

                    ShowCustomDialog(
                        L("Msg_WarningRollbackFailed"),
                        L("Msg_TheUpdateFailedAndTheAutomatic") +
                        L("Msg_YourDataLicensesMiiIsSafeInThe") +
                        Loc.Format("Msg_BackupsModupdates", backup.BackupId) +
                        L("Msg_ManuallyCopyTheFilesFromThere") +
                        Loc.Format("Msg_OriginalError", ex.Message) +
                        Loc.Format("Msg_RollbackError", rollbackEx.Message),
                        MessageBoxButton.OK);

                    goto Cleanup;
                }
            }

            // Standard error (no backup or rollback succeeded)
            DownloadProgressBar.IsIndeterminate = false;
            DownloadProgressBar.Visibility = Visibility.Collapsed;
            SetStatus(L("Msg_InstallationFailed"), (WpfBrush)FindResource("DangerBrush"));
            SetUpdateState(L("Phase_Error"), ex.Message, 0);
            ShowCustomDialog(L("Msg_InstallationError"), ex.Message, MessageBoxButton.OK);

            operationStopwatch.Stop();
            await LogOperationAsync(
                $"Failed: elapsed={operationStopwatch.Elapsed.TotalSeconds:F2}s.{Environment.NewLine}" +
                FormatDownloadFailure(ex));

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
            return Loc.Format("Msg_InstallSummaryInstalled", result.FilesWritten);

        var sb = new System.Text.StringBuilder();
        sb.Append(Loc.Format("Msg_InstallSummaryUpdated", result.FilesWritten));

        if (result.FilesPruned > 0)
            sb.Append(Loc.Format("Msg_InstallSummaryPruned", result.FilesPruned));

        if (result.FilesSkipped > 0)
            sb.Append(Loc.Format("Msg_InstallSummaryProtected", result.FilesSkipped));

        if (backup?.Files.Count > 0)
            sb.Append(Loc.Format("Msg_InstallSummaryBackup", backup.BackupId));

        sb.Append('.');

        if (result.HasErrors)
            sb.Append(Loc.Format("Msg_InstallSummaryErrors", result.Errors.Count));

        return sb.ToString();
    }

    private static string FormatDownloadResult(string label, DownloadResult result)
    {
        var seconds = Math.Max(0.001, result.Duration.TotalSeconds);
        var averageBytesPerSecond = result.BytesReceived / seconds;
        var successfulAttempt = result.Attempts.LastOrDefault(attempt => attempt.Success);
        return $"{label}: source={result.SourceUrl}, received={FormatBytes(result.BytesReceived)}, " +
               $"total={FormatBytes(result.TotalBytes)}, elapsed={result.Duration.TotalSeconds:F2}s, " +
               $"average={FormatBytes((long)averageBytesPerSecond)}/s, attempts={result.Attempts.Count}, " +
               $"retries={result.RetryCount}, http={successfulAttempt?.StatusCode?.ToString() ?? "unknown"}/" +
               $"{successfulAttempt?.HttpVersion ?? "unknown"}";
    }

    private static string FormatDownloadFailure(Exception exception)
    {
        if (exception is not DownloadFailedException downloadFailure || downloadFailure.Attempts.Count == 0)
        {
            return exception.ToString();
        }

        var attempts = downloadFailure.Attempts.Select(attempt =>
        {
            var status = attempt.StatusCode?.ToString() ?? "network";
            var error = attempt.Error.Replace('\r', ' ').Replace('\n', ' ');
            return $"url={attempt.Url}, attempt={attempt.Attempt}, status={status}, " +
                   $"elapsed={attempt.Duration.TotalSeconds:F2}s, existing={attempt.ExistingBytes}, " +
                   $"received={attempt.BytesReceived}, error={error}";
        });

        return $"{downloadFailure.Message}{Environment.NewLine}" + string.Join(Environment.NewLine, attempts);
    }

    private static async Task WriteUpdateLogAsync(string message)
    {
        var lockTaken = false;
        try
        {
            var path = Path.Combine(AppContext.BaseDirectory, "Logs", "mod-update.log");
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await UpdateLogSemaphore.WaitAsync();
            lockTaken = true;
            await File.AppendAllTextAsync(
                path,
                $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] [Launcher] {message}{Environment.NewLine}");
        }
        catch
        {
        }
        finally
        {
            if (lockTaken)
            {
                UpdateLogSemaphore.Release();
            }
        }
    }

    /// <summary>
    /// Compares every installed modpack file against the published manifest and
    /// reports what is missing, altered or left over. Repairing is offered only
    /// when the scan actually found something wrong.
    /// </summary>
    private async void VerifyModButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings, SelectedModReleaseChannel))
        {
            ShowCustomDialog(
                L("Dlg_VerifyNotInstalledTitle"),
                L("Dlg_VerifyNotInstalledBody"),
                MessageBoxButton.OK);
            return;
        }

        SetBusy(true);
        try
        {
            SetStatus(L("Status_VerifyPreparing"), (WpfBrush)FindResource("TextSecondary"));
            SetUpdateState(L("Phase_Verifying"), L("Status_VerifyPreparing"), 2);

            if (!await EnsureSelectedModReleaseLoadedAsync() || string.IsNullOrWhiteSpace(_latestModManifestUrl))
            {
                ShowCustomDialog(
                    L("Dlg_ReleaseUnavailableTitle"),
                    L("Dlg_ReleaseUnavailableBody"),
                    MessageBoxButton.OK);
                return;
            }

            SetUpdateState(L("Phase_Verifying"), L("Status_VerifyManifest"), 8);
            var manifestJson = await _networkService.DownloadStringAsync(AddNoCacheQuery(_latestModManifestUrl));
            var manifest = JsonSerializer.Deserialize<ModManifest>(manifestJson.TrimStart('\uFEFF', '\u200B'));
            ValidateModManifest(manifest);

            SetUpdateState(L("Phase_Verifying"), L("Status_VerifyScanning"), 25);
            var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
            var modSubFolder = Path.Combine(settings.GetModFolder(), modDirectoryName);
            var localFiles = await Task.Run(() => _modUpdateSafetyService.ScanLocalFilesAsync(modSubFolder));

            var localByPath = localFiles.ToDictionary(f => f.Path, StringComparer.OrdinalIgnoreCase);
            var missing = new List<string>();
            var altered = new List<string>();

            foreach (var expected in manifest!.Files)
            {
                if (!localByPath.TryGetValue(expected.Path, out var local))
                {
                    missing.Add(expected.Path);
                }
                else if (!local.Sha256.Equals(expected.Sha256, StringComparison.OrdinalIgnoreCase))
                {
                    altered.Add(expected.Path);
                }
            }

            var manifestPaths = new HashSet<string>(manifest.Files.Select(f => f.Path), StringComparer.OrdinalIgnoreCase);
            var extra = localFiles.Where(f => !manifestPaths.Contains(f.Path)).Select(f => f.Path).ToList();

            SetUpdateState(L("Phase_Verifying"), L("Status_VerifyDone"), 100);
            var problems = missing.Count + altered.Count;
            var installedVersion = GetInstalledModVersion(SelectedModReleaseChannel);

            await WriteUpdateLogAsync(
                $"[verify] channel={SelectedModReleaseChannel}, manifestVersion={manifest.ModVersion}, " +
                $"installedVersion={installedVersion}, checked={manifest.Files.Count}, missing={missing.Count}, " +
                $"altered={altered.Count}, extra={extra.Count}");

            if (problems == 0)
            {
                SetStatus(L("Status_VerifyOk"), (WpfBrush)FindResource("SuccessBrush"));
                ShowCustomDialog(
                    Loc.Format("Dlg_VerifyOkTitle"),
                    Loc.Format("Dlg_VerifyOkBody", manifest.Files.Count, manifest.ModVersion, extra.Count),
                    MessageBoxButton.OK);
                return;
            }

            SetStatus(L("Status_VerifyProblems"), (WpfBrush)FindResource("WarningBrush"));
            var preview = string.Join(
                Environment.NewLine,
                missing.Select(pth => "- " + pth).Concat(altered.Select(pth => "~ " + pth)).Take(10));
            var body = Loc.Format(
                "Dlg_VerifyProblemsBody",
                manifest.Files.Count,
                missing.Count,
                altered.Count,
                extra.Count);
            if (!string.IsNullOrEmpty(preview))
            {
                body += Environment.NewLine + Environment.NewLine + preview;
                if (problems > 10)
                {
                    body += Environment.NewLine + Loc.Format("Dlg_VerifyMoreFiles", problems - 10);
                }
            }

            body += Environment.NewLine + Environment.NewLine + L("Dlg_VerifyRepairQuestion");

            if (ShowCustomDialog(L("Dlg_VerifyProblemsTitle"), body, MessageBoxButton.YesNo) == MessageBoxResult.Yes)
            {
                SetBusy(false);
                await PerformModInstallation();
            }
        }
        catch (Exception ex)
        {
            SetStatus(L("Status_VerifyFailed"), (WpfBrush)FindResource("DangerBrush"));
            await WriteUpdateLogAsync($"[verify] failed: {ex}");
            ShowCustomDialog(L("Dlg_VerifyFailedTitle"), Loc.Format("Dlg_VerifyFailedBody", ex.Message), MessageBoxButton.OK);
        }
        finally
        {
            if (_isBusy)
            {
                SetBusy(false);
            }

            SetUpdateState(L("Phase_Idle"), string.Empty, 0);
        }
    }

    private async void RepairModButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (ShowCustomDialog(L("Msg_RepairInstallation"), L("Msg_ThisWillReDownloadAndReinstall"), MessageBoxButton.YesNo) == MessageBoxResult.Yes)
        {
            if (!await EnsureSelectedModReleaseLoadedAsync())
            {
                ShowCustomDialog(L("Msg_ReleaseUnavailable"), L("Msg_TheSelectedChannelMetadataCould"), MessageBoxButton.OK);
                return;
            }
            await PerformModInstallation();
        }
    }

    private async Task<bool> EnsureSelectedModReleaseLoadedAsync()
    {
        if (!_lastUpdateCheckUtc.HasValue || DateTime.UtcNow - _lastUpdateCheckUtc.Value > TimeSpan.FromMinutes(5))
        {
            await CheckForUpdatesAsync(showMessages: false);
        }

        return string.IsNullOrWhiteSpace(_lastUpdateError) && !string.IsNullOrWhiteSpace(_latestModVersion);
    }

    private void LaunchButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy)
        {
            return;
        }

        if (_isMigrationRequired)
        {
            ShowMigrationOverlay(_migrationTargetVersion, null, _migrationDownloadUrl);
            return;
        }

        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog(
                Loc.Format("Msg_NotInstalled", GetModDirectoryName(SelectedModReleaseChannel)),
                Loc.Format("Msg_InstallTheModpackOnceBeforeLaunching", GetChannelDisplayName(SelectedModReleaseChannel)),
                MessageBoxButton.OK);
            _navigationService.Navigate("Mods");
            return;
        }

        if (_isModUpdateRequired)
        {
            var result = ShowCustomDialog(
                L("Msg_UpdateAvailable2"),
                L("Msg_TheSelectedModpackVersionIsNot"),
                MessageBoxButton.YesNo);
            if (result != MessageBoxResult.Yes)
            {
                _navigationService.Navigate("Mods");
                return;
            }
        }

        if (string.IsNullOrWhiteSpace(settings.DolphinPath) ||  
            string.IsNullOrWhiteSpace(settings.RomPath) ||
            string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            ShowCustomDialog(L("Msg_SetupRequired"), L("Msg_ConfigureDolphinTheUserFolder"), MessageBoxButton.OK);
            _navigationService.Navigate("Settings");
            return;
        }

        if (IsExecutableRunning(settings.DolphinPath))
        {
            ShowCustomDialog(
                L("Msg_DolphinIsAlreadyRunning"),
                L("Msg_CloseDolphinBeforeLaunchingThe"),
                MessageBoxButton.OK);
            return;
        }

        if (!MarioKartControllerPanel.PrepareControllerModeForLaunch())
        {
            ShowCustomDialog(
                L("Msg_ControllerConfigurationError"),
                L("Msg_TheSelectedControllerModeCould"),
                MessageBoxButton.OK);
            _navigationService.Navigate("Settings");
            return;
        }

        var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var rootDir = GetModRoot(settings, SelectedModReleaseChannel);
        var xmlPath = Path.Combine(rootDir, "Riivolution", $"{modDirectoryName}.xml");
        if (!File.Exists(xmlPath))
        {
            ShowCustomDialog(L("Msg_ModNotFound"), L("Msg_InstallTheVanzakartModpackBefore"), MessageBoxButton.OK);
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

            var jsonPath = Path.Combine(AppContext.BaseDirectory, $"{modDirectoryName}_launcher.json");
            var json = $@"{{
  ""base-file"": ""{EscapeJsonValue(settings.RomPath)}"",
  ""display-name"": ""{modDirectoryName} Modpack"",
  ""riivolution"": {{
    ""patches"": [
      {{
        ""options"": [
          {{ ""choice"": 1, ""option-name"": ""Pack"", ""section-name"": ""{modDirectoryName}"" }},
          {(optionChoice == 2 ? $"{{ \"choice\": 2, \"option-name\": \"My Stuff\", \"section-name\": \"{modDirectoryName}\" }}," : "")}
          {{ ""choice"": {saveChoice}, ""option-name"": ""Seperate Savegame"", ""section-name"": ""{modDirectoryName}"" }}
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

            var userFolderClean = (settings.UserFolderPath ?? "").Trim().TrimEnd('\\', '/');
            var process = Process.Start(new ProcessStartInfo
            {
                FileName = settings.DolphinPath,
                Arguments = $"-b -u \"{userFolderClean}\" -e \"{jsonPath}\"",
                UseShellExecute = true,
                WorkingDirectory = Path.GetDirectoryName(settings.DolphinPath)
            });

            _isGameRunning = true;
            SetBusy(_isBusy);

            TrackGameSession(process);
            SetStatus(L("Msg_GameLaunchedEnjoyVanzakart"), (WpfBrush)FindResource("SuccessBrush"));
            ShowToast(L("Msg_RaceStarted"), L("Msg_VanzakartIsLaunching"));
            RefreshPlayStats();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_LaunchError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private async void CheckUpdatesButton_OnClick(object sender, RoutedEventArgs e) => await CheckForUpdatesAsync(true);

    private async Task CheckForUpdatesAsync(bool showMessages)
    {
        await _updateCheckLock.WaitAsync();
        try
        {
            await CheckForUpdatesCoreAsync(showMessages);
        }
        finally
        {
            _updateCheckLock.Release();
        }
    }

    private async Task CheckForUpdatesCoreAsync(bool showMessages)
    {
        var requestedChannel = SelectedModReleaseChannel;
        var channelRevision = _releaseChannelRevision;
        try
        {
            if (showMessages)
            {
                SetStatus(L("Msg_CheckingForUpdates"), (WpfBrush)FindResource("TextSecondary"));
                SetUpdateState(L("Phase_Checking"), L("Msg_ReadingVanzakartUpdateManifest"), 0);
            }

            await TryFetchEndpointsAsync();

            var noCacheUrl = $"{LauncherConfig.VersionJsonUrl}?t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
            var json = await _networkService.DownloadStringAsync(noCacheUrl);
            var info = JsonSerializer.Deserialize<VersionInfo>(json.TrimStart('\uFEFF', '\u200B')) ?? new VersionInfo();

            await FetchNewsFromServerAsync();
            var modRelease = await ResolveModReleaseAsync(info, requestedChannel);
            if (channelRevision != _releaseChannelRevision)
            {
                return;
            }
            _lastUpdateCheckUtc = DateTime.UtcNow;
            _lastUpdateError = string.Empty;

            _latestModVersion = modRelease.Version;
            if (SelectedModReleaseChannel == ModReleaseChannel.Beta)
            {
                _userPreferences.LastKnownLatestBetaModVersion = modRelease.Version;
            }
            else
            {
                _userPreferences.LastKnownLatestModVersion = modRelease.Version;
            }
            _preferencesService.Save(_userPreferences);
            _latestModUrl = modRelease.ArchiveUrl;
            _latestModMirrors = modRelease.ArchiveMirrors;
            _latestModSha256 = modRelease.ArchiveSha256;
            _latestModManifestUrl = modRelease.ManifestUrl;
            _latestModFilesUrl = modRelease.FilesUrl;
            _latestModFilesMirrors = modRelease.FilesMirrors;
            _latestModHashFilesUrl = modRelease.HashFilesUrl;
            _latestModHashFilesMirrors = modRelease.HashFilesMirrors;
            _latestMusicPackVersion = info.MusicPackVersion;
            _latestMusicPackUrl = LauncherConfig.MusicPackUrl;
            _latestMusicPackMirrors = LauncherConfig.MusicPackMirrors;
            _latestMusicPackSha256 = info.MusicPackSha256;
            _latestMusicPackChangelog = info.MusicPackChangelog ?? Array.Empty<string>();
            _latestMusicPackManifestUrl = LauncherConfig.MusicPackManifestUrl;
            _latestMusicPackFilesUrl = LauncherConfig.MusicPackFilesUrl;
            _latestMusicPackFilesMirrors = LauncherConfig.MusicPackFilesMirrors;
            _latestLauncherUrl = LauncherConfig.LauncherZipUrl;
            _latestLauncherMirrors = LauncherConfig.LauncherMirrors;
            _latestChangelog = info.Changelog ?? Array.Empty<string>();

            // 1.5.5 is the final bridge version: all legacy launcher instances are required to upgrade to 2.0.0.
            var targetVersion = (!string.IsNullOrWhiteSpace(info.LauncherVersion) && (info.LauncherVersion.StartsWith("2.") || info.LauncherVersion.StartsWith("v2.")))
                ? info.LauncherVersion
                : "2.0.0";

            var targetUrl = !string.IsNullOrWhiteSpace(info.NewLauncherUrl)
                ? info.NewLauncherUrl
                : LauncherConfig.NewLauncherZipUrl;

            ShowMigrationOverlay(targetVersion, info.MigrationMessage, targetUrl);

            var currentSettings = BuildSettingsFromUi();
            var selectedModDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
            var selectedMusicPackVersionFile = GetMusicPackVersionFile(SelectedModReleaseChannel);
            var musicPackInstalled = _musicPackService.IsInstalled(currentSettings, selectedModDirectoryName);
            var localMusicPackVersion = musicPackInstalled && File.Exists(selectedMusicPackVersionFile)
                ? File.ReadAllText(selectedMusicPackVersionFile).Trim()
                : string.Empty;
            var musicPackUpdateAvailable = musicPackInstalled && !string.IsNullOrWhiteSpace(info.MusicPackVersion) &&
                                           !string.Equals(localMusicPackVersion, info.MusicPackVersion, StringComparison.OrdinalIgnoreCase);

            if (IsModInstalled(currentSettings))
            {
                var localVersion = GetInstalledModVersion();
                if (!string.IsNullOrWhiteSpace(_latestModVersion) && _latestModVersion != localVersion)
                {
                    _isModUpdateRequired = true;
                    SetStatus(
                        Loc.Format("Msg_ModUpdateAvailableV", _latestModVersion),
                        (WpfBrush)FindResource("WarningBrush"));
                    SetUpdateState(
                        L("Msg_UpdateAvailable2"),
                        Loc.Format("Msg_InstalledLatest", localVersion, _latestModVersion),
                        0);
                    if (showMessages)
                    {
                        ShowToast(
                            L("Msg_UpdateAvailable2"),
                            Loc.Format("Msg_VIsReadyToInstall", selectedModDirectoryName, _latestModVersion));
                    }
                }
                else
                {
                    _isModUpdateRequired = false;
                    SetStatus(L("Msg_ModIsUpToDate"), (WpfBrush)FindResource("SuccessBrush"));
                    SetUpdateState(L("Msg_UpToDate"), L("Msg_NoModUpdateIsRequired"), 100);
                    if (showMessages)
                    {
                        ShowToast(
                            musicPackUpdateAvailable ? L("Msg_MusicPackUpdateAvailable") : L("Msg_NoUpdates"),
                            musicPackUpdateAvailable
                                ? Loc.Format("Msg_VanzakartMusicPackVIsReadyTo", info.MusicPackVersion)
                                : L("Msg_VanzakartAndItsOfficialPackages"));
                    }
                }
            }
            else
            {
                _isModUpdateRequired = true;
                SetStatus(Loc.Format("Msg_InstallToUseThisChannel", selectedModDirectoryName), (WpfBrush)FindResource("WarningBrush"));
                SetUpdateState(L("Msg_InstallationRequired"), Loc.Format("Msg_HasNotBeenInstalledYet", selectedModDirectoryName), 0);
            }

            RefreshAllState();
        }
        catch (Exception ex)
        {
            if (channelRevision != _releaseChannelRevision)
            {
                return;
            }

            _lastUpdateCheckUtc = DateTime.UtcNow;
            _lastUpdateError = ex.Message;
            _latestModVersion = string.Empty;
            SetStatus(L("Msg_UpdateCheckFailed"), (WpfBrush)FindResource("DangerBrush"));
            SetUpdateState(L("Msg_NetworkError"), ex.Message, 0);
            if (showMessages)
            {
                ShowToast(L("Msg_UpdateCheckFailed"), ex.Message);
            }
            RefreshHomeUpdateCard();
            ShowMigrationOverlay("2.0.0", null, LauncherConfig.NewLauncherZipUrl);
        }
    }

    private bool _isMigrationRequired;
    private string _migrationTargetVersion = "2.0.0";
    private string _migrationDownloadUrl = string.Empty;

    public void ShowMigrationOverlay(string targetVersion = "2.0.0", string? customMessage = null, string? downloadUrl = null)
    {
        _isMigrationRequired = true;
        _migrationTargetVersion = string.IsNullOrWhiteSpace(targetVersion) ? "2.0.0" : targetVersion;
        _migrationDownloadUrl = !string.IsNullOrWhiteSpace(downloadUrl) ? downloadUrl : LauncherConfig.NewLauncherZipUrl;

        MigrationVersionBadgeTextBlock.Text = $"VANZAKART LAUNCHER {_migrationTargetVersion.ToUpperInvariant()}";

        if (!string.IsNullOrWhiteSpace(customMessage))
        {
            MigrationCustomMessageTextBlock.Text = customMessage;
            MigrationCustomMessageTextBlock.Visibility = Visibility.Visible;
        }
        else
        {
            MigrationCustomMessageTextBlock.Visibility = Visibility.Collapsed;
        }

        MigrationOverlay.Visibility = Visibility.Visible;
        MigrationOverlay.Opacity = 0;

        if (MigrationCard.RenderTransform is not ScaleTransform scale)
        {
            scale = new ScaleTransform(0.96, 0.96);
            MigrationCard.RenderTransform = scale;
        }

        scale.ScaleX = 0.96;
        scale.ScaleY = 0.96;

        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        MigrationOverlay.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(200)) { EasingFunction = ease });
        scale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(0.96, 1, TimeSpan.FromMilliseconds(240)) { EasingFunction = ease });
        scale.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(0.96, 1, TimeSpan.FromMilliseconds(240)) { EasingFunction = ease });
    }

    private void ExitOldLauncherButton_OnClick(object sender, RoutedEventArgs e)
    {
        System.Windows.Application.Current.Shutdown();
    }

    private void OpenWebsiteDownloadButton_OnClick(object sender, RoutedEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = LauncherConfig.DownloadPageUrl,
                UseShellExecute = true
            });
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Dlg_ErrorTitle"), ex.Message, MessageBoxButton.OK);
        }
    }

    private async void PerformMigrationButton_OnClick(object sender, RoutedEventArgs e)
    {
        await PerformLauncherUpdateAsync(_migrationTargetVersion, _migrationDownloadUrl);
    }

    private async Task PerformLauncherUpdateAsync(string targetVersion, string? customDownloadUrl = null)
    {
        // Persist the currently visible paths before handing control to the
        // external updater. SettingsService stores the authoritative copy
        // outside the installation directory.
        _settingsService.Save(BuildSettingsFromUi());

        _latestLauncherVersion = targetVersion;
        _isDownloadingLauncherUpdate = true;
        SetBusy(true);
        ResetDownloadMetrics();

        var isExeInstaller = (!string.IsNullOrWhiteSpace(customDownloadUrl) && customDownloadUrl.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)) ||
                             LauncherConfig.NewLauncherZipUrl.EndsWith(".exe", StringComparison.OrdinalIgnoreCase);

        if (MigrationOverlay.Visibility == Visibility.Visible)
        {
            MigrationProgressSection.Visibility = Visibility.Visible;
            MigrationProgressBar.IsIndeterminate = false;
            MigrationProgressBar.Value = 0;
            MigrationProgressPercentTextBlock.Text = "0%";
            MigrationProgressStatusTextBlock.Text = "Downloading new launcher installer...";
            PerformMigrationButton.IsEnabled = false;
            OpenWebsiteDownloadButton.IsEnabled = false;
            ExitOldLauncherButton.IsEnabled = false;
        }

        DownloadProgressBar.Visibility = Visibility.Visible;
        DownloadProgressBar.IsIndeterminate = false;
        DownloadProgressBar.Value = 0;
        SetStatus(L("Msg_DownloadingLauncherUpdate"), (WpfBrush)FindResource("TextSecondary"));
        SetUpdateState(L("Msg_LauncherUpdate2"), L("Msg_DownloadingNewLauncherPackage"), 0);

        var downloadTargetFile = isExeInstaller
            ? Path.Combine(Path.GetTempPath(), $"VanzaKart-Setup_{targetVersion}_windows-x86_64.exe")
            : Path.Combine(AppContext.BaseDirectory, "Launcher_Update.zip");

        try
        {
            var progress = new Progress<(long current, long total)>(p =>
            {
                UpdateDownloadProgress(p.current, p.total);
                if (p.total > 0 && MigrationOverlay.Visibility == Visibility.Visible)
                {
                    var percent = Math.Clamp((int)((p.current * 100) / p.total), 0, 100);
                    var currentMb = (p.current / 1048576.0).ToString("0.0");
                    var totalMb = (p.total / 1048576.0).ToString("0.0");
                    MigrationProgressBar.Value = percent;
                    MigrationProgressPercentTextBlock.Text = $"{percent}%";
                    MigrationProgressStatusTextBlock.Text = $"Downloading installer: {currentMb} MB / {totalMb} MB";
                }
            });

            var mirrorList = !string.IsNullOrWhiteSpace(customDownloadUrl)
                ? [customDownloadUrl]
                : BuildLauncherMirrorList();

            await _networkService.DownloadFileWithResumeAsync(mirrorList, downloadTargetFile, progress);

            if (isExeInstaller)
            {
                if (!File.Exists(downloadTargetFile))
                {
                    throw new FileNotFoundException("Downloaded setup executable could not be found.", downloadTargetFile);
                }

                // Clean up old shortcuts, registry entries, schedule old files removal, and launch new setup
                CleanupAndUninstallOldLauncher(downloadTargetFile);

                // Immediately terminate old launcher
                System.Windows.Application.Current.Shutdown();
                return;
            }

            await _archiveService.ValidateZipAsync(downloadTargetFile);

            var launcherPath = Environment.ProcessPath ?? Path.Combine(AppContext.BaseDirectory, "VanzaKart Launcher.exe");
            var safeTargetVersion = new string(targetVersion
                .Where(character => char.IsLetterOrDigit(character) || character is '.' or '-' or '_' or '+')
                .ToArray());
            if (string.IsNullOrWhiteSpace(safeTargetVersion))
            {
                throw new InvalidDataException("The launcher update contains an invalid version number.");
            }

            LauncherUpdateHostService.Start(
                downloadTargetFile,
                AppContext.BaseDirectory,
                launcherPath,
                safeTargetVersion);
            System.Windows.Application.Current.Shutdown();
        }
        catch (Exception ex)
        {
            SetStatus(L("Msg_LauncherUpdateFailed"), (WpfBrush)FindResource("DangerBrush"));
            _isDownloadingLauncherUpdate = false;
            _latestLauncherVersion = string.Empty;
            SetUpdateState(L("Phase_Failed"), ex.Message, 0);

            if (MigrationOverlay.Visibility == Visibility.Visible)
            {
                MigrationProgressStatusTextBlock.Text = $"Download error: {ex.Message}";
                PerformMigrationButton.IsEnabled = true;
                OpenWebsiteDownloadButton.IsEnabled = true;
                ExitOldLauncherButton.IsEnabled = true;
            }

            ShowCustomDialog(L("Msg_LauncherUpdateError"), ex.Message, MessageBoxButton.OK);
            SetBusy(false);
        }
    }

    private static void CleanupAndUninstallOldLauncher(string setupExePath)
    {
        try
        {
            // 1. Remove old shortcuts from Desktop, Start Menu, Quick Launch
            var desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
            TryDeleteFile(Path.Combine(desktop, "VanzaKart Launcher.lnk"));

            var programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
            var startFolder = Path.Combine(programs, "VanzaKart");
            TryDeleteFile(Path.Combine(startFolder, "VanzaKart Launcher.lnk"));
            TryDeleteEmptyDirectory(startFolder);

            var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            TryDeleteFile(Path.Combine(appData, @"Microsoft\Internet Explorer\Quick Launch\VanzaKart Launcher.lnk"));

            // 2. Remove old uninstaller registry keys
            try
            {
                Microsoft.Win32.Registry.CurrentUser.DeleteSubKeyTree(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\VanzaKartLauncher", false);
            }
            catch { }

            try
            {
                Microsoft.Win32.Registry.CurrentUser.DeleteSubKeyTree(@"Software\Microsoft\Windows\CurrentVersion\App Paths\VanzaKart Launcher.exe", false);
            }
            catch { }

            // 3. Launch the new Tauri setup executable
            Process.Start(new ProcessStartInfo
            {
                FileName = setupExePath,
                UseShellExecute = true
            });

            // 4. Schedule old installation directory cleanup after the process terminates
            var installDir = Path.GetFullPath(AppContext.BaseDirectory);
            // Protect developer root / git repository from accidental wipe
            var isDevEnvironment = File.Exists(Path.Combine(installDir, "VanzaKartLauncher.csproj")) ||
                                   File.Exists(Path.Combine(installDir, "..", "VanzaKartLauncher.sln")) ||
                                   installDir.Contains(@"\bin\Debug\", StringComparison.OrdinalIgnoreCase);

            if (!isDevEnvironment && installDir.Length >= 8 && !string.Equals(Path.GetPathRoot(installDir), installDir, StringComparison.OrdinalIgnoreCase))
            {
                var batchPath = Path.Combine(Path.GetTempPath(), $"vanzakart_migration_cleanup_{Guid.NewGuid():N}.bat");
                var currentPid = Environment.ProcessId;
                var script = string.Join(Environment.NewLine, new[]
                {
                    "@echo off",
                    ":waitloop",
                    $"tasklist /FI \"PID eq {currentPid}\" 2>nul | find /I \"{currentPid}\" >nul",
                    "if not errorlevel 1 (",
                    "    timeout /t 1 /nobreak >nul",
                    "    goto waitloop",
                    ")",
                    "timeout /t 1 /nobreak >nul",
                    $"rd /s /q \"{installDir}\"",
                    "del \"%~f0\""
                });

                File.WriteAllText(batchPath, script, System.Text.Encoding.ASCII);
                Process.Start(new ProcessStartInfo
                {
                    FileName = batchPath,
                    UseShellExecute = true,
                    WindowStyle = ProcessWindowStyle.Hidden,
                    CreateNoWindow = true
                });
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Cleanup error during migration: {ex.Message}");
        }
    }

    private static void TryDeleteFile(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch
        {
        }
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

    private static bool IsVersionHigher(string remoteVersion, string currentVersion)
    {
        if (string.IsNullOrWhiteSpace(remoteVersion)) return false;
        if (string.IsNullOrWhiteSpace(currentVersion)) return true;

        var cleanRemote = remoteVersion.TrimStart('v', 'V');
        var cleanCurrent = currentVersion.TrimStart('v', 'V');

        if (Version.TryParse(cleanRemote, out var vRemote) && Version.TryParse(cleanCurrent, out var vCurrent))
        {
            return vRemote > vCurrent;
        }

        return !string.Equals(cleanRemote, cleanCurrent, StringComparison.OrdinalIgnoreCase);
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
        var channelDefaultUrl = SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? LauncherConfig.BetaModUrl
            : LauncherConfig.ModUrl;
        if (!channelDefaultUrl.Equals(_latestModUrl, StringComparison.OrdinalIgnoreCase))
        {
            yield return channelDefaultUrl;
        }
    }

    private IEnumerable<string> BuildModFileMirrorList(ModManifestFile file)
    {
        var fileRelativePath = file.Path;
        var escapedPath = EscapeRelativeUrlPath(fileRelativePath);
        var rawPath = fileRelativePath.Replace('\\', '/');
        var hashPath = BuildHashAddressedRelativePath(file.Sha256);
        var hashFileName = file.Sha256.Trim().ToLowerInvariant();
        var fileUrls = new List<string>();

        // 1. Percorsi file diretti dentro files/
        if (!string.IsNullOrWhiteSpace(_latestModFilesUrl))
        {
            fileUrls.AddRange(BuildDifferentialFileUrlCandidates(_latestModFilesUrl, escapedPath, rawPath));
        }

        if (_latestModFilesMirrors != null)
        {
            foreach (var mirror in _latestModFilesMirrors)
            {
                if (!string.IsNullOrWhiteSpace(mirror))
                {
                    fileUrls.AddRange(BuildDifferentialFileUrlCandidates(mirror, escapedPath, rawPath));
                }
            }
        }

        var defaultFilesUrl = SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? LauncherConfig.BetaModFilesUrl
            : LauncherConfig.ModFilesUrl;
        if (!string.IsNullOrWhiteSpace(defaultFilesUrl) && defaultFilesUrl != _latestModFilesUrl)
        {
            fileUrls.AddRange(BuildDifferentialFileUrlCandidates(defaultFilesUrl, escapedPath, rawPath));
        }

        // 2. Percorsi file per hash nella cartella indipendente _by_sha256/
        if (!string.IsNullOrWhiteSpace(_latestModHashFilesUrl))
        {
            fileUrls.AddRange(BuildDifferentialFileUrlCandidates(_latestModHashFilesUrl, hashFileName, hashFileName));
        }
        else if (!string.IsNullOrWhiteSpace(_latestModFilesUrl))
        {
            var parent = ResolveParentBaseUrl(_latestModFilesUrl);
            if (!string.IsNullOrWhiteSpace(parent))
            {
                fileUrls.AddRange(BuildDifferentialFileUrlCandidates(parent, hashPath, hashPath));
            }
        }

        if (_latestModHashFilesMirrors != null && _latestModHashFilesMirrors.Length > 0)
        {
            foreach (var mirror in _latestModHashFilesMirrors)
            {
                if (!string.IsNullOrWhiteSpace(mirror))
                {
                    fileUrls.AddRange(BuildDifferentialFileUrlCandidates(mirror, hashFileName, hashFileName));
                }
            }
        }
        else if (_latestModFilesMirrors != null)
        {
            foreach (var mirror in _latestModFilesMirrors)
            {
                if (!string.IsNullOrWhiteSpace(mirror))
                {
                    var parent = ResolveParentBaseUrl(mirror);
                    if (!string.IsNullOrWhiteSpace(parent))
                    {
                        fileUrls.AddRange(BuildDifferentialFileUrlCandidates(parent, hashPath, hashPath));
                    }
                }
            }
        }

        var defaultHashFilesUrl = SelectedModReleaseChannel == ModReleaseChannel.Beta
            ? LauncherConfig.BetaModHashFilesUrl
            : LauncherConfig.ModHashFilesUrl;
        if (!string.IsNullOrWhiteSpace(defaultHashFilesUrl) && defaultHashFilesUrl != _latestModHashFilesUrl)
        {
            fileUrls.AddRange(BuildDifferentialFileUrlCandidates(defaultHashFilesUrl, hashFileName, hashFileName));
        }

        foreach (var url in fileUrls.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            yield return url;
        }
    }

    private async Task<ModReleaseMetadata> ResolveModReleaseAsync(VersionInfo stableInfo, ModReleaseChannel channel)
    {
        if (channel == ModReleaseChannel.Stable)
        {
            if (string.IsNullOrWhiteSpace(stableInfo.ModVersion))
            {
                throw new InvalidDataException("The stable versions manifest does not contain a mod version.");
            }

            return new ModReleaseMetadata(
                stableInfo.ModVersion,
                LauncherConfig.ModUrl,
                LauncherConfig.ModMirrors,
                stableInfo.ModSha256,
                LauncherConfig.ModManifestUrl,
                LauncherConfig.ModFilesUrl,
                LauncherConfig.ModFilesMirrors,
                LauncherConfig.ModHashFilesUrl,
                LauncherConfig.ModHashFilesMirrors);
        }

        var betaManifestUrl = LauncherConfig.BetaModManifestUrl;
        var manifestJson = await _networkService.DownloadStringAsync(AddNoCacheQuery(betaManifestUrl));
        var manifest = JsonSerializer.Deserialize<ModManifest>(manifestJson.TrimStart('\uFEFF', '\u200B'))
            ?? throw new InvalidDataException("The Beta update manifest is invalid.");
        ValidateModManifest(manifest);

        if (!string.IsNullOrWhiteSpace(stableInfo.BetaModVersion) &&
            !stableInfo.BetaModVersion.Equals(manifest.ModVersion, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException(
                $"Beta metadata is out of sync: versions.json reports {stableInfo.BetaModVersion}, but the Beta manifest reports {manifest.ModVersion}.");
        }

        return new ModReleaseMetadata(
            manifest.ModVersion,
            LauncherConfig.BetaModUrl,
            LauncherConfig.BetaModMirrors,
            string.IsNullOrWhiteSpace(manifest.ArchiveSha256) ? stableInfo.BetaModSha256 : manifest.ArchiveSha256,
            betaManifestUrl,
            LauncherConfig.BetaModFilesUrl,
            LauncherConfig.BetaModFilesMirrors,
            LauncherConfig.BetaModHashFilesUrl,
            LauncherConfig.BetaModHashFilesMirrors);
    }

    private sealed record ModReleaseMetadata(
        string Version,
        string ArchiveUrl,
        string[] ArchiveMirrors,
        string ArchiveSha256,
        string ManifestUrl,
        string FilesUrl,
        string[] FilesMirrors,
        string HashFilesUrl,
        string[] HashFilesMirrors);

    private static string BuildHashAddressedRelativePath(string sha256)
    {
        return $"_by_sha256/{sha256.Trim().ToLowerInvariant()}";
    }

    private static string ResolveParentBaseUrl(string filesUrl)
    {
        if (string.IsNullOrWhiteSpace(filesUrl))
            return string.Empty;

        var trimmed = filesUrl.Trim().TrimEnd('/');
        if (trimmed.EndsWith("/files", StringComparison.OrdinalIgnoreCase))
        {
            return trimmed[..^"/files".Length];
        }

        if (Uri.TryCreate(trimmed, UriKind.Absolute, out var uri))
        {
            var segments = uri.AbsolutePath.TrimEnd('/').Split('/', StringSplitOptions.RemoveEmptyEntries);
            if (segments.Length > 1)
            {
                var parentPath = "/" + string.Join('/', segments.Take(segments.Length - 1));
                var builder = new UriBuilder(uri)
                {
                    Path = parentPath,
                    Query = null
                };
                return builder.Uri.ToString().TrimEnd('/');
            }
        }

        return trimmed;
    }

    private static void ValidateModManifest(ModManifest? manifest)
    {
        if (manifest == null || string.IsNullOrWhiteSpace(manifest.ModVersion) || manifest.Files == null || manifest.Files.Count == 0)
        {
            throw new InvalidDataException("The mod update manifest is empty or invalid.");
        }

        var paths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var file in manifest.Files)
        {
            var normalizedPath = (file.Path ?? string.Empty).Replace('\\', '/');
            var sha256 = file.Sha256 ?? string.Empty;
            if (string.IsNullOrWhiteSpace(normalizedPath) ||
                normalizedPath.StartsWith('/') ||
                normalizedPath.Split('/').Any(segment => segment is "" or "." or "..") ||
                file.Size < 0 ||
                sha256.Length != 64 ||
                !sha256.All(Uri.IsHexDigit) ||
                !paths.Add(normalizedPath))
            {
                throw new InvalidDataException($"Invalid or duplicate mod manifest entry: {file.Path}");
            }
        }

        var archiveSha256 = manifest.ArchiveSha256 ?? string.Empty;
        if (!string.IsNullOrWhiteSpace(archiveSha256) &&
            (archiveSha256.Length != 64 || !archiveSha256.All(Uri.IsHexDigit)))
        {
            throw new InvalidDataException("The mod archive hash in the manifest is invalid.");
        }
    }

    private static IEnumerable<string> BuildDifferentialFileUrlCandidates(string baseUrl, string escapedPath, string rawPath)
    {
        var escapedUrl = $"{baseUrl.TrimEnd('/')}/{escapedPath}";
        var rawUrl = $"{baseUrl.TrimEnd('/')}/{rawPath}";

        yield return AddNoCacheQuery(escapedUrl);
        if (!rawUrl.Equals(escapedUrl, StringComparison.OrdinalIgnoreCase))
        {
            yield return AddNoCacheQuery(rawUrl);
        }

        yield return escapedUrl;
        if (!rawUrl.Equals(escapedUrl, StringComparison.OrdinalIgnoreCase))
        {
            yield return rawUrl;
        }
    }

    private static string AddNoCacheQuery(string url)
    {
        if (string.IsNullOrWhiteSpace(url))
            return url;

        var separator = url.Contains('?') ? '&' : '?';
        return $"{url}{separator}t={DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
    }

    private static string EscapeRelativeUrlPath(string relativePath)
    {
        return string.Join(
            "/",
            relativePath
                .Replace('\\', '/')
                .Split('/', StringSplitOptions.RemoveEmptyEntries)
                .Select(Uri.EscapeDataString));
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

    private IEnumerable<string> BuildMusicPackMirrorList()
    {
        if (!string.IsNullOrWhiteSpace(_latestMusicPackUrl)) yield return _latestMusicPackUrl;
        foreach (var mirror in _latestMusicPackMirrors)
        {
            if (!string.IsNullOrWhiteSpace(mirror)) yield return mirror;
        }
        if (!string.Equals(_latestMusicPackUrl, LauncherConfig.MusicPackUrl, StringComparison.OrdinalIgnoreCase))
            yield return LauncherConfig.MusicPackUrl;
    }

    private IEnumerable<string> BuildMusicPackFilesBaseUrls()
    {
        if (!string.IsNullOrWhiteSpace(_latestMusicPackFilesUrl)) yield return _latestMusicPackFilesUrl;
        foreach (var mirror in _latestMusicPackFilesMirrors)
        {
            if (!string.IsNullOrWhiteSpace(mirror)) yield return mirror;
        }
        if (!string.Equals(_latestMusicPackFilesUrl, LauncherConfig.MusicPackFilesUrl, StringComparison.OrdinalIgnoreCase))
            yield return LauncherConfig.MusicPackFilesUrl;
    }

    private void ResetDownloadMetrics()
    {
        _downloadStopwatch.Reset();
        _downloadBaselineBytes = -1;
        _lastDownloadSampleBytes = 0;
        _lastDownloadSampleTime = TimeSpan.Zero;
        _smoothedDownloadBytesPerSecond = 0;
        UpdateSpeedTextBlock.Text = string.Empty;
        UpdatePercentTextBlock.Text = "0%";
    }

    private void UpdateDownloadProgress(long current, long total)
    {
        if (_downloadBaselineBytes < 0)
        {
            _downloadBaselineBytes = current;
            _lastDownloadSampleBytes = current;
            _lastDownloadSampleTime = TimeSpan.Zero;
            _downloadStopwatch.Restart();
        }

        var percent = total <= 0 ? 0 : (double)current / total * 100;
        var elapsed = _downloadStopwatch.Elapsed;
        var sampleSeconds = (elapsed - _lastDownloadSampleTime).TotalSeconds;
        if (sampleSeconds >= 0.25)
        {
            var instantSpeed = Math.Max(0, current - _lastDownloadSampleBytes) / sampleSeconds;
            _smoothedDownloadBytesPerSecond = _smoothedDownloadBytesPerSecond <= 0
                ? instantSpeed
                : (_smoothedDownloadBytesPerSecond * 0.65) + (instantSpeed * 0.35);
            _lastDownloadSampleBytes = current;
            _lastDownloadSampleTime = elapsed;
        }

        SetUpdateState(L("Phase_Downloading"), $"{FormatBytes(current)} / {FormatBytes(total)}", percent);
        UpdateSpeedTextBlock.Text = _smoothedDownloadBytesPerSecond <= 0
            ? L("Msg_MeasuringSpeed")
            : $"{FormatBytes((long)_smoothedDownloadBytesPerSecond)}/s";
        SetStatus(Loc.Format("Msg_DownloadingPercent", percent.ToString("F0", CultureInfo.InvariantCulture)), (WpfBrush)FindResource("TextSecondary"));
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
            ShowCustomDialog(L("Msg_FolderNotFound"), L("Msg_TheModFolderDoesNotExistYet"), MessageBoxButton.OK);
        }
    }

    private void OpenAddonsFolderButton_OnClick(object sender, RoutedEventArgs e)
    {
        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog(L("Msg_ModNotInstalled"), L("Msg_InstallVanzakartBeforeOpening"), MessageBoxButton.OK);
            return;
        }

        var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var folder = Path.Combine(settings.GetModFolder(), modDirectoryName, modDirectoryName, "My Stuff");
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
            ShowToast(L("Msg_DolphinDetected"), L("Msg_TheUserFolderWasDetectedAutomatically"));
        }

        SaveSettingsFromUi();
        _settingsUiBaseline = CaptureSettingsUiState();
        _hasUnsavedChanges = false;
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
            _settingsUiBaseline = CaptureSettingsUiState();
            _hasUnsavedChanges = false;
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
        _settingsUiBaseline = CaptureSettingsUiState();
        _hasUnsavedChanges = false;
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
                : Loc.Format("Msg_Downloaded", FormatBytes(item.BytesReceived));
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

            ShowToast(L("Msg_MiiSetupReady"), L("Msg_RenderAssetsInstalledSuccessfully"));
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MiiSetupError"), ex.Message, MessageBoxButton.OK);
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
            var backup = await _saveManagerService.BackupPrimarySaveAsync(
                BuildSettingsFromUi(),
                GetModDirectoryName(SelectedModReleaseChannel));
            ShowToast(L("Msg_BackupCreated"), backup);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_BackupError"), ex.Message, MessageBoxButton.OK);
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
            await _saveManagerService.ImportSaveFileAsync(
                BuildSettingsFromUi(),
                dialog.FileName,
                GetModDirectoryName(SelectedModReleaseChannel));
            ShowToast(L("Msg_SaveImported"), L("Msg_ABackupWasCreatedBeforeReplacing"));
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_ImportError"), ex.Message, MessageBoxButton.OK);
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
            await _saveManagerService.ExportPrimarySaveAsync(
                BuildSettingsFromUi(),
                dialog.FileName,
                GetModDirectoryName(SelectedModReleaseChannel));
            ShowToast(L("Msg_SaveExported"), dialog.FileName);
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_ExportError"), ex.Message, MessageBoxButton.OK);
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
            ShowCustomDialog(L("Msg_MiiCreationError"), ex.Message, MessageBoxButton.OK);
        }
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
            ShowToast(L("Msg_MiiImported"), synced ? Loc.Format("Msg_WasSyncedToDolphin", profile.Name) : profile.Name);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MiiImportError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private async void ExportMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog(L("Msg_SelectAMii"), L("Msg_SelectAMiiBeforeExporting"), MessageBoxButton.OK);
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
            ShowToast(L("Msg_MiiExported"), dialog.FileName);
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MiiExportError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private async void DuplicateMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog(L("Msg_SelectAMii"), L("Msg_SelectAMiiBeforeDuplicating"), MessageBoxButton.OK);
            return;
        }

        try
        {
            var duplicate = await _saveManagerService.DuplicateMiiProfileAsync(selected.Id);
            ShowToast(L("Msg_MiiDuplicated"), duplicate.Name);
            RefreshLicenseView();
            OpenMiiEditor(duplicate.Id);
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MiiDuplicateError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private void DeleteMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog(L("Msg_SelectAMii"), L("Msg_SelectAMiiBeforeDeleting"), MessageBoxButton.OK);
            return;
        }

        if (ShowCustomDialog(L("Msg_DeleteMii"), Loc.Format("Msg_DeleteFromTheLibraryThisCannot", selected.Name), MessageBoxButton.YesNo) != MessageBoxResult.Yes)
        {
            return;
        }

        try
        {
            _saveManagerService.DeleteMiiProfile(selected.Id);
            ShowToast(L("Msg_MiiDeleted"), selected.Name);
            RefreshLicenseView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MiiDeleteError"), ex.Message, MessageBoxButton.OK);
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
            ShowCustomDialog(L("Msg_MiiSelectionError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private void EditMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        var selected = MiiCardsListBox.SelectedItem as LauncherMiiProfile;
        if (selected == null)
        {
            ShowCustomDialog(L("Msg_SelectAMii"), L("Msg_SelectAMiiBeforeEditing"), MessageBoxButton.OK);
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
            ShowCustomDialog(L("Msg_MiiEditorError"), ex.Message, MessageBoxButton.OK);
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
        var activeModDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var activeLicensePath = _friendsViewModel?.ActiveLicense?.FilePath;
        var profile = !string.IsNullOrWhiteSpace(activeLicensePath)
            ? _saveManagerService.GetSaveProfiles(settings, activeModDirectoryName).FirstOrDefault(item => string.Equals(item.FilePath, activeLicensePath, StringComparison.OrdinalIgnoreCase))
            : _saveManagerService.GetSaveProfiles(settings, activeModDirectoryName).FirstOrDefault(item => !item.IsEmpty);

        var folder = !string.IsNullOrWhiteSpace(profile?.FilePath)
            ? Path.GetDirectoryName(profile.FilePath)
            : string.Empty;

        if (!string.IsNullOrWhiteSpace(folder) && Directory.Exists(folder))
        {
            OpenFolder(folder);
        }
        else
        {
            ShowCustomDialog(L("Msg_FolderNotFound"), L("Msg_NoDetectedLicenseSaveFolderIs"), MessageBoxButton.OK);
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
            ShowCustomDialog(L("Msg_RendererLog"), L("Msg_NoRendererLogHasBeenCreatedYet"), MessageBoxButton.OK);
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
            ShowToast(L("Msg_MiiSavedLocally"), ex.Message);
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
                ShowToast(L("Msg_MiiImportSkipped"), ex.Message);
            }
        }

        if (imported > 0)
        {
            ShowToast(L("Msg_MiiImportComplete"), Loc.Format("Msg_MiiFileSImported", imported));
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
            ShowCustomDialog(L("Msg_ModNotInstalled"), L("Msg_InstallVanzakartBeforeImporting"), MessageBoxButton.OK);
            return;
        }

        var files = (string[])e.Data.GetData(System.Windows.DataFormats.FileDrop);

        try
        {
            foreach (var path in files)
            {
                if (Directory.Exists(path) || File.Exists(path))
                    await _addonManagerService.ImportAsync(
                        settings,
                        path,
                        modDirectoryName: GetModDirectoryName(SelectedModReleaseChannel));
            }

            ShowToast(L("Msg_AddonsImported"), L("Msg_TheAddonsAreInstalledAndEnabled"));
            RefreshModsView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_ImportError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private void InstalledAddonsTabButton_OnClick(object sender, RoutedEventArgs e)
    {
        InstalledAddonsPanel.Visibility = Visibility.Visible;
        GameBananaPanel.Visibility = Visibility.Collapsed;
        InstalledAddonsTabButton.Style = (Style)FindResource("CompactPrimaryButton");
        GameBananaTabButton.Style = (Style)FindResource("CompactButton");
        RefreshInstalledAddons();
    }

    private async void MusicPackInstallButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy) return;
        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog(L("Msg_ModpackRequired"), L("Msg_InstallTheVanzakartModpackBefore2"), MessageBoxButton.OK);
            return;
        }

        if (string.IsNullOrWhiteSpace(_latestMusicPackVersion))
        {
            await CheckForUpdatesAsync(showMessages: false);
            if (string.IsNullOrWhiteSpace(_latestMusicPackVersion))
            {
                ShowCustomDialog(L("Msg_MusicPackMetadataUnavailable"), L("Msg_TheOfficialManifestDoesNotContain"), MessageBoxButton.OK);
                return;
            }
        }

        var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
        var musicPackVersionFile = GetMusicPackVersionFile(SelectedModReleaseChannel);
        var alreadyCurrent = _musicPackService.IsInstalled(settings, modDirectoryName) && File.Exists(musicPackVersionFile) &&
                             string.Equals(File.ReadAllText(musicPackVersionFile).Trim(), _latestMusicPackVersion, StringComparison.OrdinalIgnoreCase);
        if (alreadyCurrent && ShowCustomDialog(L("Msg_MusicPackUpToDate"), L("Msg_TheLatestMusicPackIsAlreadyInstalled"), MessageBoxButton.YesNo) != MessageBoxResult.Yes)
            return;

        SetBusy(true);
        MusicPackInstallButton.IsEnabled = false;
        using var cancellation = new CancellationTokenSource();
        var dialog = new AddonDownloadDialog(
            "VanzaKart Music Pack",
            MusicPackService.FileName,
            "OFFICIAL VANZAKART PACKAGE",
            "Connecting to the VanzaKart download server...") { Owner = this };
        dialog.CancelRequested += cancellation.Cancel;
        dialog.Show();

        try
        {
            var progress = new Progress<(long current, long total)>(value => dialog.UpdateDownload(value.current, value.total));
            var stages = new Progress<string>(dialog.SetStage);
            await _musicPackService.InstallAsync(settings, BuildMusicPackMirrorList().Distinct(StringComparer.OrdinalIgnoreCase),
                _latestMusicPackSha256, _latestMusicPackManifestUrl,
                BuildMusicPackFilesBaseUrls().Distinct(StringComparer.OrdinalIgnoreCase), progress, stages, cancellation.Token,
                modDirectoryName);
            File.WriteAllText(musicPackVersionFile, _latestMusicPackVersion);
            dialog.MarkCompleted(L("Msg_MusicPackInstalledTitle"), L("Msg_MusicPackInstalledBody"));
            ShowToast(L("Msg_MusicPackReady"), Loc.Format("Msg_VersionInstalled", _latestMusicPackVersion));
        }
        catch (OperationCanceledException)
        {
            dialog.MarkCancelled();
        }
        catch (Exception ex)
        {
            dialog.MarkFailed(ex.Message);
        }
        finally
        {
            SetBusy(false);
            RefreshModsView();
        }
    }

    private async void MusicPackEnabledCheckBox_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy || sender is not CheckBox checkBox) return;
        var enabled = checkBox.IsChecked == true;
        checkBox.IsEnabled = false;
        try
        {
            await _musicPackService.SetEnabledAsync(
                BuildSettingsFromUi(),
                enabled,
                modDirectoryName: GetModDirectoryName(SelectedModReleaseChannel));
            ShowToast(enabled ? L("Msg_MusicPackEnabled") : L("Msg_MusicPackDisabled"),
                enabled ? L("Msg_ItsFilesAreActiveInMyStuff") : L("Msg_ItsFilesWereRemovedFromMyStuff"));
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MusicPackError"), ex.Message, MessageBoxButton.OK);
        }
        finally
        {
            RefreshModsView();
        }
    }

    private async void MusicPackRemoveButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy || ShowCustomDialog(L("Msg_RemoveMusicPack"), L("Msg_RemoveTheOfficialMusicPackFrom"), MessageBoxButton.YesNo) != MessageBoxResult.Yes)
            return;
        try
        {
            var modDirectoryName = GetModDirectoryName(SelectedModReleaseChannel);
            await _musicPackService.UninstallAsync(BuildSettingsFromUi(), modDirectoryName: modDirectoryName);
            var musicPackVersionFile = GetMusicPackVersionFile(SelectedModReleaseChannel);
            if (File.Exists(musicPackVersionFile)) File.Delete(musicPackVersionFile);
            ShowToast(L("Msg_MusicPackRemoved"), L("Msg_TheCoreModpackWasNotChanged"));
            RefreshModsView();
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_MusicPackRemovalFailed"), ex.Message, MessageBoxButton.OK);
        }
    }

    private async void GameBananaTabButton_OnClick(object sender, RoutedEventArgs e)
    {
        InstalledAddonsPanel.Visibility = Visibility.Collapsed;
        GameBananaPanel.Visibility = Visibility.Visible;
        InstalledAddonsTabButton.Style = (Style)FindResource("CompactButton");
        GameBananaTabButton.Style = (Style)FindResource("CompactPrimaryButton");
        if (!_gameBananaLoaded) await SearchGameBananaAsync();
    }

    private async void GameBananaSearchButton_OnClick(object sender, RoutedEventArgs e) => await SearchGameBananaAsync();

    private async void GameBananaSearchTextBox_OnKeyDown(object sender, WpfKeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            await SearchGameBananaAsync();
        }
    }

    private async void GameBananaSortComboBox_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!IsLoaded || !_gameBananaLoaded) return;
        await SearchGameBananaAsync();
    }

    private string GetGameBananaSort()
    {
        return (GameBananaSortComboBox.SelectedItem as ComboBoxItem)?.Tag as string ?? "Generic_Newest";
    }

    private async Task SearchGameBananaAsync(bool append = false)
    {
        if (append && (_isLoadingGameBanana || !_gameBananaHasMore)) return;
        _gameBananaSearchCts?.Cancel();
        _gameBananaSearchCts = new CancellationTokenSource();
        var token = _gameBananaSearchCts.Token;
        _isLoadingGameBanana = true;
        var requestedPage = append ? _gameBananaPage + 1 : 1;
        GameBananaStatusTextBlock.Visibility = Visibility.Visible;
        GameBananaStatusTextBlock.Text = append ? L("Msg_LoadingMoreMarioKartWiiAddons") : L("Msg_LoadingMarioKartWiiAddons");
        GameBananaSearchButton.IsEnabled = false;
        try
        {
            var result = await _gameBananaService.SearchAsync(GameBananaSearchTextBox.Text, GetGameBananaSort(), requestedPage, token);
            if (!append) _gameBananaMods.Clear();
            foreach (var mod in result.Mods)
            {
                if (_gameBananaMods.All(existing => existing.Id != mod.Id)) _gameBananaMods.Add(mod);
            }
            _gameBananaPage = requestedPage;
            _gameBananaLoaded = true;
            _gameBananaHasMore = result.HasMore;
            GameBananaStatusTextBlock.Text = _gameBananaMods.Count == 0
                ? L("Msg_NoCompatibleMarioKartWiiAddons")
                : string.IsNullOrWhiteSpace(GameBananaSearchTextBox.Text)
                    ? $"Showing {_gameBananaMods.Count:N0} addons • {result.TotalAvailable:N0} addons available on GameBanana."
                    : $"Showing {_gameBananaMods.Count:N0} matching Mario Kart Wii addons.";
        }
        catch (OperationCanceledException) { }
        catch (Exception ex)
        {
            GameBananaStatusTextBlock.Text = Loc.Format("Msg_GamebananaIsUnavailable", ex.Message);
        }
        finally
        {
            _isLoadingGameBanana = false;
            GameBananaSearchButton.IsEnabled = !_isBusy;
        }
    }

    private async void MainContentScrollViewer_OnScrollChanged(object sender, ScrollChangedEventArgs e)
    {
        if (_currentTab != "Mods" || GameBananaPanel.Visibility != Visibility.Visible ||
            !_gameBananaLoaded || !_gameBananaHasMore || _isLoadingGameBanana)
            return;

        const double preloadDistance = 220;
        if (e.VerticalOffset + e.ViewportHeight >= e.ExtentHeight - preloadDistance)
            await SearchGameBananaAsync(append: true);
    }

    private async void InstallGameBananaModButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_isBusy || sender is not FrameworkElement installButton || installButton.Tag is not GameBananaMod mod) return;
        var settings = BuildSettingsFromUi();
        if (!IsModInstalled(settings))
        {
            ShowCustomDialog(L("Msg_ModNotInstalled"), L("Msg_InstallVanzakartBeforeInstalling"), MessageBoxButton.OK);
            return;
        }

        var selectedFile = mod.DefaultFile;
        if (mod.Files.Count > 1)
        {
            var picker = new GameBananaFilePickerDialog(mod) { Owner = this };
            if (picker.ShowDialog() != true || picker.SelectedFile == null) return;
            selectedFile = picker.SelectedFile;
        }
        if (selectedFile == null)
        {
            ShowCustomDialog(L("Msg_NoDownloadAvailable"), L("Msg_GamebananaDidNotProvideAnInstallable"), MessageBoxButton.OK);
            return;
        }

        SetBusy(true);
        installButton.IsEnabled = false;
        GameBananaStatusTextBlock.Text = Loc.Format("Msg_Downloading", mod.Name);
        using var cancellation = new CancellationTokenSource();
        var dialog = new AddonDownloadDialog(mod.Name, selectedFile.FileName) { Owner = this };
        dialog.CancelRequested += cancellation.Cancel;
        dialog.Show();
        try
        {
            var progress = new Progress<(long current, long total)>(value =>
            {
                dialog.UpdateDownload(value.current, value.total);
                var percent = value.total > 0 ? value.current * 100 / value.total : 0;
                GameBananaStatusTextBlock.Text = Loc.Format("Msg_Downloading2", mod.Name, percent);
            });
            var stages = new Progress<string>(dialog.SetStage);
            await _addonManagerService.InstallGameBananaAsync(
                settings,
                mod,
                selectedFile,
                _networkService,
                progress,
                stages,
                cancellation.Token,
                GetModDirectoryName(SelectedModReleaseChannel));
            dialog.MarkCompleted();
            GameBananaStatusTextBlock.Text = Loc.Format("Msg_InstalledAndEnabled", mod.Name);
            ShowToast(L("Msg_AddonInstalled"), mod.Name);
            RefreshModsView();
        }
        catch (OperationCanceledException)
        {
            dialog.MarkCancelled();
            GameBananaStatusTextBlock.Text = Loc.Format("Msg_InstallationOfCancelled", mod.Name);
        }
        catch (Exception ex)
        {
            dialog.MarkFailed(ex.Message);
            GameBananaStatusTextBlock.Text = L("Msg_InstallationFailed2");
        }
        finally
        {
            installButton.IsEnabled = true;
            SetBusy(false);
        }
    }

    private async void AddonEnabledCheckBox_OnClick(object sender, RoutedEventArgs e)
    {
        if (sender is not CheckBox checkBox || checkBox.Tag is not AddonInfo addon) return;
        var enabled = checkBox.IsChecked == true;
        checkBox.IsEnabled = false;
        try
        {
            var settings = BuildSettingsFromUi();
            await Task.Run(() => _addonManagerService.SetEnabledAsync(
                settings,
                addon,
                enabled,
                modDirectoryName: GetModDirectoryName(SelectedModReleaseChannel)));
            ShowToast(enabled ? L("Msg_AddonEnabled") : L("Msg_AddonDisabled"), addon.Name);
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_AddonError"), ex.Message, MessageBoxButton.OK);
        }
        finally { RefreshModsView(); }
    }

    private async void RemoveAddonButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement button || button.Tag is not AddonInfo addon) return;
        if (ShowCustomDialog(L("Msg_RemoveAddon"), Loc.Format("Msg_RemoveFromTheAddonLibrary", addon.Name), MessageBoxButton.YesNo) != MessageBoxResult.Yes) return;
        button.IsEnabled = false;
        try
        {
            var settings = BuildSettingsFromUi();
            await Task.Run(() => _addonManagerService.DeleteAsync(
                settings,
                addon,
                modDirectoryName: GetModDirectoryName(SelectedModReleaseChannel)));
            ShowToast(L("Msg_AddonRemoved"), addon.Name);
            RefreshModsView();
        }
        catch (Exception ex)
        {
            button.IsEnabled = true;
            ShowCustomDialog(L("Msg_RemoveAddonError"), ex.Message, MessageBoxButton.OK);
        }
    }

    private void OpenAddonPageButton_OnClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not AddonInfo addon) return;
        if (!string.IsNullOrWhiteSpace(addon.SourceUrl))
        {
            OpenUrl(addon.SourceUrl);
            return;
        }

        var folder = _addonManagerService.GetMyStuffFolder(
            BuildSettingsFromUi(),
            GetModDirectoryName(SelectedModReleaseChannel));
        Directory.CreateDirectory(folder);
        OpenFolder(folder);
    }

    private void OpenGameBananaPageButton_OnClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is GameBananaMod mod) OpenUrl(mod.ProfileUrl);
    }

    private void OpenTeamDonationButton_OnClick(object sender, RoutedEventArgs e)
    {
        OpenUrl(LauncherConfig.TeamDonationUrl);
    }

    private void OpenWebsiteButton_OnClick(object sender, RoutedEventArgs e)
    {
        OpenUrl(LauncherConfig.DownloadPageUrl);
    }

    private void OpenDiscordBorder_OnClick(object sender, RoutedEventArgs e)
    {
        OpenUrl(LauncherConfig.DiscordInviteUrl);
    }

    private void RefreshDebugButton_OnClick(object sender, RoutedEventArgs e)
    {
        RefreshAllState();
        ShowToast(L("Msg_DebugRefreshed"), L("Msg_LocalLauncherStateWasRefreshed"));
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
            ShowToast(L("Msg_NewsUpdated"), L("Msg_TheNewsFeedHasBeenRefreshedSuccessfully"));
        }
        catch (Exception ex)
        {
            ShowCustomDialog(L("Msg_UpdateError"), L("Msg_CouldNotRefreshNews") + ex.Message, MessageBoxButton.OK);
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
        if (sender is MediaElement { DataContext: NewsItem { HasVideo: true } } me && me.Source != null)
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

    private void DeleteGameSettingsFileButton_OnClick(object sender, RoutedEventArgs e)
    {
        var settings = BuildSettingsFromUi();
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            ShowCustomDialog(
                L("Msg_DolphinUserFolderRequired"),
                L("Msg_SelectTheDolphinUserFolderIn"),
                MessageBoxButton.OK);
            return;
        }

        var settingsFilePath = Path.Combine(
            settings.UserFolderPath,
            "Wii",
            "shared2",
            "Pulsar",
            "VanzaKart",
            "Settings.pul");

        var result = ShowCustomDialog(
            L("Msg_DeleteInGameSettings"),
            L("Msg_GoBackNowIfYouAreNotSureWhat") +
            L("Msg_DeletingSettingsPulIsAnAdvanced") +
            L("Msg_ItWillPermanentlyRemoveAllIn") +
            L("Msg_DoYouWantToDeleteSettingsPul"),
            MessageBoxButton.YesNo);

        if (result != MessageBoxResult.Yes)
        {
            return;
        }

        try
        {
            if (!File.Exists(settingsFilePath))
            {
                ShowCustomDialog(
                    L("Msg_SettingsFileNotFound"),
                    Loc.Format("Msg_SettingsPulDoesNotExistAt", settingsFilePath),
                    MessageBoxButton.OK);
                return;
            }

            File.Delete(settingsFilePath);
            ShowCustomDialog(
                L("Msg_InGameSettingsDeleted"),
                L("Msg_SettingsPulWasDeletedSuccessfully"),
                MessageBoxButton.OK);
        }
        catch (Exception ex)
        {
            ShowCustomDialog(
                L("Msg_CouldNotDeleteSettingsPul"),
                Loc.Format("Msg_TheSettingsFileCouldNotBeDeleted", ex.Message),
                MessageBoxButton.OK);
        }
    }

    private void NewsVideo_MediaFailed(object sender, ExceptionRoutedEventArgs e)
    {
        if (sender is MediaElement mediaElement)
        {
            mediaElement.Stop();
        }

        e.Handled = true;
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

    private async Task TryFetchEndpointsAsync()
    {
        try
        {
            if (string.IsNullOrWhiteSpace(LauncherConfig.EndpointsJsonUrl))
                return;

            var noCacheUrl = AddNoCacheQuery(LauncherConfig.EndpointsJsonUrl);
            var json = await _networkService.DownloadStringAsync(noCacheUrl);
            if (!string.IsNullOrWhiteSpace(json))
            {
                var endpoints = JsonSerializer.Deserialize<LauncherEndpointsInfo>(json.TrimStart('\uFEFF', '\u200B'));
                if (endpoints != null)
                {
                    LauncherConfig.ApplyEndpoints(endpoints);
                }
            }
        }
        catch
        {
            // Silently fall back to versions.json / defaults if endpoints.json is not available on server
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

    private bool _hasUnsavedChanges = false;
    private bool _isSimpleControllerMode = true;
    private string _activeCategoryTab = "Paths";
    private string _selectedControllerPort = "GC1";
    private bool _isUpdatingDolphinUi = false;
    private string? _settingsUiBaseline;
    private readonly DolphinControllerProfileManager _controllerProfileManager = new();

    private string ResolveControllerUserFolder()
    {
        var settings = BuildSettingsFromUi();
        if (!string.IsNullOrWhiteSpace(settings.UserFolderPath) &&
            Directory.Exists(settings.UserFolderPath))
        {
            return settings.UserFolderPath;
        }

        return _saveManagerService.TryAutoDetectUserFolder(settings);
    }

    private void SettingControl_Changed(object sender, RoutedEventArgs e)
    {
        if (_isUpdatingDolphinUi) return;
        RefreshSettingsDirtyState();
        if (_hasUnsavedChanges)
        {
            ShowSettingsStatusNotification(L("Msg_UnsavedChangesSelectSaveConfiguration"));
        }
    }

    private static bool IsExecutableRunning(string executablePath)
    {
        var processName = Path.GetFileNameWithoutExtension(executablePath);
        if (string.IsNullOrWhiteSpace(processName))
        {
            return false;
        }

        var processes = Process.GetProcessesByName(processName);
        try
        {
            return processes.Any(process => !process.HasExited);
        }
        finally
        {
            foreach (var process in processes)
            {
                process.Dispose();
            }
        }
    }

    private void RefreshSettingsDirtyState()
    {
        if (_isUpdatingDolphinUi)
        {
            return;
        }

        bool controllerDirty = "Controller".Equals(_activeCategoryTab, StringComparison.OrdinalIgnoreCase) &&
                               MarioKartControllerPanel?.IsDirty == true;
        bool settingsDirty = _settingsUiBaseline != null &&
                             !string.Equals(_settingsUiBaseline, CaptureSettingsUiState(), StringComparison.Ordinal);
        _hasUnsavedChanges = controllerDirty || settingsDirty;
    }

    private string CaptureSettingsUiState()
    {
        static string Text(TextBox? control) => control?.Text?.Trim() ?? string.Empty;
        static string SelectedTag(ComboBox? control) =>
            (control?.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? string.Empty;
        static string Checked(CheckBox? control) => control?.IsChecked == true ? "1" : "0";
        static string Value(Slider? control) =>
            (control?.Value ?? 0).ToString("R", System.Globalization.CultureInfo.InvariantCulture);

        return string.Join("\u001F",
            Text(DolphinPathTextBox),
            Text(UserFolderTextBox),
            Text(RomPathTextBox),
            SelectedTag(GfxBackendComboBox),
            SelectedTag(InternalResolutionComboBox),
            SelectedTag(AspectRatioComboBox),
            Checked(FullscreenCheckBox),
            Checked(VSyncCheckBox),
            Checked(RemoveBlurCheckBox),
            Checked(ShowFpsCheckBox),
            SelectedTag(AnisotropicFilteringComboBox),
            SelectedTag(AntiAliasingComboBox),
            Value(AudioVolumeSlider),
            SelectedTag(AudioBackendComboBox),
            Checked(AudioStretchingCheckBox),
            SelectedTag(LogLevelComboBox),
            Checked(LogToFileCheckBox),
            Checked(WaitForShadersCheckBox),
            Checked(BackendMultithreadingCheckBox));
    }

    private void ControllerControl_Changed(object sender, RoutedEventArgs e)
    {
        if (_isUpdatingDolphinUi) return;
        _hasUnsavedChanges = true;
        ShowSettingsStatusNotification(L("Msg_ControllerSettingsModifiedClick"));
    }

    private void SaveGlobalSettingsButton_OnClick(object sender, RoutedEventArgs e)
    {
        SaveCurrentDolphinSettingsFromUi();
        _hasUnsavedChanges = false;
        ShowSettingsStatusNotification(L("Msg_AllSettingsSavedSuccessfully"));
    }

    private void SaveControllerConfig_OnClick(object sender, RoutedEventArgs e)
    {
        SaveControllerBindingsFromUi();
        _hasUnsavedChanges = false;
        ShowSettingsStatusNotification(L("Msg_ControllerConfigurationSaved"));
    }

    private void CategoryTab_Click(object sender, RoutedEventArgs e)
    {
        if (sender is WpfButton btn && btn.Tag is string category)
        {
            SwitchSettingsTab(category);
        }
    }

    private void SwitchSettingsTab(string category)
    {
        RefreshSettingsDirtyState();
        if (_hasUnsavedChanges && !string.Equals(_activeCategoryTab, category, StringComparison.OrdinalIgnoreCase))
        {
            var dialogResult = ShowCustomDialog(
                L("Msg_UnsavedChanges2"),
                Loc.Format("Msg_YouHaveUnsavedChangesInTheSection", _activeCategoryTab),
                MessageBoxButton.YesNoCancel);

            if (dialogResult == MessageBoxResult.Yes)
            {
                if ("Controller".Equals(_activeCategoryTab, StringComparison.OrdinalIgnoreCase))
                {
                    SaveControllerBindingsFromUi();
                }
                else
                {
                    SaveCurrentDolphinSettingsFromUi();
                }
                _hasUnsavedChanges = false;
            }
            else if (dialogResult == MessageBoxResult.No)
            {
                _hasUnsavedChanges = false;
                LoadDolphinSettingsIntoUi();
            }
            else
            {
                return; // Cancel tab switch
            }
        }

        _activeCategoryTab = category;

        // Update active tab button styles with glow animation
        if (CategoryTabsStackPanel != null)
        {
            foreach (WpfButton btn in CategoryTabsStackPanel.Children.OfType<WpfButton>())
            {
                bool isActive = string.Equals(btn.Tag?.ToString(), category, StringComparison.OrdinalIgnoreCase);
                btn.Style = (Style)FindResource(isActive ? "CompactPrimaryButton" : "CompactButton");
            }
        }

        // Toggle card visibilities
        if (PathsSectionCard != null) PathsSectionCard.Visibility = "Paths".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (VideoSectionCard != null) VideoSectionCard.Visibility = "Video".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (AudioSectionCard != null) AudioSectionCard.Visibility = "Audio".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (ControllerSectionCard != null) ControllerSectionCard.Visibility = "Controller".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (AdvancedSectionCard != null) AdvancedSectionCard.Visibility = "Advanced".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (LauncherSectionCard != null) LauncherSectionCard.Visibility = "Launcher".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (TeamSectionCard != null) TeamSectionCard.Visibility = "Team".Equals(category, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;

        if ("Controller".Equals(category, StringComparison.OrdinalIgnoreCase))
        {
            MarioKartControllerPanel?.ReloadFromDolphin();
        }
    }

    private void BindingField_PreviewMouseDown(object sender, System.Windows.Input.MouseButtonEventArgs e)
    {
        if (sender is System.Windows.Controls.TextBox tb)
        {
            e.Handled = true;
            string actionName = tb.Name.Replace("SimpleBind_", "").Replace("Bind_", "");
            var window = new KeyBindingWindow(actionName) { Owner = this };
            if (window.ShowDialog() == true && !string.IsNullOrWhiteSpace(window.SelectedBinding))
            {
                tb.Text = window.SelectedBinding;
                _hasUnsavedChanges = true;
                ShowSettingsStatusNotification(Loc.Format("Msg_MappedTo", actionName, window.SelectedBinding));
            }
        }
    }

    private void ControllerModeBtn_Click(object sender, RoutedEventArgs e)
    {
        if (sender is WpfButton btn)
        {
            _isSimpleControllerMode = btn == SimpleControllerModeBtn;
            if (SimpleControllerGrid != null) SimpleControllerGrid.Visibility = _isSimpleControllerMode ? Visibility.Visible : Visibility.Collapsed;
            if (KeybindingGrid != null) KeybindingGrid.Visibility = _isSimpleControllerMode ? Visibility.Collapsed : Visibility.Visible;
            
            bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
            if (NunchukMotionGrid != null)
            {
                NunchukMotionGrid.Visibility = isWiimote ? Visibility.Visible : Visibility.Collapsed;
            }

            if (SimpleControllerModeBtn != null) SimpleControllerModeBtn.Style = (Style)FindResource(_isSimpleControllerMode ? "CompactPrimaryButton" : "CompactButton");
            if (AdvancedControllerModeBtn != null) AdvancedControllerModeBtn.Style = (Style)FindResource(_isSimpleControllerMode ? "CompactButton" : "CompactPrimaryButton");
        }
    }

    private void ControllerPortTab_Click(object sender, RoutedEventArgs e)
    {
        if (sender is WpfButton btn && btn.Tag is string portTag)
        {
            _selectedControllerPort = portTag;
            
            // Update active port button styles
            if (ControllerPortTabsStackPanel != null)
            {
                foreach (WpfButton b in ControllerPortTabsStackPanel.Children.OfType<WpfButton>())
                {
                    bool isActive = string.Equals(b.Tag?.ToString(), portTag, StringComparison.OrdinalIgnoreCase);
                    b.Style = (Style)FindResource(isActive ? "CompactPrimaryButton" : "CompactButton");
                }
            }

            // Show/Hide Wiimote Extension dropdown based on port
            bool isWiimote = portTag.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
            if (Card_WiimoteExtension != null)
            {
                Card_WiimoteExtension.Visibility = isWiimote ? Visibility.Visible : Visibility.Collapsed;
            }

            LoadControllerBindingsForPort(_selectedControllerPort);
        }
    }

    private void RefreshControllerDevices_Click(object sender, RoutedEventArgs e)
    {
        RefreshControllerDevices();
        ShowSettingsStatusNotification(L("Msg_InputDevicesRefreshed"));
    }

    private void RefreshControllerDevices(string? activeDeviceInIni = null)
    {
        if (ControllerDeviceComboBox == null) return;
        ControllerDeviceComboBox.Items.Clear();

        // 1. Keyboard / Mouse (DInput / Standard)
        var kbItem = new ComboBoxItem { Content = "⌨️ DInput/0/Keyboard Mouse", Tag = "DInput/0/Keyboard Mouse" };
        ControllerDeviceComboBox.Items.Add(kbItem);
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "⌨️ Keyboard/0/Keyboard Mouse", Tag = "Keyboard/0/Keyboard Mouse" });

        bool xinputConnected = false;
        try
        {
            if (KeyBindingWindow.XInputGetState14(0, out _) == 0 || KeyBindingWindow.XInputGetState13(0, out _) == 0)
            {
                xinputConnected = true;
            }
        }
        catch { }

        // 2. Add XInput devices (XInput/0/Gamepad .. XInput/3/Gamepad)
        for (int i = 0; i < 4; i++)
        {
            bool isConnected = false;
            try
            {
                isConnected = (KeyBindingWindow.XInputGetState14(i, out _) == 0 || KeyBindingWindow.XInputGetState13(i, out _) == 0);
            }
            catch { }

            if (isConnected || i == 0)
            {
                string status = isConnected ? "🟢 Connected" : "⚪ Standby";
                ControllerDeviceComboBox.Items.Add(new ComboBoxItem
                {
                    Content = $"🎮 XInput/{i}/Gamepad ({status})",
                    Tag = $"XInput/{i}/Gamepad"
                });
            }
        }

        // 3. Add SDL Input devices (Xbox, PlayStation, Generic)
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 SDL/0/Xbox One Controller", Tag = "SDL/0/Xbox One Controller" });
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 SDL/0/Xbox 360 Controller", Tag = "SDL/0/Xbox 360 Controller" });
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 SDL/0/XInput Controller", Tag = "SDL/0/XInput Controller" });
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 SDL/0/DualSense Wireless Controller", Tag = "SDL/0/DualSense Wireless Controller" });
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 SDL/0/PS4 Controller", Tag = "SDL/0/PS4 Controller" });

        // 4. Add Windows Gaming Input (WGInput) devices
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 WGInput/0/Xbox One Game Controller", Tag = "WGInput/0/Xbox One Game Controller" });
        ControllerDeviceComboBox.Items.Add(new ComboBoxItem { Content = "🎮 WGInput/0/Xbox Controller", Tag = "WGInput/0/Xbox Controller" });

        // 5. Check if activeDeviceInIni is a custom string not in the above list -> add dynamically!
        if (!string.IsNullOrWhiteSpace(activeDeviceInIni))
        {
            bool exists = ControllerDeviceComboBox.Items
                .OfType<ComboBoxItem>()
                .Any(item => string.Equals(item.Tag?.ToString(), activeDeviceInIni, StringComparison.OrdinalIgnoreCase));

            if (!exists)
            {
                var customItem = new ComboBoxItem
                {
                    Content = $"🎮 {activeDeviceInIni} (Active in Dolphin)",
                    Tag = activeDeviceInIni
                };
                ControllerDeviceComboBox.Items.Insert(1, customItem);
            }

            SetComboBoxByTag(ControllerDeviceComboBox, activeDeviceInIni);
        }

        // Fallback selection if no match
        if (ControllerDeviceComboBox.SelectedItem == null)
        {
            if (xinputConnected)
            {
                SetComboBoxByTag(ControllerDeviceComboBox, "XInput/0/Gamepad");
                if (ControllerDeviceComboBox.SelectedItem == null)
                {
                    SetComboBoxByTag(ControllerDeviceComboBox, "SDL/0/Xbox One Controller");
                }
            }

            if (ControllerDeviceComboBox.SelectedItem == null)
            {
                ControllerDeviceComboBox.SelectedItem = kbItem;
            }
        }

        string deviceName = (ControllerDeviceComboBox.SelectedItem as ComboBoxItem)?.Content?.ToString() ?? "Keyboard / Mouse";
        if (ControllerStatusBannerText != null)
        {
            ControllerStatusBannerText.Text = Loc.Format("Msg_SelectedInputDevice", deviceName);
        }
    }

    private void LoadControllerBindingsForPort(string portTag)
    {
        _isUpdatingDolphinUi = true;
        try
        {
            var settings = BuildSettingsFromUi();
            string userFolder = settings.UserFolderPath;
            if (string.IsNullOrWhiteSpace(userFolder) || !Directory.Exists(userFolder))
            {
                userFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            }

            bool isWiimote = portTag.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
            int portIndex = 1;
            if (int.TryParse(portTag.Replace("GC", "").Replace("WII", ""), out int pIdx))
            {
                portIndex = pIdx;
            }

            if (NunchukMotionGrid != null)
            {
                NunchukMotionGrid.Visibility = isWiimote ? Visibility.Visible : Visibility.Collapsed;
            }

            var bindings = _controllerProfileManager.ReadActiveBindings(userFolder, isWiimote, portIndex);

            RefreshControllerDevices(bindings.GetValueOrDefault("Device"));

            if (isWiimote && bindings.TryGetValue("Extension", out var ext))
            {
                if (NunchukEnableCheckBox != null)
                {
                    NunchukEnableCheckBox.IsChecked = !string.Equals(ext, "None", StringComparison.OrdinalIgnoreCase);
                }
            }

            // Rumble / Vibration
            if (VibrationCheckBox != null)
            {
                VibrationCheckBox.IsChecked = bindings.TryGetValue("Rumble/Motor", out var motor) && !string.IsNullOrWhiteSpace(motor);
            }

            // Analog Deadzone
            string dzKey = isWiimote ? "Nunchuk/Stick/Dead Zone" : "Main Stick/Dead Zone";
            string dzStr = bindings.GetValueOrDefault(dzKey, "10.");
            if (double.TryParse(dzStr.Replace(".", "").Trim(), out double dzVal))
            {
                if (AnalogDeadzoneSlider != null) AnalogDeadzoneSlider.Value = Math.Clamp(dzVal, 0, 50);
                if (AnalogDeadzoneTextBox != null) AnalogDeadzoneTextBox.Text = $"{dzVal:F0}";
            }

            UpdateSimpleControllerLayout();
            bool hasNunchuk = NunchukEnableCheckBox?.IsChecked == true;

            if (isWiimote)
            {
                Bind_A.Text = bindings.GetValueOrDefault("Buttons/A", "`KEY_A`");
                Bind_B.Text = bindings.GetValueOrDefault("Buttons/B", "`KEY_B`");
                Bind_X.Text = bindings.GetValueOrDefault("Buttons/1", "`KEY_1`");
                Bind_Y.Text = bindings.GetValueOrDefault("Buttons/2", "`KEY_2`");
                Bind_Z.Text = bindings.GetValueOrDefault("Buttons/Minus", "`MINUS`");
                Bind_Start.Text = bindings.GetValueOrDefault("Buttons/Plus", "`PLUS`");
                Bind_MainUp.Text = bindings.GetValueOrDefault("D-Pad/Up", "`UP`");
                Bind_MainDown.Text = bindings.GetValueOrDefault("D-Pad/Down", "`DOWN`");
                Bind_MainLeft.Text = bindings.GetValueOrDefault("D-Pad/Left", "`LEFT`");
                Bind_MainRight.Text = bindings.GetValueOrDefault("D-Pad/Right", "`RIGHT`");
                Bind_CUp.Text = bindings.GetValueOrDefault("IR/Up", "`I`");
                Bind_CDown.Text = bindings.GetValueOrDefault("IR/Down", "`K`");
                Bind_L.Text = bindings.GetValueOrDefault("Shake/X", "`SPACE`");
                Bind_R.Text = bindings.GetValueOrDefault("Shake/Y", "`SPACE`");

                // Nunchuk & Motion
                if (Bind_NunchukUp != null) Bind_NunchukUp.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Up", "`UP`");
                if (Bind_NunchukDown != null) Bind_NunchukDown.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Down", "`DOWN`");
                if (Bind_NunchukLeft != null) Bind_NunchukLeft.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Left", "`LEFT`");
                if (Bind_NunchukRight != null) Bind_NunchukRight.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Right", "`RIGHT`");
                if (Bind_NunchukC != null) Bind_NunchukC.Text = bindings.GetValueOrDefault("Nunchuk/Buttons/C", "`CONTROL`");
                if (Bind_NunchukZ != null) Bind_NunchukZ.Text = bindings.GetValueOrDefault("Nunchuk/Buttons/Z", "`SHIFT`");

                if (Bind_ShakeX != null) Bind_ShakeX.Text = bindings.GetValueOrDefault("Shake/X", "`SPACE`");
                if (Bind_ShakeY != null) Bind_ShakeY.Text = bindings.GetValueOrDefault("Shake/Y", "`SPACE`");
                if (Bind_ShakeZ != null) Bind_ShakeZ.Text = bindings.GetValueOrDefault("Shake/Z", "`SPACE`");

                if (Bind_TiltLeft != null) Bind_TiltLeft.Text = bindings.GetValueOrDefault("Tilt/Left", "`LEFT`");
                if (Bind_TiltRight != null) Bind_TiltRight.Text = bindings.GetValueOrDefault("Tilt/Right", "`RIGHT`");
                if (Bind_TiltForward != null) Bind_TiltForward.Text = bindings.GetValueOrDefault("Tilt/Forward", "`UP`");
                if (Bind_TiltBackward != null) Bind_TiltBackward.Text = bindings.GetValueOrDefault("Tilt/Backward", "`DOWN`");

                string deviceTag = bindings.GetValueOrDefault("Device", "");
                bool isGamepad = deviceTag.StartsWith("XInput", StringComparison.OrdinalIgnoreCase) || deviceTag.StartsWith("SDL", StringComparison.OrdinalIgnoreCase) || deviceTag.StartsWith("WGInput", StringComparison.OrdinalIgnoreCase);

                if (hasNunchuk)
                {
                    // Wiimote + Nunchuk
                    SimpleBind_Accelerate.Text = bindings.TryGetValue("Buttons/A", out var accA) && !string.IsNullOrWhiteSpace(accA) ? accA : bindings.GetValueOrDefault("Buttons/2", isGamepad ? "`Button A`" : "`KEY_A`");
                    SimpleBind_Brake.Text = bindings.TryGetValue("Buttons/B", out var brkB) && !string.IsNullOrWhiteSpace(brkB) ? brkB : bindings.GetValueOrDefault("Buttons/1", isGamepad ? "`Button B`" : "`KEY_B`");
                    SimpleBind_Drift.Text = bindings.GetValueOrDefault("Buttons/B", bindings.GetValueOrDefault("Triggers/R", isGamepad ? "`Trigger R`" : "`SPACE`"));
                    SimpleBind_Item.Text = bindings.GetValueOrDefault("Nunchuk/Buttons/Z", isGamepad ? "`Trigger L`" : "`SHIFT`");
                    SimpleBind_LookBack.Text = bindings.GetValueOrDefault("Nunchuk/Buttons/C", isGamepad ? "`Bumper L`" : "`CONTROL`");
                    SimpleBind_Trick.Text = bindings.GetValueOrDefault("Shake/X", isGamepad ? "`Button Y`" : "`SPACE`");
                    SimpleBind_Pause.Text = bindings.GetValueOrDefault("Buttons/Plus", isGamepad ? "`Menu`" : "`RETURN`");

                    if (SimpleBind_StickUp != null) SimpleBind_StickUp.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Up", isGamepad ? "`Left Y+`" : "`UP`");
                    if (SimpleBind_StickDown != null) SimpleBind_StickDown.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Down", isGamepad ? "`Left Y-`" : "`DOWN`");
                    if (SimpleBind_StickLeft != null) SimpleBind_StickLeft.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Left", isGamepad ? "`Left X-`" : "`LEFT`");
                    if (SimpleBind_StickRight != null) SimpleBind_StickRight.Text = bindings.GetValueOrDefault("Nunchuk/Stick/Right", isGamepad ? "`Left X+`" : "`RIGHT`");

                    if (SimpleBind_DPadUp != null) SimpleBind_DPadUp.Text = bindings.GetValueOrDefault("D-Pad/Up", "`UP`");
                    if (SimpleBind_DPadDown != null) SimpleBind_DPadDown.Text = bindings.GetValueOrDefault("D-Pad/Down", "`DOWN`");
                    if (SimpleBind_DPadLeft != null) SimpleBind_DPadLeft.Text = bindings.GetValueOrDefault("D-Pad/Left", "`LEFT`");
                    if (SimpleBind_DPadRight != null) SimpleBind_DPadRight.Text = bindings.GetValueOrDefault("D-Pad/Right", "`RIGHT`");
                }
                else
                {
                    // Wiimote Solo (Horizontal)
                    SimpleBind_Accelerate.Text = bindings.GetValueOrDefault("Buttons/2", isGamepad ? "`Button A`" : "`KEY_2`");
                    SimpleBind_Brake.Text = bindings.GetValueOrDefault("Buttons/1", isGamepad ? "`Button B`" : "`KEY_1`");
                    SimpleBind_Drift.Text = bindings.GetValueOrDefault("Buttons/B", isGamepad ? "`Trigger R`" : "`KEY_B`");
                    SimpleBind_Item.Text = bindings.GetValueOrDefault("D-Pad/Left", isGamepad ? "`D-Pad Left`" : "`LEFT`");
                    SimpleBind_LookBack.Text = bindings.GetValueOrDefault("Buttons/A", isGamepad ? "`Button X`" : "`KEY_A`");
                    SimpleBind_Trick.Text = bindings.GetValueOrDefault("Shake/X", isGamepad ? "`Button Y`" : "`SPACE`");
                    SimpleBind_Pause.Text = bindings.GetValueOrDefault("Buttons/Plus", isGamepad ? "`Menu`" : "`RETURN`");

                    if (SimpleBind_StickUp != null) SimpleBind_StickUp.Text = bindings.GetValueOrDefault("Tilt/Forward", isGamepad ? "`Left Y+`" : "`UP`");
                    if (SimpleBind_StickDown != null) SimpleBind_StickDown.Text = bindings.GetValueOrDefault("Tilt/Backward", isGamepad ? "`Left Y-`" : "`DOWN`");
                    if (SimpleBind_StickLeft != null) SimpleBind_StickLeft.Text = bindings.GetValueOrDefault("Tilt/Left", isGamepad ? "`Left X-`" : "`LEFT`");
                    if (SimpleBind_StickRight != null) SimpleBind_StickRight.Text = bindings.GetValueOrDefault("Tilt/Right", isGamepad ? "`Left X+`" : "`RIGHT`");

                    if (SimpleBind_DPadUp != null) SimpleBind_DPadUp.Text = bindings.GetValueOrDefault("D-Pad/Up", "`UP`");
                    if (SimpleBind_DPadDown != null) SimpleBind_DPadDown.Text = bindings.GetValueOrDefault("D-Pad/Down", "`DOWN`");
                    if (SimpleBind_DPadLeft != null) SimpleBind_DPadLeft.Text = bindings.GetValueOrDefault("D-Pad/Left", "`LEFT`");
                    if (SimpleBind_DPadRight != null) SimpleBind_DPadRight.Text = bindings.GetValueOrDefault("D-Pad/Right", "`RIGHT`");
                }
            }
            else
            {
                Bind_A.Text = bindings.GetValueOrDefault("Buttons/A", "`KEY_A`");
                Bind_B.Text = bindings.GetValueOrDefault("Buttons/B", "`KEY_B`");
                Bind_X.Text = bindings.GetValueOrDefault("Buttons/X", "`KEY_X`");
                Bind_Y.Text = bindings.GetValueOrDefault("Buttons/Y", "`KEY_Y`");
                Bind_Z.Text = bindings.GetValueOrDefault("Buttons/Z", "`KEY_Z`");
                Bind_Start.Text = bindings.GetValueOrDefault("Buttons/Start", "`RETURN`");
                Bind_MainUp.Text = bindings.GetValueOrDefault("Main Stick/Up", "`Left Y+`");
                Bind_MainDown.Text = bindings.GetValueOrDefault("Main Stick/Down", "`Left Y-`");
                Bind_MainLeft.Text = bindings.GetValueOrDefault("Main Stick/Left", "`Left X-`");
                Bind_MainRight.Text = bindings.GetValueOrDefault("Main Stick/Right", "`Left X+`");
                Bind_CUp.Text = bindings.GetValueOrDefault("C-Stick/Up", "`Right Y+`");
                Bind_CDown.Text = bindings.GetValueOrDefault("C-Stick/Down", "`Right Y-`");
                Bind_L.Text = bindings.GetValueOrDefault("Triggers/L", "`SHIFT`");
                Bind_R.Text = bindings.GetValueOrDefault("Triggers/R", "`SPACE`");

                // Simple Mode MKWii (GameCube)
                SimpleBind_Accelerate.Text = bindings.GetValueOrDefault("Buttons/A", "`KEY_A`");
                SimpleBind_Brake.Text = bindings.GetValueOrDefault("Buttons/B", "`KEY_B`");
                SimpleBind_Drift.Text = bindings.GetValueOrDefault("Triggers/R", "`SPACE`");
                SimpleBind_Item.Text = bindings.GetValueOrDefault("Triggers/L", "`SHIFT`");
                SimpleBind_LookBack.Text = bindings.GetValueOrDefault("Buttons/X", "`KEY_X`");
                SimpleBind_Trick.Text = bindings.GetValueOrDefault("D-Pad/Up", "`UP`");
                SimpleBind_Pause.Text = bindings.GetValueOrDefault("Buttons/Start", "`RETURN`");

                if (SimpleBind_StickUp != null) SimpleBind_StickUp.Text = bindings.GetValueOrDefault("Main Stick/Up", "`Left Y+`");
                if (SimpleBind_StickDown != null) SimpleBind_StickDown.Text = bindings.GetValueOrDefault("Main Stick/Down", "`Left Y-`");
                if (SimpleBind_StickLeft != null) SimpleBind_StickLeft.Text = bindings.GetValueOrDefault("Main Stick/Left", "`Left X-`");
                if (SimpleBind_StickRight != null) SimpleBind_StickRight.Text = bindings.GetValueOrDefault("Main Stick/Right", "`Left X+`");

                if (SimpleBind_DPadUp != null) SimpleBind_DPadUp.Text = bindings.GetValueOrDefault("D-Pad/Up", "`UP`");
                if (SimpleBind_DPadDown != null) SimpleBind_DPadDown.Text = bindings.GetValueOrDefault("D-Pad/Down", "`DOWN`");
                if (SimpleBind_DPadLeft != null) SimpleBind_DPadLeft.Text = bindings.GetValueOrDefault("D-Pad/Left", "`LEFT`");
                if (SimpleBind_DPadRight != null) SimpleBind_DPadRight.Text = bindings.GetValueOrDefault("D-Pad/Right", "`RIGHT`");
            }

            if (ControllerProfileComboBox != null)
            {
                ControllerProfileComboBox.Items.Clear();
                var profiles = _controllerProfileManager.GetAvailableProfiles(userFolder, isWiimote);
                foreach (var profile in profiles)
                {
                    ControllerProfileComboBox.Items.Add(profile);
                }
                if (ControllerProfileComboBox.Items.Count > 0 && ControllerProfileComboBox.SelectedIndex < 0)
                {
                    ControllerProfileComboBox.SelectedIndex = 0;
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed loading controller bindings: {ex.Message}");
        }
        finally
        {
            _isUpdatingDolphinUi = false;
            _hasUnsavedChanges = false;
        }
    }

    private void LoadProfileButton_Click(object sender, RoutedEventArgs e)
    {
        string? profileName = ControllerProfileComboBox?.SelectedItem?.ToString();
        if (string.IsNullOrWhiteSpace(profileName)) return;

        var settings = BuildSettingsFromUi();
        bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
        int portIndex = 1;
        if (int.TryParse(_selectedControllerPort.Replace("GC", "").Replace("WII", ""), out int pIdx)) portIndex = pIdx;

        if (_controllerProfileManager.LoadProfile(settings.UserFolderPath, isWiimote, portIndex, profileName))
        {
            LoadControllerBindingsForPort(_selectedControllerPort);
            SaveControllerBindingsFromUi();
            if (ControllerProfileComboBox != null) ControllerProfileComboBox.SelectedItem = profileName;
            ShowSettingsStatusNotification(Loc.Format("Msg_LoadedProfile", profileName));
        }
    }

    private void SaveProfileButton_Click(object sender, RoutedEventArgs e)
    {
        string? profileName = ControllerProfileComboBox?.Text?.Trim();
        if (string.IsNullOrWhiteSpace(profileName)) return;

        SaveControllerBindingsFromUi();
        var settings = BuildSettingsFromUi();
        bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
        int portIndex = 1;
        if (int.TryParse(_selectedControllerPort.Replace("GC", "").Replace("WII", ""), out int pIdx)) portIndex = pIdx;

        var bindings = _controllerProfileManager.ReadActiveBindings(settings.UserFolderPath, isWiimote, portIndex);
        if (_controllerProfileManager.SaveProfile(settings.UserFolderPath, isWiimote, profileName, bindings))
        {
            LoadControllerBindingsForPort(_selectedControllerPort);
            ShowSettingsStatusNotification(Loc.Format("Msg_ProfileSaved", profileName));
        }
    }

    private void DeleteProfileButton_Click(object sender, RoutedEventArgs e)
    {
        string? profileName = ControllerProfileComboBox?.SelectedItem?.ToString();
        if (string.IsNullOrWhiteSpace(profileName)) return;

        var settings = BuildSettingsFromUi();
        bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
        if (_controllerProfileManager.DeleteProfile(settings.UserFolderPath, isWiimote, profileName))
        {
            LoadControllerBindingsForPort(_selectedControllerPort);
            ShowSettingsStatusNotification(Loc.Format("Msg_DeletedProfile", profileName));
        }
    }

    private void UpdateSimpleControllerLayout()
    {
        bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
        bool hasNunchuk = NunchukEnableCheckBox?.IsChecked == true;

        if (!isWiimote)
        {
            // --- GAMECUBE / GAMEPAD SCHEME ---
            if (SimpleLabel_HeaderTitle != null) SimpleLabel_HeaderTitle.Text = L("Msg_MarioKartWiiControlsGamecube");
            if (SimpleLabel_Accelerate != null) SimpleLabel_Accelerate.Text = L("Msg_AccelerateButtonA");
            if (SimpleLabel_Brake != null) SimpleLabel_Brake.Text = L("Msg_BrakeReverseButtonB");
            if (SimpleLabel_Drift != null) SimpleLabel_Drift.Text = L("Msg_DriftMiniTurboTriggerR");
            if (SimpleLabel_Item != null) SimpleLabel_Item.Text = L("Msg_UseItemTriggerL");
            if (SimpleLabel_LookBack != null) SimpleLabel_LookBack.Text = L("Msg_RearViewButtonXY");
            if (SimpleLabel_Trick != null) SimpleLabel_Trick.Text = L("Msg_TrickWheelieDPadUp");
            if (SimpleLabel_Pause != null) SimpleLabel_Pause.Text = L("Msg_PauseGameStartMenu");
            if (SimpleLabel_StickHeader != null) SimpleLabel_StickHeader.Text = L("Msg_MainAnalogStickSteeringPitch");
            if (SimpleLabel_DPadHeader != null) SimpleLabel_DPadHeader.Text = L("Msg_DPadTricksMenuSelection");
        }
        else if (hasNunchuk)
        {
            // --- WIIMOTE + NUNCHUK SCHEME ---
            if (SimpleLabel_HeaderTitle != null) SimpleLabel_HeaderTitle.Text = L("Msg_MarioKartWiiControlsWiimoteNunchuk");
            if (SimpleLabel_Accelerate != null) SimpleLabel_Accelerate.Text = L("Msg_AccelerateButtonA");
            if (SimpleLabel_Brake != null) SimpleLabel_Brake.Text = L("Msg_BrakeReverseButtonB");
            if (SimpleLabel_Drift != null) SimpleLabel_Drift.Text = L("Msg_DriftHopButtonBX");
            if (SimpleLabel_Item != null) SimpleLabel_Item.Text = L("Msg_UseItemNunchukZ");
            if (SimpleLabel_LookBack != null) SimpleLabel_LookBack.Text = L("Msg_RearViewNunchukC");
            if (SimpleLabel_Trick != null) SimpleLabel_Trick.Text = L("Msg_TrickWheelieShakeWiimoteNunchuk");
            if (SimpleLabel_Pause != null) SimpleLabel_Pause.Text = L("Msg_PauseGamePlus");
            if (SimpleLabel_StickHeader != null) SimpleLabel_StickHeader.Text = L("Msg_NunchukControlStickSteeringPitch");
            if (SimpleLabel_DPadHeader != null) SimpleLabel_DPadHeader.Text = L("Msg_DPadManualWheelieItems");
        }
        else
        {
            // --- WIIMOTE SOLO (HORIZONTAL) SCHEME ---
            if (SimpleLabel_HeaderTitle != null) SimpleLabel_HeaderTitle.Text = L("Msg_MarioKartWiiControlsWiimoteSolo");
            if (SimpleLabel_Accelerate != null) SimpleLabel_Accelerate.Text = L("Msg_AccelerateButton2");
            if (SimpleLabel_Brake != null) SimpleLabel_Brake.Text = L("Msg_BrakeReverseButton1");
            if (SimpleLabel_Drift != null) SimpleLabel_Drift.Text = L("Msg_DriftHopButtonB");
            if (SimpleLabel_Item != null) SimpleLabel_Item.Text = L("Msg_UseItemDPadLeftRight");
            if (SimpleLabel_LookBack != null) SimpleLabel_LookBack.Text = L("Msg_RearViewButtonA");
            if (SimpleLabel_Trick != null) SimpleLabel_Trick.Text = L("Msg_TrickWheelieShakeDPadUp");
            if (SimpleLabel_Pause != null) SimpleLabel_Pause.Text = L("Msg_PauseGamePlus");
            if (SimpleLabel_StickHeader != null) SimpleLabel_StickHeader.Text = L("Msg_TiltMotionSteeringTiltLeftRight");
            if (SimpleLabel_DPadHeader != null) SimpleLabel_DPadHeader.Text = L("Msg_DPadSteeringItemUsage");
        }
    }

    private void NunchukEnableCheckBox_Click(object sender, RoutedEventArgs e)
    {
        if (_isUpdatingDolphinUi) return;
        bool hasNunchuk = NunchukEnableCheckBox?.IsChecked == true;
        string extTag = hasNunchuk ? "Nunchuk" : "None";

        // 1. Update layout text labels
        UpdateSimpleControllerLayout();

        // 2. Write only extension tag to INI first
        SaveExtensionToIniOnly(extTag);

        // 3. Reload port bindings into UI textboxes for the target scheme
        LoadControllerBindingsForPort(_selectedControllerPort);

        // 4. Save full updated UI bindings
        SaveControllerBindingsFromUi();

        ControllerControl_Changed(sender, e);
    }

    private void SaveExtensionToIniOnly(string extensionTag)
    {
        try
        {
            var settings = BuildSettingsFromUi();
            string userFolder = settings.UserFolderPath;
            if (string.IsNullOrWhiteSpace(userFolder) || !Directory.Exists(userFolder))
            {
                userFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            }
            if (string.IsNullOrWhiteSpace(userFolder)) return;

            bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
            if (!isWiimote) return;

            int portIndex = 1;
            if (int.TryParse(_selectedControllerPort.Replace("GC", "").Replace("WII", ""), out int pIdx))
            {
                portIndex = pIdx;
            }

            var bindings = _controllerProfileManager.ReadActiveBindings(userFolder, true, portIndex);
            bindings["Extension"] = extensionTag;
            _controllerProfileManager.SaveActiveBindings(userFolder, true, portIndex, bindings);
        }
        catch { }
    }

    private void ResetControllerSection_OnClick(object sender, RoutedEventArgs e) { LoadControllerBindingsForPort(_selectedControllerPort); ShowSettingsStatusNotification(L("Msg_ControllerSectionReset")); }

    private void AudioVolumeTextBox_TextChanged(object sender, TextChangedEventArgs e) { if (double.TryParse(AudioVolumeTextBox.Text, out double val) && AudioVolumeSlider != null) AudioVolumeSlider.Value = Math.Clamp(val, 0, 100); }
    private void AudioVolumeSlider_OnValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e) { if (AudioVolumeTextBox != null) AudioVolumeTextBox.Text = $"{e.NewValue:F0}"; }
    private void AnalogSensitivityTextBox_TextChanged(object sender, TextChangedEventArgs e) { if (double.TryParse(AnalogSensitivityTextBox.Text, out double val) && AnalogSensitivitySlider != null) AnalogSensitivitySlider.Value = Math.Clamp(val, 50, 150); }
    private void AnalogSensitivitySlider_OnValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e) { if (AnalogSensitivityTextBox != null) AnalogSensitivityTextBox.Text = $"{e.NewValue:F0}"; }
    private void AnalogDeadzoneTextBox_TextChanged(object sender, TextChangedEventArgs e) { if (double.TryParse(AnalogDeadzoneTextBox.Text, out double val) && AnalogDeadzoneSlider != null) AnalogDeadzoneSlider.Value = Math.Clamp(val, 0, 50); ControllerControl_Changed(sender, e); }
    private void AnalogDeadzoneSlider_OnValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e) { if (AnalogDeadzoneTextBox != null) AnalogDeadzoneTextBox.Text = $"{e.NewValue:F0}"; ControllerControl_Changed(sender, e); }

    private void SettingsSearchTextBox_OnTextChanged(object sender, TextChangedEventArgs e)
    {
        if (SearchPlaceholderText != null)
        {
            SearchPlaceholderText.Visibility = string.IsNullOrEmpty(SettingsSearchTextBox.Text) ? Visibility.Visible : Visibility.Collapsed;
        }

        string query = SettingsSearchTextBox.Text.Trim();

        if (string.IsNullOrWhiteSpace(query))
        {
            if (SettingsView != null)
            {
                RestoreAllSettingCards(SettingsView);
            }
            SwitchSettingsTab(_activeCategoryTab);
            return;
        }

        Border[] sectionCards = new[]
        {
            PathsSectionCard, VideoSectionCard, AudioSectionCard, ControllerSectionCard,
            AdvancedSectionCard, LauncherSectionCard, TeamSectionCard
        };

        string lowerQuery = query.ToLowerInvariant();

        foreach (var sectionCard in sectionCards)
        {
            if (sectionCard == null) continue;

            bool hasMatch = FilterSectionByQuery(sectionCard, lowerQuery);
            sectionCard.Visibility = hasMatch ? Visibility.Visible : Visibility.Collapsed;
        }
    }

    private void RestoreAllSettingCards(DependencyObject parent)
    {
        if (parent == null) return;
        int count = VisualTreeHelper.GetChildrenCount(parent);
        for (int i = 0; i < count; i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is FrameworkElement fe)
            {
                if (fe.Name != null && fe.Name.StartsWith("Card_"))
                {
                    fe.Visibility = Visibility.Visible;
                }
                RestoreAllSettingCards(child);
            }
        }
    }

    private static bool FilterSectionByQuery(DependencyObject parent, string query)
    {
        if (parent == null) return false;

        bool childMatchedAny = false;
        int count = VisualTreeHelper.GetChildrenCount(parent);

        for (int i = 0; i < count; i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);

            if (child is FrameworkElement fe)
            {
                bool isCard = fe.Name != null && fe.Name.StartsWith("Card_");

                if (isCard)
                {
                    string cardContent = GetAllTextFromElement(fe).ToLowerInvariant();
                    bool isMatch = cardContent.Contains(query);
                    fe.Visibility = isMatch ? Visibility.Visible : Visibility.Collapsed;
                    if (isMatch) childMatchedAny = true;
                }
                else
                {
                    string directText = GetDirectTextFromElement(fe).ToLowerInvariant();
                    if (!string.IsNullOrWhiteSpace(directText) && directText.Contains(query))
                    {
                        childMatchedAny = true;
                    }

                    bool childMatched = FilterSectionByQuery(child, query);
                    if (childMatched) childMatchedAny = true;
                }
            }
        }

        return childMatchedAny;
    }

    private static string GetDirectTextFromElement(DependencyObject element)
    {
        if (element is TextBlock tb) return tb.Text ?? "";
        if (element is CheckBox cb) return cb.Content?.ToString() ?? "";
        if (element is Button b) return b.Content?.ToString() ?? "";
        if (element is TextBox tbox) return tbox.Text ?? "";
        return "";
    }

    private static string GetAllTextFromElement(DependencyObject element)
    {
        var sb = new System.Text.StringBuilder();
        CollectTextRecursive(element, sb);
        return sb.ToString();
    }

    private static void CollectTextRecursive(DependencyObject parent, System.Text.StringBuilder sb)
    {
        if (parent == null) return;
        if (parent is TextBlock tb && !string.IsNullOrWhiteSpace(tb.Text))
        {
            sb.Append(' ').Append(tb.Text);
        }
        else if (parent is CheckBox cb && cb.Content != null)
        {
            sb.Append(' ').Append(cb.Content.ToString());
        }
        else if (parent is Button b && b.Content != null)
        {
            sb.Append(' ').Append(b.Content.ToString());
        }
        else if (parent is ComboBox combo)
        {
            foreach (ComboBoxItem item in combo.Items.OfType<ComboBoxItem>())
            {
                if (item.Content != null) sb.Append(' ').Append(item.Content.ToString());
            }
        }

        int count = VisualTreeHelper.GetChildrenCount(parent);
        for (int i = 0; i < count; i++)
        {
            CollectTextRecursive(VisualTreeHelper.GetChild(parent, i), sb);
        }
    }
    private void OptimizeVanzaKartButton_OnClick(object sender, RoutedEventArgs e)
    {
        var wasUpdating = _isUpdatingDolphinUi;
        _isUpdatingDolphinUi = true;
        try
        {
            SetComboBoxByTag(GfxBackendComboBox, "Vulkan");
            SetComboBoxByTag(AspectRatioComboBox, "1");
            if (FullscreenCheckBox != null) FullscreenCheckBox.IsChecked = true;
            if (RemoveBlurCheckBox != null) RemoveBlurCheckBox.IsChecked = true;
        }
        finally
        {
            _isUpdatingDolphinUi = wasUpdating;
        }

        SettingControl_Changed(sender, e);
        ShowSettingsStatusNotification(L("Msg_VanzakartOptimizedPresetApplied"));
    }
    private void ResetAllSettingsButton_OnClick(object sender, RoutedEventArgs e) => ShowSettingsStatusNotification(L("Msg_SettingsReset"));
    private void BackupConfigButton_OnClick(object sender, RoutedEventArgs e) => ShowSettingsStatusNotification(L("Msg_ConfigBackedUp"));
    private void ExportImportConfigButton_OnClick(object sender, RoutedEventArgs e) => ShowSettingsStatusNotification(L("Msg_ConfigExported"));
    private void DolphinPathTextBox_OnTextChanged(object sender, TextChangedEventArgs e) => SettingControl_Changed(sender, e);
    private void OpenLauncherLogsFolder_OnClick(object sender, RoutedEventArgs e) =>
        OpenLauncherDataFolder("Logs", "logs");

    private void OpenLauncherBackupsFolder_OnClick(object sender, RoutedEventArgs e) =>
        OpenLauncherDataFolder("Backups", "backups");

    private void OpenLauncherDataFolder(string folderName, string displayName)
    {
        try
        {
            var folder = Path.Combine(AppContext.BaseDirectory, folderName);
            Directory.CreateDirectory(folder);
            OpenFolder(folder);
            ShowSettingsStatusNotification(Loc.Format("Msg_OpenedLauncherFolder", displayName));
        }
        catch (Exception ex)
        {
            ShowSettingsStatusNotification(Loc.Format("Msg_CouldNotOpenTheLauncherFolder", displayName, ex.Message));
        }
    }
    private void ResetVideoSection_OnClick(object sender, RoutedEventArgs e) => ShowSettingsStatusNotification(L("Msg_VideoSectionReset"));
    private void ResetAudioSection_OnClick(object sender, RoutedEventArgs e) => ShowSettingsStatusNotification(L("Msg_AudioSectionReset"));

    private static void SetComboBoxByTag(ComboBox? comboBox, string tagValue)
    {
        if (comboBox == null || string.IsNullOrWhiteSpace(tagValue)) return;
        foreach (ComboBoxItem item in comboBox.Items.OfType<ComboBoxItem>())
        {
            if (string.Equals(item.Tag?.ToString(), tagValue, StringComparison.OrdinalIgnoreCase))
            {
                comboBox.SelectedItem = item;
                break;
            }
        }
    }

    private void LoadDolphinSettingsIntoUi()
    {
        _isUpdatingDolphinUi = true;
        try
        {
            var settings = BuildSettingsFromUi();
            string userFolder = settings.UserFolderPath;
            if (string.IsNullOrWhiteSpace(userFolder) || !Directory.Exists(userFolder))
            {
                userFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            }

            if (!string.IsNullOrWhiteSpace(userFolder) && Directory.Exists(userFolder))
            {
                var model = _dolphinSettingsManager.LoadSettings(userFolder, settings);

                // Video / Graphics
                SetComboBoxByTag(GfxBackendComboBox, model.GfxBackend);
                SetComboBoxByTag(InternalResolutionComboBox, model.InternalResolution.ToString());
                SetComboBoxByTag(AspectRatioComboBox, model.AspectRatio.ToString());
                if (VSyncCheckBox != null) VSyncCheckBox.IsChecked = model.VSync;
                if (FullscreenCheckBox != null) FullscreenCheckBox.IsChecked = model.Fullscreen;
                SetComboBoxByTag(AntiAliasingComboBox, model.AntiAliasing.ToString());
                SetComboBoxByTag(AnisotropicFilteringComboBox, model.AnisotropicFiltering.ToString());
                if (RemoveBlurCheckBox != null) RemoveBlurCheckBox.IsChecked = model.RemoveBlur;
                if (ShowFpsCheckBox != null) ShowFpsCheckBox.IsChecked = model.ShowFPS;

                // Audio
                if (AudioVolumeSlider != null) AudioVolumeSlider.Value = model.AudioVolume;
                if (AudioVolumeTextBox != null) AudioVolumeTextBox.Text = model.AudioVolume.ToString();
                SetComboBoxByTag(AudioBackendComboBox, model.AudioBackend);
                if (AudioStretchingCheckBox != null) AudioStretchingCheckBox.IsChecked = model.AudioStretching;

                // Advanced
                SetComboBoxByTag(LogLevelComboBox, model.LogLevel);
                if (LogToFileCheckBox != null) LogToFileCheckBox.IsChecked = model.LogToFile;
                if (WaitForShadersCheckBox != null) WaitForShadersCheckBox.IsChecked = model.WaitForShadersBeforeStarting;
                if (BackendMultithreadingCheckBox != null) BackendMultithreadingCheckBox.IsChecked = model.BackendMultithreading;
            }

            MarioKartControllerPanel?.ReloadFromDolphin();
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed loading Dolphin settings: {ex.Message}");
        }
        finally
        {
            _isUpdatingDolphinUi = false;
            _settingsUiBaseline = CaptureSettingsUiState();
            _hasUnsavedChanges = false;
        }
    }

    private void SaveCurrentDolphinSettingsFromUi()
    {
        try
        {
            var settings = BuildSettingsFromUi();
            string userFolder = settings.UserFolderPath;
            if (string.IsNullOrWhiteSpace(userFolder) || !Directory.Exists(userFolder))
            {
                userFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            }

            if (string.IsNullOrWhiteSpace(userFolder))
            {
                ShowSettingsStatusNotification(L("Msg_DolphinUserFolderPathIsNotSet"));
                return;
            }

            var model = _dolphinSettingsManager.LoadSettings(userFolder, settings);
            model.DolphinExecutablePath = settings.DolphinPath ?? "";
            model.UserFolderPath = userFolder;
            model.ModpackPath = settings.RomPath ?? "";

            if (GfxBackendComboBox != null) model.GfxBackend = (GfxBackendComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "Vulkan";
            if (InternalResolutionComboBox != null && int.TryParse((InternalResolutionComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString(), out int res)) model.InternalResolution = res;
            if (AspectRatioComboBox != null && int.TryParse((AspectRatioComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString(), out int ar)) model.AspectRatio = ar;
            if (VSyncCheckBox != null) model.VSync = VSyncCheckBox.IsChecked == true;
            if (FullscreenCheckBox != null) model.Fullscreen = FullscreenCheckBox.IsChecked == true;
            if (AntiAliasingComboBox != null && int.TryParse((AntiAliasingComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString(), out int aa)) model.AntiAliasing = aa;
            if (AnisotropicFilteringComboBox != null && int.TryParse((AnisotropicFilteringComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString(), out int af)) model.AnisotropicFiltering = af;
            if (RemoveBlurCheckBox != null) model.RemoveBlur = RemoveBlurCheckBox.IsChecked == true;
            if (ShowFpsCheckBox != null) model.ShowFPS = ShowFpsCheckBox.IsChecked == true;

            if (AudioVolumeSlider != null) model.AudioVolume = (int)AudioVolumeSlider.Value;
            if (AudioBackendComboBox != null) model.AudioBackend = (AudioBackendComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "Cubeb";
            if (AudioStretchingCheckBox != null) model.AudioStretching = AudioStretchingCheckBox.IsChecked == true;

            if (LogLevelComboBox != null) model.LogLevel = (LogLevelComboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "Notice";
            if (LogToFileCheckBox != null) model.LogToFile = LogToFileCheckBox.IsChecked == true;
            if (WaitForShadersCheckBox != null) model.WaitForShadersBeforeStarting = WaitForShadersCheckBox.IsChecked == true;
            if (BackendMultithreadingCheckBox != null) model.BackendMultithreading = BackendMultithreadingCheckBox.IsChecked != false;

            _dolphinSettingsManager.SaveSettings(userFolder, model);
            SaveControllerBindingsFromUi();
            _settingsService.Save(settings);
            _settingsUiBaseline = CaptureSettingsUiState();
            _hasUnsavedChanges = false;
        }
        catch (Exception ex)
        {
            ShowSettingsStatusNotification(Loc.Format("Msg_ErrorSavingSettings", ex.Message));
        }
    }

    private void SaveControllerBindingsFromUi()
    {
        if (MarioKartControllerPanel != null)
        {
            if (MarioKartControllerPanel.SaveToDolphin())
            {
                _hasUnsavedChanges = false;
            }
            return;
        }

        // Skip saving controller bindings when launcher controller management is disabled
        if (ManageControllersCheckBox?.IsChecked != true) return;

        try
        {
            var settings = BuildSettingsFromUi();
            string userFolder = settings.UserFolderPath;
            if (string.IsNullOrWhiteSpace(userFolder) || !Directory.Exists(userFolder))
            {
                userFolder = _saveManagerService.TryAutoDetectUserFolder(settings);
            }

            if (string.IsNullOrWhiteSpace(userFolder))
            {
                ShowSettingsStatusNotification(L("Msg_DolphinUserFolderPathIsNotSet"));
                return;
            }

            bool isWiimote = _selectedControllerPort.StartsWith("WII", StringComparison.OrdinalIgnoreCase);
            int portIndex = 1;
            if (int.TryParse(_selectedControllerPort.Replace("GC", "").Replace("WII", ""), out int pIdx))
            {
                portIndex = pIdx;
            }

            string deviceTag = (ControllerDeviceComboBox?.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "Keyboard/0/Keyboard Mouse";

            var bindings = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Device"] = deviceTag
            };

            bindings["Rumble/Motor"] = VibrationCheckBox?.IsChecked == true ? "Motor" : "";

            int deadZone = (int)(AnalogDeadzoneSlider?.Value ?? 10);
            string dzValStr = $"{deadZone}.";
            if (isWiimote)
            {
                bindings["Nunchuk/Stick/Dead Zone"] = dzValStr;
            }
            else
            {
                bindings["Main Stick/Dead Zone"] = dzValStr;
                bindings["C-Stick/Dead Zone"] = dzValStr;
            }

            if (isWiimote)
            {
                bindings["Source"] = "1";
                bindings["Options/Connect"] = "True";

                bool hasNunchuk = NunchukEnableCheckBox?.IsChecked == true;
                string extTag = hasNunchuk ? "Nunchuk" : "None";
                bindings["Extension"] = extTag;

                // Wiimote buttons (from Advanced Mode textboxes)
                bindings["Buttons/A"] = Bind_A.Text;
                bindings["Buttons/B"] = Bind_B.Text;
                bindings["Buttons/1"] = Bind_X.Text;
                bindings["Buttons/2"] = Bind_Y.Text;
                bindings["Buttons/Minus"] = Bind_Z.Text;
                bindings["Buttons/Plus"] = Bind_Start.Text;
                bindings["D-Pad/Up"] = Bind_MainUp.Text;
                bindings["D-Pad/Down"] = Bind_MainDown.Text;
                bindings["D-Pad/Left"] = Bind_MainLeft.Text;
                bindings["D-Pad/Right"] = Bind_MainRight.Text;
                bindings["IR/Up"] = Bind_CUp.Text;
                bindings["IR/Down"] = Bind_CDown.Text;

                // Nunchuk Stick & Buttons
                if (Bind_NunchukUp != null) bindings["Nunchuk/Stick/Up"] = Bind_NunchukUp.Text;
                if (Bind_NunchukDown != null) bindings["Nunchuk/Stick/Down"] = Bind_NunchukDown.Text;
                if (Bind_NunchukLeft != null) bindings["Nunchuk/Stick/Left"] = Bind_NunchukLeft.Text;
                if (Bind_NunchukRight != null) bindings["Nunchuk/Stick/Right"] = Bind_NunchukRight.Text;
                if (Bind_NunchukC != null) bindings["Nunchuk/Buttons/C"] = Bind_NunchukC.Text;
                if (Bind_NunchukZ != null) bindings["Nunchuk/Buttons/Z"] = Bind_NunchukZ.Text;

                // Motion Shake & Tilt
                if (Bind_ShakeX != null) bindings["Shake/X"] = Bind_ShakeX.Text;
                if (Bind_ShakeY != null) bindings["Shake/Y"] = Bind_ShakeY.Text;
                if (Bind_ShakeZ != null) bindings["Shake/Z"] = Bind_ShakeZ.Text;
                if (Bind_TiltLeft != null) bindings["Tilt/Left"] = Bind_TiltLeft.Text;
                if (Bind_TiltRight != null) bindings["Tilt/Right"] = Bind_TiltRight.Text;
                if (Bind_TiltForward != null) bindings["Tilt/Forward"] = Bind_TiltForward.Text;
                if (Bind_TiltBackward != null) bindings["Tilt/Backward"] = Bind_TiltBackward.Text;

                // Override from Simple Mode if user was in Simple Mode
                if (_isSimpleControllerMode)
                {
                    if (extTag == "None")
                    {
                        // Wiimote Solo (Horizontal)
                        bindings["Buttons/2"] = SimpleBind_Accelerate.Text;
                        bindings["Buttons/1"] = SimpleBind_Brake.Text;
                        bindings["Buttons/B"] = SimpleBind_Drift.Text;
                        bindings["D-Pad/Left"] = SimpleBind_Item.Text;
                        bindings["Buttons/A"] = SimpleBind_LookBack.Text;
                        bindings["Shake/X"] = SimpleBind_Trick.Text;
                        bindings["Shake/Y"] = SimpleBind_Trick.Text;
                        bindings["Shake/Z"] = SimpleBind_Trick.Text;
                        bindings["Buttons/Plus"] = SimpleBind_Pause.Text;

                        if (SimpleBind_StickLeft != null) bindings["Tilt/Left"] = SimpleBind_StickLeft.Text;
                        if (SimpleBind_StickRight != null) bindings["Tilt/Right"] = SimpleBind_StickRight.Text;
                        if (SimpleBind_StickUp != null) bindings["Tilt/Forward"] = SimpleBind_StickUp.Text;
                        if (SimpleBind_StickDown != null) bindings["Tilt/Backward"] = SimpleBind_StickDown.Text;

                        if (SimpleBind_DPadUp != null) bindings["D-Pad/Up"] = SimpleBind_DPadUp.Text;
                        if (SimpleBind_DPadDown != null) bindings["D-Pad/Down"] = SimpleBind_DPadDown.Text;
                    }
                    else if (extTag == "Classic")
                    {
                        // Classic Controller
                        bindings["Classic/Buttons/A"] = SimpleBind_Accelerate.Text;
                        bindings["Classic/Buttons/B"] = SimpleBind_Brake.Text;
                        bindings["Classic/Triggers/R"] = SimpleBind_Drift.Text;
                        bindings["Classic/Triggers/L"] = SimpleBind_Item.Text;
                        bindings["Classic/Buttons/X"] = SimpleBind_LookBack.Text;
                        bindings["Classic/D-Pad/Up"] = SimpleBind_Trick.Text;
                        bindings["Classic/Buttons/Plus"] = SimpleBind_Pause.Text;

                        if (SimpleBind_StickUp != null) bindings["Classic/Left Stick/Up"] = SimpleBind_StickUp.Text;
                        if (SimpleBind_StickDown != null) bindings["Classic/Left Stick/Down"] = SimpleBind_StickDown.Text;
                        if (SimpleBind_StickLeft != null) bindings["Classic/Left Stick/Left"] = SimpleBind_StickLeft.Text;
                        if (SimpleBind_StickRight != null) bindings["Classic/Left Stick/Right"] = SimpleBind_StickRight.Text;

                        if (SimpleBind_DPadUp != null) bindings["Classic/D-Pad/Up"] = SimpleBind_DPadUp.Text;
                        if (SimpleBind_DPadDown != null) bindings["Classic/D-Pad/Down"] = SimpleBind_DPadDown.Text;
                        if (SimpleBind_DPadLeft != null) bindings["Classic/D-Pad/Left"] = SimpleBind_DPadLeft.Text;
                        if (SimpleBind_DPadRight != null) bindings["Classic/D-Pad/Right"] = SimpleBind_DPadRight.Text;
                    }
                    else
                    {
                        // Wiimote + Nunchuk
                        bindings["Buttons/2"] = SimpleBind_Accelerate.Text;
                        bindings["Buttons/1"] = SimpleBind_Brake.Text;
                        bindings["Buttons/B"] = SimpleBind_Drift.Text;
                        bindings["Nunchuk/Buttons/Z"] = SimpleBind_Item.Text;
                        bindings["Nunchuk/Buttons/C"] = SimpleBind_LookBack.Text;
                        bindings["Shake/X"] = SimpleBind_Trick.Text;
                        bindings["Shake/Y"] = SimpleBind_Trick.Text;
                        bindings["Shake/Z"] = SimpleBind_Trick.Text;
                        bindings["Buttons/Plus"] = SimpleBind_Pause.Text;

                        if (SimpleBind_StickUp != null) bindings["Nunchuk/Stick/Up"] = SimpleBind_StickUp.Text;
                        if (SimpleBind_StickDown != null) bindings["Nunchuk/Stick/Down"] = SimpleBind_StickDown.Text;
                        if (SimpleBind_StickLeft != null) bindings["Nunchuk/Stick/Left"] = SimpleBind_StickLeft.Text;
                        if (SimpleBind_StickRight != null) bindings["Nunchuk/Stick/Right"] = SimpleBind_StickRight.Text;

                        if (SimpleBind_DPadUp != null) bindings["D-Pad/Up"] = SimpleBind_DPadUp.Text;
                        if (SimpleBind_DPadDown != null) bindings["D-Pad/Down"] = SimpleBind_DPadDown.Text;
                        if (SimpleBind_DPadLeft != null) bindings["D-Pad/Left"] = SimpleBind_DPadLeft.Text;
                        if (SimpleBind_DPadRight != null) bindings["D-Pad/Right"] = SimpleBind_DPadRight.Text;
                    }
                }
            }
            else
            {
                // GameCube buttons
                bindings["Buttons/A"] = Bind_A.Text;
                bindings["Buttons/B"] = Bind_B.Text;
                bindings["Buttons/X"] = Bind_X.Text;
                bindings["Buttons/Y"] = Bind_Y.Text;
                bindings["Buttons/Z"] = Bind_Z.Text;
                bindings["Buttons/Start"] = Bind_Start.Text;
                bindings["Main Stick/Up"] = Bind_MainUp.Text;
                bindings["Main Stick/Down"] = Bind_MainDown.Text;
                bindings["Main Stick/Left"] = Bind_MainLeft.Text;
                bindings["Main Stick/Right"] = Bind_MainRight.Text;
                bindings["C-Stick/Up"] = Bind_CUp.Text;
                bindings["C-Stick/Down"] = Bind_CDown.Text;
                bindings["Triggers/L"] = Bind_L.Text;
                bindings["Triggers/R"] = Bind_R.Text;

                // Override from Simple Mode if user was in Simple Mode
                if (_isSimpleControllerMode)
                {
                    bindings["Buttons/A"] = SimpleBind_Accelerate.Text;
                    bindings["Buttons/B"] = SimpleBind_Brake.Text;
                    bindings["Triggers/R"] = SimpleBind_Drift.Text;
                    bindings["Triggers/L"] = SimpleBind_Item.Text;
                    bindings["Buttons/X"] = SimpleBind_LookBack.Text;
                    bindings["D-Pad/Up"] = SimpleBind_Trick.Text;
                    bindings["Buttons/Start"] = SimpleBind_Pause.Text;

                    // Analog Stick (Main Stick)
                    if (SimpleBind_StickUp != null) bindings["Main Stick/Up"] = SimpleBind_StickUp.Text;
                    if (SimpleBind_StickDown != null) bindings["Main Stick/Down"] = SimpleBind_StickDown.Text;
                    if (SimpleBind_StickLeft != null) bindings["Main Stick/Left"] = SimpleBind_StickLeft.Text;
                    if (SimpleBind_StickRight != null) bindings["Main Stick/Right"] = SimpleBind_StickRight.Text;

                    // D-Pad
                    if (SimpleBind_DPadUp != null) bindings["D-Pad/Up"] = SimpleBind_DPadUp.Text;
                    if (SimpleBind_DPadDown != null) bindings["D-Pad/Down"] = SimpleBind_DPadDown.Text;
                    if (SimpleBind_DPadLeft != null) bindings["D-Pad/Left"] = SimpleBind_DPadLeft.Text;
                    if (SimpleBind_DPadRight != null) bindings["D-Pad/Right"] = SimpleBind_DPadRight.Text;
                }
            }

            // Update active controller devices in Dolphin.ini under [Core]
            string dolphinIniPath = Path.Combine(userFolder, "Config", "Dolphin.ini");
            var coreUpdates = new Dictionary<string, string>();

            if (isWiimote)
            {
                int wIdx = Math.Max(0, portIndex - 1);
                coreUpdates[$"WiimoteSource{wIdx}"] = "1";
                if (portIndex == 1)
                {
                    coreUpdates["SIDevice0"] = "0";
                }
            }
            else
            {
                int gcIdx = Math.Max(0, portIndex - 1);
                coreUpdates[$"SIDevice{gcIdx}"] = "6";
                if (portIndex == 1)
                {
                    coreUpdates["WiimoteSource0"] = "0";
                }
            }

            var iniUpdates = new Dictionary<string, Dictionary<string, string>>
            {
                ["Core"] = coreUpdates
            };
            new DolphinIniService().UpdateIni(dolphinIniPath, iniUpdates);

            _controllerProfileManager.SaveActiveBindings(userFolder, isWiimote, portIndex, bindings);
            _hasUnsavedChanges = false;
            ShowSettingsStatusNotification(Loc.Format("Msg_ControllerSettingsSavedFor", _selectedControllerPort));
        }
        catch (Exception ex)
        {
            ShowSettingsStatusNotification(Loc.Format("Msg_FailedSavingControllerSettings", ex.Message));
        }
    }

    private void ManageControllersCheckBox_OnChanged(object sender, RoutedEventArgs e)
    {
        if (ControllerMappingContent == null) return;
        bool enabled = ManageControllersCheckBox?.IsChecked == true;
        ControllerMappingContent.Visibility = enabled ? Visibility.Visible : Visibility.Collapsed;

        if (enabled)
        {
            ShowSettingsStatusNotification(L("Msg_LauncherControllerMappingEnabled"));
        }
        else
        {
            ShowSettingsStatusNotification(L("Msg_LauncherControllerMappingDisabled"));
        }
    }

    private void ShowSettingsStatusNotification(string msg)
    {
        SetStatus(msg, (WpfBrush)FindResource("AccentBrush"));
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

    public MessageBoxResult Result => _result;

    public CustomDialog(string title, string message, MessageBoxButton buttons)
    {
        _buttons = buttons;
        var usesExpandedLayout = message.Length > 120 || message.Count(ch => ch == '\n') >= 2;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        Width = 600;
        MinHeight = usesExpandedLayout ? 400 : 330;
        SizeToContent = SizeToContent.Height;
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
            Width = 520,
            MinHeight = usesExpandedLayout ? 300 : 230,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Center,
            VerticalAlignment = System.Windows.VerticalAlignment.Center,
            Margin = new Thickness(40),
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

        var dialogGrid = new Grid();
        dialogGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        dialogGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        dialogGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var titleTextBlock = new TextBlock
        {
            Text = title,
            FontSize = 20,
            FontWeight = FontWeights.Black,
            Foreground = WpfBrushes.White,
            Margin = new Thickness(0, 0, 0, 12),
            FontFamily = new FontFamily("Segoe UI")
        };
        Grid.SetRow(titleTextBlock, 0);
        dialogGrid.Children.Add(titleTextBlock);

        var messageTextBlock = new TextBlock
        {
            Text = message,
            FontSize = 14,
            Foreground = new SolidColorBrush(WpfColor.FromRgb(0xA7, 0xB4, 0xCE)),
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Segoe UI"),
            Margin = new Thickness(0, 0, 0, 22)
        };
        Grid.SetRow(messageTextBlock, 1);
        dialogGrid.Children.Add(messageTextBlock);

        var buttonPanel = new StackPanel
        {
            Orientation = System.Windows.Controls.Orientation.Horizontal,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Right,
            Margin = new Thickness(0, 4, 0, 6)
        };

        if (buttons == MessageBoxButton.YesNoCancel)
        {
            var saveBtn = CreateDialogButton("Save & Switch", true);
            saveBtn.Click += (_, _) => { _result = MessageBoxResult.Yes; Close(); };
            buttonPanel.Children.Add(saveBtn);

            var discardBtn = CreateDialogButton("Discard", false);
            discardBtn.Margin = new Thickness(8, 0, 0, 0);
            discardBtn.Click += (_, _) => { _result = MessageBoxResult.No; Close(); };
            buttonPanel.Children.Add(discardBtn);

            var cancelBtn = CreateDialogButton("Cancel", false);
            cancelBtn.Margin = new Thickness(8, 0, 0, 0);
            cancelBtn.Click += (_, _) => { _result = MessageBoxResult.Cancel; Close(); };
            buttonPanel.Children.Add(cancelBtn);
        }
        else
        {
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
        }

        Grid.SetRow(buttonPanel, 2);
        dialogGrid.Children.Add(buttonPanel);
        root.Child = dialogGrid;
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

public sealed class KeyBindingWindow : Window
{
    public string SelectedBinding { get; private set; } = "";
    private readonly System.Windows.Threading.DispatcherTimer _timer;

    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct XINPUT_STATE
    {
        public uint dwPacketNumber;
        public XINPUT_GAMEPAD Gamepad;
    }

    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct XINPUT_GAMEPAD
    {
        public ushort wButtons;
        public byte bLeftTrigger;
        public byte bRightTrigger;
        public short sThumbLX;
        public short sThumbLY;
        public short sThumbRX;
        public short sThumbRY;
    }

    [System.Runtime.InteropServices.DllImport("xinput1_4.dll", EntryPoint = "XInputGetState")]
    public static extern int XInputGetState14(int dwUserIndex, out XINPUT_STATE pState);

    [System.Runtime.InteropServices.DllImport("xinput1_3.dll", EntryPoint = "XInputGetState")]
    public static extern int XInputGetState13(int dwUserIndex, out XINPUT_STATE pState);

    public KeyBindingWindow(string actionName)
    {
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        Width = 460;
        Height = 260;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        Background = System.Windows.Media.Brushes.Transparent;
        Topmost = true;

        var rootBorder = new Border
        {
            CornerRadius = new CornerRadius(16),
            Background = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x11, 0x18, 0x27)),
            BorderBrush = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x00, 0xF2, 0xFF)),
            BorderThickness = new Thickness(2),
            Padding = new Thickness(24),
            Effect = new System.Windows.Media.Effects.DropShadowEffect { BlurRadius = 30, Opacity = 0.6, Color = System.Windows.Media.Color.FromRgb(0x00, 0xF2, 0xFF) }
        };

        var stack = new StackPanel();
        stack.Children.Add(new TextBlock { Text = "🎮 REBIND CONTROLLER / KEYBOARD INPUT", FontSize = 16, FontWeight = FontWeights.Black, Foreground = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x00, 0xF2, 0xFF)), Margin = new Thickness(0, 0, 0, 10) });
        stack.Children.Add(new TextBlock { Text = $"Press any key on Keyboard or button on Gamepad for:\n[{actionName}]", FontSize = 14, FontWeight = FontWeights.Bold, Foreground = System.Windows.Media.Brushes.White, Margin = new Thickness(0, 0, 0, 16), TextWrapping = TextWrapping.Wrap });
        stack.Children.Add(new TextBlock { Text = "Listening for input... (Press ESC to cancel)", FontSize = 12, Foreground = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x94, 0xA3, 0xB8)), HorizontalAlignment = System.Windows.HorizontalAlignment.Center, Margin = new Thickness(0, 0, 0, 16) });

        var cancelBtn = new WpfButton
        {
            Content = "Cancel",
            Width = 110,
            Height = 34,
            FontWeight = FontWeights.Bold,
            Foreground = System.Windows.Media.Brushes.White,
            Background = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x21, 0x2B, 0x43)),
            BorderBrush = new SolidColorBrush(System.Windows.Media.Color.FromRgb(0x43, 0x51, 0x70)),
            BorderThickness = new Thickness(1),
            Cursor = System.Windows.Input.Cursors.Hand,
            HorizontalAlignment = System.Windows.HorizontalAlignment.Center
        };
        cancelBtn.Click += (_, _) => { DialogResult = false; Close(); };
        stack.Children.Add(cancelBtn);

        rootBorder.Child = stack;
        Content = rootBorder;

        KeyDown += KeyBindingWindow_KeyDown;

        _timer = new System.Windows.Threading.DispatcherTimer { Interval = TimeSpan.FromMilliseconds(50) };
        _timer.Tick += PollGamepadState;
        _timer.Start();
    }

    private void KeyBindingWindow_KeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (e.Key == System.Windows.Input.Key.Escape)
        {
            DialogResult = false;
            Close();
            return;
        }

        string dolphinKey = e.Key switch
        {
            System.Windows.Input.Key.Space => "`SPACE`",
            System.Windows.Input.Key.Return => "`RETURN`",
            System.Windows.Input.Key.Back => "`BACK`",
            System.Windows.Input.Key.Tab => "`TAB`",
            System.Windows.Input.Key.Up => "`UP`",
            System.Windows.Input.Key.Down => "`DOWN`",
            System.Windows.Input.Key.Left => "`LEFT`",
            System.Windows.Input.Key.Right => "`RIGHT`",
            System.Windows.Input.Key.LeftShift or System.Windows.Input.Key.RightShift => "`SHIFT`",
            System.Windows.Input.Key.LeftCtrl or System.Windows.Input.Key.RightCtrl => "`CONTROL`",
            System.Windows.Input.Key.LeftAlt or System.Windows.Input.Key.RightAlt => "`MENU`",
            _ => $"`{e.Key}`"
        };

        SelectedBinding = dolphinKey;
        _timer.Stop();
        DialogResult = true;
        Close();
    }

    private void PollGamepadState(object? sender, EventArgs e)
    {
        const short DEADZONE = 16000;
        try
        {
            if (XInputGetState14(0, out var state) == 0 || XInputGetState13(0, out state) == 0)
            {
                // Buttons (Dolphin SDL format: Button S=South/A, Button E=East/B, Button W=West/X, Button N=North/Y)
                ushort btn = state.Gamepad.wButtons;
                if ((btn & 0x1000) != 0) SetBindingAndClose("`Button S`");          // A (South)
                else if ((btn & 0x2000) != 0) SetBindingAndClose("`Button E`");     // B (East)
                else if ((btn & 0x4000) != 0) SetBindingAndClose("`Button W`");     // X (West)
                else if ((btn & 0x8000) != 0) SetBindingAndClose("`Button N`");     // Y (North)
                else if ((btn & 0x0100) != 0) SetBindingAndClose("`Shoulder L`");   // Left Bumper
                else if ((btn & 0x0200) != 0) SetBindingAndClose("`Shoulder R`");   // Right Bumper
                else if ((btn & 0x0010) != 0) SetBindingAndClose("`Start`");        // Start
                else if ((btn & 0x0020) != 0) SetBindingAndClose("`Back`");         // Back
                else if ((btn & 0x0040) != 0) SetBindingAndClose("`Thumb L`");      // Left Stick Click
                else if ((btn & 0x0080) != 0) SetBindingAndClose("`Thumb R`");      // Right Stick Click
                else if ((btn & 0x0001) != 0) SetBindingAndClose("`Pad N`");        // D-Pad Up
                else if ((btn & 0x0002) != 0) SetBindingAndClose("`Pad S`");        // D-Pad Down
                else if ((btn & 0x0004) != 0) SetBindingAndClose("`Pad W`");        // D-Pad Left
                else if ((btn & 0x0008) != 0) SetBindingAndClose("`Pad E`");        // D-Pad Right

                // Triggers (analog)
                else if (state.Gamepad.bLeftTrigger > 100) SetBindingAndClose("`Trigger L`");
                else if (state.Gamepad.bRightTrigger > 100) SetBindingAndClose("`Trigger R`");

                // Left Analog Stick axes
                else if (state.Gamepad.sThumbLY > DEADZONE) SetBindingAndClose("`Left Y+`");   // Left Stick Up
                else if (state.Gamepad.sThumbLY < -DEADZONE) SetBindingAndClose("`Left Y-`");  // Left Stick Down
                else if (state.Gamepad.sThumbLX < -DEADZONE) SetBindingAndClose("`Left X-`");  // Left Stick Left
                else if (state.Gamepad.sThumbLX > DEADZONE) SetBindingAndClose("`Left X+`");   // Left Stick Right

                // Right Analog Stick axes
                else if (state.Gamepad.sThumbRY > DEADZONE) SetBindingAndClose("`Right Y+`");  // Right Stick Up
                else if (state.Gamepad.sThumbRY < -DEADZONE) SetBindingAndClose("`Right Y-`"); // Right Stick Down
                else if (state.Gamepad.sThumbRX < -DEADZONE) SetBindingAndClose("`Right X-`"); // Right Stick Left
                else if (state.Gamepad.sThumbRX > DEADZONE) SetBindingAndClose("`Right X+`");  // Right Stick Right
            }
        }
        catch { }
    }

    private void SetBindingAndClose(string binding)
    {
        SelectedBinding = binding;
        _timer.Stop();
        DialogResult = true;
        Close();
    }
}
