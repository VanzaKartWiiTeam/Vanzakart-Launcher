using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class LauncherEndpointsInfo
{
    [JsonPropertyName("endpoints_url")]
    public string EndpointsUrl { get; set; } = string.Empty;

    [JsonPropertyName("endpoints_json_url")]
    public string EndpointsJsonUrl { get; set; } = string.Empty;

    [JsonPropertyName("versions_json_url")]
    public string VersionsJsonUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_url")]
    public string ModUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] ModMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_manifest_url")]
    public string ModManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_url")]
    public string ModFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] ModFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_hash_files_url")]
    public string ModHashFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_hash_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] ModHashFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_url")]
    public string BetaModUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaModMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_manifest_url")]
    public string BetaModManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_files_url")]
    public string BetaModFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaModFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_hash_files_url")]
    public string BetaModHashFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_hash_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaModHashFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_url")]
    public string MusicPackUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_manifest_url")]
    public string MusicPackManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_files_url")]
    public string MusicPackFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("launcher_url")]
    public string LauncherUrl { get; set; } = string.Empty;

    [JsonPropertyName("launcher_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] LauncherMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("news_url")]
    public string NewsUrl { get; set; } = string.Empty;

    [JsonPropertyName("news_json_url")]
    public string NewsJsonUrl { get; set; } = string.Empty;

    [JsonPropertyName("leaderboard_api_url")]
    public string LeaderboardApiUrl { get; set; } = string.Empty;

    [JsonPropertyName("leaderboard_details_api_url")]
    public string LeaderboardDetailsApiUrl { get; set; } = string.Empty;

    [JsonPropertyName("rooms_api_url")]
    public string RoomsApiUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_token_verify_api_url")]
    public string BetaTokenVerifyApiUrl { get; set; } = string.Empty;

    [JsonPropertyName("download_page_url")]
    public string DownloadPageUrl { get; set; } = string.Empty;

    [JsonPropertyName("mii_rendering_archive_url")]
    public string MiiRenderingArchiveUrl { get; set; } = string.Empty;

    [JsonPropertyName("server_base_url")]
    public string ServerBaseUrl { get; set; } = string.Empty;

    [JsonPropertyName("rank_images_base_url")]
    public string RankImagesBaseUrl { get; set; } = string.Empty;
}
