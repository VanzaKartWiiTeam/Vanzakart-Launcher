namespace VanzaKartLauncher.Models;

public sealed class SaveBackupInfo
{
    public string DisplayName { get; init; } = string.Empty;
    public string FilePath { get; init; } = string.Empty;
    public DateTime CreatedUtc { get; init; }
    public long SizeBytes { get; init; }
}
