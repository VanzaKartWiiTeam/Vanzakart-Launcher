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
        
        Sorts = new ObservableCollection<string> { "Global Rank", "Points", "Wins", "Games", "Winrate", "Prestige" };
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
                    foreach (var rankedPlayer in response.Players)
                    {
                        _allPlayersRaw.Add(CreatePlayer(rankedPlayer));
                    }
                }

                SummaryText = $"{_allPlayersRaw.Count:N0} ranked players | Updated {DateTime.Now:HH:mm}";
                
                UpdateSelfFlags();
                ApplyFiltersAndSorting();
                StartAvatarRenderingBackground();
                StartRankImageCachingBackground();
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
            "Wins" => players
                .OrderByDescending(p => p.Wins)
                .ThenByDescending(p => p.Points)
                .ThenBy(p => p.Position),
            "Games" => players
                .OrderByDescending(p => p.TotalGames)
                .ThenByDescending(p => p.Points)
                .ThenBy(p => p.Position),
            "Winrate" => players
                .OrderByDescending(p => p.Winrate)
                .ThenByDescending(p => p.Wins)
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

    private static LeaderboardPlayerInfo CreatePlayer(LeaderboardApiPlayer rankedPlayer)
    {
        int resolvedPrestigeRank = rankedPlayer.GetPrestigeRank(rankedPlayer.PrestigeRank, rankedPlayer.Points);
        int totalGames = rankedPlayer.Races > 0 ? rankedPlayer.Races : rankedPlayer.Games;
        int safeWins = Math.Min(Math.Max(0, rankedPlayer.Wins), Math.Max(0, totalGames));
        double winrate = totalGames > 0
            ? Math.Round((double)safeWins / totalGames * 100.0, 1)
            : rankedPlayer.Winrate;

        string? rawRankUrl = NormalizeServerAssetUrl(rankedPlayer.GetRankImageUrl());
        string? rankImageUrl = resolvedPrestigeRank >= 1
            ? (rawRankUrl ?? $"{LauncherConfig.RankImagesBaseUrl.TrimEnd('/')}/rank-{resolvedPrestigeRank}.png")
            : null;

        return new LeaderboardPlayerInfo
        {
            Position = rankedPlayer.Position,
            Name = rankedPlayer.Name,
            Points = rankedPlayer.Points,
            Wins = safeWins,
            Races = totalGames,
            Games = totalGames,
            Winrate = winrate,
            FriendCode = rankedPlayer.FriendCode,
            PrestigeRank = resolvedPrestigeRank,
            LastSeen = rankedPlayer.LastSeen,
            VrLast24Hours = rankedPlayer.VrLast24Hours,
            VrLastWeek = rankedPlayer.VrLastWeek,
            VrLastMonth = rankedPlayer.VrLastMonth,
            IsSuspicious = rankedPlayer.IsSuspicious,
            MiiData = rankedPlayer.MiiData,
            MiiImage = rankedPlayer.MiiImage,
            RankImageUrl = rankImageUrl
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

        return Uri.TryCreate(new Uri(LauncherConfig.ServerBaseUrl), value, out var serverUri)
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

                        // If already cached with valid high-quality size, use immediately
                        if (File.Exists(cachePath))
                        {
                            try
                            {
                                var info = new FileInfo(cachePath);
                                if (info.Length >= 10000)
                                {
                                    player.AvatarImagePath = cachePath;
                                    continue;
                                }
                            }
                            catch
                            {
                                // Ignore
                            }
                        }

                        // Render the high-resolution 512x512 Mii image
                        var path = await _miiAvatarRenderService.EnsureAvatarAsync(wiiMii, token);
                        var silhouettePath = _miiAvatarRenderService.GetFallbackSilhouettePath();

                        if (!string.IsNullOrWhiteSpace(path) && path != silhouettePath)
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

    private static readonly HttpClient _rankImageHttpClient = new(new SocketsHttpHandler
    {
        ConnectTimeout = TimeSpan.FromSeconds(10),
        PooledConnectionLifetime = TimeSpan.FromMinutes(15),
        SslOptions = new System.Net.Security.SslClientAuthenticationOptions
        {
            RemoteCertificateValidationCallback = (sender, cert, chain, sslPolicyErrors) => true
        }
    })
    {
        Timeout = TimeSpan.FromSeconds(15)
    };

    private void StartRankImageCachingBackground()
    {
        var players = _allPlayersRaw.Where(p => p.HasPrestigeRank).ToList();
        if (players.Count == 0) return;

        var cacheDir = Path.Combine(AppContext.BaseDirectory, "Cache", "RankImages");
        Directory.CreateDirectory(cacheDir);

        Task.Run(async () =>
        {
            var defaultPath = Path.Combine(cacheDir, "rank-1.png");

            // Ensure rank-1.png is downloaded as default fallback for all ranked players
            if (!File.Exists(defaultPath) || new FileInfo(defaultPath).Length < 500)
            {
                try
                {
                    var defaultBytes = await _rankImageHttpClient.GetByteArrayAsync($"{LauncherConfig.RankImagesBaseUrl.TrimEnd('/')}/rank-1.png");
                    if (defaultBytes.Length >= 500)
                    {
                        await File.WriteAllBytesAsync(defaultPath, defaultBytes);
                    }
                }
                catch { /* ignore */ }
            }

            // Assign rank-1.png fallback to all ranked players immediately so no badge appears blank
            if (File.Exists(defaultPath))
            {
                foreach (var p in players)
                {
                    if (string.IsNullOrWhiteSpace(p.RankImageUrl) || p.RankImageUrl.StartsWith("http", StringComparison.OrdinalIgnoreCase))
                    {
                        p.RankImageUrl = defaultPath;
                    }
                }
            }

            // Download specific rank numbers if available on server
            var uniqueRanks = players.Select(p => p.PrestigeRank).Distinct().Where(r => r >= 1).ToList();

            foreach (int rank in uniqueRanks)
            {
                var localPath = Path.Combine(cacheDir, $"rank-{rank}.png");

                if (File.Exists(localPath) && new FileInfo(localPath).Length >= 500)
                {
                    foreach (var p in players.Where(p => p.PrestigeRank == rank))
                    {
                        p.RankImageUrl = localPath;
                    }
                    continue;
                }

                try
                {
                    var url = $"{LauncherConfig.RankImagesBaseUrl.TrimEnd('/')}/rank-{rank}.png";
                    var bytes = await _rankImageHttpClient.GetByteArrayAsync(url);
                    if (bytes.Length >= 500)
                    {
                        await File.WriteAllBytesAsync(localPath, bytes);
                        foreach (var p in players.Where(p => p.PrestigeRank == rank))
                        {
                            p.RankImageUrl = localPath;
                        }
                    }
                }
                catch
                {
                    // 404 or network failure -> keep rank-1.png fallback
                }
            }
        });
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

        [System.Text.Json.Serialization.JsonPropertyName("prestigeRank")]
        public int PrestigeRank { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("prestige_rank")]
        public int PrestigeRankAlt { set => PrestigeRank = value; }

        [System.Text.Json.Serialization.JsonPropertyName("pr")]
        public int PrAlt { set => PrestigeRank = value; }

        [System.Text.Json.Serialization.JsonPropertyName("rank")]
        public int Rank { get; set; }

        public int Wins { get; set; }
        public int Races { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("games")]
        public int Games { get; set; }

        public double Winrate { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("last_seen")]
        public DateTimeOffset? LastSeen { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("lastSeen")]
        public DateTimeOffset? LastSeenAlt { set => LastSeen = value; }

        [System.Text.Json.Serialization.JsonPropertyName("vr_last_24_hours")]
        public int VrLast24Hours { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("vr_gain_24h")]
        public int VrGain24h { set => VrLast24Hours = value; }

        [System.Text.Json.Serialization.JsonPropertyName("vrLast24Hours")]
        public int VrLast24HoursAlt { set => VrLast24Hours = value; }

        [System.Text.Json.Serialization.JsonPropertyName("vr_gain_week")]
        public int VrLastWeek { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("vr_gain_month")]
        public int VrLastMonth { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("is_suspicious")]
        public bool IsSuspicious { get; set; }

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

        public int GetPrestigeRank(int directPrestigeRank, int points = 0)
        {
            if (directPrestigeRank >= 1)
            {
                return directPrestigeRank;
            }

            if (AdditionalFields != null)
            {
                string[] aliases = { "prestigeRank", "pr", "prestige_rank", "prestige", "rank_prestige" };
                foreach (string alias in aliases)
                {
                    var entry = AdditionalFields.FirstOrDefault(pair =>
                        string.Equals(pair.Key, alias, StringComparison.OrdinalIgnoreCase));
                    if (!string.IsNullOrEmpty(entry.Key))
                    {
                        if (entry.Value.ValueKind == JsonValueKind.Number && entry.Value.TryGetInt32(out int val) && val >= 1)
                        {
                            return val;
                        }
                        if (entry.Value.ValueKind == JsonValueKind.String && int.TryParse(entry.Value.GetString(), out int strVal) && strVal >= 1)
                        {
                            return strVal;
                        }
                    }
                }
            }

            return 0;
        }

        private static string? FirstNotEmpty(params string?[] values) =>
            values.FirstOrDefault(value => !string.IsNullOrWhiteSpace(value));
    }

}
