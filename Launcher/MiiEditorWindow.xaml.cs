using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media.Imaging;
using Microsoft.Win32;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher;

public partial class MiiEditorWindow : Window
{
    private readonly SaveManagerService _saveManagerService;
    private readonly LauncherSettings _settings;
    private readonly string _miiId;
    private MiiEditorState _resetState;
    private CancellationTokenSource? _autosaveCts;
    private bool _isLoading;
    private int _autosaveVersion;

    public MiiEditorWindow(SaveManagerService saveManagerService, LauncherSettings settings, string miiId)
    {
        _saveManagerService = saveManagerService;
        _settings = settings;
        _miiId = miiId;

        InitializeComponent();
        _resetState = _saveManagerService.LoadMiiEditorState(_miiId);

        Loaded += (_, _) =>
        {
            ApplyState(_resetState);
            QueueAutosave(renderImmediately: true);
        };
        Closing += (_, _) => _autosaveCts?.Cancel();
    }

    private void EditorControl_OnChanged(object sender, RoutedEventArgs e)
    {
        if (_isLoading || !IsLoaded)
        {
            return;
        }

        QueueAutosave(renderImmediately: false);
    }

    private void QueueAutosave(bool renderImmediately)
    {
        _autosaveCts?.Cancel();
        _autosaveCts = new CancellationTokenSource();
        var token = _autosaveCts.Token;
        var version = ++_autosaveVersion;

        var state = ReadState();
        PreviewNameTextBlock.Text = state.Name;
        PreviewMetaTextBlock.Text = $"{(state.IsFemale ? "Female" : "Male")}   Color {state.FavoriteColorIndex + 1}";
        RenderStatusTextBlock.Text = renderImmediately ? "Renderer starting..." : "Waiting for edits to settle...";
        AutosaveTextBlock.Text = "Autosave queued";

        _ = AutosaveAsync(state, version, renderImmediately ? TimeSpan.Zero : TimeSpan.FromMilliseconds(420), token);
    }

    private async Task AutosaveAsync(MiiEditorState state, int version, TimeSpan delay, CancellationToken cancellationToken)
    {
        try
        {
            if (delay > TimeSpan.Zero)
            {
                await Task.Delay(delay, cancellationToken);
            }

            await Dispatcher.InvokeAsync(() =>
            {
                PreviewPlaceholderTextBlock.Visibility = Visibility.Visible;
                RenderStatusTextBlock.Text = "Rendering live preview...";
                AutosaveTextBlock.Text = "Saving real Mii data...";
            });

            var profile = await _saveManagerService.UpdateMiiProfileAsync(_miiId, state, cancellationToken);
            if (version != _autosaveVersion)
            {
                return;
            }

            if (!string.IsNullOrWhiteSpace(_settings.UserFolderPath) && Directory.Exists(_settings.UserFolderPath))
            {
                try
                {
                    await _saveManagerService.SyncMiiToDolphinAsync(_settings, profile, cancellationToken);
                    await Dispatcher.InvokeAsync(() => AutosaveTextBlock.Text = "Autosaved and synced to Dolphin");
                }
                catch (Exception ex)
                {
                    await Dispatcher.InvokeAsync(() => AutosaveTextBlock.Text = $"Autosaved locally. Dolphin sync: {ex.Message}");
                }
            }
            else
            {
                await Dispatcher.InvokeAsync(() => AutosaveTextBlock.Text = "Autosaved locally");
            }

            await Dispatcher.InvokeAsync(() =>
            {
                SetPreviewImage(profile.AvatarImagePath);
                RenderStatusTextBlock.Text = profile.RenderStatusText;
                PreviewPlaceholderTextBlock.Visibility = profile.HasAvatarImage ? Visibility.Collapsed : Visibility.Visible;
            });
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            await Dispatcher.InvokeAsync(() =>
            {
                RenderStatusTextBlock.Text = ex.Message;
                AutosaveTextBlock.Text = "Autosave failed";
            });
        }
    }

    private MiiEditorState ReadState()
    {
        var previous = _resetState;
        return new MiiEditorState
        {
            Name = NameTextBox.Text.Trim(),
            CreatorName = CreatorTextBox.Text.Trim(),
            IsFemale = FemaleCheckBox.IsChecked == true,
            IsFavorite = FavoriteCheckBox.IsChecked == true,
            FavoriteColorIndex = GetSlider(FavoriteColorSlider),
            BirthMonth = previous.BirthMonth,
            BirthDay = previous.BirthDay,
            Height = GetSlider(HeightSlider),
            Weight = GetSlider(WeightSlider),
            MiiId = previous.MiiId,
            SystemId0 = previous.SystemId0,
            SystemId1 = previous.SystemId1,
            SystemId2 = previous.SystemId2,
            SystemId3 = previous.SystemId3,
            FaceShape = GetSlider(FaceShapeSlider),
            SkinColor = GetSlider(SkinColorSlider),
            FacialFeature = GetSlider(FacialFeatureSlider),
            HairType = GetSlider(HairTypeSlider),
            HairColor = GetSlider(HairColorSlider),
            HairFlipped = HairFlipCheckBox.IsChecked == true,
            EyebrowType = GetSlider(EyebrowTypeSlider),
            EyebrowRotation = GetSlider(EyebrowRotationSlider),
            EyebrowColor = GetSlider(EyebrowColorSlider),
            EyebrowSize = GetSlider(EyebrowSizeSlider),
            EyebrowVertical = GetSlider(EyebrowVerticalSlider),
            EyebrowSpacing = GetSlider(EyebrowSpacingSlider),
            EyeType = GetSlider(EyeTypeSlider),
            EyeRotation = GetSlider(EyeRotationSlider),
            EyeVertical = GetSlider(EyeVerticalSlider),
            EyeColor = GetSlider(EyeColorSlider),
            EyeSize = GetSlider(EyeSizeSlider),
            EyeSpacing = GetSlider(EyeSpacingSlider),
            NoseType = GetSlider(NoseTypeSlider),
            NoseSize = GetSlider(NoseSizeSlider),
            NoseVertical = GetSlider(NoseVerticalSlider),
            MouthType = GetSlider(MouthTypeSlider),
            MouthColor = GetSlider(MouthColorSlider),
            MouthSize = GetSlider(MouthSizeSlider),
            MouthVertical = GetSlider(MouthVerticalSlider),
            GlassesType = GetSlider(GlassesTypeSlider),
            GlassesColor = GetSlider(GlassesColorSlider),
            GlassesSize = GetSlider(GlassesSizeSlider),
            GlassesVertical = GetSlider(GlassesVerticalSlider),
            MustacheType = GetSlider(MustacheTypeSlider),
            BeardType = GetSlider(BeardTypeSlider),
            FacialHairColor = GetSlider(FacialHairColorSlider),
            MustacheSize = GetSlider(MustacheSizeSlider),
            MustacheVertical = GetSlider(MustacheVerticalSlider),
            MoleEnabled = MoleEnabledCheckBox.IsChecked == true,
            MoleSize = GetSlider(MoleSizeSlider),
            MoleVertical = GetSlider(MoleVerticalSlider),
            MoleHorizontal = GetSlider(MoleHorizontalSlider)
        };
    }

    private void ApplyState(MiiEditorState state)
    {
        _isLoading = true;
        try
        {
            NameTextBox.Text = state.Name;
            CreatorTextBox.Text = state.CreatorName;
            FemaleCheckBox.IsChecked = state.IsFemale;
            FavoriteCheckBox.IsChecked = state.IsFavorite;
            SetSlider(FavoriteColorSlider, state.FavoriteColorIndex);
            SetSlider(HeightSlider, state.Height);
            SetSlider(WeightSlider, state.Weight);
            SetSlider(FaceShapeSlider, state.FaceShape);
            SetSlider(SkinColorSlider, state.SkinColor);
            SetSlider(FacialFeatureSlider, state.FacialFeature);
            SetSlider(HairTypeSlider, state.HairType);
            SetSlider(HairColorSlider, state.HairColor);
            HairFlipCheckBox.IsChecked = state.HairFlipped;
            SetSlider(EyebrowTypeSlider, state.EyebrowType);
            SetSlider(EyebrowRotationSlider, state.EyebrowRotation);
            SetSlider(EyebrowColorSlider, state.EyebrowColor);
            SetSlider(EyebrowSizeSlider, state.EyebrowSize);
            SetSlider(EyebrowVerticalSlider, state.EyebrowVertical);
            SetSlider(EyebrowSpacingSlider, state.EyebrowSpacing);
            SetSlider(EyeTypeSlider, state.EyeType);
            SetSlider(EyeRotationSlider, state.EyeRotation);
            SetSlider(EyeVerticalSlider, state.EyeVertical);
            SetSlider(EyeColorSlider, state.EyeColor);
            SetSlider(EyeSizeSlider, state.EyeSize);
            SetSlider(EyeSpacingSlider, state.EyeSpacing);
            SetSlider(NoseTypeSlider, state.NoseType);
            SetSlider(NoseSizeSlider, state.NoseSize);
            SetSlider(NoseVerticalSlider, state.NoseVertical);
            SetSlider(MouthTypeSlider, state.MouthType);
            SetSlider(MouthColorSlider, state.MouthColor);
            SetSlider(MouthSizeSlider, state.MouthSize);
            SetSlider(MouthVerticalSlider, state.MouthVertical);
            SetSlider(GlassesTypeSlider, state.GlassesType);
            SetSlider(GlassesColorSlider, state.GlassesColor);
            SetSlider(GlassesSizeSlider, state.GlassesSize);
            SetSlider(GlassesVerticalSlider, state.GlassesVertical);
            SetSlider(MustacheTypeSlider, state.MustacheType);
            SetSlider(BeardTypeSlider, state.BeardType);
            SetSlider(FacialHairColorSlider, state.FacialHairColor);
            SetSlider(MustacheSizeSlider, state.MustacheSize);
            SetSlider(MustacheVerticalSlider, state.MustacheVertical);
            MoleEnabledCheckBox.IsChecked = state.MoleEnabled;
            SetSlider(MoleSizeSlider, state.MoleSize);
            SetSlider(MoleVerticalSlider, state.MoleVertical);
            SetSlider(MoleHorizontalSlider, state.MoleHorizontal);
            PreviewNameTextBlock.Text = state.Name;
        }
        finally
        {
            _isLoading = false;
        }
    }

    private void RandomizeButton_OnClick(object sender, RoutedEventArgs e)
    {
        var current = ReadState();
        var randomized = _saveManagerService.CreateRandomMiiState(string.IsNullOrWhiteSpace(current.Name) ? "Vanza Mii" : current.Name);
        randomized.MiiId = current.MiiId;
        randomized.SystemId0 = current.SystemId0;
        randomized.SystemId1 = current.SystemId1;
        randomized.SystemId2 = current.SystemId2;
        randomized.SystemId3 = current.SystemId3;
        randomized.BirthMonth = current.BirthMonth;
        randomized.BirthDay = current.BirthDay;
        ApplyState(randomized);
        QueueAutosave(renderImmediately: false);
    }

    private void ResetButton_OnClick(object sender, RoutedEventArgs e)
    {
        ApplyState(_resetState.Clone());
        QueueAutosave(renderImmediately: false);
    }

    private async void ExportButton_OnClick(object sender, RoutedEventArgs e)
    {
        var dialog = new SaveFileDialog
        {
            Filter = "Wii Mii (*.mii)|*.mii|VanzaKart Mii profile (*.vk-mii)|*.vk-mii|JSON profile (*.json)|*.json",
            FileName = $"{SanitizeFileName(NameTextBox.Text)}.mii"
        };

        if (dialog.ShowDialog(this) != true)
        {
            return;
        }

        try
        {
            await _saveManagerService.ExportMiiProfileAsync(_miiId, dialog.FileName);
            AutosaveTextBlock.Text = $"Exported: {dialog.FileName}";
        }
        catch (Exception ex)
        {
            AutosaveTextBlock.Text = $"Export failed: {ex.Message}";
        }
    }

    private void CloseButton_OnClick(object sender, RoutedEventArgs e)
    {
        Close();
    }

    private static int GetSlider(Slider slider)
    {
        return (int)Math.Round(slider.Value);
    }

    private static void SetSlider(Slider slider, int value)
    {
        slider.Value = Math.Clamp(value, (int)slider.Minimum, (int)slider.Maximum);
    }

    private void SetPreviewImage(string path)
    {
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path))
        {
            PreviewImage.Source = null;
            return;
        }

        var bitmap = new BitmapImage();
        bitmap.BeginInit();
        bitmap.CacheOption = BitmapCacheOption.OnLoad;
        bitmap.UriSource = new Uri(path, UriKind.Absolute);
        bitmap.EndInit();
        bitmap.Freeze();
        PreviewImage.Source = bitmap;
    }

    private static string SanitizeFileName(string value)
    {
        var safe = string.IsNullOrWhiteSpace(value) ? "Mii" : value.Trim();
        foreach (var invalid in Path.GetInvalidFileNameChars())
        {
            safe = safe.Replace(invalid, '_');
        }

        return safe;
    }
}
