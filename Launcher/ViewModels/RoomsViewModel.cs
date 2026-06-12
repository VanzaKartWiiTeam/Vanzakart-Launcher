using System.Collections.ObjectModel;
using System.Net.Http;
using System.Text.Json;
using System.Windows.Threading;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher.ViewModels;

public sealed class RoomsViewModel : BaseViewModel
{
    private readonly NetworkService _networkService;
    private readonly DispatcherTimer _refreshTimer;
    private bool _isLoading;
    private bool _hasError;
    private string _errorMessage = string.Empty;
    private bool _isEmpty;
    private int _activeRoomsCount;
    private int _onlinePlayersCount;
    private string _serverStatus = "Offline";
    private string _lastUpdatedText = "Never updated";

    public RoomsViewModel(NetworkService networkService)
    {
        _networkService = networkService;
        Rooms = new ObservableCollection<RoomInfo>();
        
        // Auto-refresh timer: runs every 15 seconds
        _refreshTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(15)
        };
        _refreshTimer.Tick += async (s, e) => await RefreshAsync(false);
    }

    public ObservableCollection<RoomInfo> Rooms { get; }

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

    public bool IsEmpty
    {
        get => _isEmpty;
        private set => SetProperty(ref _isEmpty, value);
    }

    public int ActiveRoomsCount
    {
        get => _activeRoomsCount;
        private set => SetProperty(ref _activeRoomsCount, value);
    }

    public int OnlinePlayersCount
    {
        get => _onlinePlayersCount;
        private set => SetProperty(ref _onlinePlayersCount, value);
    }

    public string ServerStatus
    {
        get => _serverStatus;
        private set => SetProperty(ref _serverStatus, value);
    }

    public string LastUpdatedText
    {
        get => _lastUpdatedText;
        private set => SetProperty(ref _lastUpdatedText, value);
    }

    public void StartAutoRefresh()
    {
        _refreshTimer.Start();
    }

    public void StopAutoRefresh()
    {
        _refreshTimer.Stop();
    }

    public async Task RefreshAsync(bool showLoadingState = true)
    {
        if (showLoadingState)
        {
            IsLoading = true;
            HasError = false;
            IsEmpty = false;
        }

        try
        {
            // Note: In a production scenario, we query the endpoint defined in LauncherConfig
            string url = LauncherConfig.RoomsApiUrl;
            string json = await _networkService.DownloadStringAsync(url);
            
            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };
            
            var response = JsonSerializer.Deserialize<RoomsApiResponse>(json, options);
            
            if (response != null && response.Success)
            {
                Rooms.Clear();
                if (response.Rooms != null)
                {
                    foreach (var room in response.Rooms)
                    {
                        Rooms.Add(room);
                    }
                }

                IsEmpty = Rooms.Count == 0;
                
                if (response.Meta != null)
                {
                    ActiveRoomsCount = Math.Max(response.Meta.TotalRooms, Rooms.Count);
                    OnlinePlayersCount = Math.Max(response.Meta.TotalPlayers, System.Linq.Enumerable.Sum(Rooms, r => r.PlayerCount));
                    ServerStatus = response.Meta.Status ?? "Online";
                    
                    if (DateTime.TryParse(response.Meta.LastUpdated, out var parsedDate))
                    {
                        LastUpdatedText = parsedDate.ToLocalTime().ToString("HH:mm:ss");
                    }
                    else
                    {
                        LastUpdatedText = response.Meta.LastUpdated ?? DateTime.Now.ToString("HH:mm:ss");
                    }
                }
                
                HasError = false;
            }
            else
            {
                throw new Exception("L'API ha restituito una risposta non valida.");
            }
        }
        catch (Exception ex)
        {
            // Se non stiamo mostrando lo skeleton loading (auto-refresh), non sovrascriviamo le stanze esistenti
            if (showLoadingState)
            {
                HasError = true;
                ErrorMessage = $"Impossibile caricare le stanze: {ex.Message}";
                Rooms.Clear();
            }
        }
        finally
        {
            if (showLoadingState)
            {
                IsLoading = false;
            }
        }
    }

    private sealed class RoomsApiResponse
    {
        public bool Success { get; set; }
        public ApiResponseMeta? Meta { get; set; }
        public List<RoomInfo>? Rooms { get; set; }
    }

    private sealed class ApiResponseMeta
    {
        public int TotalPlayers { get; set; }
        public int TotalRooms { get; set; }
        public int PublicRooms { get; set; }
        public int PrivateRooms { get; set; }
        public string? LastUpdated { get; set; }
        public string? Status { get; set; }
    }
}
