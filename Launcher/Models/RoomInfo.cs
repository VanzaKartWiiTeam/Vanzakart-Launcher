using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Models;

public sealed class RoomInfo
{
    public string Id { get; init; } = string.Empty;
    public string Name { get; init; } = string.Empty;
    public string Host { get; init; } = string.Empty;

    [JsonPropertyName("player_count")]
    public int PlayerCount { get; init; }

    [JsonPropertyName("max_players")]
    public int MaxPlayers { get; init; } = 12;
    public string Mode { get; init; } = "Versus";
    public string Track { get; init; } = "Choosing Track...";
    public string Region { get; init; } = "Worldwide";
    public string Status { get; init; } = "In Lobby";

    public string DisplayPlayerCount => $"{PlayerCount}/{MaxPlayers}";
    public bool IsRacing => string.Equals(Status, "Racing", StringComparison.OrdinalIgnoreCase);
}
