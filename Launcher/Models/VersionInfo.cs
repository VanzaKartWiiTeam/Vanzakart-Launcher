using System.Text.Json;
using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class VersionInfo
{
    [JsonPropertyName("mod_version")]
    public string ModVersion { get; set; } = string.Empty;

    [JsonPropertyName("mod_sha256")]
    public string ModSha256 { get; set; } = string.Empty;

    [JsonPropertyName("changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] Changelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("beta_mod_version")]
    public string BetaModVersion { get; set; } = string.Empty;

    [JsonPropertyName("beta_mod_sha256")]
    public string BetaModSha256 { get; set; } = string.Empty;

    [JsonPropertyName("beta_changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] BetaChangelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("music_pack_version")]
    public string MusicPackVersion { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_sha256")]
    public string MusicPackSha256 { get; set; } = string.Empty;

    [JsonPropertyName("music_pack_changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] MusicPackChangelog { get; set; } = Array.Empty<string>();

    [JsonPropertyName("launcher_version")]
    public string LauncherVersion { get; set; } = string.Empty;

    [JsonPropertyName("launcher_changelog")]
    [JsonConverter(typeof(StringArrayOrSingleConverter))]
    public string[] LauncherChangelog { get; set; } = Array.Empty<string>();
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
