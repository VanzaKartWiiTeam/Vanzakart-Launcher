using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class MiiFileParserService
{
    public const int WiiMiiBlockSize = 74;

    private static readonly string[] SupportedExtensions =
    [
        ".mii",
        ".miigx",
        ".mae",
        ".rcd",
        ".rsd"
    ];

    private static readonly string[] FavoriteColorHex =
    [
        "#FF3B3B",
        "#FF8A2A",
        "#FFD166",
        "#9CFF5E",
        "#317a11",
        "#3B82F6",
        "#8EE7FF",
        "#FF5CAB",
        "#A855F7",
        "#3d260c",
        "#F7FAFF",
        "#03010a"
    ];

    private static readonly int[] MakeupMap = [0, 1, 6, 9, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0];
    private static readonly int[] WrinklesMap = [0, 0, 0, 0, 5, 2, 3, 7, 8, 0, 9, 11, 0, 0, 0, 0];

    public async Task<MiiFileMetadata> ReadMetadataAsync(string filePath, CancellationToken cancellationToken = default)
    {
        var mii = await ReadMiiDataAsync(filePath, cancellationToken);

        return new MiiFileMetadata
        {
            FormatName = mii.FormatName,
            SuggestedName = mii.Name,
            SizeBytes = new FileInfo(filePath).Length,
            Sha256 = mii.Sha256,
            RawMiiBase64 = mii.RawMiiBase64,
            StudioData = mii.StudioData,
            CreatorName = mii.CreatorName,
            FavoriteColor = mii.FavoriteColor,
            FavoriteColorIndex = mii.FavoriteColorIndex,
            MiiId = mii.MiiId,
            IsFemale = mii.IsFemale,
            IsFavorite = mii.IsFavorite
        };
    }

    public async Task<WiiMiiData> ReadMiiDataAsync(string filePath, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(filePath))
        {
            throw new FileNotFoundException("The selected Mii file does not exist.", filePath);
        }

        var bytes = await File.ReadAllBytesAsync(filePath, cancellationToken);
        var rawBlock = ExtractMiiBlock(bytes)
            ?? throw new InvalidDataException("The selected file does not contain a valid Wii Mii block.");

        return ParseWiiMiiBlock(rawBlock, ResolveFormatName(filePath), filePath);
    }

    public WiiMiiData ParseWiiMiiBlock(byte[] rawBlock, string formatName = "Wii Mii", string sourcePath = "")
    {
        if (rawBlock.Length != WiiMiiBlockSize)
        {
            throw new InvalidDataException("Wii Mii data must be exactly 74 bytes.");
        }

        if (!LooksLikeWiiMii(rawBlock))
        {
            throw new InvalidDataException("The Mii block failed validation.");
        }

        var header = ReadUInt16BigEndian(rawBlock, 0);
        var favoriteColorIndex = (int)((header >> 1) & 0x0F);
        var month = Math.Clamp((header >> 10) & 0x0F, 1, 12);
        var day = Math.Clamp((header >> 5) & 0x1F, 1, 31);
        var name = ReadMiiString(rawBlock, 0x02);
        var creator = ReadMiiString(rawBlock, 0x36);

        return new WiiMiiData
        {
            Name = string.IsNullOrWhiteSpace(name) ? "Mii" : name,
            CreatorName = creator,
            FormatName = formatName,
            SourceFilePath = sourcePath,
            RawMiiBase64 = Convert.ToBase64String(rawBlock),
            StudioData = BuildStudioData(rawBlock),
            Sha256 = ComputeSha256(rawBlock),
            FavoriteColor = ResolveFavoriteColor(favoriteColorIndex),
            FavoriteColorIndex = favoriteColorIndex,
            MiiId = ReadUInt32BigEndian(rawBlock, 0x18),
            Height = rawBlock[0x16],
            Weight = rawBlock[0x17],
            IsFemale = (header & 0x4000) != 0,
            IsFavorite = (header & 0x01) != 0,
            CreatedDate = new DateTime(2000, month, day)
        };
    }

    public byte[] CreateDefaultMiiBytes(string name, int favoriteColorIndex, bool isFemale)
    {
        return CreateMiiBytes(new MiiEditorState
        {
            Name = name,
            CreatorName = "VanzaKart",
            FavoriteColorIndex = favoriteColorIndex,
            IsFemale = isFemale,
            IsFavorite = true
        });
    }

    public WiiMiiData CreateDefaultMii(string name, int favoriteColorIndex, bool isFemale)
    {
        return ParseWiiMiiBlock(CreateDefaultMiiBytes(name, favoriteColorIndex, isFemale), "Launcher-created Wii Mii");
    }

    public WiiMiiData CreateMii(MiiEditorState state, string formatName = "Launcher-created Wii Mii")
    {
        return ParseWiiMiiBlock(CreateMiiBytes(state), formatName);
    }

    public byte[] CreateMiiBytes(MiiEditorState state)
    {
        state = NormalizeEditorState(state);
        var raw = new byte[WiiMiiBlockSize];
        var systemId = ResolveSystemId(state);

        var favoriteColorIndex = Math.Clamp(state.FavoriteColorIndex, 0, FavoriteColorHex.Length - 1);
        ushort header = 0;
        if (state.IsFemale)
        {
            header |= 0x4000;
        }

        header |= (ushort)((Math.Clamp(state.BirthMonth, 1, 12) & 0x0F) << 10);
        header |= (ushort)((Math.Clamp(state.BirthDay, 1, 31) & 0x1F) << 5);
        header |= (ushort)((favoriteColorIndex & 0x0F) << 1);
        if (state.IsFavorite)
        {
            header |= 0x01;
        }

        WriteUInt16BigEndian(raw, 0x00, header);
        WriteMiiString(raw, 0x02, NormalizeMiiName(state.Name, "Vanza Mii"));
        raw[0x16] = (byte)Math.Clamp(state.Height, 0, 127);
        raw[0x17] = (byte)Math.Clamp(state.Weight, 0, 127);
        WriteUInt32BigEndian(raw, 0x18, state.MiiId == 0 ? GenerateMiiId() : state.MiiId);
        Buffer.BlockCopy(systemId, 0, raw, 0x1C, systemId.Length);

        WriteUInt16BigEndian(raw, 0x20, BuildFaceWord(state.FaceShape, state.SkinColor, state.FacialFeature));
        WriteUInt16BigEndian(raw, 0x22, BuildHairWord(state.HairType, state.HairColor, state.HairFlipped));
        WriteUInt32BigEndian(raw, 0x24, BuildEyebrowWord(
            state.EyebrowType,
            state.EyebrowRotation,
            state.EyebrowColor,
            state.EyebrowSize,
            state.EyebrowVertical,
            state.EyebrowSpacing));
        WriteUInt32BigEndian(raw, 0x28, BuildEyeWord(
            state.EyeType,
            state.EyeRotation,
            state.EyeVertical,
            state.EyeColor,
            state.EyeSize,
            state.EyeSpacing));
        WriteUInt16BigEndian(raw, 0x2C, BuildNoseWord(state.NoseType, state.NoseSize, state.NoseVertical));
        WriteUInt16BigEndian(raw, 0x2E, BuildLipWord(state.MouthType, state.MouthColor, state.MouthSize, state.MouthVertical));
        WriteUInt16BigEndian(raw, 0x30, BuildGlassesWord(state.GlassesType, state.GlassesColor, state.GlassesSize, state.GlassesVertical));
        WriteUInt16BigEndian(raw, 0x32, BuildFacialHairWord(
            state.MustacheType,
            state.BeardType,
            state.FacialHairColor,
            state.MustacheSize,
            state.MustacheVertical));
        WriteUInt16BigEndian(raw, 0x34, BuildMoleWord(state.MoleEnabled, state.MoleSize, state.MoleVertical, state.MoleHorizontal));
        WriteMiiString(raw, 0x36, NormalizeMiiName(state.CreatorName, "VanzaKart"));

        return raw;
    }

    public MiiEditorState ReadEditorState(byte[] rawBlock)
    {
        if (rawBlock.Length != WiiMiiBlockSize)
        {
            throw new InvalidDataException("Wii Mii data must be exactly 74 bytes.");
        }

        if (!LooksLikeWiiMii(rawBlock))
        {
            throw new InvalidDataException("The Mii block failed validation.");
        }

        var header = ReadUInt16BigEndian(rawBlock, 0x00);
        var face = ReadUInt16BigEndian(rawBlock, 0x20);
        var hair = ReadUInt16BigEndian(rawBlock, 0x22);
        var brow = ReadUInt32BigEndian(rawBlock, 0x24);
        var eye = ReadUInt32BigEndian(rawBlock, 0x28);
        var nose = ReadUInt16BigEndian(rawBlock, 0x2C);
        var mouth = ReadUInt16BigEndian(rawBlock, 0x2E);
        var glasses = ReadUInt16BigEndian(rawBlock, 0x30);
        var facial = ReadUInt16BigEndian(rawBlock, 0x32);
        var mole = ReadUInt16BigEndian(rawBlock, 0x34);

        return new MiiEditorState
        {
            Name = ReadMiiString(rawBlock, 0x02),
            CreatorName = ReadMiiString(rawBlock, 0x36),
            IsFemale = (header & 0x4000) != 0,
            IsFavorite = (header & 0x01) != 0,
            FavoriteColorIndex = (int)((header >> 1) & 0x0F),
            BirthMonth = Math.Clamp((header >> 10) & 0x0F, 1, 12),
            BirthDay = Math.Clamp((header >> 5) & 0x1F, 1, 31),
            Height = rawBlock[0x16],
            Weight = rawBlock[0x17],
            MiiId = ReadUInt32BigEndian(rawBlock, 0x18),
            SystemId0 = rawBlock[0x1C],
            SystemId1 = rawBlock[0x1D],
            SystemId2 = rawBlock[0x1E],
            SystemId3 = rawBlock[0x1F],
            FaceShape = face >> 13,
            SkinColor = (face >> 10) & 0x07,
            FacialFeature = (face >> 6) & 0x0F,
            HairType = hair >> 9,
            HairColor = (hair >> 6) & 0x07,
            HairFlipped = ((hair >> 5) & 0x01) != 0,
            EyebrowType = (int)(brow >> 27),
            EyebrowRotation = (int)((brow >> 22) & 0x0F),
            EyebrowColor = (int)((brow >> 13) & 0x07),
            EyebrowSize = (int)((brow >> 9) & 0x0F),
            EyebrowVertical = (int)((brow >> 4) & 0x1F),
            EyebrowSpacing = (int)(brow & 0x0F),
            EyeType = (int)(eye >> 26),
            EyeRotation = (int)((eye >> 21) & 0x07),
            EyeVertical = (int)((eye >> 16) & 0x1F),
            EyeColor = (int)((eye >> 13) & 0x07),
            EyeSize = (int)((eye >> 9) & 0x07),
            EyeSpacing = (int)((eye >> 5) & 0x0F),
            NoseType = nose >> 12,
            NoseSize = (nose >> 8) & 0x0F,
            NoseVertical = (nose >> 3) & 0x1F,
            MouthType = mouth >> 11,
            MouthColor = (mouth >> 9) & 0x03,
            MouthSize = (mouth >> 5) & 0x0F,
            MouthVertical = mouth & 0x1F,
            GlassesType = glasses >> 12,
            GlassesColor = (glasses >> 9) & 0x07,
            GlassesSize = (glasses >> 5) & 0x07,
            GlassesVertical = glasses & 0x1F,
            MustacheType = facial >> 14,
            BeardType = (facial >> 12) & 0x03,
            FacialHairColor = (facial >> 9) & 0x07,
            MustacheSize = (facial >> 5) & 0x0F,
            MustacheVertical = facial & 0x1F,
            MoleEnabled = ((mole >> 15) & 0x01) != 0,
            MoleSize = (mole >> 11) & 0x0F,
            MoleVertical = (mole >> 6) & 0x1F,
            MoleHorizontal = (mole >> 1) & 0x1F
        };
    }

    public byte[]? ExtractMiiBlock(byte[] fileBytes)
    {
        if (fileBytes.Length < WiiMiiBlockSize)
        {
            return null;
        }

        foreach (var offset in CandidateOffsets(fileBytes.Length))
        {
            if (offset < 0 || offset + WiiMiiBlockSize > fileBytes.Length)
            {
                continue;
            }

            var candidate = fileBytes.AsSpan(offset, WiiMiiBlockSize).ToArray();
            if (LooksLikeWiiMii(candidate))
            {
                return candidate;
            }
        }

        var scanLimit = Math.Min(fileBytes.Length - WiiMiiBlockSize, 4096);
        if (fileBytes.Length <= 1024 * 1024)
        {
            scanLimit = fileBytes.Length - WiiMiiBlockSize;
        }

        for (var offset = 0; offset <= scanLimit; offset++)
        {
            var candidate = fileBytes.AsSpan(offset, WiiMiiBlockSize).ToArray();
            if (LooksLikeWiiMii(candidate))
            {
                return candidate;
            }
        }

        return null;
    }

    public static bool IsSupportedMiiFile(string filePath)
    {
        return SupportedExtensions.Contains(Path.GetExtension(filePath).ToLowerInvariant());
    }

    public static string ResolveFavoriteColor(int favoriteColorIndex)
    {
        return favoriteColorIndex >= 0 && favoriteColorIndex < FavoriteColorHex.Length
            ? FavoriteColorHex[favoriteColorIndex]
            : FavoriteColorHex[4];
    }

    private static IEnumerable<int> CandidateOffsets(int length)
    {
        yield return 0;
        yield return 2;
        yield return 4;
        yield return 8;
        yield return 0x10;
        yield return 0x20;
        yield return 0x40;
        yield return 0x60;
        yield return 0xF0;
        yield return Math.Max(0, length - WiiMiiBlockSize);
    }

    private static bool LooksLikeWiiMii(byte[] raw)
    {
        if (raw.Length != WiiMiiBlockSize)
        {
            return false;
        }

        if (raw.All(value => value == 0x00) || raw.All(value => value == 0xFF))
        {
            return false;
        }

        var name = ReadMiiString(raw, 0x02);
        if (string.IsNullOrWhiteSpace(name) || name.Length > 10)
        {
            return false;
        }

        if (name.Any(ch => char.IsControl(ch) && ch != '\t'))
        {
            return false;
        }

        var header = ReadUInt16BigEndian(raw, 0);
        var favoriteColor = (header >> 1) & 0x0F;
        var month = (header >> 10) & 0x0F;
        var day = (header >> 5) & 0x1F;

        return favoriteColor <= 11
               && month <= 12
               && day <= 31
               && raw[0x16] <= 127
               && raw[0x17] <= 127
               && LooksLikeValidFeatureBlock(raw);
    }

    private static bool LooksLikeValidFeatureBlock(byte[] raw)
    {
        var face = ReadUInt16BigEndian(raw, 0x20);
        var hair = ReadUInt16BigEndian(raw, 0x22);
        var brow = ReadUInt32BigEndian(raw, 0x24);
        var eye = ReadUInt32BigEndian(raw, 0x28);
        var nose = ReadUInt16BigEndian(raw, 0x2C);
        var mouth = ReadUInt16BigEndian(raw, 0x2E);
        var glasses = ReadUInt16BigEndian(raw, 0x30);
        var facial = ReadUInt16BigEndian(raw, 0x32);

        return (face >> 13) <= 7
               && ((face >> 10) & 0x07) <= 5
               && ((face >> 6) & 0x0F) <= 11
               && (hair >> 9) <= 71
               && ((hair >> 6) & 0x07) <= 7
               && (brow >> 27) <= 23
               && ((brow >> 13) & 0x07) <= 7
               && (eye >> 26) <= 47
               && ((eye >> 13) & 0x07) <= 5
               && (nose >> 12) <= 11
               && (mouth >> 11) <= 23
               && (glasses >> 12) <= 8
               && (facial >> 14) <= 3
               && ((facial >> 12) & 0x03) <= 3;
    }

    private static string ResolveFormatName(string path)
    {
        return Path.GetExtension(path).ToLowerInvariant() switch
        {
            ".miigx" => "Mii GX",
            ".mae" => "MAE Mii",
            ".rcd" => "Wii Mii database block",
            ".rsd" => "Wii Mii database block",
            ".mii" => "Wii Mii",
            _ => "Wii Mii"
        };
    }

    private static string BuildStudioData(byte[] raw)
    {
        var studio = new byte[46];

        var basic = ReadUInt16BigEndian(raw, 0);
        studio[0x16] = (byte)(((basic >> 14) & 1) == 1 ? 1 : 0);
        studio[0x15] = (byte)((basic >> 1) & 0xF);
        studio[0x1E] = raw[0x16];
        studio[0x02] = raw[0x17];

        var face = ReadUInt16BigEndian(raw, 0x20);
        var facialFeature = (int)((face >> 6) & 0x0F);
        studio[0x13] = (byte)(face >> 13);
        studio[0x11] = (byte)((face >> 10) & 0x07);
        studio[0x14] = (byte)WrinklesMap[Math.Clamp(facialFeature, 0, WrinklesMap.Length - 1)];
        studio[0x12] = (byte)MakeupMap[Math.Clamp(facialFeature, 0, MakeupMap.Length - 1)];

        var hair = ReadUInt16BigEndian(raw, 0x22);
        var hairColor = (int)((hair >> 6) & 0x07);
        studio[0x1D] = (byte)(hair >> 9);
        studio[0x1B] = (byte)(hairColor == 0 ? 8 : hairColor);
        studio[0x1C] = (byte)((hair >> 5) & 1);

        var brow = ReadUInt32BigEndian(raw, 0x24);
        var browColor = (int)((brow >> 13) & 0x07);
        studio[0x0E] = (byte)(brow >> 27);
        studio[0x0C] = (byte)((brow >> 22) & 0x0F);
        studio[0x0B] = (byte)(browColor == 0 ? 8 : browColor);
        studio[0x0D] = (byte)((brow >> 9) & 0x0F);
        studio[0x0A] = 3;
        studio[0x10] = (byte)((brow >> 4) & 0x1F);
        studio[0x0F] = (byte)(brow & 0x0F);

        var eye = ReadUInt32BigEndian(raw, 0x28);
        studio[0x07] = (byte)(eye >> 26);
        studio[0x05] = (byte)((eye >> 21) & 0x07);
        studio[0x09] = (byte)((eye >> 16) & 0x1F);
        studio[0x04] = (byte)(((eye >> 13) & 0x07) + 8);
        studio[0x06] = (byte)((eye >> 9) & 0x07);
        studio[0x03] = 3;
        studio[0x08] = (byte)((eye >> 5) & 0x0F);

        var nose = ReadUInt16BigEndian(raw, 0x2C);
        studio[0x2C] = (byte)(nose >> 12);
        studio[0x2B] = (byte)((nose >> 8) & 0x0F);
        studio[0x2D] = (byte)((nose >> 3) & 0x1F);

        var mouth = ReadUInt16BigEndian(raw, 0x2E);
        var mouthColor = (int)((mouth >> 9) & 0x03);
        studio[0x26] = (byte)(mouth >> 11);
        studio[0x24] = (byte)(mouthColor + 19);
        studio[0x25] = (byte)((mouth >> 5) & 0x0F);
        studio[0x23] = 3;
        studio[0x27] = (byte)(mouth & 0x1F);

        var glasses = ReadUInt16BigEndian(raw, 0x30);
        var glassesColor = (int)((glasses >> 9) & 0x07);
        studio[0x19] = (byte)(glasses >> 12);
        studio[0x17] = glassesColor switch
        {
            0 => 8,
            < 6 => (byte)(glassesColor + 13),
            _ => 0
        };
        studio[0x18] = (byte)((glasses >> 5) & 0x07);
        studio[0x1A] = (byte)(glasses & 0x1F);

        var facial = ReadUInt16BigEndian(raw, 0x32);
        var facialHairColor = (int)((facial >> 9) & 0x07);
        studio[0x29] = (byte)(facial >> 14);
        studio[0x01] = (byte)((facial >> 12) & 0x03);
        studio[0x00] = (byte)(facialHairColor == 0 ? 8 : facialHairColor);
        studio[0x28] = (byte)((facial >> 5) & 0x0F);
        studio[0x2A] = (byte)(facial & 0x1F);

        var mole = ReadUInt16BigEndian(raw, 0x34);
        studio[0x20] = (byte)(mole >> 15);
        studio[0x1F] = (byte)((mole >> 11) & 0x0F);
        studio[0x22] = (byte)((mole >> 6) & 0x1F);
        studio[0x21] = (byte)((mole >> 1) & 0x1F);

        return EncodeStudioData(studio);
    }

    private static string EncodeStudioData(byte[] studioData)
    {
        var output = new StringBuilder("00", (studioData.Length + 1) * 2);
        byte rolling = 0;

        foreach (var value in studioData)
        {
            var encoded = (byte)((7 + (value ^ rolling)) & 0xFF);
            rolling = encoded;
            output.Append(encoded.ToString("x2", CultureInfo.InvariantCulture));
        }

        return output.ToString();
    }

    private static uint GenerateMiiId()
    {
        var epoch = new DateTime(2006, 1, 1, 0, 0, 0, DateTimeKind.Utc);
        var counter = (uint)((DateTime.UtcNow - epoch).TotalSeconds / 4);
        var entropy = RandomNumberGenerator.GetBytes(1)[0] & 0x1F;
        return (0b100u << 29) | ((counter + (uint)entropy) & 0x1FFFFFFFu);
    }

    private static byte[] ResolveSystemId(MiiEditorState state)
    {
        if (state.SystemId0 != 0 || state.SystemId1 != 0 || state.SystemId2 != 0 || state.SystemId3 != 0)
        {
            return [state.SystemId0, state.SystemId1, state.SystemId2, state.SystemId3];
        }

        return RandomNumberGenerator.GetBytes(4);
    }

    private static MiiEditorState NormalizeEditorState(MiiEditorState state)
    {
        var normalized = state.Clone();
        normalized.Name = NormalizeMiiName(normalized.Name, "Vanza Mii");
        normalized.CreatorName = NormalizeMiiName(normalized.CreatorName, "VanzaKart");
        normalized.FavoriteColorIndex = Math.Clamp(normalized.FavoriteColorIndex, 0, 11);
        normalized.BirthMonth = Math.Clamp(normalized.BirthMonth, 1, 12);
        normalized.BirthDay = Math.Clamp(normalized.BirthDay, 1, 31);
        normalized.Height = Math.Clamp(normalized.Height, 0, 127);
        normalized.Weight = Math.Clamp(normalized.Weight, 0, 127);
        normalized.FaceShape = Math.Clamp(normalized.FaceShape, 0, 7);
        normalized.SkinColor = Math.Clamp(normalized.SkinColor, 0, 5);
        normalized.FacialFeature = Math.Clamp(normalized.FacialFeature, 0, 11);
        normalized.HairType = Math.Clamp(normalized.HairType, 0, 71);
        normalized.HairColor = Math.Clamp(normalized.HairColor, 0, 7);
        normalized.EyebrowType = Math.Clamp(normalized.EyebrowType, 0, 23);
        normalized.EyebrowRotation = Math.Clamp(normalized.EyebrowRotation, 0, 15);
        normalized.EyebrowColor = Math.Clamp(normalized.EyebrowColor, 0, 7);
        normalized.EyebrowSize = Math.Clamp(normalized.EyebrowSize, 0, 15);
        normalized.EyebrowVertical = Math.Clamp(normalized.EyebrowVertical, 0, 31);
        normalized.EyebrowSpacing = Math.Clamp(normalized.EyebrowSpacing, 0, 15);
        normalized.EyeType = Math.Clamp(normalized.EyeType, 0, 47);
        normalized.EyeRotation = Math.Clamp(normalized.EyeRotation, 0, 7);
        normalized.EyeVertical = Math.Clamp(normalized.EyeVertical, 0, 31);
        normalized.EyeColor = Math.Clamp(normalized.EyeColor, 0, 5);
        normalized.EyeSize = Math.Clamp(normalized.EyeSize, 0, 7);
        normalized.EyeSpacing = Math.Clamp(normalized.EyeSpacing, 0, 15);
        normalized.NoseType = Math.Clamp(normalized.NoseType, 0, 11);
        normalized.NoseSize = Math.Clamp(normalized.NoseSize, 0, 15);
        normalized.NoseVertical = Math.Clamp(normalized.NoseVertical, 0, 31);
        normalized.MouthType = Math.Clamp(normalized.MouthType, 0, 23);
        normalized.MouthColor = Math.Clamp(normalized.MouthColor, 0, 3);
        normalized.MouthSize = Math.Clamp(normalized.MouthSize, 0, 15);
        normalized.MouthVertical = Math.Clamp(normalized.MouthVertical, 0, 31);
        normalized.GlassesType = Math.Clamp(normalized.GlassesType, 0, 8);
        normalized.GlassesColor = Math.Clamp(normalized.GlassesColor, 0, 5);
        normalized.GlassesSize = Math.Clamp(normalized.GlassesSize, 0, 7);
        normalized.GlassesVertical = Math.Clamp(normalized.GlassesVertical, 0, 31);
        normalized.MustacheType = Math.Clamp(normalized.MustacheType, 0, 3);
        normalized.BeardType = Math.Clamp(normalized.BeardType, 0, 3);
        normalized.FacialHairColor = Math.Clamp(normalized.FacialHairColor, 0, 7);
        normalized.MustacheSize = Math.Clamp(normalized.MustacheSize, 0, 15);
        normalized.MustacheVertical = Math.Clamp(normalized.MustacheVertical, 0, 31);
        normalized.MoleSize = Math.Clamp(normalized.MoleSize, 0, 15);
        normalized.MoleVertical = Math.Clamp(normalized.MoleVertical, 0, 31);
        normalized.MoleHorizontal = Math.Clamp(normalized.MoleHorizontal, 0, 31);
        return normalized;
    }

    private static string NormalizeMiiName(string value, string fallback)
    {
        var name = string.IsNullOrWhiteSpace(value) ? fallback : value.Trim();
        return name.Length <= 10 ? name : name[..10];
    }

    private static string ReadMiiString(byte[] bytes, int offset)
    {
        if (offset < 0 || offset + 20 > bytes.Length)
        {
            return string.Empty;
        }

        return Encoding.BigEndianUnicode.GetString(bytes, offset, 20)
            .Replace("\0", string.Empty, StringComparison.Ordinal)
            .Trim();
    }

    private static void WriteMiiString(byte[] bytes, int offset, string value)
    {
        Array.Clear(bytes, offset, 20);
        var encoded = Encoding.BigEndianUnicode.GetBytes(NormalizeMiiName(value, "Mii"));
        Buffer.BlockCopy(encoded, 0, bytes, offset, Math.Min(encoded.Length, 20));
    }

    private static ushort ReadUInt16BigEndian(byte[] bytes, int offset)
    {
        return (ushort)((bytes[offset] << 8) | bytes[offset + 1]);
    }

    private static uint ReadUInt32BigEndian(byte[] bytes, int offset)
    {
        return ((uint)bytes[offset] << 24)
               | ((uint)bytes[offset + 1] << 16)
               | ((uint)bytes[offset + 2] << 8)
               | bytes[offset + 3];
    }

    private static void WriteUInt16BigEndian(byte[] bytes, int offset, ushort value)
    {
        bytes[offset] = (byte)(value >> 8);
        bytes[offset + 1] = (byte)value;
    }

    private static void WriteUInt32BigEndian(byte[] bytes, int offset, uint value)
    {
        bytes[offset] = (byte)(value >> 24);
        bytes[offset + 1] = (byte)(value >> 16);
        bytes[offset + 2] = (byte)(value >> 8);
        bytes[offset + 3] = (byte)value;
    }

    private static ushort BuildFaceWord(int faceShape, int skinColor, int facialFeature)
    {
        return (ushort)(((faceShape & 0x07) << 13) | ((skinColor & 0x07) << 10) | ((facialFeature & 0x0F) << 6));
    }

    private static ushort BuildHairWord(int type, int color, bool flipped)
    {
        return (ushort)(((type & 0x7F) << 9) | ((color & 0x07) << 6) | ((flipped ? 1 : 0) << 5));
    }

    private static uint BuildEyebrowWord(int type, int rotation, int color, int size, int vertical, int spacing)
    {
        return ((uint)(type & 0x1F) << 27)
               | ((uint)(rotation & 0x0F) << 22)
               | ((uint)(color & 0x07) << 13)
               | ((uint)(size & 0x0F) << 9)
               | ((uint)(vertical & 0x1F) << 4)
               | (uint)(spacing & 0x0F);
    }

    private static uint BuildEyeWord(int type, int rotation, int vertical, int color, int size, int spacing)
    {
        return ((uint)(type & 0x3F) << 26)
               | ((uint)(rotation & 0x07) << 21)
               | ((uint)(vertical & 0x1F) << 16)
               | ((uint)(color & 0x07) << 13)
               | ((uint)(size & 0x07) << 9)
               | ((uint)(spacing & 0x0F) << 5);
    }

    private static ushort BuildNoseWord(int type, int size, int vertical)
    {
        return (ushort)(((type & 0x0F) << 12) | ((size & 0x0F) << 8) | ((vertical & 0x1F) << 3));
    }

    private static ushort BuildLipWord(int type, int color, int size, int vertical)
    {
        return (ushort)(((type & 0x1F) << 11) | ((color & 0x03) << 9) | ((size & 0x0F) << 5) | (vertical & 0x1F));
    }

    private static ushort BuildGlassesWord(int type, int color, int size, int vertical)
    {
        return (ushort)(((type & 0x0F) << 12) | ((color & 0x07) << 9) | ((size & 0x07) << 5) | (vertical & 0x1F));
    }

    private static ushort BuildFacialHairWord(int mustache, int beard, int color, int size, int vertical)
    {
        return (ushort)(((mustache & 0x03) << 14) | ((beard & 0x03) << 12) | ((color & 0x07) << 9) | ((size & 0x0F) << 5) | (vertical & 0x1F));
    }

    private static ushort BuildMoleWord(bool exists, int size, int vertical, int horizontal)
    {
        return (ushort)(((exists ? 1 : 0) << 15) | ((size & 0x0F) << 11) | ((vertical & 0x1F) << 6) | ((horizontal & 0x1F) << 1));
    }

    private static string ComputeSha256(byte[] bytes)
    {
        return Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
    }
}
