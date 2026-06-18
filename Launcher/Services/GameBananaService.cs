using System.IO;
using System.Net;
using System.Text.Json;
using System.Text.RegularExpressions;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class GameBananaService
{
    public const int MarioKartWiiGameId = 5896;
    private const string ApiRoot = "https://gamebanana.com/apiv11";
    private readonly NetworkService _network;

    public GameBananaService(NetworkService network) => _network = network;

    public async Task<GameBananaSearchResult> SearchAsync(
        string? search,
        string sort = "Generic_Newest",
        int page = 1,
        CancellationToken cancellationToken = default)
    {
        const int perPage = 30;
        var url = string.IsNullOrWhiteSpace(search)
            ? $"{ApiRoot}/Mod/Index?_nPage={page}&_nPerpage={perPage}&_aFilters%5BGeneric_Game%5D={MarioKartWiiGameId}&_sSort={Uri.EscapeDataString(sort)}"
            : $"{ApiRoot}/Util/Search/Results?_sSearchString={Uri.EscapeDataString($"Mario Kart Wii {search.Trim()}")}&_sModelName=Mod&_nPage={page}&_nPerpage={perPage}";
        var json = await _network.DownloadStringAsync(url, cancellationToken);

        using var document = JsonDocument.Parse(json);
        if (!document.RootElement.TryGetProperty("_aRecords", out var records))
            return new GameBananaSearchResult();

        var totalAvailable = 0;
        if (document.RootElement.TryGetProperty("_aMetadata", out var metadata))
            totalAvailable = ReadInt(metadata, "_nRecordCount");

        var ids = records.EnumerateArray()
            .Where(record => ReadBool(record, "_bHasFiles"))
            .Select(record => ReadInt(record, "_idRow"))
            .Where(id => id > 0)
            .Distinct()
            .Take(perPage)
            .ToArray();

        using var gate = new SemaphoreSlim(6);
        var tasks = ids.Select(async id =>
        {
            await gate.WaitAsync(cancellationToken);
            try { return await GetModAsync(id, cancellationToken); }
            catch { return null; }
            finally { gate.Release(); }
        });

        var results = (await Task.WhenAll(tasks)).Where(mod => mod != null).Cast<GameBananaMod>();
        if (!string.IsNullOrWhiteSpace(search))
        {
            results = sort switch
            {
                "Generic_MostLiked" => results.OrderByDescending(mod => mod.Likes),
                "Generic_MostViewed" => results.OrderByDescending(mod => mod.Views),
                "Generic_MostDownloaded" => results.OrderByDescending(mod => mod.Downloads),
                "Generic_Alphabetically" => results.OrderBy(mod => mod.Name, StringComparer.CurrentCultureIgnoreCase),
                _ => results
            };
        }

        return new GameBananaSearchResult
        {
            Mods = results.ToArray(),
            TotalAvailable = totalAvailable,
            HasMore = page * perPage < totalAvailable
        };
    }

    private async Task<GameBananaMod?> GetModAsync(int id, CancellationToken cancellationToken)
    {
        const string properties = "_idRow,_sName,_sProfileUrl,_aSubmitter,_aFiles,_aPreviewMedia,_sText,_nViewCount,_nLikeCount,_aGame";
        var url = $"{ApiRoot}/Mod/{id}?_csvProperties={properties}";
        var json = await _network.DownloadStringAsync(url, cancellationToken);
        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;

        if (!root.TryGetProperty("_aGame", out var game) || ReadInt(game, "_idRow") != MarioKartWiiGameId)
            return null;
        if (!root.TryGetProperty("_aFiles", out var files) || files.ValueKind != JsonValueKind.Array)
            return null;

        var file = files.EnumerateArray()
            .Where(item => !string.IsNullOrWhiteSpace(ReadString(item, "_sDownloadUrl")))
            .OrderByDescending(item => ReadLong(item, "_tsDateAdded"))
            .FirstOrDefault();
        if (file.ValueKind == JsonValueKind.Undefined)
            return null;

        var previewUrl = string.Empty;
        if (root.TryGetProperty("_aPreviewMedia", out var media) &&
            media.TryGetProperty("_aImages", out var images) && images.ValueKind == JsonValueKind.Array)
        {
            var image = images.EnumerateArray().FirstOrDefault();
            var baseUrl = ReadString(image, "_sBaseUrl").TrimEnd('/');
            var imageFile = ReadString(image, "_sFile220");
            if (string.IsNullOrWhiteSpace(imageFile)) imageFile = ReadString(image, "_sFile");
            if (!string.IsNullOrWhiteSpace(baseUrl) && !string.IsNullOrWhiteSpace(imageFile))
                previewUrl = $"{baseUrl}/{imageFile}";
        }

        var submitter = root.TryGetProperty("_aSubmitter", out var author) ? author : default;
        return new GameBananaMod
        {
            Id = id,
            FileId = ReadInt(file, "_idRow"),
            Name = ReadString(root, "_sName"),
            Author = ReadString(submitter, "_sName"),
            Description = StripHtml(ReadString(root, "_sText")),
            ProfileUrl = ReadString(root, "_sProfileUrl"),
            PreviewUrl = previewUrl,
            DownloadUrl = ReadString(file, "_sDownloadUrl"),
            FileName = ReadString(file, "_sFile"),
            FileSize = ReadLong(file, "_nFilesize"),
            Views = ReadInt(root, "_nViewCount"),
            Likes = ReadInt(root, "_nLikeCount"),
            Downloads = ReadInt(file, "_nDownloadCount")
        };
    }

    private static string StripHtml(string value)
    {
        var text = Regex.Replace(value, "<[^>]+>", " ");
        text = WebUtility.HtmlDecode(text);
        return Regex.Replace(text, "\\s+", " ").Trim();
    }

    private static string ReadString(JsonElement element, string name) =>
        element.ValueKind == JsonValueKind.Object && element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString() ?? string.Empty : string.Empty;
    private static int ReadInt(JsonElement element, string name) => (int)Math.Clamp(ReadLong(element, name), int.MinValue, int.MaxValue);
    private static long ReadLong(JsonElement element, string name) =>
        element.ValueKind == JsonValueKind.Object && element.TryGetProperty(name, out var value) && value.TryGetInt64(out var result) ? result : 0;
    private static bool ReadBool(JsonElement element, string name) =>
        element.ValueKind == JsonValueKind.Object && element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.True;
}
