using System.Text.Json;
using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class VersionInfo
{
    [JsonPropertyName("mod_version")]
    public string ModVersion { get; set; } = string.Empty;

    [JsonPropertyName("launcher_version")]
    public string LauncherVersion { get; set; } = string.Empty;

    [JsonPropertyName("mod_url")]
    public string ModUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] ModMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_sha256")]
    public string ModSha256 { get; set; } = string.Empty;

    [JsonPropertyName("launcher_url")]
    public string LauncherUrl { get; set; } = string.Empty;

    [JsonPropertyName("launcher_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] LauncherMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] Changelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("mod_manifest_url")]
    public string ModManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_url")]
    public string ModFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("mod_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] ModFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_version")]
    public string BetaModVersion { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_url")]
    public string BetaModUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaModMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_sha256")]
    public string BetaModSha256 { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_manifest_url")]
    public string BetaModManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_files_url")]
    public string BetaModFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaModFilesMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaChangelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_version")]
    public string MusicPackVersion { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_url")]
    public string MusicPackUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackMirrors { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_sha256")]
    public string MusicPackSha256 { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackChangelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_manifest_url")]
    public string MusicPackManifestUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_files_url")]
    public string MusicPackFilesUrl { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_files_mirrors")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackFilesMirrors { get; set; } = Array.Empty<string>();
}

public sealed class StringArrayOrSingleConverter : JsonConverter<string[]>
{
    public override string[] Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.Null) return Array.Empty<string>();
        if (reader.TokenType == JsonTokenType.String)
        {
            var single = reader.GetString();
            return string.IsNullOrWhiteSpace(single) ? Array.Empty<string>() : [single];
        }
        if (reader.TokenType != JsonTokenType.StartArray)
            throw new JsonException("Expected a string or an array of strings.");

        var values = new List<string>();
        while (reader.Read() && reader.TokenType != JsonTokenType.EndArray)
        {
            if (reader.TokenType == JsonTokenType.String)
            {
                var value = reader.GetString();
                if (!string.IsNullOrWhiteSpace(value)) values.Add(value);
            }
            else if (reader.TokenType != JsonTokenType.Null)
            {
                using var ignored = JsonDocument.ParseValue(ref reader);
            }
        }
        return values.ToArray();
    }

    public override void Write(Utf8JsonWriter writer, string[] value, JsonSerializerOptions options)
    {
        writer.WriteStartArray();
        foreach (var item in value ?? Array.Empty<string>()) writer.WriteStringValue(item);
        writer.WriteEndArray();
    }
}
