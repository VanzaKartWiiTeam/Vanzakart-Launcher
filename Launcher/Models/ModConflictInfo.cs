namespace VanzaKartLauncher.Models;

public sealed class ModConflictInfo
{
    public string FileName { get; init; } = string.Empty;
    public int Count { get; init; }
    public string Locations { get; init; } = string.Empty;
}
