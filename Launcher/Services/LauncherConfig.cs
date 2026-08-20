using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public static class LauncherConfig
{
    public const string CurrentLauncherVersion = "1.5.1";
    public const string VersionJsonUrl = "https://sitodaking.it:8443/Launcher/versions.json";
    public const string DefaultEndpointsJsonUrl = "https://sitodaking.it:8443/Launcher/endpoints.json";
    public static string EndpointsJsonUrl { get; set; } = DefaultEndpointsJsonUrl;
    public const string DiscordAppId = "1475218891192926288";
    public const string ProductName = "VanzaKart Launcher";

    // Default Fallback URLs
    public const string DefaultModUrl = "https://sitodaking.it:8443/Modpack/VanzaKart.zip";
    public const string DefaultLauncherZipUrl = "https://sitodaking.it:8443/Launcher/vanzakart_launcher.zip";
    public const string DefaultNewsJsonUrl = "https://sitodaking.it:8443/Launcher/news.json";
    public const string DefaultDownloadPageUrl = "https://vwfc.sitodaking.it/";
    public const string DefaultMiiRenderingArchiveUrl = "https://web.archive.org/web/20180502054513id_/http://download-cdn.miitomo.com/native/20180125111639/android/v2/asset_model_character_mii_AFLResHigh_2_3_dat.zip";
    public const string DefaultLeaderboardApiUrl = "https://sitodaking.it:8443/api/vk_leaderboard.php";
    public const string DefaultLeaderboardDetailsApiUrl = "https://sitodaking.it:8443/api/leaderboard/";
    public const string DefaultRoomsApiUrl = "https://sitodaking.it:8443/api/vk_rooms.php";
    public const string DefaultBetaTokenVerifyApiUrl = "https://sitodaking.it:8443/api/vk_beta_token.php";
    public const string DefaultModManifestUrl = "https://sitodaking.it:8443/Modpack/manifest_files.json";
    public const string DefaultModFilesUrl = "https://sitodaking.it:8443/Modpack/files/";
    public const string DefaultModHashFilesUrl = "https://sitodaking.it:8443/Modpack/_by_sha256/";
    public const string DefaultBetaModUrl = "https://sitodaking.it:8443/VanzakartBeta/VKBeta.zip";
    public const string DefaultBetaModManifestUrl = "https://sitodaking.it:8443/VanzakartBeta/manifest_files.json";
    public const string DefaultBetaModFilesUrl = "https://sitodaking.it:8443/VanzakartBeta/files/";
    public const string DefaultBetaModHashFilesUrl = "https://sitodaking.it:8443/VanzakartBeta/_by_sha256/";
    public const string DefaultMusicPackUrl = "https://sitodaking.it:8443/MusicPack/vanzakart_musicpack.zip";
    public const string DefaultMusicPackManifestUrl = "https://sitodaking.it:8443/MusicPack/manifest_files.json";
    public const string DefaultMusicPackFilesUrl = "https://sitodaking.it:8443/MusicPack/files/";
    public const string DefaultServerBaseUrl = "https://sitodaking.it:8443/";
    public const string DefaultRankImagesBaseUrl = "https://sitodaking.it:8443/FOOTAGE/ranks/";

    // Active Runtime URLs and Mirrors (aggiornati all'avvio da endpoints.json)
    public static string ModUrl { get; set; } = DefaultModUrl;
    public static string[] ModMirrors { get; set; } = Array.Empty<string>();
    public static string LauncherZipUrl { get; set; } = DefaultLauncherZipUrl;
    public static string[] LauncherMirrors { get; set; } = Array.Empty<string>();
    public static string NewsJsonUrl { get; set; } = DefaultNewsJsonUrl;
    public static string DownloadPageUrl { get; set; } = DefaultDownloadPageUrl;
    public static string MiiRenderingArchiveUrl { get; set; } = DefaultMiiRenderingArchiveUrl;
    public static string LeaderboardApiUrl { get; set; } = DefaultLeaderboardApiUrl;
    public static string LeaderboardDetailsApiUrl { get; set; } = DefaultLeaderboardDetailsApiUrl;
    public static string RoomsApiUrl { get; set; } = DefaultRoomsApiUrl;
    public static string BetaTokenVerifyApiUrl { get; set; } = DefaultBetaTokenVerifyApiUrl;
    public static string ModManifestUrl { get; set; } = DefaultModManifestUrl;
    public static string ModFilesUrl { get; set; } = DefaultModFilesUrl;
    public static string[] ModFilesMirrors { get; set; } = Array.Empty<string>();
    public static string ModHashFilesUrl { get; set; } = DefaultModHashFilesUrl;
    public static string[] ModHashFilesMirrors { get; set; } = Array.Empty<string>();
    public static string BetaModUrl { get; set; } = DefaultBetaModUrl;
    public static string[] BetaModMirrors { get; set; } = Array.Empty<string>();
    public static string BetaModManifestUrl { get; set; } = DefaultBetaModManifestUrl;
    public static string BetaModFilesUrl { get; set; } = DefaultBetaModFilesUrl;
    public static string[] BetaModFilesMirrors { get; set; } = Array.Empty<string>();
    public static string BetaModHashFilesUrl { get; set; } = DefaultBetaModHashFilesUrl;
    public static string[] BetaModHashFilesMirrors { get; set; } = Array.Empty<string>();
    public static string MusicPackUrl { get; set; } = DefaultMusicPackUrl;
    public static string[] MusicPackMirrors { get; set; } = Array.Empty<string>();
    public static string MusicPackManifestUrl { get; set; } = DefaultMusicPackManifestUrl;
    public static string MusicPackFilesUrl { get; set; } = DefaultMusicPackFilesUrl;
    public static string[] MusicPackFilesMirrors { get; set; } = Array.Empty<string>();
    public static string ServerBaseUrl { get; set; } = DefaultServerBaseUrl;
    public static string RankImagesBaseUrl { get; set; } = DefaultRankImagesBaseUrl;

    public static void ApplyEndpoints(LauncherEndpointsInfo endpoints)
    {
        if (endpoints == null) return;
        if (!string.IsNullOrWhiteSpace(endpoints.EndpointsUrl)) EndpointsJsonUrl = endpoints.EndpointsUrl;
        else if (!string.IsNullOrWhiteSpace(endpoints.EndpointsJsonUrl)) EndpointsJsonUrl = endpoints.EndpointsJsonUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.ModUrl)) ModUrl = endpoints.ModUrl;
        if (endpoints.ModMirrors != null) ModMirrors = endpoints.ModMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.LauncherUrl)) LauncherZipUrl = endpoints.LauncherUrl;
        if (endpoints.LauncherMirrors != null) LauncherMirrors = endpoints.LauncherMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.NewsUrl)) NewsJsonUrl = endpoints.NewsUrl;
        else if (!string.IsNullOrWhiteSpace(endpoints.NewsJsonUrl)) NewsJsonUrl = endpoints.NewsJsonUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.DownloadPageUrl)) DownloadPageUrl = endpoints.DownloadPageUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.MiiRenderingArchiveUrl)) MiiRenderingArchiveUrl = endpoints.MiiRenderingArchiveUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.LeaderboardApiUrl)) LeaderboardApiUrl = endpoints.LeaderboardApiUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.LeaderboardDetailsApiUrl)) LeaderboardDetailsApiUrl = endpoints.LeaderboardDetailsApiUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.RoomsApiUrl)) RoomsApiUrl = endpoints.RoomsApiUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.BetaTokenVerifyApiUrl)) BetaTokenVerifyApiUrl = endpoints.BetaTokenVerifyApiUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.ModManifestUrl)) ModManifestUrl = endpoints.ModManifestUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.ModFilesUrl)) ModFilesUrl = endpoints.ModFilesUrl;
        if (endpoints.ModFilesMirrors != null) ModFilesMirrors = endpoints.ModFilesMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.ModHashFilesUrl)) ModHashFilesUrl = endpoints.ModHashFilesUrl;
        if (endpoints.ModHashFilesMirrors != null) ModHashFilesMirrors = endpoints.ModHashFilesMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.BetaModUrl)) BetaModUrl = endpoints.BetaModUrl;
        if (endpoints.BetaModMirrors != null) BetaModMirrors = endpoints.BetaModMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.BetaModManifestUrl)) BetaModManifestUrl = endpoints.BetaModManifestUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.BetaModFilesUrl)) BetaModFilesUrl = endpoints.BetaModFilesUrl;
        if (endpoints.BetaModFilesMirrors != null) BetaModFilesMirrors = endpoints.BetaModFilesMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.BetaModHashFilesUrl)) BetaModHashFilesUrl = endpoints.BetaModHashFilesUrl;
        if (endpoints.BetaModHashFilesMirrors != null) BetaModHashFilesMirrors = endpoints.BetaModHashFilesMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.MusicPackUrl)) MusicPackUrl = endpoints.MusicPackUrl;
        if (endpoints.MusicPackMirrors != null) MusicPackMirrors = endpoints.MusicPackMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.MusicPackManifestUrl)) MusicPackManifestUrl = endpoints.MusicPackManifestUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.MusicPackFilesUrl)) MusicPackFilesUrl = endpoints.MusicPackFilesUrl;
        if (endpoints.MusicPackFilesMirrors != null) MusicPackFilesMirrors = endpoints.MusicPackFilesMirrors;
        if (!string.IsNullOrWhiteSpace(endpoints.ServerBaseUrl)) ServerBaseUrl = endpoints.ServerBaseUrl;
        if (!string.IsNullOrWhiteSpace(endpoints.RankImagesBaseUrl)) RankImagesBaseUrl = endpoints.RankImagesBaseUrl;
    }
}
