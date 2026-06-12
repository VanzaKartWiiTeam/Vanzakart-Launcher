using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using VanzaKartLauncher;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher.ViewModels;

public sealed class FriendPlayerInfo : BaseViewModel
{
    public int SlotIndex { get; init; }
    public uint ProfileId { get; init; }
    public string FriendCode { get; init; } = string.Empty;
    public string LocalMiiName { get; init; } = string.Empty;

    private string _displayName = string.Empty;
    public string DisplayName
    {
        get => _displayName;
        set => SetProperty(ref _displayName, value);
    }

    private int _vr = 5000;
    public int Vr
    {
        get => _vr;
        set => SetProperty(ref _vr, value);
    }

    private int _br = 5000;
    public int Br
    {
        get => _br;
        set => SetProperty(ref _br, value);
    }

    private int _wins;
    public int Wins
    {
        get => _wins;
        set => SetProperty(ref _wins, value);
    }

    private int _losses;
    public int Losses
    {
        get => _losses;
        set => SetProperty(ref _losses, value);
    }

    private string _avatarImagePath = string.Empty;
    public string AvatarImagePath
    {
        get => _avatarImagePath;
        set
        {
            if (SetProperty(ref _avatarImagePath, value))
            {
                OnPropertyChanged(nameof(HasAvatarImage));
            }
        }
    }

    public bool HasAvatarImage => !string.IsNullOrWhiteSpace(AvatarImagePath);

    private string _status = "Offline";
    public string Status
    {
        get => _status;
        set => SetProperty(ref _status, value);
    }

    public string AvatarInitial => string.IsNullOrWhiteSpace(DisplayName) ? "?" : DisplayName.Substring(0, 1).ToUpperInvariant();

    public byte[] MiiData { get; init; } = [];
    public WiiMiiData? ParsedMii { get; init; }
    public byte RosterIndex { get; init; }
    public byte GameRegion { get; init; }
    public byte CountryId { get; init; }
    public byte RegionId { get; init; }
    public ushort CityId { get; init; }
    public ushort GlobeX { get; init; }
    public ushort GlobeY { get; init; }
    public bool IsPending { get; init; }

    public string StatsLine => $"Wins: {Wins}  |  Losses: {Losses}";
}

public sealed class FriendsViewModel : BaseViewModel
{
    private readonly NetworkService _networkService;
    private readonly MiiFileParserService _miiFileParserService = new();
    private readonly MiiAvatarRenderService _miiAvatarRenderService = new();
    private CancellationTokenSource? _avatarCts;

    private SaveProfileInfo? _activeLicense;
    private string _friendCodeInput = string.Empty;
    private bool _isLoading;
    private bool _hasError;
    private string _errorMessage = string.Empty;

    public FriendsViewModel(NetworkService networkService)
    {
        _networkService = networkService;
        FriendsList = new ObservableCollection<FriendPlayerInfo>();
        FriendsList.CollectionChanged += (s, e) =>
        {
            OnPropertyChanged(nameof(ShowEmptyPlaceholder));
        };
    }

    public ObservableCollection<FriendPlayerInfo> FriendsList { get; }

    public SaveProfileInfo? ActiveLicense
    {
        get => _activeLicense;
        set
        {
            if (SetProperty(ref _activeLicense, value))
            {
                OnPropertyChanged(nameof(HasActiveLicense));
                OnPropertyChanged(nameof(HasNoActiveLicense));
                OnPropertyChanged(nameof(ShowEmptyPlaceholder));
                LoadFriends();
            }
        }
    }

    public bool HasActiveLicense => ActiveLicense != null;
    public bool HasNoActiveLicense => ActiveLicense == null;
    public bool ShowEmptyPlaceholder => ActiveLicense != null && FriendsList.Count == 0;

    public string FriendCodeInput
    {
        get => _friendCodeInput;
        set => SetProperty(ref _friendCodeInput, value);
    }

    public bool IsLoading
    {
        get => _isLoading;
        private set => SetProperty(ref _isLoading, value);
    }

    public bool HasError
    {
        get => _hasError;
        private set => SetProperty(ref _hasError, value);
    }

    public string ErrorMessage
    {
        get => _errorMessage;
        private set => SetProperty(ref _errorMessage, value);
    }

    public void LoadFriends()
    {
        _avatarCts?.Cancel();
        FriendsList.Clear();
        HasError = false;
        ErrorMessage = string.Empty;

        if (ActiveLicense == null)
        {
            return;
        }

        var filePath = ActiveLicense.FilePath;
        var slotIndex = ActiveLicense.SlotIndex;

        _ = Task.Run(async () =>
        {
            try
            {
                var friends = RksysManager.ReadFriends(filePath, slotIndex, _miiFileParserService);
                
                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    foreach (var f in friends)
                    {
                        FriendsList.Add(new FriendPlayerInfo
                        {
                            SlotIndex = f.SlotIndex,
                            ProfileId = f.ProfileId,
                            FriendCode = f.FriendCode,
                            LocalMiiName = f.MiiName,
                            DisplayName = f.MiiName,
                            Vr = f.RaceRating,
                            Br = f.BattleRating,
                            Wins = f.Wins,
                            Losses = f.Losses,
                            Status = f.IsPending ? "Pending" : "Offline",
                            MiiData = f.MiiData,
                            ParsedMii = f.ParsedMii,
                            RosterIndex = f.RosterIndex,
                            GameRegion = f.GameRegion,
                            CountryId = f.CountryId,
                            RegionId = f.RegionId,
                            CityId = f.CityId,
                            GlobeX = f.GlobeX,
                            GlobeY = f.GlobeY,
                            IsPending = f.IsPending
                        });
                    }
                });

                _ = ResolveOnlineDetailsAsync();
            }
            catch (Exception ex)
            {
                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    HasError = true;
                    ErrorMessage = $"Unable to load friend list: {ex.Message}";
                });
            }
        });
    }

    public async Task AddFriendAsync()
    {
        if (ActiveLicense == null)
        {
            ShowCustomDialog("Error", "No active license selected.");
            return;
        }

        string input = FriendCodeInput?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(input))
        {
            ShowCustomDialog("Error", "Please enter a valid friend code.");
            return;
        }

        if (!RksysManager.TryParseFriendCode(input, out uint pid, out string parseError))
        {
            ShowCustomDialog("Invalid Friend Code", parseError);
            return;
        }

        // Check for duplicates
        string cleanInputFc = CleanFriendCode(input);
        if (FriendsList.Any(f => CleanFriendCode(f.FriendCode) == cleanInputFc))
        {
            ShowCustomDialog("Duplicate", "This friend is already present in your friend list.");
            return;
        }

        IsLoading = true;
        try
        {
            await Task.Run(() => RksysManager.AddFriend(ActiveLicense.FilePath, ActiveLicense.SlotIndex, pid));
            FriendCodeInput = string.Empty;
            LoadFriends();
            ShowCustomDialog("Success", "Friend added successfully!");
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Error", $"Error adding friend: {ex.Message}");
        }
        finally
        {
            IsLoading = false;
        }
    }

    public async Task RemoveFriendAsync(FriendPlayerInfo friend)
    {
        if (ActiveLicense == null || friend == null) return;

        var result = ShowCustomDialog("Remove Friend", $"Are you sure you want to remove '{friend.DisplayName}' ({friend.FriendCode})?", MessageBoxButton.YesNo);
        if (result != MessageBoxResult.Yes) return;

        IsLoading = true;
        try
        {
            await Task.Run(() => RksysManager.RemoveFriend(ActiveLicense.FilePath, ActiveLicense.SlotIndex, friend.SlotIndex));
            LoadFriends();
            ShowCustomDialog("Success", "Friend removed successfully.");
        }
        catch (Exception ex)
        {
            ShowCustomDialog("Error", $"Error removing friend: {ex.Message}");
        }
        finally
        {
            IsLoading = false;
        }
    }

    private async Task ResolveOnlineDetailsAsync()
    {
        _avatarCts?.Cancel();
        _avatarCts = new CancellationTokenSource();
        var token = _avatarCts.Token;

        try
        {
            string url = $"{LauncherConfig.LeaderboardApiUrl}?limit=200";
            string json = await _networkService.DownloadStringAsync(url, token);

            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };

            var response = JsonSerializer.Deserialize<LeaderboardApiResponse>(json, options);
            if (response != null && response.Success && response.Players != null)
            {
                var playerMap = response.Players
                    .Where(p => !string.IsNullOrEmpty(p.FriendCode))
                    .ToDictionary(
                        p => CleanFriendCode(p.FriendCode),
                        p => p,
                        StringComparer.OrdinalIgnoreCase
                    );

                foreach (var friend in FriendsList)
                {
                    string cleanFc = CleanFriendCode(friend.FriendCode);
                    if (playerMap.TryGetValue(cleanFc, out var onlineInfo))
                    {
                        friend.DisplayName = onlineInfo.Name;
                        friend.Vr = onlineInfo.Points;
                        friend.Wins = onlineInfo.Wins;
                        friend.Losses = onlineInfo.Races - onlineInfo.Wins;
                    }
                }
            }
        }
        catch
        {
            // Fail silently on API network issues
        }

        // Render avatars in background
        _ = Task.Run(async () =>
        {
            try
            {
                foreach (var friend in FriendsList.ToList())
                {
                    if (token.IsCancellationRequested) break;

                    string? path = null;
                    if (friend.ParsedMii != null)
                    {
                        path = await _miiAvatarRenderService.EnsureAvatarAsync(friend.ParsedMii, token);
                    }

                    if (string.IsNullOrWhiteSpace(path) || path == _miiAvatarRenderService.GetFallbackSilhouettePath())
                    {
                        // Fallback to silhouette or online image
                        path = _miiAvatarRenderService.GetFallbackSilhouettePath();
                    }

                    if (!string.IsNullOrWhiteSpace(path))
                    {
                        friend.AvatarImagePath = path;
                    }
                }
            }
            catch
            {
                // Fail silently on rendering errors
            }
        }, token);
    }

    private static string CleanFriendCode(string fc)
    {
        if (string.IsNullOrWhiteSpace(fc)) return string.Empty;
        return new string(fc.Where(char.IsLetterOrDigit).ToArray());
    }

    private static MessageBoxResult ShowCustomDialog(string title, string message, MessageBoxButton buttons = MessageBoxButton.OK)
    {
        return Application.Current.Dispatcher.Invoke(() =>
        {
            var owner = Application.Current.Windows.OfType<Window>().FirstOrDefault(w => w.IsActive) 
                        ?? Application.Current.MainWindow;
            var dialog = new CustomDialog(title, message, buttons)
            {
                Owner = owner
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
        });
    }

    private sealed class LeaderboardApiResponse
    {
        public bool Success { get; set; }
        public List<LeaderboardPlayerInfo>? Players { get; set; }
    }
}
