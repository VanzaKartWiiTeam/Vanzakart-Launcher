using System.IO;
using System.Security.Cryptography;
using System.Text;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class MkwiiSaveParserService
{
    private const string RksysMagic = "RKSD0006";
    private const string RkpdMagic = "RKPD";
    private const int RkpdSize = 0x8CC0;
    private const int MaxLicenseSlots = 4;
    private const int MiiDbHeaderOffset = 0x04;
    private const int MiiDbSlots = 100;
    private const int MiiDbCrcOffset = 0x1F1DE;
    private const int MiiDbSize = 779_968;

    private readonly MiiFileParserService _miiParser;
    private readonly MiiAvatarRenderService _avatarRenderer;

    public MkwiiSaveParserService(MiiFileParserService miiParser, MiiAvatarRenderService avatarRenderer)
    {
        _miiParser = miiParser;
        _avatarRenderer = avatarRenderer;
    }

    public string GetMiiDatabasePath(string userFolderPath)
    {
        return string.IsNullOrWhiteSpace(userFolderPath)
            ? string.Empty
            : Path.Combine(userFolderPath, "Wii", "shared2", "menu", "FaceLib", "RFL_DB.dat");
    }

    public IReadOnlyDictionary<uint, WiiMiiData> ReadMiiDatabase(string userFolderPath)
    {
        var path = GetMiiDatabasePath(userFolderPath);
        var result = new Dictionary<uint, WiiMiiData>();
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path))
        {
            return result;
        }

        byte[] database;
        try
        {
            database = File.ReadAllBytes(path);
        }
        catch
        {
            return result;
        }

        for (var i = 0; i < MiiDbSlots; i++)
        {
            var offset = MiiDbHeaderOffset + i * MiiFileParserService.WiiMiiBlockSize;
            if (offset + MiiFileParserService.WiiMiiBlockSize > database.Length)
            {
                break;
            }

            var block = database.AsSpan(offset, MiiFileParserService.WiiMiiBlockSize).ToArray();
            try
            {
                var parsed = _miiParser.ParseWiiMiiBlock(block, "Dolphin Mii DB", path);
                if (parsed.MiiId != 0 && !result.ContainsKey(parsed.MiiId))
                {
                    result.Add(parsed.MiiId, AttachCachedAvatar(parsed));
                }
            }
            catch
            {
            }
        }

        return result;
    }

    public IReadOnlyList<string> FindMarioKartSaveFiles(string userFolderPath)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || !Directory.Exists(userFolderPath))
        {
            return Array.Empty<string>();
        }

        var files = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var roots = new[]
        {
            Path.Combine(userFolderPath, "Wii", "title"),
            Path.Combine(userFolderPath, "Load", "Riivolution"),
            Path.Combine(userFolderPath, "Wii")
        };

        foreach (var root in roots.Where(Directory.Exists))
        {
            AddSaveFiles(root, files);
        }

        if (files.Count == 0)
        {
            AddSaveFiles(userFolderPath, files);
        }

        return files.OrderBy(path => path).ToArray();
    }

    public IReadOnlyList<string> FindVanzaKartSaveFiles(LauncherSettings settings)
    {
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath))
        {
            return Array.Empty<string>();
        }

        var modRoot = Path.Combine(settings.GetModFolder(), "VanzaKart");
        return FindMarioKartSaveFiles(settings.UserFolderPath)
            .Where(path => IsVanzaKartSavePath(path, modRoot))
            .Select(path => new FileInfo(path))
            .Where(info => info.Exists)
            .OrderByDescending(info => info.LastWriteTimeUtc)
            .Take(1)
            .Select(info => info.FullName)
            .ToArray();
    }

    public IReadOnlyList<SaveProfileInfo> ReadLicenseCards(string rksysPath, IReadOnlyDictionary<uint, WiiMiiData> miiDatabase)
    {
        var data = File.ReadAllBytes(rksysPath);
        if (data.Length < RksysMagic.Length || Encoding.ASCII.GetString(data, 0, RksysMagic.Length) != RksysMagic)
        {
            return Array.Empty<SaveProfileInfo>();
        }

        var fileInfo = new FileInfo(rksysPath);
        var cards = new List<SaveProfileInfo>();

        for (var slot = 0; slot < MaxLicenseSlots; slot++)
        {
            var rkpdOffset = RksysMagic.Length + slot * RkpdSize;
            if (rkpdOffset + RkpdMagic.Length >= data.Length)
            {
                break;
            }

            if (Encoding.ASCII.GetString(data, rkpdOffset, RkpdMagic.Length) != RkpdMagic)
            {
                continue;
            }

            var licenseName = ReadUtf16BigEndian(data, rkpdOffset + 0x14, 20);
            var miiId = ReadUInt32BigEndian(data, rkpdOffset + 0x28);
            var profileId = ReadUInt32BigEndian(data, rkpdOffset + 0x5C);
            var vr = ReadUInt16BigEndian(data, rkpdOffset + 0xB0);
            var br = ReadUInt16BigEndian(data, rkpdOffset + 0xB2);
            var races = ReadUInt32BigEndian(data, rkpdOffset + 0xB4);
            var wins = ReadUInt32BigEndian(data, rkpdOffset + 0xDC);

            miiDatabase.TryGetValue(miiId, out var mii);
            if (string.IsNullOrWhiteSpace(licenseName) && mii == null && profileId == 0)
            {
                continue;
            }

            var displayName = string.IsNullOrWhiteSpace(licenseName)
                ? $"License {slot + 1}"
                : licenseName;

            var gameId = GetGameIdFromPath(rksysPath);
            var friendCode = profileId != 0 && !string.IsNullOrEmpty(gameId)
                ? CalculateFriendCode(profileId, gameId)
                : string.Empty;

            cards.Add(new SaveProfileInfo
            {
                DisplayName = displayName,
                Subtitle = $"Slot {slot + 1}  \u2022  {BuildRegionLabel(rksysPath)}",
                FilePath = rksysPath,
                SourceLabel = "Dolphin save",
                MiiName = mii?.Name ?? "Mii not found in RFL_DB.dat",
                AvatarInitial = BuildInitial(mii?.Name ?? displayName),
                AvatarImagePath = mii?.AvatarImagePath ?? string.Empty,
                AvatarStatus = mii == null
                    ? "Mii not found in Dolphin database"
                    : string.IsNullOrWhiteSpace(mii.AvatarImagePath) ? "Render queued" : "Rendered",
                AccentColor = mii?.FavoriteColor ?? "#39E7FF",
                FriendCode = friendCode,
                ProfileId = profileId,
                MiiId = miiId,
                Vr = vr,
                Br = br,
                Races = races,
                Wins = wins,
                LastModifiedUtc = fileInfo.LastWriteTimeUtc,
                SizeBytes = fileInfo.Length,
                IsLauncherManaged = false
            });
        }

        return cards;
    }

    public async Task<bool> EnsureMiiDatabaseAvatarCacheAsync(string userFolderPath, CancellationToken cancellationToken = default)
    {
        var renderedAny = false;
        foreach (var mii in ReadMiiDatabase(userFolderPath).Values)
        {
            if (!string.IsNullOrWhiteSpace(mii.AvatarImagePath))
            {
                continue;
            }

            var avatar = await _avatarRenderer.EnsureAvatarRenderAsync(mii, cancellationToken);
            renderedAny |= avatar.IsReady;
        }

        return renderedAny;
    }

    public async Task AddOrUpdateMiiInDatabaseAsync(
        string userFolderPath,
        LauncherMiiProfile profile,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath))
        {
            throw new InvalidOperationException("Dolphin user folder is not configured.");
        }

        if (string.IsNullOrWhiteSpace(profile.RawMiiBase64))
        {
            throw new InvalidOperationException("Selected Mii does not contain real Wii Mii data.");
        }

        var raw = Convert.FromBase64String(profile.RawMiiBase64);
        if (raw.Length != MiiFileParserService.WiiMiiBlockSize)
        {
            throw new InvalidDataException("Selected Mii data is not a valid Wii Mii block.");
        }

        var dbPath = GetMiiDatabasePath(userFolderPath);
        Directory.CreateDirectory(Path.GetDirectoryName(dbPath)!);

        var db = File.Exists(dbPath)
            ? await File.ReadAllBytesAsync(dbPath, cancellationToken)
            : CreateEmptyMiiDatabase();

        BackupDatabase(dbPath);

        var targetOffset = FindMiiSlotOffset(db, profile.MiiId);
        if (targetOffset < 0)
        {
            targetOffset = FindEmptyMiiSlotOffset(db);
        }

        if (targetOffset < 0)
        {
            throw new InvalidOperationException("Dolphin Mii database is full.");
        }

        Buffer.BlockCopy(raw, 0, db, targetOffset, raw.Length);
        WriteMiiDatabaseCrc(db);
        await File.WriteAllBytesAsync(dbPath, db, cancellationToken);
    }

    public void DeleteMiiFromDatabase(string userFolderPath, uint miiId)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || miiId == 0)
        {
            return;
        }

        var dbPath = GetMiiDatabasePath(userFolderPath);
        if (!File.Exists(dbPath))
        {
            return;
        }

        try
        {
            var db = File.ReadAllBytes(dbPath);
            var offset = FindMiiSlotOffset(db, miiId);
            if (offset >= 0)
            {
                BackupDatabase(dbPath);
                var empty = new byte[MiiFileParserService.WiiMiiBlockSize];
                Buffer.BlockCopy(empty, 0, db, offset, empty.Length);
                WriteMiiDatabaseCrc(db);
                File.WriteAllBytes(dbPath, db);
            }
        }
        catch
        {
            throw;
        }
    }

    private WiiMiiData AttachCachedAvatar(WiiMiiData mii)
    {
        return new WiiMiiData
        {
            Name = mii.Name,
            CreatorName = mii.CreatorName,
            FormatName = mii.FormatName,
            SourceFilePath = mii.SourceFilePath,
            RawMiiBase64 = mii.RawMiiBase64,
            StudioData = mii.StudioData,
            Sha256 = mii.Sha256,
            FavoriteColor = mii.FavoriteColor,
            AvatarImagePath = _avatarRenderer.TryGetCachedAvatar(mii),
            MiiId = mii.MiiId,
            FavoriteColorIndex = mii.FavoriteColorIndex,
            Height = mii.Height,
            Weight = mii.Weight,
            IsFemale = mii.IsFemale,
            IsFavorite = mii.IsFavorite,
            CreatedDate = mii.CreatedDate
        };
    }

    private static void AddSaveFiles(string root, ISet<string> files)
    {
        try
        {
            foreach (var path in Directory.EnumerateFiles(root, "rksys.dat", SearchOption.AllDirectories))
            {
                files.Add(path);
            }
        }
        catch
        {
        }
    }

    private static bool IsVanzaKartSavePath(string path, string modRoot)
    {
        if (!string.IsNullOrWhiteSpace(modRoot))
        {
            try
            {
                var relative = Path.GetRelativePath(modRoot, path);
                if (!relative.StartsWith("..", StringComparison.Ordinal) && !Path.IsPathRooted(relative))
                {
                    return true;
                }
            }
            catch
            {
            }
        }

        return path.Contains($"{Path.DirectorySeparatorChar}VanzaKart{Path.DirectorySeparatorChar}", StringComparison.OrdinalIgnoreCase)
               || path.Contains($"{Path.AltDirectorySeparatorChar}VanzaKart{Path.AltDirectorySeparatorChar}", StringComparison.OrdinalIgnoreCase);
    }

    private static int FindMiiSlotOffset(byte[] db, uint miiId)
    {
        if (miiId == 0)
        {
            return -1;
        }

        for (var i = 0; i < MiiDbSlots; i++)
        {
            var offset = MiiDbHeaderOffset + i * MiiFileParserService.WiiMiiBlockSize;
            if (offset + MiiFileParserService.WiiMiiBlockSize > db.Length)
            {
                break;
            }

            if (ReadUInt32BigEndian(db, offset + 0x18) == miiId)
            {
                return offset;
            }
        }

        return -1;
    }

    private static int FindEmptyMiiSlotOffset(byte[] db)
    {
        for (var i = 0; i < MiiDbSlots; i++)
        {
            var offset = MiiDbHeaderOffset + i * MiiFileParserService.WiiMiiBlockSize;
            if (offset + MiiFileParserService.WiiMiiBlockSize > db.Length)
            {
                break;
            }

            var block = db.AsSpan(offset, MiiFileParserService.WiiMiiBlockSize);
            if (block.ToArray().All(value => value == 0x00) || block.ToArray().All(value => value == 0xFF))
            {
                return offset;
            }
        }

        return -1;
    }

    private static byte[] CreateEmptyMiiDatabase()
    {
        var db = new byte[MiiDbSize];
        db[0] = 0x52;
        db[1] = 0x4E;
        db[2] = 0x4F;
        db[3] = 0x44;
        db[0x1CE0 + 0x0C] = 0x80;
        db[0x1D00] = 0x52;
        db[0x1D01] = 0x4E;
        db[0x1D02] = 0x48;
        db[0x1D03] = 0x44;
        db[0x1D04] = 0xFF;
        db[0x1D05] = 0xFF;
        db[0x1D06] = 0xFF;
        db[0x1D07] = 0xFF;
        WriteMiiDatabaseCrc(db);
        return db;
    }

    private static void BackupDatabase(string dbPath)
    {
        if (!File.Exists(dbPath))
        {
            return;
        }

        var backupFolder = Path.Combine(AppContext.BaseDirectory, "Backups", "MiiDatabase");
        Directory.CreateDirectory(backupFolder);
        var destination = Path.Combine(backupFolder, $"RFL_DB_{DateTime.Now:yyyyMMdd_HHmmss}.dat");
        File.Copy(dbPath, destination, overwrite: false);
    }

    private static void WriteMiiDatabaseCrc(byte[] db)
    {
        if (db.Length < MiiDbCrcOffset + 2)
        {
            return;
        }

        var crc = ComputeCrc16Ccitt(db, 0, MiiDbCrcOffset);
        db[MiiDbCrcOffset] = (byte)(crc >> 8);
        db[MiiDbCrcOffset + 1] = (byte)crc;
    }

    private static ushort ComputeCrc16Ccitt(byte[] data, int offset, int count)
    {
        ushort crc = 0;
        for (var i = offset; i < offset + count && i < data.Length; i++)
        {
            crc ^= (ushort)(data[i] << 8);
            for (var bit = 0; bit < 8; bit++)
            {
                crc = (crc & 0x8000) != 0
                    ? (ushort)((crc << 1) ^ 0x1021)
                    : (ushort)(crc << 1);
            }
        }

        return crc;
    }

    private static ushort ReadUInt16BigEndian(byte[] bytes, int offset)
    {
        return offset + 1 < bytes.Length
            ? (ushort)((bytes[offset] << 8) | bytes[offset + 1])
            : (ushort)0;
    }

    private static uint ReadUInt32BigEndian(byte[] bytes, int offset)
    {
        return offset + 3 < bytes.Length
            ? ((uint)bytes[offset] << 24) | ((uint)bytes[offset + 1] << 16) | ((uint)bytes[offset + 2] << 8) | bytes[offset + 3]
            : 0;
    }

    private static string ReadUtf16BigEndian(byte[] bytes, int offset, int byteCount)
    {
        if (offset < 0 || offset + byteCount > bytes.Length)
        {
            return string.Empty;
        }

        return Encoding.BigEndianUnicode.GetString(bytes, offset, byteCount)
            .Replace("\0", string.Empty, StringComparison.Ordinal)
            .Trim();
    }

    private static string BuildInitial(string value)
    {
        return string.IsNullOrWhiteSpace(value)
            ? "M"
            : value.Trim()[0].ToString().ToUpperInvariant();
    }

    private static string BuildRegionLabel(string rksysPath)
    {
        var gameId = GetGameIdFromPath(rksysPath);
        if (string.IsNullOrEmpty(gameId) || gameId.Length < 4)
        {
            return "Mario Kart Wii";
        }

        return gameId[3] switch
        {
            'P' => "PAL (Europe)",
            'E' => "NTSC-U (USA)",
            'J' => "NTSC-J (Japan)",
            'K' => "NTSC-K (Korea)",
            _ => $"Region {gameId[3]}"
        };
    }

    private static string GetGameIdFromPath(string rksysPath)
    {
        var parent = Directory.GetParent(rksysPath);
        if (parent == null || parent.Name.Length < 4)
        {
            return string.Empty;
        }

        var name = parent.Name;

        // If the folder name is a hex-encoded title ID (e.g. "524d4350" for RMCP), decode it
        if (name.Length >= 8 && name.All(c => Uri.IsHexDigit(c)))
        {
            try
            {
                var bytes = new byte[name.Length / 2];
                for (var i = 0; i < bytes.Length; i++)
                {
                    bytes[i] = Convert.ToByte(name.Substring(i * 2, 2), 16);
                }
                var decoded = Encoding.ASCII.GetString(bytes);
                if (decoded.Length >= 4 && decoded.All(c => c >= 0x20 && c < 0x7F))
                {
                    return decoded[..4];
                }
            }
            catch
            {
            }
        }

        // If the folder name is already the Game ID (e.g. "RMCP")
        if (name.Length >= 4 && name[..4].All(c => c >= 0x20 && c < 0x7F))
        {
            return name[..4];
        }

        return string.Empty;
    }

    private static string CalculateFriendCode(uint profileId, string gameId)
    {
        if (profileId == 0 || string.IsNullOrEmpty(gameId) || gameId.Length < 4)
        {
            return string.Empty;
        }

        try
        {
            // Build the 8-byte buffer: PID (little-endian) + reversed Game ID (ASCII)
            var buffer = new byte[8];
            buffer[0] = (byte)(profileId & 0xFF);
            buffer[1] = (byte)((profileId >> 8) & 0xFF);
            buffer[2] = (byte)((profileId >> 16) & 0xFF);
            buffer[3] = (byte)((profileId >> 24) & 0xFF);

            // Reversing "RMCJ" yields "JCMR" -> [0x4A, 0x43, 0x4D, 0x52]
            // We ALWAYS use "RMCJ" as the game ID for Mario Kart Wii friend codes
            var targetGameId = "RMCJ";
            var reversed = targetGameId.ToCharArray();
            Array.Reverse(reversed);
            for (var i = 0; i < 4; i++)
            {
                buffer[4 + i] = (byte)reversed[i];
            }

            var hash = MD5.HashData(buffer);
            var checksum = (byte)(hash[0] >> 1);

            // Friend code = (checksum << 32) | profileId
            var fc = ((long)checksum << 32) | profileId;
            var fcStr = fc.ToString().PadLeft(12, '0');

            // Format as XXXX-XXXX-XXXX
            return $"{fcStr[..4]}-{fcStr[4..8]}-{fcStr[8..12]}";
        }
        catch
        {
            return string.Empty;
        }
    }
}
