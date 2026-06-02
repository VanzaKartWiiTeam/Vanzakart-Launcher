using System.IO;
using System.Text.Json;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class SaveManagerService
{
    private const string MarioKartSaveName = "rksys.dat";
    private readonly MiiFileParserService _miiFileParserService = new();
    private readonly MiiAvatarRenderService _miiAvatarRenderService = new();
    private readonly DolphinPathResolverService _dolphinPathResolverService = new();
    private readonly MkwiiSaveParserService _mkwiiSaveParserService;

    public SaveManagerService()
    {
        _mkwiiSaveParserService = new MkwiiSaveParserService(_miiFileParserService, _miiAvatarRenderService);
    }

    public string GetWiiRoot(LauncherSettings settings)
    {
        return string.IsNullOrWhiteSpace(settings.UserFolderPath)
            ? string.Empty
            : Path.Combine(settings.UserFolderPath, "Wii");
    }

    public string GetMiiDatabasePath(LauncherSettings settings)
    {
        return _mkwiiSaveParserService.GetMiiDatabasePath(settings.UserFolderPath);
    }

    public string TryAutoDetectUserFolder(LauncherSettings settings)
    {
        return _dolphinPathResolverService.TryFindUserFolderPath(settings.DolphinPath);
    }

    public IReadOnlyList<string> FindDolphinUserFolderCandidates(LauncherSettings settings)
    {
        return _dolphinPathResolverService.FindUserFolderCandidates(settings.DolphinPath);
    }

    public string GetBackupFolder()
    {
        return Path.Combine(AppContext.BaseDirectory, "Backups", "Licenses");
    }

    public string GetLauncherProfilesFolder()
    {
        return Path.Combine(AppContext.BaseDirectory, "Profiles");
    }

    public string GetLauncherMiisFolder()
    {
        return Path.Combine(GetLauncherProfilesFolder(), "Miis");
    }

    public string GetMiiImportsFolder()
    {
        return Path.Combine(GetLauncherProfilesFolder(), "MiiImports");
    }

    public string GetActiveMiiPath()
    {
        return Path.Combine(GetLauncherProfilesFolder(), "active_mii.txt");
    }

    public string GetLauncherMiiProfilePath()
    {
        return Path.Combine(GetLauncherProfilesFolder(), "mii_profile.json");
    }

    public IReadOnlyList<SaveProfileInfo> GetSaveProfiles(LauncherSettings settings)
    {
        if (string.IsNullOrWhiteSpace(settings.UserFolderPath) || !Directory.Exists(settings.UserFolderPath))
        {
            return Array.Empty<SaveProfileInfo>();
        }

        try
        {
            var miiDatabase = _mkwiiSaveParserService.ReadMiiDatabase(settings.UserFolderPath);
            var cards = new List<SaveProfileInfo>();
            foreach (var saveFile in _mkwiiSaveParserService.FindVanzaKartSaveFiles(settings))
            {
                var parsedCards = _mkwiiSaveParserService.ReadLicenseCards(saveFile, miiDatabase);
                if (parsedCards.Count > 0)
                {
                    cards.AddRange(parsedCards);
                    continue;
                }

                var info = new FileInfo(saveFile);
                cards.Add(new SaveProfileInfo
                {
                    DisplayName = BuildProfileName(saveFile, settings.UserFolderPath),
                    Subtitle = $"Modified {info.LastWriteTime:g}",
                    FilePath = saveFile,
                    SourceLabel = "Dolphin save",
                    MiiName = "No valid license slots",
                    AvatarInitial = "M",
                    AccentColor = "#39E7FF",
                    LastModifiedUtc = info.LastWriteTimeUtc,
                    SizeBytes = info.Length,
                    IsLauncherManaged = false
                });
            }

            return cards
                .OrderByDescending(profile => profile.LastModifiedUtc)
                .ToArray();
        }
        catch
        {
            return Array.Empty<SaveProfileInfo>();
        }
    }

    public SaveProfileInfo? GetPrimarySaveProfile(LauncherSettings settings)
    {
        return GetSaveProfiles(settings).FirstOrDefault();
    }

    public LauncherMiiProfile LoadMiiProfile()
    {
        var active = LoadActiveMiiProfile();
        if (active != null)
        {
            return active;
        }

        try
        {
            var path = GetLauncherMiiProfilePath();
            if (!File.Exists(path))
            {
                return new LauncherMiiProfile();
            }

            var json = File.ReadAllText(path);
            return JsonSerializer.Deserialize<LauncherMiiProfile>(json) ?? new LauncherMiiProfile();
        }
        catch
        {
            return new LauncherMiiProfile();
        }
    }

    public IReadOnlyList<LauncherMiiProfile> LoadMiiProfiles()
    {
        var profiles = new List<LauncherMiiProfile>();
        var folder = GetLauncherMiisFolder();

        if (Directory.Exists(folder))
        {
            foreach (var file in Directory.EnumerateFiles(folder, "*.json", SearchOption.TopDirectoryOnly))
            {
                try
                {
                    var json = File.ReadAllText(file);
                    var profile = JsonSerializer.Deserialize<LauncherMiiProfile>(json);
                    if (profile != null)
                    {
                        profiles.Add(EnsureMiiDefaults(profile));
                    }
                }
                catch
                {
                }
            }
        }

        if (profiles.Count == 0)
        {
            var legacy = LoadLegacyMiiProfile();
            if (legacy is { IsRealMii: true })
            {
                profiles.Add(legacy);
                SaveMiiProfile(legacy);
                SetActiveMii(legacy.Id);
            }
        }

        return profiles
            .OrderByDescending(profile => profile.IsFavorite)
            .ThenBy(profile => profile.Name)
            .ToArray();
    }

    public LauncherMiiProfile? LoadActiveMiiProfile()
    {
        try
        {
            var activePath = GetActiveMiiPath();
            if (!File.Exists(activePath))
            {
                return null;
            }

            var activeId = File.ReadAllText(activePath).Trim();
            return LoadMiiProfiles().FirstOrDefault(profile => profile.Id == activeId);
        }
        catch
        {
            return null;
        }
    }

    public async Task<LauncherMiiProfile> CreateMiiProfileAsync(string name, string favoriteColor, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Choose a Mii name.", nameof(name));
        }

        var state = new MiiEditorState
        {
            Name = name,
            CreatorName = "VanzaKart",
            FavoriteColorIndex = ResolveFavoriteColorIndex(favoriteColor),
            IsFemale = false,
            IsFavorite = LoadMiiProfiles().Count == 0
        };

        return await CreateMiiProfileAsync(state, cancellationToken);
    }

    public async Task<LauncherMiiProfile> CreateMiiProfileAsync(MiiEditorState state, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(state.Name))
        {
            throw new ArgumentException("Choose a Mii name.", nameof(state));
        }

        var miiData = _miiFileParserService.CreateMii(state);
        var avatar = await _miiAvatarRenderService.EnsureAvatarRenderAsync(miiData, cancellationToken);
        var profile = new LauncherMiiProfile
        {
            Id = Guid.NewGuid().ToString("N"),
            Name = miiData.Name,
            FavoriteColor = miiData.FavoriteColor,
            CreatedUtc = DateTime.UtcNow,
            SourceLabel = "Wii Mii",
            RawMiiBase64 = miiData.RawMiiBase64,
            StudioData = miiData.StudioData,
            AvatarImagePath = avatar.AvatarPath,
            RenderState = avatar.State.ToString(),
            RenderMessage = avatar.Message,
            LastRenderedUtc = avatar.IsReady ? avatar.UpdatedUtc : null,
            CreatorName = miiData.CreatorName,
            MiiId = miiData.MiiId,
            FavoriteColorIndex = miiData.FavoriteColorIndex,
            IsFemale = miiData.IsFemale,
            IsFavorite = state.IsFavorite || LoadMiiProfiles().Count == 0
        };

        await SaveMiiProfileAsync(profile, cancellationToken);
        SetActiveMii(profile.Id);
        return profile;
    }

    public async Task<LauncherMiiProfile> ImportMiiProfileAsync(string sourceFile, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(sourceFile))
        {
            throw new FileNotFoundException("The selected Mii file does not exist.", sourceFile);
        }

        var extension = Path.GetExtension(sourceFile).ToLowerInvariant();
        MiiFileMetadata? metadata = null;
        LauncherMiiProfile? imported = null;

        if (extension is ".json" or ".vk-mii")
        {
            try
            {
                var json = await File.ReadAllTextAsync(sourceFile, cancellationToken);
                imported = JsonSerializer.Deserialize<LauncherMiiProfile>(json);
            }
            catch
            {
            }
        }
        else
        {
            metadata = await _miiFileParserService.ReadMetadataAsync(sourceFile, cancellationToken);
        }

        if (imported != null && !string.IsNullOrWhiteSpace(imported.RawMiiBase64))
        {
            var raw = Convert.FromBase64String(imported.RawMiiBase64);
            var parsed = _miiFileParserService.ParseWiiMiiBlock(raw, imported.SourceLabel, sourceFile);
            metadata = new MiiFileMetadata
            {
                FormatName = parsed.FormatName,
                SuggestedName = parsed.Name,
                SizeBytes = raw.Length,
                Sha256 = parsed.Sha256,
                RawMiiBase64 = parsed.RawMiiBase64,
                StudioData = parsed.StudioData,
                CreatorName = parsed.CreatorName,
                FavoriteColor = parsed.FavoriteColor,
                FavoriteColorIndex = parsed.FavoriteColorIndex,
                MiiId = parsed.MiiId,
                IsFemale = parsed.IsFemale,
                IsFavorite = parsed.IsFavorite
            };
        }

        if (metadata is not { IsRealMii: true })
        {
            throw new InvalidDataException("The selected file does not contain real Wii Mii data.");
        }

        metadata = EnsureImportedMiiIdentity(metadata);

        Directory.CreateDirectory(GetMiiImportsFolder());
        var importCopy = Path.Combine(GetMiiImportsFolder(), $"{Path.GetFileNameWithoutExtension(sourceFile)}_{DateTime.Now:yyyyMMddHHmmss}{extension}");
        File.Copy(sourceFile, importCopy, overwrite: true);

        var profile = EnsureMiiDefaults(imported ?? new LauncherMiiProfile());
        profile.Id = Guid.NewGuid().ToString("N");
        profile.Name = metadata.SuggestedName;
        profile.SourceLabel = metadata.FormatName;
        profile.ImportedFilePath = importCopy;
        profile.RawMiiBase64 = metadata.RawMiiBase64;
        profile.StudioData = metadata.StudioData;
        profile.CreatorName = metadata.CreatorName;
        profile.FavoriteColor = metadata.FavoriteColor;
        profile.FavoriteColorIndex = metadata.FavoriteColorIndex;
        profile.MiiId = metadata.MiiId;
        profile.IsFemale = metadata.IsFemale;
        var renderResult = await _miiAvatarRenderService.EnsureAvatarRenderAsync(
            _miiFileParserService.ParseWiiMiiBlock(Convert.FromBase64String(metadata.RawMiiBase64), metadata.FormatName, importCopy),
            cancellationToken);
        profile.AvatarImagePath = renderResult.AvatarPath;
        profile.RenderState = renderResult.State.ToString();
        profile.RenderMessage = renderResult.Message;
        profile.LastRenderedUtc = renderResult.IsReady ? renderResult.UpdatedUtc : null;
        profile.CreatedUtc = DateTime.UtcNow;

        await SaveMiiProfileAsync(profile, cancellationToken);
        SetActiveMii(profile.Id);
        return profile;
    }

    public async Task ExportMiiProfileAsync(string miiId, string destinationFile, CancellationToken cancellationToken = default)
    {
        var profile = LoadMiiProfiles().FirstOrDefault(item => item.Id == miiId)
            ?? throw new InvalidOperationException("Select a Mii to export.");

        var directory = Path.GetDirectoryName(destinationFile);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var extension = Path.GetExtension(destinationFile).ToLowerInvariant();
        if (extension is ".mii" or ".rcd" or ".rsd")
        {
            if (string.IsNullOrWhiteSpace(profile.RawMiiBase64))
            {
                throw new InvalidOperationException("Selected Mii does not contain real Wii Mii data.");
            }

            await File.WriteAllBytesAsync(destinationFile, Convert.FromBase64String(profile.RawMiiBase64), cancellationToken);
            return;
        }

        var json = JsonSerializer.Serialize(profile, new JsonSerializerOptions { WriteIndented = true });
        await File.WriteAllTextAsync(destinationFile, json, cancellationToken);
    }

    public async Task<LauncherMiiProfile> DuplicateMiiProfileAsync(string miiId, CancellationToken cancellationToken = default)
    {
        var source = LoadMiiProfiles().FirstOrDefault(item => item.Id == miiId)
            ?? throw new InvalidOperationException("Select a Mii to duplicate.");

        if (string.IsNullOrWhiteSpace(source.RawMiiBase64))
        {
            throw new InvalidOperationException("Selected Mii does not contain real Wii Mii data.");
        }

        var state = _miiFileParserService.ReadEditorState(Convert.FromBase64String(source.RawMiiBase64));
        state.Name = NormalizeMiiNameForDuplicate(source.Name);
        state.MiiId = 0;
        state.SystemId0 = 0;
        state.SystemId1 = 0;
        state.SystemId2 = 0;
        state.SystemId3 = 0;
        state.IsFavorite = false;

        var duplicatedMii = _miiFileParserService.CreateMii(state, "Real duplicate");
        var renderResult = await _miiAvatarRenderService.EnsureAvatarRenderAsync(duplicatedMii, cancellationToken);
        var duplicate = new LauncherMiiProfile
        {
            Id = Guid.NewGuid().ToString("N"),
            Name = duplicatedMii.Name,
            FavoriteColor = duplicatedMii.FavoriteColor,
            CreatedUtc = DateTime.UtcNow,
            SourceLabel = "Real duplicate",
            ImportedFilePath = source.ImportedFilePath,
            RawMiiBase64 = duplicatedMii.RawMiiBase64,
            StudioData = duplicatedMii.StudioData,
            AvatarImagePath = renderResult.AvatarPath,
            RenderState = renderResult.State.ToString(),
            RenderMessage = renderResult.Message,
            LastRenderedUtc = renderResult.IsReady ? renderResult.UpdatedUtc : null,
            CreatorName = duplicatedMii.CreatorName,
            MiiId = duplicatedMii.MiiId,
            FavoriteColorIndex = duplicatedMii.FavoriteColorIndex,
            IsFemale = duplicatedMii.IsFemale,
            IsFavorite = false
        };

        await SaveMiiProfileAsync(duplicate, cancellationToken);
        SetActiveMii(duplicate.Id);
        return duplicate;
    }

    public void DeleteMiiProfile(string miiId)
    {
        var path = GetMiiProfilePath(miiId);
        if (File.Exists(path))
        {
            File.Delete(path);
        }

        var next = LoadMiiProfiles().FirstOrDefault();
        if (next != null)
        {
            SetActiveMii(next.Id);
        }
        else if (File.Exists(GetActiveMiiPath()))
        {
            File.Delete(GetActiveMiiPath());
        }
    }

    public void SetActiveMii(string miiId)
    {
        Directory.CreateDirectory(GetLauncherProfilesFolder());
        File.WriteAllText(GetActiveMiiPath(), miiId);
    }

    public MiiEditorState LoadMiiEditorState(string miiId)
    {
        var profile = LoadMiiProfiles().FirstOrDefault(item => item.Id == miiId)
            ?? throw new InvalidOperationException("Select a Mii to edit.");

        if (string.IsNullOrWhiteSpace(profile.RawMiiBase64))
        {
            throw new InvalidOperationException("Selected profile does not contain real Wii Mii data.");
        }

        return _miiFileParserService.ReadEditorState(Convert.FromBase64String(profile.RawMiiBase64));
    }

    public async Task<LauncherMiiProfile> UpdateMiiProfileAsync(string miiId, MiiEditorState state, CancellationToken cancellationToken = default)
    {
        var profile = LoadMiiProfiles().FirstOrDefault(item => item.Id == miiId)
            ?? throw new InvalidOperationException("Select a Mii to edit.");

        var miiData = _miiFileParserService.CreateMii(state, "Edited Wii Mii");
        var renderResult = await _miiAvatarRenderService.EnsureAvatarRenderAsync(miiData, cancellationToken);

        profile.Name = miiData.Name;
        profile.SourceLabel = "Edited Wii Mii";
        profile.RawMiiBase64 = miiData.RawMiiBase64;
        profile.StudioData = miiData.StudioData;
        profile.CreatorName = miiData.CreatorName;
        profile.FavoriteColor = miiData.FavoriteColor;
        profile.FavoriteColorIndex = miiData.FavoriteColorIndex;
        profile.MiiId = miiData.MiiId;
        profile.IsFemale = miiData.IsFemale;
        profile.IsFavorite = state.IsFavorite;
        profile.AvatarImagePath = renderResult.AvatarPath;
        profile.RenderState = renderResult.State.ToString();
        profile.RenderMessage = renderResult.Message;
        profile.LastRenderedUtc = renderResult.IsReady ? renderResult.UpdatedUtc : profile.LastRenderedUtc;

        await SaveMiiProfileAsync(profile, cancellationToken);
        SetActiveMii(profile.Id);
        return profile;
    }

    public async Task<bool> EnsureLauncherMiiAvatarCacheAsync(CancellationToken cancellationToken = default)
    {
        var changed = false;
        foreach (var profile in LoadMiiProfiles())
        {
            if (string.IsNullOrWhiteSpace(profile.RawMiiBase64) || !string.IsNullOrWhiteSpace(profile.AvatarImagePath))
            {
                continue;
            }

            try
            {
                var mii = _miiFileParserService.ParseWiiMiiBlock(Convert.FromBase64String(profile.RawMiiBase64), profile.SourceLabel);
                var render = await _miiAvatarRenderService.EnsureAvatarRenderAsync(mii, cancellationToken);
                profile.AvatarImagePath = render.AvatarPath;
                profile.RenderState = render.State.ToString();
                profile.RenderMessage = render.Message;
                profile.LastRenderedUtc = render.IsReady ? render.UpdatedUtc : profile.LastRenderedUtc;
                await SaveMiiProfileAsync(profile, cancellationToken);
                changed = true;
            }
            catch
            {
            }
        }

        return changed;
    }

    public MiiEditorState CreateRandomMiiState(string baseName = "Vanza Mii")
    {
        var random = Random.Shared;
        return new MiiEditorState
        {
            Name = baseName,
            CreatorName = "VanzaKart",
            IsFemale = random.Next(0, 2) == 0,
            IsFavorite = true,
            FavoriteColorIndex = random.Next(0, 12),
            Height = random.Next(32, 96),
            Weight = random.Next(32, 96),
            FaceShape = random.Next(0, 8),
            SkinColor = random.Next(0, 6),
            FacialFeature = random.Next(0, 12),
            HairType = random.Next(0, 72),
            HairColor = random.Next(0, 8),
            HairFlipped = random.Next(0, 2) == 0,
            EyebrowType = random.Next(0, 24),
            EyebrowRotation = random.Next(0, 12),
            EyebrowColor = random.Next(0, 8),
            EyebrowSize = random.Next(2, 9),
            EyebrowVertical = random.Next(4, 18),
            EyebrowSpacing = random.Next(0, 8),
            EyeType = random.Next(0, 48),
            EyeRotation = random.Next(0, 8),
            EyeVertical = random.Next(6, 20),
            EyeColor = random.Next(0, 6),
            EyeSize = random.Next(2, 7),
            EyeSpacing = random.Next(0, 8),
            NoseType = random.Next(0, 12),
            NoseSize = random.Next(2, 9),
            NoseVertical = random.Next(6, 18),
            MouthType = random.Next(0, 24),
            MouthColor = random.Next(0, 4),
            MouthSize = random.Next(2, 9),
            MouthVertical = random.Next(8, 20),
            GlassesType = random.Next(0, 8),
            GlassesColor = random.Next(0, 6),
            GlassesSize = random.Next(2, 7),
            GlassesVertical = random.Next(6, 18),
            MustacheType = random.Next(0, 4),
            BeardType = random.Next(0, 4),
            FacialHairColor = random.Next(0, 8),
            MustacheSize = random.Next(2, 9),
            MustacheVertical = random.Next(6, 18),
            MoleEnabled = random.Next(0, 3) == 0,
            MoleSize = random.Next(2, 9),
            MoleVertical = random.Next(4, 20),
            MoleHorizontal = random.Next(4, 20)
        };
    }

    private async Task SaveMiiProfileAsync(LauncherMiiProfile profile, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(GetLauncherMiisFolder());
        var json = JsonSerializer.Serialize(EnsureMiiDefaults(profile), new JsonSerializerOptions { WriteIndented = true });
        await File.WriteAllTextAsync(GetMiiProfilePath(profile.Id), json, cancellationToken);

        Directory.CreateDirectory(GetLauncherProfilesFolder());
        var legacyJson = JsonSerializer.Serialize(EnsureMiiDefaults(profile), new JsonSerializerOptions { WriteIndented = true });
        await File.WriteAllTextAsync(GetLauncherMiiProfilePath(), legacyJson, cancellationToken);
    }

    private void SaveMiiProfile(LauncherMiiProfile profile)
    {
        Directory.CreateDirectory(GetLauncherMiisFolder());
        var json = JsonSerializer.Serialize(EnsureMiiDefaults(profile), new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(GetMiiProfilePath(profile.Id), json);
    }

    private string GetMiiProfilePath(string miiId)
    {
        return Path.Combine(GetLauncherMiisFolder(), $"{miiId}.json");
    }

    private LauncherMiiProfile? LoadLegacyMiiProfile()
    {
        try
        {
            var path = GetLauncherMiiProfilePath();
            if (!File.Exists(path))
            {
                return null;
            }

            var json = File.ReadAllText(path);
            var profile = JsonSerializer.Deserialize<LauncherMiiProfile>(json);
            return profile == null ? null : EnsureMiiDefaults(profile);
        }
        catch
        {
            return null;
        }
    }

    private static LauncherMiiProfile EnsureMiiDefaults(LauncherMiiProfile profile)
    {
        if (string.IsNullOrWhiteSpace(profile.Id))
        {
            profile.Id = Guid.NewGuid().ToString("N");
        }

        if (string.IsNullOrWhiteSpace(profile.Name))
        {
            profile.Name = "Vanza Mii";
        }

        if (!string.IsNullOrWhiteSpace(profile.RawMiiBase64))
        {
            try
            {
                var parser = new MiiFileParserService();
                var parsed = parser.ParseWiiMiiBlock(Convert.FromBase64String(profile.RawMiiBase64), profile.SourceLabel);
                var renderer = new MiiAvatarRenderService();
                profile.Name = parsed.Name;
                profile.CreatorName = parsed.CreatorName;
                profile.StudioData = parsed.StudioData;
                profile.MiiId = parsed.MiiId;
                profile.FavoriteColor = parsed.FavoriteColor;
                profile.FavoriteColorIndex = parsed.FavoriteColorIndex;
                profile.IsFemale = parsed.IsFemale;
                if (!string.IsNullOrWhiteSpace(profile.AvatarImagePath) && !File.Exists(profile.AvatarImagePath))
                {
                    profile.AvatarImagePath = string.Empty;
                }

                if (string.IsNullOrWhiteSpace(profile.AvatarImagePath))
                {
                    profile.AvatarImagePath = renderer.TryGetCachedAvatar(parsed);
                }
            }
            catch
            {
            }
        }

        profile.FavoriteColor = NormalizeColor(profile.FavoriteColor);
        if (profile.CreatedUtc == default)
        {
            profile.CreatedUtc = DateTime.UtcNow;
        }

        if (string.IsNullOrWhiteSpace(profile.SourceLabel))
        {
            profile.SourceLabel = "Launcher";
        }

        if (string.IsNullOrWhiteSpace(profile.RenderState))
        {
            profile.RenderState = string.IsNullOrWhiteSpace(profile.AvatarImagePath) ? "Queued" : "Ready";
        }

        if (string.IsNullOrWhiteSpace(profile.RenderMessage))
        {
            profile.RenderMessage = string.IsNullOrWhiteSpace(profile.AvatarImagePath)
                ? "Render queued"
                : "Rendered";
        }

        return profile;
    }

    public async Task SyncMiiToDolphinAsync(LauncherSettings settings, LauncherMiiProfile profile, CancellationToken cancellationToken = default)
    {
        await _mkwiiSaveParserService.AddOrUpdateMiiInDatabaseAsync(settings.UserFolderPath, profile, cancellationToken);
    }

    public Task<bool> EnsureDolphinMiiAvatarCacheAsync(LauncherSettings settings, CancellationToken cancellationToken = default)
    {
        return _mkwiiSaveParserService.EnsureMiiDatabaseAvatarCacheAsync(settings.UserFolderPath, cancellationToken);
    }

    public int GetBackupCount()
    {
        var folder = GetBackupFolder();
        if (!Directory.Exists(folder))
        {
            return 0;
        }

        return Directory.EnumerateFiles(folder, "*.dat", SearchOption.TopDirectoryOnly).Count();
    }

    public string GetLatestBackupLabel()
    {
        var folder = GetBackupFolder();
        if (!Directory.Exists(folder))
        {
            return "No backups yet";
        }

        var latest = Directory.EnumerateFiles(folder, "*.dat", SearchOption.TopDirectoryOnly)
            .Select(path => new FileInfo(path))
            .OrderByDescending(file => file.LastWriteTimeUtc)
            .FirstOrDefault();

        return latest == null
            ? "No backups yet"
            : latest.LastWriteTime.ToString("g");
    }

    public IReadOnlyList<SaveBackupInfo> GetBackups()
    {
        var folder = GetBackupFolder();
        if (!Directory.Exists(folder))
        {
            return Array.Empty<SaveBackupInfo>();
        }

        return Directory.EnumerateFiles(folder, "*.dat", SearchOption.TopDirectoryOnly)
            .Select(path =>
            {
                var info = new FileInfo(path);
                return new SaveBackupInfo
                {
                    DisplayName = Path.GetFileNameWithoutExtension(path),
                    FilePath = path,
                    CreatedUtc = info.LastWriteTimeUtc,
                    SizeBytes = info.Length
                };
            })
            .OrderByDescending(backup => backup.CreatedUtc)
            .ToArray();
    }

    public async Task<string> BackupPrimarySaveAsync(LauncherSettings settings, CancellationToken cancellationToken = default)
    {
        var profile = GetPrimarySaveProfile(settings)
            ?? throw new InvalidOperationException("No Mario Kart Wii save was found in the selected Dolphin user folder.");

        var backupFolder = GetBackupFolder();
        Directory.CreateDirectory(backupFolder);

        var fileName = $"rksys_{DateTime.Now:yyyyMMdd_HHmmss}.dat";
        var destination = Path.Combine(backupFolder, fileName);

        await CopyFileAsync(profile.FilePath, destination, cancellationToken);
        return destination;
    }

    public async Task ImportSaveFileAsync(LauncherSettings settings, string sourceFile, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(sourceFile))
        {
            throw new FileNotFoundException("The selected save file does not exist.", sourceFile);
        }

        var profile = GetPrimarySaveProfile(settings)
            ?? throw new InvalidOperationException("Create or launch a Mario Kart Wii save in Dolphin before importing over it.");

        await BackupPrimarySaveAsync(settings, cancellationToken);
        await CopyFileAsync(sourceFile, profile.FilePath, cancellationToken);
    }

    public async Task RestoreBackupAsync(LauncherSettings settings, string backupFile, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(backupFile))
        {
            throw new FileNotFoundException("The selected backup does not exist.", backupFile);
        }

        var profile = GetPrimarySaveProfile(settings)
            ?? throw new InvalidOperationException("Create or launch a Mario Kart Wii save in Dolphin before restoring a backup.");

        await BackupPrimarySaveAsync(settings, cancellationToken);
        await CopyFileAsync(backupFile, profile.FilePath, cancellationToken);
    }

    public async Task ExportPrimarySaveAsync(LauncherSettings settings, string destinationFile, CancellationToken cancellationToken = default)
    {
        var profile = GetPrimarySaveProfile(settings)
            ?? throw new InvalidOperationException("No Mario Kart Wii save was found to export.");

        await CopyFileAsync(profile.FilePath, destinationFile, cancellationToken);
    }

    private static async Task CopyFileAsync(string source, string destination, CancellationToken cancellationToken)
    {
        var directory = Path.GetDirectoryName(destination);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        await using var sourceStream = new FileStream(source, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        await using var destinationStream = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None, 81920, true);
        await sourceStream.CopyToAsync(destinationStream, cancellationToken);
    }

    private static string BuildProfileName(string savePath, string wiiRoot)
    {
        var relative = Path.GetRelativePath(wiiRoot, savePath);
        var segments = relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        return segments.Length >= 3 ? $"License {segments[^3]}" : "Mario Kart Wii License";
    }

    private static string BuildInitial(string value)
    {
        return string.IsNullOrWhiteSpace(value)
            ? "M"
            : value.Trim()[0].ToString().ToUpperInvariant();
    }

    private static string NormalizeColor(string value)
    {
        return value.StartsWith('#') && value.Length == 7
            ? value
            : "#39E7FF";
    }

    private MiiFileMetadata EnsureImportedMiiIdentity(MiiFileMetadata metadata)
    {
        if (metadata.MiiId != 0)
        {
            return metadata;
        }

        var raw = Convert.FromBase64String(metadata.RawMiiBase64);
        var state = _miiFileParserService.ReadEditorState(raw);
        state.MiiId = 0;
        var regenerated = _miiFileParserService.CreateMii(state, metadata.FormatName);
        return new MiiFileMetadata
        {
            FormatName = metadata.FormatName,
            SuggestedName = regenerated.Name,
            SizeBytes = metadata.SizeBytes,
            Sha256 = regenerated.Sha256,
            RawMiiBase64 = regenerated.RawMiiBase64,
            StudioData = regenerated.StudioData,
            CreatorName = regenerated.CreatorName,
            FavoriteColor = regenerated.FavoriteColor,
            FavoriteColorIndex = regenerated.FavoriteColorIndex,
            MiiId = regenerated.MiiId,
            IsFemale = regenerated.IsFemale,
            IsFavorite = regenerated.IsFavorite
        };
    }

    private static string NormalizeMiiNameForDuplicate(string value)
    {
        var baseName = string.IsNullOrWhiteSpace(value) ? "Mii" : value.Trim();
        const string suffix = " 2";
        if (baseName.Length + suffix.Length <= 10)
        {
            return baseName + suffix;
        }

        return baseName[..Math.Max(1, 10 - suffix.Length)] + suffix;
    }

    private static int ResolveFavoriteColorIndex(string value)
    {
        var normalized = NormalizeColor(value);
        var palette = new[]
        {
            "#FF3B3B",
            "#FF8A2A",
            "#FFD166",
            "#9CFF5E",
            "#39E7FF",
            "#3B82F6",
            "#A855F7",
            "#FF5CAB",
            "#8EE7FF",
            "#6CF0A6",
            "#F7FAFF",
            "#111827"
        };

        var index = Array.FindIndex(palette, color => color.Equals(normalized, StringComparison.OrdinalIgnoreCase));
        return index >= 0 ? index : 4;
    }
}
