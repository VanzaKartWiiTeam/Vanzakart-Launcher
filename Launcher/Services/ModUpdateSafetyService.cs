using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

/// <summary>
/// Protects user-data (patenti, Mii, profili, save) durante un aggiornamento del modpack.
/// 
/// Flusso di aggiornamento sicuro:
///   1. CreateBackupAsync      – salva tutti i file protetti in Backups/ e in VanzaKart_UserData/
///   2. ApplyZipUpdateAsync    – estrae lo ZIP sovrascrivendo solo i file del modpack;
///                               i file protetti vengono saltati automaticamente.
///                               I file modpack non più presenti nello ZIP vengono rimossi
///                               (pruning) per evitare file obsoleti, ma quelli protetti
///                               non vengono mai toccati.
///   3. RestoreBackupAsync     – (usato solo in caso di rollback) ripristina il backup.
/// </summary>
public sealed class ModUpdateSafetyService
{
    // ── Nomi di cartella protetti ────────────────────────────────────────────
    private static readonly string[] ProtectedDirectoryNames =
    [
        "My Stuff",
        "UserData",
        "userdata",
        "Saves",
        "Save",
        "Licenses",
        "License",
        "Patenti",
        "Profiles",
        "Miis",
        "Mii"
    ];

    // ── Nomi di file protetti (esatti) ───────────────────────────────────────
    private static readonly string[] ProtectedFileNames =
    [
        "rksys.dat",
        "RFL_DB.dat",
        "active_mii.txt",
        "mii_profile.json"
    ];

    // ── Estensioni protette ──────────────────────────────────────────────────
    private static readonly string[] ProtectedExtensions =
    [
        ".mii",
        ".miigx",
        ".mae",
        ".vk-mii"
    ];

    // ════════════════════════════════════════════════════════════════════════
    // Percorsi pubblici
    // ════════════════════════════════════════════════════════════════════════

    public string GetBackupRoot()
        => Path.Combine(AppContext.BaseDirectory, "Backups", "ModUpdates");

    public string GetOperationLogPath()
        => Path.Combine(AppContext.BaseDirectory, "Logs", "mod-update.log");

    public string GetModRoot(LauncherSettings settings)
        => Path.Combine(settings.GetModFolder(), "VanzaKart");

    public string GetUserDataRoot(LauncherSettings settings)
        => Path.Combine(settings.GetModFolder(), "VanzaKart_UserData");

    // ════════════════════════════════════════════════════════════════════════
    // Protezione path
    // ════════════════════════════════════════════════════════════════════════

    public bool IsProtectedUserDataPath(string path, LauncherSettings settings)
    {
        var modRoot = GetModRoot(settings);
        if (string.IsNullOrWhiteSpace(path) || string.IsNullOrWhiteSpace(modRoot))
            return false;

        try
        {
            var relative = Path.GetRelativePath(modRoot, path);
            return IsProtectedRelativePath(relative);
        }
        catch
        {
            return false;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 1. BACKUP
    // ════════════════════════════════════════════════════════════════════════

    public async Task<ModUpdateBackup> CreateBackupAsync(
        LauncherSettings settings,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot     = GetModRoot(settings);
        var userDataRoot= GetUserDataRoot(settings);
        var backupId    = DateTime.Now.ToString("yyyyMMdd_HHmmss");
        var backupFolder= Path.Combine(GetBackupRoot(), backupId);
        var files       = new List<ModUpdateBackupFile>();

        Directory.CreateDirectory(backupFolder);
        Directory.CreateDirectory(userDataRoot);

        if (!Directory.Exists(modRoot))
        {
            await WriteLogAsync($"backup {backupId}: mod root non trovato, niente da preservare", cancellationToken);
            return new ModUpdateBackup
            {
                BackupId     = backupId,
                BackupFolder = backupFolder,
                ModRoot      = modRoot,
                UserDataRoot = userDataRoot,
                Files        = files
            };
        }

        foreach (var file in EnumerateUserDataFiles(modRoot))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative   = Path.GetRelativePath(modRoot, file);
            progress?.Report($"Preservo {relative}");

            var backupPath = Path.Combine(backupFolder, "files", relative);
            await CopyFileAsync(file, backupPath, cancellationToken);

            var mirrorPath = Path.Combine(userDataRoot, relative);
            await CopyFileAsync(file, mirrorPath, cancellationToken);

            files.Add(new ModUpdateBackupFile
            {
                RelativePath = relative,
                BackupPath   = backupPath,
                Sha256       = await ComputeSha256Async(file, cancellationToken),
                SizeBytes    = new FileInfo(file).Length
            });
        }

        var result = new ModUpdateBackup
        {
            BackupId     = backupId,
            BackupFolder = backupFolder,
            ModRoot      = modRoot,
            UserDataRoot = userDataRoot,
            Files        = files
        };

        var manifestPath = Path.Combine(backupFolder, "manifest.json");
        await File.WriteAllTextAsync(
            manifestPath,
            JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }),
            cancellationToken);

        await WriteLogAsync($"backup {backupId}: preservati {files.Count} file utente", cancellationToken);
        return result;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 2. APPLICAZIONE ZIP – estrazione selettiva (cuore del miglioramento)
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// Estrae lo ZIP del modpack nella cartella di destinazione in modo sicuro:
    /// • i file PROTETTI (patenti, Mii, save…) presenti nella destinazione non
    ///   vengono mai sovrascritti né eliminati;
    /// • i file del modpack vengono aggiornati/aggiunti normalmente;
    /// • i file presenti nella destinazione ma assenti nello ZIP (file obsoleti
    ///   del modpack) vengono eliminati (pruning), TRANNE se sono protetti.
    /// </summary>
    /// <param name="zipPath">Percorso dello ZIP già scaricato e verificato.</param>
    /// <param name="destinationRoot">Cartella radice in cui estrarre (es. modFolder).</param>
    /// <param name="modSubFolder">
    ///   Sotto-cartella del modpack all'interno di destinationRoot
    ///   (es. …/VanzaKart). Usata per il pruning.
    /// </param>
    /// <param name="settings">Impostazioni correnti del launcher.</param>
    /// <param name="progress">Progresso 0-100.</param>
    /// <param name="cancellationToken">Token di cancellazione.</param>
    /// <returns>Riepilogo dell'operazione.</returns>
    public async Task<ModUpdateResult> ApplyZipUpdateAsync(
        string zipPath,
        string destinationRoot,
        string modSubFolder,
        LauncherSettings settings,
        IProgress<int>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot = GetModRoot(settings);

        using var archive = ZipFile.OpenRead(zipPath);
        var entries       = archive.Entries.Where(e => !string.IsNullOrEmpty(e.Name)).ToList();
        var totalEntries  = Math.Max(1, entries.Count);

        // --- insieme dei path relativi presenti nello ZIP (per il pruning) ---
        var zipRelativePaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        var written  = 0;
        var skipped  = 0;
        var pruned   = 0;
        var errors   = new List<string>();
        var done     = 0;

        // ── 2a. Estrazione selettiva ─────────────────────────────────────────
        foreach (var entry in entries)
        {
            cancellationToken.ThrowIfCancellationRequested();

            // Normalizza il percorso relativo (zip usa '/')
            var entryRelative = entry.FullName.Replace('/', Path.DirectorySeparatorChar);
            var destPath      = Path.GetFullPath(Path.Combine(destinationRoot, entryRelative));

            // Sicurezza: impedisce zip-slip
            if (!destPath.StartsWith(Path.GetFullPath(destinationRoot) + Path.DirectorySeparatorChar,
                    StringComparison.OrdinalIgnoreCase))
            {
                errors.Add($"Voce ZIP sospetta ignorata: {entry.FullName}");
                done++;
                continue;
            }

            // Registra per il pruning (path relativo rispetto a modSubFolder)
            var relativeToModSub = Path.GetRelativePath(modSubFolder, destPath);
            zipRelativePaths.Add(relativeToModSub);

            // ── Controlla se il file è protetto ───────────────────────────────
            var relativeToModRoot = Path.GetRelativePath(modRoot, destPath);
            if (IsProtectedRelativePath(relativeToModRoot))
            {
                skipped++;
                done++;
                progress?.Report(done * 100 / totalEntries);
                await WriteLogAsync($"skip (protetto): {relativeToModRoot}", cancellationToken);
                continue;
            }

            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
                entry.ExtractToFile(destPath, overwrite: true);
                written++;
            }
            catch (Exception ex)
            {
                errors.Add($"{entryRelative}: {ex.Message}");
            }

            done++;
            progress?.Report(done * 100 / totalEntries);
        }

        // ── 2b. Pruning: rimuove file del modpack non più presenti nello ZIP ─
        if (Directory.Exists(modSubFolder))
        {
            var existingFiles = Directory.EnumerateFiles(modSubFolder, "*", SearchOption.AllDirectories);
            foreach (var existingFile in existingFiles)
            {
                cancellationToken.ThrowIfCancellationRequested();

                var relToModSub  = Path.GetRelativePath(modSubFolder, existingFile);
                var relToModRoot = Path.GetRelativePath(modRoot, existingFile);

                // Non toccare mai i file protetti
                if (IsProtectedRelativePath(relToModRoot))
                    continue;

                // Se il file NON è nello ZIP, è obsoleto → rimuovi
                if (!zipRelativePaths.Contains(relToModSub))
                {
                    try
                    {
                        File.Delete(existingFile);
                        pruned++;
                        await WriteLogAsync($"pruned (obsoleto): {relToModSub}", cancellationToken);
                    }
                    catch (Exception ex)
                    {
                        errors.Add($"pruning {relToModSub}: {ex.Message}");
                    }
                }
            }

            // Rimuovi cartelle vuote lasciate dal pruning (non protette)
            RemoveEmptyDirectories(modSubFolder, modRoot);
        }

        var result = new ModUpdateResult
        {
            FilesWritten = written,
            FilesSkipped = skipped,
            FilesPruned  = pruned,
            Errors       = errors
        };

        await WriteLogAsync(
            $"update applicato: {written} scritti, {skipped} saltati (protetti), {pruned} rimossi (obsoleti)" +
            (errors.Count > 0 ? $", {errors.Count} errori" : string.Empty),
            cancellationToken);

        return result;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 3. RESTORE (rollback in caso di errore)
    // ════════════════════════════════════════════════════════════════════════

    public async Task RestoreBackupAsync(
        ModUpdateBackup backup,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        if (backup.Files.Count == 0)
        {
            await WriteLogAsync($"restore {backup.BackupId}: nessun file da ripristinare", cancellationToken);
            return;
        }

        Directory.CreateDirectory(backup.ModRoot);
        foreach (var file in backup.Files)
        {
            cancellationToken.ThrowIfCancellationRequested();
            progress?.Report($"Ripristino {file.RelativePath}");
            var destination = Path.Combine(backup.ModRoot, file.RelativePath);
            await CopyFileAsync(file.BackupPath, destination, cancellationToken);
        }

        await VerifyBackupRestoreAsync(backup, cancellationToken);
        await WriteLogAsync($"restore {backup.BackupId}: ripristinati {backup.Files.Count} file utente", cancellationToken);
    }

    // ════════════════════════════════════════════════════════════════════════
    // Migrazione user-data (utility per primo avvio post-update)
    // ════════════════════════════════════════════════════════════════════════

    public async Task MigrateUserDataAsync(
        LauncherSettings settings,
        IProgress<string>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var modRoot      = GetModRoot(settings);
        var userDataRoot = GetUserDataRoot(settings);
        if (!Directory.Exists(modRoot))
            return;

        Directory.CreateDirectory(userDataRoot);
        var migrated = 0;
        foreach (var file in EnumerateUserDataFiles(modRoot))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relative    = Path.GetRelativePath(modRoot, file);
            var destination = Path.Combine(userDataRoot, relative);
            progress?.Report($"Migro {relative}");
            await CopyFileAsync(file, destination, cancellationToken);
            migrated++;
        }

        await WriteLogAsync($"migrazione: copiati {migrated} file utente in {userDataRoot}", cancellationToken);
    }

    // ════════════════════════════════════════════════════════════════════════
    // Helper privati
    // ════════════════════════════════════════════════════════════════════════

    private IEnumerable<string> EnumerateUserDataFiles(string modRoot)
    {
        if (!Directory.Exists(modRoot))
            yield break;

        IEnumerable<string> files;
        try
        {
            files = Directory.EnumerateFiles(modRoot, "*", SearchOption.AllDirectories).ToArray();
        }
        catch
        {
            yield break;
        }

        foreach (var file in files)
        {
            var relative = Path.GetRelativePath(modRoot, file);
            if (IsProtectedRelativePath(relative))
                yield return file;
        }
    }

    private static bool IsProtectedRelativePath(string relative)
    {
        if (string.IsNullOrWhiteSpace(relative) || relative.StartsWith("..", StringComparison.Ordinal))
            return false;

        var segments = relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);

        if (segments.Any(s => ProtectedDirectoryNames.Contains(s, StringComparer.OrdinalIgnoreCase)))
            return true;

        var fileName  = Path.GetFileName(relative);
        if (ProtectedFileNames.Contains(fileName, StringComparer.OrdinalIgnoreCase))
            return true;

        var extension = Path.GetExtension(relative);
        if (ProtectedExtensions.Contains(extension, StringComparer.OrdinalIgnoreCase))
            return true;

        return relative.Contains("save",    StringComparison.OrdinalIgnoreCase)
            || relative.Contains("license", StringComparison.OrdinalIgnoreCase)
            || relative.Contains("patent",  StringComparison.OrdinalIgnoreCase)
            || relative.Contains("mii",     StringComparison.OrdinalIgnoreCase)
            || relative.Contains("profile", StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>Rimuove ricorsivamente le sotto-cartelle vuote di <paramref name="root"/>,
    /// senza mai toccare le cartelle protette.</summary>
    private void RemoveEmptyDirectories(string root, string modRoot)
    {
        foreach (var dir in Directory.EnumerateDirectories(root, "*", SearchOption.AllDirectories)
                                     .OrderByDescending(d => d.Length)) // prima le più profonde
        {
            var relToModRoot = Path.GetRelativePath(modRoot, dir);
            if (IsProtectedRelativePath(relToModRoot))
                continue;

            try
            {
                if (!Directory.EnumerateFileSystemEntries(dir).Any())
                    Directory.Delete(dir);
            }
            catch
            {
                // ignora errori di cancellazione cartella
            }
        }
    }

    private static async Task VerifyBackupRestoreAsync(ModUpdateBackup backup, CancellationToken cancellationToken)
    {
        foreach (var file in backup.Files)
        {
            var destination = Path.Combine(backup.ModRoot, file.RelativePath);
            if (!File.Exists(destination))
                throw new IOException($"Ripristino fallito: {file.RelativePath} mancante.");

            var hash = await ComputeSha256Async(destination, cancellationToken);
            if (!hash.Equals(file.Sha256, StringComparison.OrdinalIgnoreCase))
                throw new IOException($"Ripristino fallito: hash non corrispondente per {file.RelativePath}.");
        }
    }

    private static async Task CopyFileAsync(string source, string destination, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        await using var input  = new FileStream(source,      FileMode.Open,   FileAccess.Read,  FileShare.ReadWrite, 81920, true);
        await using var output = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None,      81920, true);
        await input.CopyToAsync(output, cancellationToken);
    }

    private static async Task<string> ComputeSha256Async(string path, CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 81920, true);
        return Convert.ToHexString(await SHA256.HashDataAsync(stream, cancellationToken)).ToLowerInvariant();
    }

    private async Task WriteLogAsync(string message, CancellationToken cancellationToken)
    {
        try
        {
            var path = GetOperationLogPath();
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await File.AppendAllTextAsync(
                path,
                $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] {message}{Environment.NewLine}",
                cancellationToken);
        }
        catch
        {
            // il logging non deve mai far crashare il processo principale
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DTO di riepilogo restituito da ApplyZipUpdateAsync
// ════════════════════════════════════════════════════════════════════════════

/// <summary>Riepilogo dell'operazione di aggiornamento ZIP.</summary>
public sealed class ModUpdateResult
{
    /// <summary>File del modpack scritti/aggiornati.</summary>
    public int FilesWritten { get; init; }

    /// <summary>File utente (protetti) saltati senza modifica.</summary>
    public int FilesSkipped { get; init; }

    /// <summary>File del modpack obsoleti rimossi (non presenti nel nuovo ZIP).</summary>
    public int FilesPruned  { get; init; }

    /// <summary>Eventuali errori non fatali durante l'estrazione.</summary>
    public List<string> Errors { get; init; } = [];

    public bool HasErrors => Errors.Count > 0;

    public override string ToString() =>
        $"{FilesWritten} aggiornati, {FilesSkipped} protetti (saltati), {FilesPruned} obsoleti rimossi" +
        (HasErrors ? $", {Errors.Count} errori" : string.Empty);
}
