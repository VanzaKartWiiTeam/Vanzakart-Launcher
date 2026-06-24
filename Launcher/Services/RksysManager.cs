using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class RksysManager
{
    private const string RksysMagic = "RKSD0006";
    private const string RkpdMagic = "RKPD";
    private const int RkpdSize = 0x8CC0;
    
    public const int FriendMainOffset = 0x56D0;
    public const int FriendSecondaryOffset = 0x8B50;
    public const int FriendStride = 0x1C0;
    public const int FriendSecondaryStride = 0x0C;
    public const int NumFriends = 30;
    
    public const int GlobalCrcOffset = 0x27FFC;
    public const int DwcOffset = 0x40;
    public const int DwcDataLen = 0x3C;
    private const int RksysChecksumDataLength = 0x27FFC;
    private const int RksysChecksumOffset = 0x27FFC;

    private static readonly uint[] Crc32Table;
    private static readonly byte[] DefaultFcSalt = [ (byte)'J', (byte)'C', (byte)'M', (byte)'R' ];

    static RksysManager()
    {
        Crc32Table = new uint[256];
        for (uint i = 0; i < 256; i++)
        {
            uint entry = i;
            for (int j = 0; j < 8; j++)
            {
                if ((entry & 1) == 1)
                {
                    entry = (entry >> 1) ^ 0xEDB88320;
                }
                else
                {
                    entry >>= 1;
                }
            }
            Crc32Table[i] = entry;
        }
    }

    public static uint ComputeCrc32(byte[] data, int offset, int length)
    {
        uint crc = 0xFFFFFFFF;
        for (int i = 0; i < length; i++)
        {
            byte b = data[offset + i];
            crc = (crc >> 8) ^ Crc32Table[(crc ^ b) & 0xFF];
        }
        return crc ^ 0xFFFFFFFF;
    }

    public static uint ComputeReversedWordsCrc32(byte[] data, int offset, int length)
    {
        byte[] tmp = new byte[length];
        for (int i = 0; i < length; i += 4)
        {
            tmp[i] = data[offset + i + 3];
            tmp[i + 1] = data[offset + i + 2];
            tmp[i + 2] = data[offset + i + 1];
            tmp[i + 3] = data[offset + i];
        }
        return ComputeCrc32(tmp, 0, length);
    }

    public static byte CalculateFriendCodeChecksum(uint pid)
    {
        var buffer = new byte[8];
        buffer[0] = (byte)(pid & 0xFF);
        buffer[1] = (byte)((pid >> 8) & 0xFF);
        buffer[2] = (byte)((pid >> 16) & 0xFF);
        buffer[3] = (byte)((pid >> 24) & 0xFF);
        
        // Reversing "RMCJ" yields "JCMR" -> [0x4A, 0x43, 0x4D, 0x52]
        buffer[4] = DefaultFcSalt[0];
        buffer[5] = DefaultFcSalt[1];
        buffer[6] = DefaultFcSalt[2];
        buffer[7] = DefaultFcSalt[3];

        var hash = MD5.HashData(buffer);
        return (byte)((hash[0] >> 1) & 0x7F);
    }

    public static string FormatFriendCode(uint pid)
    {
        byte csum = CalculateFriendCodeChecksum(pid);
        long fcVal = ((long)csum << 32) | pid;
        string fcStr = fcVal.ToString().PadLeft(12, '0');
        return $"{fcStr[..4]}-{fcStr[4..8]}-{fcStr[8..12]}";
    }

    public static bool TryParseFriendCode(string fcStr, out uint pid, out string errorMessage)
    {
        pid = 0;
        errorMessage = string.Empty;
        
        var clean = new string(fcStr.Where(char.IsDigit).ToArray());
        if (clean.Length != 12)
        {
            errorMessage = "The friend code must contain exactly 12 digits.";
            return false;
        }
        
        if (!long.TryParse(clean, out long fcVal))
        {
            errorMessage = "Invalid friend code.";
            return false;
        }
        
        long max39 = (1L << 39) - 1;
        if (fcVal > max39)
        {
            errorMessage = "The friend code is out of range for Mario Kart Wii.";
            return false;
        }
        
        uint extractedPid = (uint)(fcVal & 0xFFFFFFFF);
        byte extractedCsum = (byte)((fcVal >> 32) & 0x7F);
        
        byte expectedCsum = CalculateFriendCodeChecksum(extractedPid);
        if (extractedCsum != expectedCsum)
        {
            errorMessage = "The friend code checksum is invalid.";
            return false;
        }
        
        pid = extractedPid;
        return true;
    }

    public static List<SaveFriendInfo> ReadFriends(string rksysPath, int licenseIndex, MiiFileParserService miiParser)
    {
        var result = new List<SaveFriendInfo>();
        if (!File.Exists(rksysPath)) return result;

        var data = File.ReadAllBytes(rksysPath);
        if (data.Length < RksysMagic.Length || Encoding.ASCII.GetString(data, 0, RksysMagic.Length) != RksysMagic)
        {
            return result;
        }

        int licenseOffset = 0x08 + licenseIndex * RkpdSize;
        if (licenseOffset + RkpdSize > data.Length) return result;

        if (Encoding.ASCII.GetString(data, licenseOffset, RkpdMagic.Length) != RkpdMagic)
        {
            return result; // License is empty
        }

        int mainBase = licenseOffset + FriendMainOffset;
        int secBase = licenseOffset + FriendSecondaryOffset;

        for (int i = 0; i < NumFriends; i++)
        {
            int ptr = mainBase + i * FriendStride;
            int secPtr = secBase + i * FriendSecondaryStride;

            uint pid = ReadUInt32BigEndian(data, ptr + 0x04);
            ushort flag = ReadUInt16BigEndian(data, ptr + 0x10);
            ushort losses = ReadUInt16BigEndian(data, ptr + 0x12);
            ushort wins = ReadUInt16BigEndian(data, ptr + 0x14);
            ushort raceR = ReadUInt16BigEndian(data, ptr + 0x16);
            ushort battleR = ReadUInt16BigEndian(data, ptr + 0x18);
            byte country = data[ptr + 0x68];
            byte region = data[ptr + 0x69];
            ushort globeX = ReadUInt16BigEndian(data, ptr + 0x6C);
            ushort globeY = ReadUInt16BigEndian(data, ptr + 0x6E);
            uint key0 = ReadUInt32BigEndian(data, ptr + 0x00);
            uint wordA = ReadUInt32BigEndian(data, ptr + 0x08);
            uint wordB = ReadUInt32BigEndian(data, ptr + 0x0C);
            byte secCtrl = data[secPtr + 0x02];
            byte rosterIndex = data[ptr + 0x66];

            bool isEmpty = pid == 0 &&
                           (flag & 0x03) == 0 &&
                           key0 == 0 &&
                           wordA == 0 &&
                           wordB == 0 &&
                           losses == 0 &&
                           wins == 0 &&
                           raceR == 0 &&
                           battleR == 0 &&
                           country == 0 &&
                           region == 0 &&
                           globeX == 0 &&
                           globeY == 0 &&
                           secCtrl == 0;

            if (isEmpty) continue;

            // Extract the 74-byte Mii block
            byte[] miiData = new byte[74];
            Buffer.BlockCopy(data, ptr + 0x1A, miiData, 0, 74);

            string miiName = "Mii";
            WiiMiiData? parsedMii = null;
            try
            {
                // Verify if there is real Mii data
                bool hasMii = miiData.Any(b => b != 0);
                if (hasMii)
                {
                    parsedMii = miiParser.ParseWiiMiiBlock(miiData, "Friend Mii");
                    miiName = parsedMii.Name;
                }
            }
            catch
            {
                // Keep default
            }

            bool isPending = (secCtrl == 0x18) || ((flag & 0x03) == 0x01);

            result.Add(new SaveFriendInfo
            {
                SlotIndex = i,
                ProfileId = pid,
                FriendCode = FormatFriendCode(pid),
                Wins = wins,
                Losses = losses,
                RaceRating = raceR,
                BattleRating = battleR,
                MiiName = miiName,
                MiiData = miiData,
                ParsedMii = parsedMii,
                RosterIndex = rosterIndex,
                GameRegion = data[ptr + 0x67],
                CountryId = country,
                RegionId = region,
                CityId = ReadUInt16BigEndian(data, ptr + 0x6A),
                GlobeX = globeX,
                GlobeY = globeY,
                IsPending = isPending
            });
        }

        return result;
    }

    public static void AddFriend(string rksysPath, int licenseIndex, uint friendPid)
    {
        if (!File.Exists(rksysPath)) return;

        var data = File.ReadAllBytes(rksysPath);
        if (data.Length < RksysMagic.Length || Encoding.ASCII.GetString(data, 0, RksysMagic.Length) != RksysMagic)
        {
            throw new InvalidDataException("The selected file is not a valid Mario Kart Wii save.");
        }

        if (!TryGetRksysChecksumMode(data, out var checksumMode))
        {
            throw new InvalidDataException("The selected save checksum could not be verified. The file was not modified.");
        }

        int licenseOffset = 0x08 + licenseIndex * RkpdSize;
        if (licenseOffset + RkpdSize > data.Length ||
            Encoding.ASCII.GetString(data, licenseOffset, RkpdMagic.Length) != RkpdMagic)
        {
            throw new InvalidOperationException("The selected license slot is empty or invalid.");
        }

        int mainBase = licenseOffset + FriendMainOffset;
        int secBase = licenseOffset + FriendSecondaryOffset;

        // 1. Find the first empty slot
        int targetSlot = -1;
        for (int i = 0; i < NumFriends; i++)
        {
            int ptr = mainBase + i * FriendStride;
            uint pid = ReadUInt32BigEndian(data, ptr + 0x04);
            if (pid == 0)
            {
                targetSlot = i;
                break;
            }
        }

        if (targetSlot == -1)
        {
            throw new InvalidOperationException("The friend list is full (maximum 30 friends).");
        }

        // 2. Write the friend data
        int targetPtr = mainBase + targetSlot * FriendStride;
        int targetSecPtr = secBase + targetSlot * FriendSecondaryStride;

        // Start from a clean slot. Some saves can contain leftover data in slots
        // whose PID is zero; keeping those bytes creates malformed online entries.
        Array.Clear(data, targetPtr, FriendStride);
        Array.Clear(data, targetSecPtr, FriendSecondaryStride);

        uint checksum = CalculateFriendCodeChecksum(friendPid);

        // FriendKeyHi at 0x00
        WriteUInt32BigEndian(data, targetPtr + 0x00, checksum);
        // ProfileID at 0x04
        WriteUInt32BigEndian(data, targetPtr + 0x04, friendPid);
        // BaseSlotState flag at 0x10 -> pending/outgoing request.
        // A launcher can only add the friend code; the game/server must later
        // fill the confirmed friend data. Marking it as confirmed immediately
        // leaves the DWC roster in an invalid state and can break online login.
        WriteUInt16BigEndian(data, targetPtr + 0x10, 0x0001);

        // Losses, Wins, VR, BR stats
        WriteUInt16BigEndian(data, targetPtr + 0x12, 0);
        WriteUInt16BigEndian(data, targetPtr + 0x14, 0);
        WriteUInt16BigEndian(data, targetPtr + 0x16, 5000); // Default VR
        WriteUInt16BigEndian(data, targetPtr + 0x18, 5000); // Default BR

        // Roster index
        data[targetPtr + 0x66] = (byte)(targetSlot + 1);

        // Secondary control flag -> pending/outgoing request.
        data[targetSecPtr + 0x02] = 0x18;
        // Secondary Profile ID mirror fields
        WriteUInt32BigEndian(data, targetSecPtr + 0x04, friendPid);
        WriteUInt32BigEndian(data, targetSecPtr + 0x08, friendPid);

        // 3. Recalculate the rksys checksum using the same mode that the save
        // already used. Do not rewrite the DWC login CRC here: adding a friend
        // does not alter the DWC login block, and writing the wrong variant can
        // make online fail while the license still appears readable.
        WriteRksysChecksum(data, checksumMode);
        WriteSaveFileSafely(rksysPath, data);
    }

    public static void RemoveFriend(string rksysPath, int licenseIndex, int slotIndex)
    {
        if (!File.Exists(rksysPath)) return;

        var data = File.ReadAllBytes(rksysPath);
        if (data.Length < RksysMagic.Length || Encoding.ASCII.GetString(data, 0, RksysMagic.Length) != RksysMagic)
        {
            throw new InvalidDataException("The selected file is not a valid Mario Kart Wii save.");
        }

        if (!TryGetRksysChecksumMode(data, out var checksumMode))
        {
            throw new InvalidDataException("The selected save checksum could not be verified. The file was not modified.");
        }

        int licenseOffset = 0x08 + licenseIndex * RkpdSize;
        if (licenseOffset + RkpdSize > data.Length ||
            Encoding.ASCII.GetString(data, licenseOffset, RkpdMagic.Length) != RkpdMagic)
        {
            throw new InvalidOperationException("The selected license slot is empty or invalid.");
        }

        int mainBase = licenseOffset + FriendMainOffset;
        int secBase = licenseOffset + FriendSecondaryOffset;

        int targetPtr = mainBase + slotIndex * FriendStride;
        int targetSecPtr = secBase + slotIndex * FriendSecondaryStride;

        // Zero out the main slot (448 bytes)
        Array.Clear(data, targetPtr, FriendStride);
        // Zero out the secondary slot (12 bytes)
        Array.Clear(data, targetSecPtr, FriendSecondaryStride);

        // Recalculate checksum and Save
        WriteRksysChecksum(data, checksumMode);
        WriteSaveFileSafely(rksysPath, data);
    }

    private static void WriteSaveFileSafely(string rksysPath, byte[] data)
    {
        var saveDirectory = Path.GetDirectoryName(rksysPath)
            ?? throw new InvalidOperationException("The selected save path is invalid.");
        Directory.CreateDirectory(saveDirectory);

        var backupDirectory = Path.Combine(AppContext.BaseDirectory, "Backups", "Friends");
        Directory.CreateDirectory(backupDirectory);

        var timestamp = DateTime.Now.ToString("yyyyMMdd_HHmmss");
        var backupPath = Path.Combine(backupDirectory, $"rksys_friends_{timestamp}.dat");
        var tempPath = Path.Combine(saveDirectory, $".rksys_friends_{Guid.NewGuid():N}.tmp");

        File.WriteAllBytes(tempPath, data);
        try
        {
            File.Replace(tempPath, rksysPath, backupPath, ignoreMetadataErrors: true);
        }
        catch
        {
            try
            {
                if (File.Exists(rksysPath) && !File.Exists(backupPath))
                    File.Copy(rksysPath, backupPath, overwrite: false);
            }
            catch
            {
            }

            File.Copy(tempPath, rksysPath, overwrite: true);
        }
        finally
        {
            try
            {
                if (File.Exists(tempPath))
                    File.Delete(tempPath);
            }
            catch
            {
            }
        }
    }

    private static bool TryGetRksysChecksumMode(byte[] data, out Crc32Mode mode)
    {
        mode = Crc32Mode.ReflectedInitFFFFFFFFXorFFFFFFFF;
        if (data.Length < RksysChecksumOffset + 4)
        {
            return false;
        }

        var stored = ReadUInt32BigEndian(data, RksysChecksumOffset);
        foreach (var candidate in Enum.GetValues<Crc32Mode>())
        {
            if (ComputeRksysCrc32(data, candidate) == stored)
            {
                mode = candidate;
                return true;
            }
        }

        return false;
    }

    private static void WriteRksysChecksum(byte[] data, Crc32Mode mode)
    {
        if (data.Length < RksysChecksumOffset + 4)
        {
            throw new InvalidDataException("The selected save file is too small to contain an rksys checksum.");
        }

        WriteUInt32BigEndian(data, RksysChecksumOffset, ComputeRksysCrc32(data, mode));
    }

    private static uint ComputeRksysCrc32(byte[] data, Crc32Mode mode)
    {
        var length = Math.Min(RksysChecksumDataLength, data.Length);
        return mode switch
        {
            Crc32Mode.ReflectedInit00000000Xor00000000 => ComputeReflectedCrc32(data, length, 0x00000000, 0x00000000),
            Crc32Mode.NormalInitFFFFFFFFXorFFFFFFFF => ComputeNormalCrc32(data, length, 0xFFFFFFFF, 0xFFFFFFFF),
            Crc32Mode.NormalInit00000000Xor00000000 => ComputeNormalCrc32(data, length, 0x00000000, 0x00000000),
            _ => ComputeReflectedCrc32(data, length, 0xFFFFFFFF, 0xFFFFFFFF)
        };
    }

    private static uint ComputeReflectedCrc32(byte[] data, int length, uint initial, uint xorOut)
    {
        var crc = initial;
        for (var i = 0; i < length; i++)
        {
            crc ^= data[i];
            for (var bit = 0; bit < 8; bit++)
            {
                crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB88320u : crc >> 1;
            }
        }

        return crc ^ xorOut;
    }

    private static uint ComputeNormalCrc32(byte[] data, int length, uint initial, uint xorOut)
    {
        var crc = initial;
        for (var i = 0; i < length; i++)
        {
            crc ^= (uint)data[i] << 24;
            for (var bit = 0; bit < 8; bit++)
            {
                crc = (crc & 0x80000000u) != 0 ? (crc << 1) ^ 0x04C11DB7u : crc << 1;
            }
        }

        return crc ^ xorOut;
    }

    private static ushort ReadUInt16BigEndian(byte[] data, int offset)
    {
        return (ushort)((data[offset] << 8) | data[offset + 1]);
    }

    private static void WriteUInt16BigEndian(byte[] data, int offset, ushort value)
    {
        data[offset] = (byte)((value >> 8) & 0xFF);
        data[offset + 1] = (byte)(value & 0xFF);
    }

    private static uint ReadUInt32BigEndian(byte[] data, int offset)
    {
        return ((uint)data[offset] << 24) |
               ((uint)data[offset + 1] << 16) |
               ((uint)data[offset + 2] << 8) |
               data[offset + 3];
    }

    private static void WriteUInt32BigEndian(byte[] data, int offset, uint value)
    {
        data[offset] = (byte)((value >> 24) & 0xFF);
        data[offset + 1] = (byte)((value >> 16) & 0xFF);
        data[offset + 2] = (byte)((value >> 8) & 0xFF);
        data[offset + 3] = (byte)(value & 0xFF);
    }

    private enum Crc32Mode
    {
        ReflectedInitFFFFFFFFXorFFFFFFFF,
        ReflectedInit00000000Xor00000000,
        NormalInitFFFFFFFFXorFFFFFFFF,
        NormalInit00000000Xor00000000
    }
}

public sealed class SaveFriendInfo
{
    public int SlotIndex { get; set; }
    public uint ProfileId { get; set; }
    public string FriendCode { get; set; } = string.Empty;
    public ushort Wins { get; set; }
    public ushort Losses { get; set; }
    public ushort RaceRating { get; set; }
    public ushort BattleRating { get; set; }
    public string MiiName { get; set; } = string.Empty;
    public byte[] MiiData { get; set; } = [];
    public WiiMiiData? ParsedMii { get; set; }
    public byte RosterIndex { get; set; }
    public byte GameRegion { get; set; }
    public byte CountryId { get; set; }
    public byte RegionId { get; set; }
    public ushort CityId { get; set; }
    public ushort GlobeX { get; set; }
    public ushort GlobeY { get; set; }
    public bool IsPending { get; set; }
}
