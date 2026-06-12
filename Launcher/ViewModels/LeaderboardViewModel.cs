using System.Collections.ObjectModel;
using System.IO;
using System.Net.Http;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher.ViewModels;

public sealed class LeaderboardViewModel : BaseViewModel
{
    private readonly NetworkService _networkService;
    private readonly MiiFileParserService _miiFileParserService = new();
    private readonly MiiAvatarRenderService _miiAvatarRenderService = new();
    private CancellationTokenSource? _avatarCts;
    private readonly List<LeaderboardPlayerInfo> _allPlayersRaw = new();
    private bool _isLoading;
    private bool _hasError;
    private string _errorMessage = string.Empty;
    private string _searchText = string.Empty;
    private string _selectedSort = "Points";
    private LeaderboardPlayerInfo? _top1;
    private LeaderboardPlayerInfo? _top2;
    private LeaderboardPlayerInfo? _top3;
    private List<string> _localFriendCodes = new();

    public LeaderboardViewModel(NetworkService networkService)
    {
        _networkService = networkService;
        Players = new ObservableCollection<LeaderboardPlayerInfo>();
        
        Sorts = new ObservableCollection<string> { "Points", "Wins", "Win Rate" };
    }

    public ObservableCollection<LeaderboardPlayerInfo> Players { get; }
    public ObservableCollection<string> Sorts { get; }

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

    public string SearchText
    {
        get => _searchText;
        set
        {
            if (SetProperty(ref _searchText, value))
            {
                ApplyFiltersAndSorting();
            }
        }
    }

    public string SelectedSort
    {
        get => _selectedSort;
        set
        {
            if (SetProperty(ref _selectedSort, value))
            {
                ApplyFiltersAndSorting();
            }
        }
    }

    public LeaderboardPlayerInfo? Top1
    {
        get => _top1;
        private set => SetProperty(ref _top1, value);
    }

    public LeaderboardPlayerInfo? Top2
    {
        get => _top2;
        private set => SetProperty(ref _top2, value);
    }

    public LeaderboardPlayerInfo? Top3
    {
        get => _top3;
        private set => SetProperty(ref _top3, value);
    }

    public void UpdateLocalFriendCodes(IEnumerable<string> friendCodes)
    {
        _localFriendCodes = friendCodes
            .Where(fc => !string.IsNullOrWhiteSpace(fc))
            .Select(CleanFriendCode)
            .ToList();
            
        UpdateSelfFlags();
    }

    public async Task RefreshAsync()
    {
        _avatarCts?.Cancel(); // Cancel any running avatar render task

        IsLoading = true;
        HasError = false;

        try
        {
            // Note: In a production scenario, we query the endpoint defined in LauncherConfig
            string url = $"{LauncherConfig.LeaderboardApiUrl}?limit=100";
            string json = await _networkService.DownloadStringAsync(url);
            
            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };
            
            var response = JsonSerializer.Deserialize<LeaderboardApiResponse>(json, options);
            
            if (response != null && response.Success)
            {
                _allPlayersRaw.Clear();
                if (response.Players != null)
                {
                    _allPlayersRaw.AddRange(response.Players);
                }
                
                UpdateSelfFlags();
                ApplyFiltersAndSorting();
                StartAvatarRenderingBackground();
                HasError = false;
            }
            else
            {
                throw new Exception("The API returned an invalid response.");
            }
        }
        catch (Exception ex)
        {
            HasError = true;
            ErrorMessage = $"Unable to load leaderboard: {ex.Message}";
            _allPlayersRaw.Clear();
            Players.Clear();
            Top1 = null;
            Top2 = null;
            Top3 = null;
        }
        finally
        {
            IsLoading = false;
        }
    }

    private void UpdateSelfFlags()
    {
        foreach (var player in _allPlayersRaw)
        {
            string cleanPlayerFc = CleanFriendCode(player.FriendCode);
            player.IsSelf = _localFriendCodes.Contains(cleanPlayerFc);
        }
        
        if (Top1 != null) Top1.IsSelf = _localFriendCodes.Contains(CleanFriendCode(Top1.FriendCode));
        if (Top2 != null) Top2.IsSelf = _localFriendCodes.Contains(CleanFriendCode(Top2.FriendCode));
        if (Top3 != null) Top3.IsSelf = _localFriendCodes.Contains(CleanFriendCode(Top3.FriendCode));
    }

    private void ApplyFiltersAndSorting()
    {
        // 1. Applica Filtro Ricerca e Rango
        var filtered = _allPlayersRaw.AsEnumerable();

        if (!string.IsNullOrWhiteSpace(SearchText))
        {
            string search = SearchText.Trim().ToLowerInvariant();
            filtered = filtered.Where(p => 
                p.Name.ToLowerInvariant().Contains(search) || 
                p.FriendCode.Replace("-", "").Contains(search)
            );
        }



        // 2. Applica Ordinamento
        switch (SelectedSort)
        {
            case "Wins":
                filtered = filtered.OrderByDescending(p => p.Wins);
                break;
            case "Win Rate":
                filtered = filtered.OrderByDescending(p => p.WinRate);
                break;
            case "Points":
            default:
                filtered = filtered.OrderByDescending(p => p.Points);
                break;
        }

        var list = filtered.ToList();

        // 3. Estrai i primi 3 assoluti della lista totale (non filtrata per ricerca) per il podio
        var podiumList = _allPlayersRaw.Take(3).ToList();
        
        Top1 = podiumList.Count > 0 ? podiumList[0] : null;
        Top2 = podiumList.Count > 1 ? podiumList[1] : null;
        Top3 = podiumList.Count > 2 ? podiumList[2] : null;

        // Per la lista sotto, mostriamo tutti gli elementi filtrati, ma escludendo i primi 3 assoluti se la lista visualizzata non è filtrata
        // In questo modo, se mostriamo la classifica generale senza filtri, la lista sotto parte dal 4° posto.
        // Se c'è una ricerca o un filtro attivo, mostriamo TUTTI i risultati filtrati per facilitare la ricerca dell'utente.
        Players.Clear();
        
        bool hasActiveFilters = !string.IsNullOrWhiteSpace(SearchText);

        foreach (var player in list)
        {
            if (hasActiveFilters || player.Position > 3)
            {
                Players.Add(player);
            }
        }
    }

    private void StartAvatarRenderingBackground()
    {
        _avatarCts = new CancellationTokenSource();
        var token = _avatarCts.Token;
        var players = _allPlayersRaw.ToList();

        Task.Run(async () =>
        {
            try
            {
                foreach (var player in players)
                {
                    if (token.IsCancellationRequested) break;
                    if (string.IsNullOrWhiteSpace(player.MiiData)) continue;

                    try
                    {
                        var rawBlock = Convert.FromBase64String(player.MiiData);
                        var wiiMii = _miiFileParserService.ParseWiiMiiBlock(rawBlock);
                        
                        var cacheKey = MiiAvatarRenderService.GetRenderCacheKey(wiiMii);
                        var cachePath = _miiAvatarRenderService.GetAvatarCachePath(cacheKey);

                        // Delete existing low-resolution cached database thumbnails (< 10KB) to force high-quality rendering
                        if (File.Exists(cachePath))
                        {
                            try
                            {
                                var info = new FileInfo(cachePath);
                                if (info.Length < 10000)
                                {
                                    File.Delete(cachePath);
                                }
                            }
                            catch
                            {
                                // Ignore file errors
                            }
                        }

                        // Render the high-resolution 512x512 Mii image
                        var path = await _miiAvatarRenderService.EnsureAvatarAsync(wiiMii, token);
                        var silhouettePath = _miiAvatarRenderService.GetFallbackSilhouettePath();

                        // Fallback to the pre-rendered database image if high-res rendering failed or returned silhouette (e.g. offline)
                        if ((string.IsNullOrWhiteSpace(path) || path == silhouettePath) && !string.IsNullOrWhiteSpace(player.MiiImage))
                        {
                            try
                            {
                                Directory.CreateDirectory(_miiAvatarRenderService.GetAvatarCacheFolder());
                                await File.WriteAllBytesAsync(cachePath, Convert.FromBase64String(player.MiiImage), token);
                                path = cachePath;
                            }
                            catch
                            {
                                // Ignore file write errors
                            }
                        }

                        if (!string.IsNullOrWhiteSpace(path))
                        {
                            player.AvatarImagePath = path;
                        }
                    }
                    catch
                    {
                        // Ignore individual player avatar errors
                    }
                }
            }
            catch (OperationCanceledException)
            {
                // Ignored
            }
        }, token);
    }

    private static string CleanFriendCode(string fc)
    {
        if (string.IsNullOrWhiteSpace(fc)) return string.Empty;
        return new string(fc.Where(char.IsLetterOrDigit).ToArray());
    }

    private sealed class LeaderboardApiResponse
    {
        public bool Success { get; set; }
        public List<LeaderboardPlayerInfo>? Players { get; set; }
    }
}
