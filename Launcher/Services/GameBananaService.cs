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
    private Task<IReadOnlyList<CatalogEntry>>? _catalogTask;

    public GameBananaService(NetworkService network) => _network = network;

    public async Task<GameBananaSearchResult> SearchAsync(
        string? search,
        string sort = "Generic_Newest",
        int page = 1,
        CancellationToken cancellationToken = default)
    {
        const int perPage = 30;
        int[] ids;
        var totalAvailable = 0;
        var hasMore = false;
        if (string.IsNullOrWhiteSpace(search))
        {
            var url = $"{ApiRoot}/Mod/Index?_nPage={page}&_nPerpage={perPage}&_aFilters%5BGeneric_Game%5D={MarioKartWiiGameId}&_sSort={Uri.EscapeDataString(sort)}";
            var pageResult = await ReadSearchPageAsync(url, page, cancellationToken);
            ids = pageResult.Ids.Take(perPage).ToArray();
            totalAvailable = pageResult.Total;
            hasMore = pageResult.HasMore;
        }
        else
        {
            var searchResult = await FindCatalogCandidatesAsync(search.Trim(), page, cancellationToken);
            ids = searchResult.Ids.Take(60).ToArray();
            totalAvailable = searchResult.Total;
            hasMore = searchResult.HasMore;
        }

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
            HasMore = hasMore
        };
    }

    private async Task<SearchPage> FindCatalogCandidatesAsync(string search, int page, CancellationToken cancellationToken)
    {
        var catalog = await GetCatalogAsync(cancellationToken);
        var normalizedSearch = NormalizeForSearch(search);
        var ranked = catalog.Select(entry => new { Entry = entry, Score = GetFuzzyScore(normalizedSearch, NormalizeForSearch(entry.Name)) })
            .Where(item => item.Score < int.MaxValue)
            .OrderBy(item => item.Score)
            .ThenBy(item => item.Entry.Name, StringComparer.CurrentCultureIgnoreCase)
            .ToArray();
        const int perPage = 30;
        return new SearchPage(
            ranked.Skip((page - 1) * perPage).Take(perPage).Select(item => item.Entry.Id).ToArray(),
            ranked.Length,
            page * perPage < ranked.Length);
    }

    private async Task<IReadOnlyList<CatalogEntry>> GetCatalogAsync(CancellationToken cancellationToken)
    {
        _catalogTask ??= LoadCatalogAsync(CancellationToken.None);
        try { return await _catalogTask.WaitAsync(cancellationToken); }
        catch
        {
            if (_catalogTask.IsFaulted || _catalogTask.IsCanceled) _catalogTask = null;
            throw;
        }
    }

    private async Task<IReadOnlyList<CatalogEntry>> LoadCatalogAsync(CancellationToken cancellationToken)
    {
        const int perPage = 50;
        var first = await ReadCatalogPageAsync(1, perPage, cancellationToken);
        var pageCount = Math.Max(1, (int)Math.Ceiling(first.Total / (double)perPage));
        using var gate = new SemaphoreSlim(5);
        var tasks = Enumerable.Range(2, Math.Max(0, pageCount - 1)).Select(async page =>
        {
            await gate.WaitAsync(cancellationToken);
            try { return await ReadCatalogPageAsync(page, perPage, cancellationToken); }
            finally { gate.Release(); }
        });
        var remaining = await Task.WhenAll(tasks);
        return first.Entries.Concat(remaining.SelectMany(result => result.Entries))
            .Where(entry => entry.Id > 0 && !string.IsNullOrWhiteSpace(entry.Name))
            .DistinctBy(entry => entry.Id)
            .ToArray();
    }

    private async Task<CatalogPage> ReadCatalogPageAsync(int page, int perPage, CancellationToken cancellationToken)
    {
        var url = $"{ApiRoot}/Mod/Index?_nPage={page}&_nPerpage={perPage}&_aFilters%5BGeneric_Game%5D={MarioKartWiiGameId}&_sSort=Generic_Alphabetically";
        var json = await _network.DownloadStringAsync(url, cancellationToken);
        using var document = JsonDocument.Parse(json);
        var total = document.RootElement.TryGetProperty("_aMetadata", out var metadata) ? ReadInt(metadata, "_nRecordCount") : 0;
        var entries = document.RootElement.TryGetProperty("_aRecords", out var records)
            ? records.EnumerateArray()
                .Where(record => ReadBool(record, "_bHasFiles"))
                .Select(record => new CatalogEntry(ReadInt(record, "_idRow"), ReadString(record, "_sName")))
                .ToArray()
            : Array.Empty<CatalogEntry>();
        return new CatalogPage(entries, total);
    }

    private static string NormalizeForSearch(string value)
    {
        var decomposed = value.ToLowerInvariant().Normalize(System.Text.NormalizationForm.FormD);
        var chars = decomposed.Where(ch =>
            System.Globalization.CharUnicodeInfo.GetUnicodeCategory(ch) != System.Globalization.UnicodeCategory.NonSpacingMark)
            .Select(ch => char.IsLetterOrDigit(ch) ? ch : ' ')
            .ToArray();
        return Regex.Replace(new string(chars), "\\s+", " ").Trim();
    }

    private static int GetFuzzyScore(string query, string name)
    {
        if (query.Length == 0 || name.Length == 0) return int.MaxValue;
        if (name == query) return 0;
        if (name.StartsWith(query, StringComparison.Ordinal)) return 2;
        var containsAt = name.IndexOf(query, StringComparison.Ordinal);
        if (containsAt >= 0) return 5 + containsAt;

        var queryTokens = query.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var nameTokens = name.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var totalDistance = 0;
        foreach (var queryToken in queryTokens)
        {
            if (nameTokens.Any(token => token.StartsWith(queryToken, StringComparison.Ordinal))) continue;
            var best = nameTokens.Min(token => LevenshteinDistance(queryToken, token));
            var tolerance = Math.Max(1, (int)Math.Ceiling(queryToken.Length * 0.4));
            if (best > tolerance) return int.MaxValue;
            totalDistance += best;
        }
        return 20 + totalDistance;
    }

    private static int LevenshteinDistance(string left, string right)
    {
        var previous = Enumerable.Range(0, right.Length + 1).ToArray();
        for (var i = 1; i <= left.Length; i++)
        {
            var current = new int[right.Length + 1];
            current[0] = i;
            for (var j = 1; j <= right.Length; j++)
                current[j] = Math.Min(Math.Min(current[j - 1] + 1, previous[j] + 1), previous[j - 1] + (left[i - 1] == right[j - 1] ? 0 : 1));
            previous = current;
        }
        return previous[right.Length];
    }

    private async Task<SearchPage> ReadSearchPageAsync(string url, int page, CancellationToken cancellationToken)
    {
        var json = await _network.DownloadStringAsync(url, cancellationToken);
        using var document = JsonDocument.Parse(json);
        if (!document.RootElement.TryGetProperty("_aRecords", out var records)) return new SearchPage(Array.Empty<int>(), 0, false);
        var ids = records.EnumerateArray()
            .Where(record => ReadBool(record, "_bHasFiles"))
            .Select(record => ReadInt(record, "_idRow"))
            .Where(id => id > 0)
            .ToArray();
        var total = 0;
        var perPage = Math.Max(ids.Length, 15);
        var complete = false;
        if (document.RootElement.TryGetProperty("_aMetadata", out var metadata))
        {
            total = ReadInt(metadata, "_nRecordCount");
            perPage = Math.Max(1, ReadInt(metadata, "_nPerpage"));
            complete = ReadBool(metadata, "_bIsComplete");
        }
        return new SearchPage(ids, total, !complete && page * perPage < total);
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

        var downloadFiles = files.EnumerateArray()
            .Where(item => !string.IsNullOrWhiteSpace(ReadString(item, "_sDownloadUrl")))
            .Select(item => new GameBananaFile
            {
                FileId = ReadInt(item, "_idRow"),
                FileName = ReadString(item, "_sFile"),
                Description = StripHtml(ReadString(item, "_sDescription")),
                DownloadUrl = ReadString(item, "_sDownloadUrl"),
                FileSize = ReadLong(item, "_nFilesize"),
                DownloadCount = ReadInt(item, "_nDownloadCount"),
                DateAddedUtc = DateTimeOffset.FromUnixTimeSeconds(ReadLong(item, "_tsDateAdded")).UtcDateTime
            })
            .ToList();
        if (downloadFiles.Count == 0)
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
            Name = ReadString(root, "_sName"),
            Author = ReadString(submitter, "_sName"),
            Description = StripHtml(ReadString(root, "_sText")),
            ProfileUrl = ReadString(root, "_sProfileUrl"),
            PreviewUrl = previewUrl,
            Files = downloadFiles,
            Views = ReadInt(root, "_nViewCount"),
            Likes = ReadInt(root, "_nLikeCount"),
            Downloads = downloadFiles.Sum(file => file.DownloadCount)
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

    private sealed record SearchPage(int[] Ids, int Total, bool HasMore);
    private sealed record CatalogEntry(int Id, string Name);
    private sealed record CatalogPage(CatalogEntry[] Entries, int Total);
}
