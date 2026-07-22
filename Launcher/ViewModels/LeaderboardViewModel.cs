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
    private string _selectedSort = "Global Rank";
    private string _summaryText = "Waiting for rankings";
    private LeaderboardPlayerInfo? _top1;
    private LeaderboardPlayerInfo? _top2;
    private LeaderboardPlayerInfo? _top3;
    private List<string> _localFriendCodes = new();

    public LeaderboardViewModel(NetworkService networkService)
    {
        _networkService = networkService;
        Players = new ObservableCollection<LeaderboardPlayerInfo>();
        
        Sorts = new ObservableCollection<string> { "Global Rank", "Points", "Prestige" };
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

    public string SummaryText
    {
        get => _summaryText;
        private set => SetProperty(ref _summaryText, value);
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
            string url = $"{LauncherConfig.LeaderboardApiUrl}?limit=200&offset=0";
            Task<string> rankingRequest = _networkService.DownloadStringAsync(url);
            Task<LeaderboardDetailsResponse?> detailsRequest = TryDownloadDetailsAsync();
            string json = await rankingRequest;
            
            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };
            
            var response = JsonSerializer.Deserialize<LeaderboardApiResponse>(json, options);
            
            if (response != null && response.Success)
            {
                var details = await detailsRequest;
                var detailsByFriendCode = details?.Players?
                    .Where(player => !string.IsNullOrWhiteSpace(player.FriendCode))
                    .GroupBy(player => CleanFriendCode(player.FriendCode), StringComparer.OrdinalIgnoreCase)
                    .ToDictionary(group => group.Key, group => group.First(), StringComparer.OrdinalIgnoreCase)
                    ?? new Dictionary<string, LeaderboardDetailsPlayer>(StringComparer.OrdinalIgnoreCase);

                _allPlayersRaw.Clear();
                if (response.Players != null)
                {
                    foreach (var rankedPlayer in response.Players)
                    {
                        detailsByFriendCode.TryGetValue(CleanFriendCode(rankedPlayer.FriendCode), out var detail);
                        _allPlayersRaw.Add(CreatePlayer(rankedPlayer, detail));
                    }
                }

                SummaryText = $"{_allPlayersRaw.Count:N0} ranked players | Updated {DateTime.Now:HH:mm}";
                
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
            SummaryText = "Rankings unavailable";
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
        // The API position is authoritative for the global ranking. Alternative
        // views can still assign a temporary display order without changing it.
        var globalSorted = SortPlayers(_allPlayersRaw).ToList();
        for (var i = 0; i < globalSorted.Count; i++)
        {
            globalSorted[i].DisplayPosition = i + 1;
        }

        // 2. Applica Filtro Ricerca sulla classifica già ordinata
        var filtered = globalSorted.AsEnumerable();

        if (!string.IsNullOrWhiteSpace(SearchText))
        {
            string search = SearchText.Trim().ToLowerInvariant();
            filtered = filtered.Where(p => 
                p.Name.ToLowerInvariant().Contains(search) || 
                p.FriendCode.Replace("-", "").Contains(search)
            );
        }

        var list = filtered.ToList();

        // The podium always represents the authoritative global top three.
        var podiumList = _allPlayersRaw
            .OrderBy(player => player.Position)
            .ThenByDescending(player => player.Points)
            .Take(3)
            .ToList();
        var podiumPlayers = podiumList.ToHashSet();
        
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
            if (hasActiveFilters || !podiumPlayers.Contains(player))
            {
                Players.Add(player);
            }
        }
    }

    private IEnumerable<LeaderboardPlayerInfo> SortPlayers(IEnumerable<LeaderboardPlayerInfo> players)
    {
        return SelectedSort switch
        {
            "Points" => players
                .OrderByDescending(p => p.Points)
                .ThenBy(p => p.Position),
            "Prestige" => players
                .OrderByDescending(p => p.PrestigeRank)
                .ThenByDescending(p => p.Points)
                .ThenBy(p => p.Position),
            _ => players
                .OrderBy(p => p.Position)
                .ThenByDescending(p => p.Points)
        };
    }

    private async Task<LeaderboardDetailsResponse?> TryDownloadDetailsAsync()
    {
        try
        {
            string json = await _networkService.DownloadStringAsync(LauncherConfig.LeaderboardDetailsApiUrl);
            return JsonSerializer.Deserialize<LeaderboardDetailsResponse>(json, new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            });
        }
        catch
        {
            // The PHP ranking remains usable if the optional details endpoint is unavailable.
            return null;
        }
    }

    private static LeaderboardPlayerInfo CreatePlayer(
        LeaderboardApiPlayer rankedPlayer,
        LeaderboardDetailsPlayer? detail)
    {
        int resolvedPrestigeRank = detail?.GetPrestigeRank(detail.PrestigeRank) 
                                 ?? rankedPlayer.GetPrestigeRank(rankedPlayer.PrestigeRank);
        if (resolvedPrestigeRank == 0 && detail != null)
        {
            resolvedPrestigeRank = rankedPlayer.GetPrestigeRank(rankedPlayer.PrestigeRank);
        }

        return new LeaderboardPlayerInfo
        {
            Position = rankedPlayer.Position > 0 ? rankedPlayer.Position : detail?.Rank ?? 0,
            Name = string.IsNullOrWhiteSpace(rankedPlayer.Name) ? detail?.Name ?? string.Empty : rankedPlayer.Name,
            Points = rankedPlayer.Points > 0 ? rankedPlayer.Points : detail?.Vr ?? 0,
            FriendCode = string.IsNullOrWhiteSpace(rankedPlayer.FriendCode)
                ? detail?.FriendCode ?? string.Empty
                : rankedPlayer.FriendCode,
            PrestigeRank = resolvedPrestigeRank,
            LastSeen = detail?.LastSeen,
            IsSuspicious = detail?.IsSuspicious ?? false,
            VrLast24Hours = detail?.VrStats?.Last24Hours ?? 0,
            VrLastWeek = detail?.VrStats?.LastWeek ?? 0,
            VrLastMonth = detail?.VrStats?.LastMonth ?? 0,
            MiiData = detail?.MiiData ?? rankedPlayer.MiiData,
            MiiImage = detail?.MiiImageBase64 ?? rankedPlayer.MiiImage,
            RankImageUrl = NormalizeServerAssetUrl(
                rankedPlayer.GetRankImageUrl() ?? detail?.GetRankImageUrl())
        };
    }

    private static string? NormalizeServerAssetUrl(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        if (Uri.TryCreate(value, UriKind.Absolute, out var absoluteUri))
        {
            return absoluteUri.ToString();
        }

        return Uri.TryCreate(new Uri("https://sitodaking.it:8443/"), value, out var serverUri)
            ? serverUri.ToString()
            : null;
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
        public LeaderboardApiMeta? Meta { get; set; }
        public List<LeaderboardApiPlayer>? Players { get; set; }
    }

    private sealed class LeaderboardApiMeta
    {
        public int Limit { get; set; }
        public int Offset { get; set; }
        public int Count { get; set; }
    }

    private sealed class LeaderboardApiPlayer : RankImageApiModel
    {
        public int Position { get; set; }
        public string Name { get; set; } = string.Empty;
        public int Points { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("fc")]
        public string FriendCode { get; set; } = string.Empty;

        public int PrestigeRank { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("mii_data")]
        public string? MiiData { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("mii_image")]
        public string? MiiImage { get; set; }
    }

    private sealed class LeaderboardDetailsResponse
    {
        public List<LeaderboardDetailsPlayer>? Players { get; set; }
        public int TotalCount { get; set; }
    }

    private sealed class LeaderboardDetailsPlayer : RankImageApiModel
    {
        public string Name { get; set; } = string.Empty;
        public string FriendCode { get; set; } = string.Empty;
        public int Vr { get; set; }
        public int Rank { get; set; }
        public int PrestigeRank { get; set; }
        public DateTimeOffset? LastSeen { get; set; }
        public bool IsSuspicious { get; set; }
        public LeaderboardVrStats? VrStats { get; set; }
        public string? MiiImageBase64 { get; set; }
        public string? MiiData { get; set; }
    }

    private sealed class LeaderboardVrStats
    {
        public int Last24Hours { get; set; }
        public int LastWeek { get; set; }
        public int LastMonth { get; set; }
    }

    private abstract class RankImageApiModel
    {
        public string? RankImageUrl { get; set; }
        public string? RankIconUrl { get; set; }
        public string? RankImage { get; set; }
        public string? RankIcon { get; set; }
        public string? BadgeImageUrl { get; set; }

        [System.Text.Json.Serialization.JsonExtensionData]
        public Dictionary<string, JsonElement>? AdditionalFields { get; set; }

        public string? GetRankImageUrl()
        {
            string? direct = FirstNotEmpty(RankImageUrl, RankIconUrl, RankImage, RankIcon, BadgeImageUrl);
            if (!string.IsNullOrWhiteSpace(direct) || AdditionalFields == null)
            {
                return direct;
            }

            string[] aliases =
            {
                "rank_image_url", "rank_icon_url", "rank_image", "rank_icon",
                "rankBadgeUrl", "rank_badge_url", "badge_url", "prestigeIconUrl",
                "prestige_icon_url"
            };

            foreach (string alias in aliases)
            {
                var entry = AdditionalFields.FirstOrDefault(pair =>
                    string.Equals(pair.Key, alias, StringComparison.OrdinalIgnoreCase));
                if (!string.IsNullOrEmpty(entry.Key) && entry.Value.ValueKind == JsonValueKind.String)
                {
                    string? candidate = entry.Value.GetString();
                    if (!string.IsNullOrWhiteSpace(candidate))
                    {
                        return candidate;
                    }
                }
            }

            return null;
        }

        public int GetPrestigeRank(int directPrestigeRank)
        {
            if (directPrestigeRank >= 1 && directPrestigeRank <= 8)
            {
                return directPrestigeRank;
            }

            if (AdditionalFields == null)
            {
                return 0;
            }

            string[] aliases = { "prestigeRank", "pr", "prestige_rank", "prestige", "rank_prestige" };
            foreach (string alias in aliases)
            {
                var entry = AdditionalFields.FirstOrDefault(pair =>
                    string.Equals(pair.Key, alias, StringComparison.OrdinalIgnoreCase));
                if (!string.IsNullOrEmpty(entry.Key))
                {
                    if (entry.Value.ValueKind == JsonValueKind.Number && entry.Value.TryGetInt32(out int val) && val >= 1 && val <= 8)
                    {
                        return val;
                    }
                    if (entry.Value.ValueKind == JsonValueKind.String && int.TryParse(entry.Value.GetString(), out int strVal) && strVal >= 1 && strVal <= 8)
                    {
                        return strVal;
                    }
                }
            }

            return 0;
        }

        private static string? FirstNotEmpty(params string?[] values) =>
            values.FirstOrDefault(value => !string.IsNullOrWhiteSpace(value));
    }

}
