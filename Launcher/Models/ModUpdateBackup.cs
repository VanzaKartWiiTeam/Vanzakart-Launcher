namespace VanzaKartLauncher.Models;

public sealed class ModUpdateBackup
{
    public string BackupId { get; init; } = string.Empty;
    public string BackupFolder { get; init; } = string.Empty;
    public string ModRoot { get; init; } = string.Empty;
    public string UserDataRoot { get; init; } = string.Empty;
    public IReadOnlyList<ModUpdateBackupFile> Files { get; init; } = Array.Empty<ModUpdateBackupFile>();
    public bool HasFiles => Files.Count > 0;
}

public sealed class ModUpdateBackupFile
{
    public string RelativePath { get; init; } = string.Empty;
    public string BackupPath { get; init; } = string.Empty;
    public string Sha256 { get; init; } = string.Empty;
    public long SizeBytes { get; init; }
}
