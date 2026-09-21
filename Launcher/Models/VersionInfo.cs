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

    [JsonPropertyName("mandatory_launcher_update")]
    public bool MandatoryLauncherUpdate { get; set; } = false;

    [JsonPropertyName("new_launcher_url")]
    public string NewLauncherUrl { get; set; } = string.Empty;

    [JsonPropertyName("migration_message")]
    public string MigrationMessage { get; set; } = string.Empty;
}

public sealed class StringArrayOrSingleConverter : JsonConverter<string[]>
{
    public override string[] Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.Null)
        {
            return Array.Empty<string>();
        }

        if (reader.TokenType == JsonTokenType.String)
        {
            var single = reader.GetString();
            return string.IsNullOrWhiteSpace(single) ? Array.Empty<string>() : [single.Trim()];
        }

        if (reader.TokenType == JsonTokenType.StartArray)
        {
            var values = new List<string>();
            while (reader.Read() && reader.TokenType != JsonTokenType.EndArray)
            {
                if (reader.TokenType == JsonTokenType.String)
                {
                    var value = reader.GetString();
                    if (!string.IsNullOrWhiteSpace(value))
                    {
                        values.Add(value.Trim());
                    }
                }
                else if (reader.TokenType == JsonTokenType.Number || reader.TokenType == JsonTokenType.True || reader.TokenType == JsonTokenType.False)
                {
                    using var doc = JsonDocument.ParseValue(ref reader);
                    var raw = doc.RootElement.GetRawText();
                    if (!string.IsNullOrWhiteSpace(raw))
                    {
                        values.Add(raw.Trim());
                    }
                }
                else if (reader.TokenType != JsonTokenType.Null)
                {
                    using var ignored = JsonDocument.ParseValue(ref reader);
                }
            }
            return values.ToArray();
        }

        if (reader.TokenType == JsonTokenType.StartObject)
        {
            var values = new List<string>();
            using var doc = JsonDocument.ParseValue(ref reader);
            foreach (var prop in doc.RootElement.EnumerateObject())
            {
                if (prop.Value.ValueKind == JsonValueKind.String)
                {
                    var val = prop.Value.GetString();
                    if (!string.IsNullOrWhiteSpace(val))
                    {
                        values.Add(val.Trim());
                    }
                }
                else if (prop.Value.ValueKind == JsonValueKind.Array)
                {
                    foreach (var elem in prop.Value.EnumerateArray())
                    {
                        if (elem.ValueKind == JsonValueKind.String)
                        {
                            var val = elem.GetString();
                            if (!string.IsNullOrWhiteSpace(val))
                            {
                                values.Add(val.Trim());
                            }
                        }
                    }
                }
            }
            return values.ToArray();
        }

        try
        {
            using var fallbackDoc = JsonDocument.ParseValue(ref reader);
            var raw = fallbackDoc.RootElement.GetString() ?? fallbackDoc.RootElement.GetRawText();
            return string.IsNullOrWhiteSpace(raw) ? Array.Empty<string>() : [raw.Trim()];
        }
        catch
        {
            return Array.Empty<string>();
        }
    }

    public override void Write(Utf8JsonWriter writer, string[] value, JsonSerializerOptions options)
    {
        writer.WriteStartArray();
        if (value != null)
        {
            foreach (var item in value)
            {
                if (item != null)
                {
                    writer.WriteStringValue(item);
                }
            }
        }
        writer.WriteEndArray();
    }
}

