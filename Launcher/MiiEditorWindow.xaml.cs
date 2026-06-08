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
    private const int OptionsPerPage = 6;
    private static readonly SemaphoreSlim FeaturePreviewRenderGate = new(3, 3);
    private readonly SaveManagerService _saveManagerService;
    private readonly MiiFileParserService _miiParser = new();
    private readonly MiiAvatarRenderService _avatarRenderer = new();
    private readonly LauncherSettings _settings;
    private readonly string _miiId;
    private readonly List<CategoryDefinition> _categories = [];
    private readonly List<FeatureOption> _featureOptions = [];
    private MiiEditorState _resetState;
    private CancellationTokenSource? _previewCts;
    private CancellationTokenSource? _featurePreviewCts;
    private bool _isLoading;
    private bool _hasUnsavedChanges;
    private bool _forceClose;
    private int _previewVersion;
    private int _categoryIndex;
    private int _featurePage;
    private string _previewType = "face";

    public MiiEditorWindow(SaveManagerService saveManagerService, LauncherSettings settings, string miiId)
    {
        _saveManagerService = saveManagerService;
        _settings = settings;
        _miiId = miiId;

        InitializeComponent();
        BuildCategories();
        BuildNameSymbolButtons();
        BuildCategoryButtons();
        _resetState = _saveManagerService.LoadMiiEditorState(_miiId);

        Loaded += (_, _) =>
        {
            ApplyState(_resetState);
            UpdatePreviewTypeButtons();
            BuildFeatureOptionsForSelectedCategory(resetPage: true);
            QueuePreviewRender(markDirty: false, renderImmediately: true);
        };
        Closing += (_, _) =>
        {
            _previewCts?.Cancel();
            _featurePreviewCts?.Cancel();
        };
    }

    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        if (_hasUnsavedChanges && !_forceClose)
        {
            var result = ShowCustomDialog(
                "Unsaved Changes",
                "There are unsaved changes. Do you want to close without saving?",
                MessageBoxButton.YesNo);

            if (result != MessageBoxResult.Yes)
            {
                e.Cancel = true;
                return;
            }

            _forceClose = true;
        }

        base.OnClosing(e);
    }

    private void EditorControl_OnChanged(object sender, RoutedEventArgs e)
    {
        if (_isLoading || !IsLoaded)
        {
            return;
        }

        QueuePreviewRender(markDirty: true, renderImmediately: false);
    }

    private void QueuePreviewRender(bool markDirty, bool renderImmediately)
    {
        _previewCts?.Cancel();
        _previewCts = new CancellationTokenSource();
        var token = _previewCts.Token;
        var version = ++_previewVersion;

        var state = ReadState();
        PreviewNameTextBlock.Text = state.Name;
        PreviewMetaTextBlock.Text = $"{(state.IsFemale ? "Female" : "Male")}   Color {state.FavoriteColorIndex + 1}   Born {state.BirthMonth}/{state.BirthDay}";
        RenderStatusTextBlock.Text = renderImmediately ? "Renderer starting..." : "Preview queued...";
        if (markDirty)
        {
            _hasUnsavedChanges = true;
        }

        AutosaveTextBlock.Text = _hasUnsavedChanges ? "Unsaved changes" : "No unsaved changes";
        UpdateValueLabels();

        _ = PreviewRenderAsync(state, version, renderImmediately ? TimeSpan.Zero : TimeSpan.FromMilliseconds(260), token);
    }

    private async Task PreviewRenderAsync(MiiEditorState state, int version, TimeSpan delay, CancellationToken cancellationToken)
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
                RenderStatusTextBlock.Text = "Rendering temporary preview...";
            });

            var mii = _miiParser.CreateMii(state, "Editor temporary preview");
            var render = await _avatarRenderer.EnsureAvatarRenderAsync(mii, _previewType, 0, cancellationToken);
            if (version != _previewVersion)
            {
                return;
            }

            await Dispatcher.InvokeAsync(() =>
            {
                SetPreviewImage(render.AvatarPath);
                RenderStatusTextBlock.Text = render.Message;
                PreviewPlaceholderTextBlock.Visibility = render.IsReady ? Visibility.Collapsed : Visibility.Visible;
                AutosaveTextBlock.Text = _hasUnsavedChanges ? "Unsaved changes" : "No unsaved changes";
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
                AutosaveTextBlock.Text = _hasUnsavedChanges ? "Unsaved changes" : "Preview failed";
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
            BirthMonth = GetSlider(BirthMonthSlider),
            BirthDay = GetSlider(BirthDaySlider),
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
            SetSlider(BirthMonthSlider, state.BirthMonth);
            SetSlider(BirthDaySlider, state.BirthDay);
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
            UpdateValueLabels();
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
        BuildFeatureOptionsForSelectedCategory(resetPage: false);
        QueuePreviewRender(markDirty: true, renderImmediately: false);
    }

    private void ResetButton_OnClick(object sender, RoutedEventArgs e)
    {
        ApplyState(_resetState.Clone());
        BuildFeatureOptionsForSelectedCategory(resetPage: false);
        QueuePreviewRender(markDirty: true, renderImmediately: false);
    }

    private async void SaveMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        SaveMiiButton.IsEnabled = false;
        CancelMiiButton.IsEnabled = false;
        try
        {
            _previewCts?.Cancel();
            var state = ReadState();
            AutosaveTextBlock.Text = "Saving...";
            RenderStatusTextBlock.Text = "Writing real Mii data...";
            var profile = await _saveManagerService.UpdateMiiProfileAsync(_miiId, state);

            if (!string.IsNullOrWhiteSpace(_settings.UserFolderPath) && Directory.Exists(_settings.UserFolderPath))
            {
                try
                {
                    await _saveManagerService.SyncMiiToDolphinAsync(_settings, profile);
                    AutosaveTextBlock.Text = "Saved and synced to Dolphin";
                }
                catch (Exception ex)
                {
                    AutosaveTextBlock.Text = $"Saved locally. Dolphin sync: {ex.Message}";
                }
            }
            else
            {
                AutosaveTextBlock.Text = "Saved locally";
            }

            _resetState = _saveManagerService.LoadMiiEditorState(_miiId);
            _hasUnsavedChanges = false;
            SetPreviewImage(profile.AvatarImagePath);
            PreviewPlaceholderTextBlock.Visibility = profile.HasAvatarImage ? Visibility.Collapsed : Visibility.Visible;
            RenderStatusTextBlock.Text = profile.RenderStatusText;
        }
        catch (Exception ex)
        {
            AutosaveTextBlock.Text = "Save failed";
            RenderStatusTextBlock.Text = ex.Message;
        }
        finally
        {
            SaveMiiButton.IsEnabled = true;
            CancelMiiButton.IsEnabled = true;
        }
    }

    private void CancelMiiButton_OnClick(object sender, RoutedEventArgs e)
    {
        Close();
    }

    private void BuildCategories()
    {
        _categories.Clear();
        _categories.AddRange(
        [
            new CategoryDefinition("Body", "Base", "Base", "Gender, favorites and basic proportions."),
            new CategoryDefinition("Colors", "Colors", "Colors", "Choose your favorite color and refine colors in Adjust."),
            new CategoryDefinition("Face", "Face", "Face", "Browse face shapes, then adjust skin and details."),
            new CategoryDefinition("Hair", "Hair", "Hair", "Browse main styles; color and flip are in Adjust."),
            new CategoryDefinition("Eyes", "Eyes", "Eyes", "Choose an eye style; position and size are in Adjust."),
            new CategoryDefinition("Brows", "Brows", "Eyebrows", "Choose a style, then adjust rotation and spacing."),
            new CategoryDefinition("Nose", "Nose", "Nose", "Choose a nose and refine size and position."),
            new CategoryDefinition("Mouth", "Mouth", "Mouth", "Choose a mouth and refine color, size and height."),
            new CategoryDefinition("Beard", "Facial Hair", "Facial Hair", "Mustaches and beards with rendered previews."),
            new CategoryDefinition("Glasses", "Glasses", "Glasses", "Choose a model, then adjust color and position."),
            new CategoryDefinition("Mole", "Mole", "Mole", "Toggle the beauty mark and adjust its position.")
        ]);
    }

    private void BuildCategoryButtons()
    {
        CategoryRailWrapPanel.Children.Clear();
        for (var i = 0; i < _categories.Count; i++)
        {
            var index = i;
            var category = _categories[i];
            var button = new Button
            {
                Content = category.Label,
                Height = 44,
                MinWidth = 80,
                Margin = new Thickness(0, 0, 8, 8),
                Padding = new Thickness(16, 0, 16, 0),
                Style = (Style)FindResource(i == _categoryIndex ? "EditorPrimaryButton" : "EditorButton"),
                ToolTip = category.Hint
            };
            button.Click += (_, _) => SelectCategory(index);
            CategoryRailWrapPanel.Children.Add(button);
        }
    }

    private void SelectCategory(int index)
    {
        if (_categories.Count == 0)
        {
            return;
        }

        _categoryIndex = (index + _categories.Count) % _categories.Count;
        _featurePage = 0;
        BuildCategoryButtons();
        BuildFeatureOptionsForSelectedCategory(resetPage: true);
        ConfigureAdjustPopupForCurrentCategory();

        if (CategoryRailWrapPanel.Children.Count > _categoryIndex)
        {
            if (CategoryRailWrapPanel.Children[_categoryIndex] is FrameworkElement element)
            {
                element.BringIntoView();
            }
        }
    }

    private void PreviousCategoryButton_OnClick(object sender, RoutedEventArgs e)
    {
        SelectCategory(_categoryIndex - 1);
    }

    private void NextCategoryButton_OnClick(object sender, RoutedEventArgs e)
    {
        SelectCategory(_categoryIndex + 1);
    }

    private void PreviousOptionsButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_featurePage <= 0)
        {
            return;
        }

        _featurePage--;
        RenderFeaturePage();
    }

    private void NextOptionsButton_OnClick(object sender, RoutedEventArgs e)
    {
        var pageCount = GetFeaturePageCount();
        if (_featurePage >= pageCount - 1)
        {
            return;
        }

        _featurePage++;
        RenderFeaturePage();
    }

    private void SymbolButton_OnClick(object sender, RoutedEventArgs e)
    {
        SymbolPopup.IsOpen = true;
    }

    private void OpenAdjustPopupButton_OnClick(object sender, RoutedEventArgs e)
    {
        ConfigureAdjustPopupForCurrentCategory();
        AdjustPopup.IsOpen = true;
    }

    private void CloseAdjustPopupButton_OnClick(object sender, RoutedEventArgs e)
    {
        AdjustPopup.IsOpen = false;
    }

    private void ConfigureAdjustPopupForCurrentCategory()
    {
        if (_categories.Count == 0 || AdjustTabs == null)
        {
            return;
        }

        var key = _categories[_categoryIndex].Key;
        var visibleTab = key switch
        {
            "Face" => AdjustFaceTab,
            "Hair" => AdjustHairTab,
            "Eyes" => AdjustEyesTab,
            "Brows" => AdjustBrowsTab,
            "Nose" => AdjustNoseTab,
            "Mouth" => AdjustMouthTab,
            "Beard" => AdjustBeardTab,
            "Glasses" => AdjustGlassesTab,
            "Mole" => AdjustMoleTab,
            _ => AdjustBaseTab
        };

        foreach (var tab in new[] { AdjustBaseTab, AdjustFaceTab, AdjustHairTab, AdjustEyesTab, AdjustBrowsTab, AdjustNoseTab, AdjustMouthTab, AdjustBeardTab, AdjustGlassesTab, AdjustMoleTab })
        {
            tab.Visibility = tab == visibleTab ? Visibility.Visible : Visibility.Collapsed;
        }

        AdjustTabs.SelectedItem = visibleTab;
    }

    private void BuildNameSymbolButtons()
    {
        string[] symbols =
        [
            "★", "☆", "♡", "♥", "♦", "♣", "♠", "♪", "♫", "☀", "☁", "☂",
            "→", "←", "↑", "↓", "↔", "✓", "✕", "?", "!", "…", "・", "。",
            "①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨", "⑩",
            "ⓐ", "ⓑ", "Ⓐ", "Ⓑ", "Ⓢ", "Ⓜ", "©", "®", "™"
        ];

        foreach (var symbol in symbols)
        {
            var button = new Button
            {
                Content = symbol,
                Width = 28,
                Height = 28,
                Padding = new Thickness(0),
                Margin = new Thickness(0, 0, 5, 5),
                FontWeight = FontWeights.Black,
                Style = (Style)FindResource("EditorButton")
            };
            button.Click += (_, _) => InsertNameSymbol(symbol);
            NameSymbolWrapPanel.Children.Add(button);
        }
    }

    private void InsertNameSymbol(string symbol)
    {
        var start = NameTextBox.SelectionStart;
        var text = NameTextBox.Text.Remove(start, NameTextBox.SelectionLength).Insert(start, symbol);
        NameTextBox.Text = text.Length <= 10 ? text : text[..10];
        NameTextBox.SelectionStart = Math.Min(start + symbol.Length, NameTextBox.Text.Length);
        NameTextBox.Focus();
    }

    private void BuildFeatureOptionsForSelectedCategory(bool resetPage)
    {
        if (!IsLoaded)
        {
            return;
        }

        _featurePreviewCts?.Cancel();
        _featurePreviewCts = new CancellationTokenSource();
        _featureOptions.Clear();

        if (resetPage)
        {
            _featurePage = 0;
        }

        var category = _categories.Count == 0 ? new CategoryDefinition("Body", "Base", "Base", string.Empty) : _categories[_categoryIndex];
        CurrentCategoryTitleTextBlock.Text = category.Title;
        CurrentCategoryHintTextBlock.Text = category.Hint;

        switch (category.Key)
        {
            case "Body":
                AddToggleOptions("Male", "Male gender", FemaleCheckBox, false, (state, value) => state.IsFemale = value == 1);
                AddToggleOptions("Female", "Female gender", FemaleCheckBox, true, (state, value) => state.IsFemale = value == 1);
                break;
            case "Colors":
                AddColorOptions("Favorite colors", FavoriteColorSlider);
                break;
            case "Face":
                AddRenderOptions("Face shapes", FaceShapeSlider, 0, 7, (state, value) => state.FaceShape = value);
                break;
            case "Hair":
                AddRenderOptions("Hair styles", HairTypeSlider, 0, 71, (state, value) => state.HairType = value);
                break;
            case "Eyes":
                AddRenderOptions("Eye styles", EyeTypeSlider, 0, 47, (state, value) => state.EyeType = value);
                break;
            case "Brows":
                AddRenderOptions("Brow styles", EyebrowTypeSlider, 0, 23, (state, value) => state.EyebrowType = value);
                break;
            case "Nose":
                AddRenderOptions("Nose styles", NoseTypeSlider, 0, 11, (state, value) => state.NoseType = value);
                break;
            case "Mouth":
                AddRenderOptions("Mouth styles", MouthTypeSlider, 0, 23, (state, value) => state.MouthType = value);
                break;
            case "Beard":
                AddRenderOptions("Mustaches", MustacheTypeSlider, 0, 3, (state, value) => state.MustacheType = value);
                AddRenderOptions("Beards", BeardTypeSlider, 0, 3, (state, value) => state.BeardType = value);
                break;
            case "Glasses":
                AddRenderOptions("Glasses", GlassesTypeSlider, 0, 8, (state, value) => state.GlassesType = value);
                break;
            case "Mole":
                AddToggleOptions("Off", "Mole disabled", MoleEnabledCheckBox, false, (state, value) => state.MoleEnabled = value == 1);
                AddToggleOptions("On", "Mole enabled", MoleEnabledCheckBox, true, (state, value) => state.MoleEnabled = value == 1);
                break;
        }

        RenderFeaturePage();
    }

    private void AddRenderOptions(string group, Slider targetSlider, int min, int max, Action<MiiEditorState, int> mutate)
    {
        for (var value = min; value <= max; value++)
        {
            var captured = value;
            _featureOptions.Add(new FeatureOption(
                $"{group} #{value + 1}",
                value,
                () => GetSlider(targetSlider) == captured,
                () => targetSlider.Value = captured,
                mutate,
                true,
                string.Empty));
        }
    }

    private void AddColorOptions(string title, Slider targetSlider)
    {
        string[] colors =
        [
            "#FF3B3B", "#FF8A2A", "#FFD166", "#9CFF5E", "#317a11", "#3B82F6",
            "#8EE7FF", "#FF5CAB", "#A855F7", "#3d260c", "#F7FAFF", "#111827"
        ];

        for (var i = 0; i < colors.Length; i++)
        {
            var captured = i;
            _featureOptions.Add(new FeatureOption(
                $"{title} #{i + 1}",
                i,
                () => GetSlider(targetSlider) == captured,
                () => targetSlider.Value = captured,
                null,
                false,
                colors[i]));
        }
    }

    private void AddToggleOptions(string label, string title, CheckBox checkBox, bool enabled, Action<MiiEditorState, int> mutate)
    {
        var value = enabled ? 1 : 0;
        _featureOptions.Add(new FeatureOption(
            title,
            value,
            () => checkBox.IsChecked == enabled,
            () => checkBox.IsChecked = enabled,
            mutate,
            true,
            string.Empty,
            label));
    }

    private void RenderFeaturePage()
    {
        _featurePreviewCts?.Cancel();
        _featurePreviewCts = new CancellationTokenSource();
        var token = _featurePreviewCts.Token;

        FeaturePreviewContainer.Children.Clear();

        var category = _categories.Count == 0 ? new CategoryDefinition("Body", "Base", "Base", string.Empty) : _categories[_categoryIndex];

        if (category.Key == "Beard")
        {
            // Disable pagination controls for facial hair tab since we show all styles at once
            OptionsPageTextBlock.Text = "1/1";
            PreviousOptionsButton.IsEnabled = false;
            NextOptionsButton.IsEnabled = false;

            var mustacheOptions = _featureOptions.Where(o => o.Title.StartsWith("Mustaches")).ToArray();
            var beardOptions = _featureOptions.Where(o => o.Title.StartsWith("Beards")).ToArray();

            // Mustache Section Header
            var mustacheHeader = new TextBlock
            {
                Text = "MUSTACHE STYLE (Select one)",
                Foreground = (System.Windows.Media.Brush)FindResource("EditorTextSecondary"),
                FontWeight = FontWeights.Bold,
                FontSize = 13,
                Margin = new Thickness(0, 6, 0, 8),
                HorizontalAlignment = HorizontalAlignment.Left
            };
            FeaturePreviewContainer.Children.Add(mustacheHeader);

            var mustacheWrapPanel = new WrapPanel { HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(12) };
            foreach (var option in mustacheOptions)
            {
                var button = CreateFeatureButton(option.IsSelected());
                button.ToolTip = option.Title;
                var image = new Image { Width = 110, Height = 110, Stretch = System.Windows.Media.Stretch.Uniform };
                System.Windows.Media.RenderOptions.SetBitmapScalingMode(image, System.Windows.Media.BitmapScalingMode.HighQuality);
                button.Content = new StackPanel { Children = { image, CreateOptionLabel($"Style {option.Value + 1}") } };
                if (option.Mutate != null)
                {
                    _ = RenderFeatureOptionAsync(image, option, token);
                }
                button.Click += (_, _) =>
                {
                    option.Apply();
                    RenderFeaturePage();
                };
                mustacheWrapPanel.Children.Add(button);
            }
            FeaturePreviewContainer.Children.Add(mustacheWrapPanel);

            // Beard Section Header
            var beardHeader = new TextBlock
            {
                Text = "BEARD STYLE (Select one)",
                Foreground = (System.Windows.Media.Brush)FindResource("EditorTextSecondary"),
                FontWeight = FontWeights.Bold,
                FontSize = 13,
                Margin = new Thickness(0, 12, 0, 8),
                HorizontalAlignment = HorizontalAlignment.Left
            };
            FeaturePreviewContainer.Children.Add(beardHeader);

            var beardWrapPanel = new WrapPanel { HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(12) };
            foreach (var option in beardOptions)
            {
                var button = CreateFeatureButton(option.IsSelected());
                button.ToolTip = option.Title;
                var image = new Image { Width = 110, Height = 110, Stretch = System.Windows.Media.Stretch.Uniform };
                System.Windows.Media.RenderOptions.SetBitmapScalingMode(image, System.Windows.Media.BitmapScalingMode.HighQuality);
                button.Content = new StackPanel { Children = { image, CreateOptionLabel($"Style {option.Value + 1}") } };
                if (option.Mutate != null)
                {
                    _ = RenderFeatureOptionAsync(image, option, token);
                }
                button.Click += (_, _) =>
                {
                    option.Apply();
                    RenderFeaturePage();
                };
                beardWrapPanel.Children.Add(button);
            }
            FeaturePreviewContainer.Children.Add(beardWrapPanel);
        }
        else
        {
            var pageCount = GetFeaturePageCount();
            _featurePage = Math.Clamp(_featurePage, 0, Math.Max(0, pageCount - 1));
            var visibleOptions = _featureOptions
                .Skip(_featurePage * OptionsPerPage)
                .Take(OptionsPerPage)
                .ToArray();

            var wrapPanel = new WrapPanel { HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(12) };
            foreach (var option in visibleOptions)
            {
                var button = CreateFeatureButton(option.IsSelected());
                button.ToolTip = option.Title;

                if (!string.IsNullOrWhiteSpace(option.Color))
                {
                    var color = (System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(option.Color);
                    button.Content = new StackPanel
                    {
                        Children =
                        {
                            new Border
                            {
                                Width = 110,
                                Height = 110,
                                CornerRadius = new CornerRadius(12),
                                Background = new System.Windows.Media.SolidColorBrush(color),
                                BorderBrush = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Colors.White),
                                BorderThickness = option.IsSelected() ? new Thickness(3) : new Thickness(0)
                            },
                            CreateOptionLabel(option.Label ?? (option.Value + 1).ToString())
                        }
                    };
                }
                else
                {
                    var image = new Image { Width = 110, Height = 110, Stretch = System.Windows.Media.Stretch.Uniform };
                    System.Windows.Media.RenderOptions.SetBitmapScalingMode(image, System.Windows.Media.BitmapScalingMode.HighQuality);
                    button.Content = new StackPanel { Children = { image, CreateOptionLabel(option.Label ?? (option.Value + 1).ToString()) } };
                    if (option.Mutate != null)
                    {
                        _ = RenderFeatureOptionAsync(image, option, token);
                    }
                }

                button.Click += (_, _) =>
                {
                    option.Apply();
                    RenderFeaturePage();
                };
                wrapPanel.Children.Add(button);
            }
            FeaturePreviewContainer.Children.Add(wrapPanel);

            OptionsPageTextBlock.Text = $"{(_featureOptions.Count == 0 ? 0 : _featurePage + 1)}/{pageCount}";
            PreviousOptionsButton.IsEnabled = _featurePage > 0;
            NextOptionsButton.IsEnabled = _featurePage < pageCount - 1;
        }
    }

    private int GetFeaturePageCount()
    {
        return Math.Max(1, (int)Math.Ceiling(_featureOptions.Count / (double)OptionsPerPage));
    }

    private Button CreateFeatureButton(bool selected)
    {
        return new Button
        {
            Width = 142,
            Height = 166,
            Padding = new Thickness(8),
            Margin = new Thickness(0, 0, 16, 16),
            Style = (Style)FindResource(selected ? "EditorPrimaryButton" : "EditorButton")
        };
    }

    private static TextBlock CreateOptionLabel(string text)
    {
        return new TextBlock
        {
            Text = text,
            HorizontalAlignment = HorizontalAlignment.Center,
            TextAlignment = TextAlignment.Center,
            FontWeight = FontWeights.Bold,
            FontSize = 11.5,
            Margin = new Thickness(0, 6, 0, 0),
            TextTrimming = TextTrimming.CharacterEllipsis
        };
    }

    private void UpdateValueLabels()
    {
        if (!IsLoaded)
        {
            return;
        }

        FavoriteColorValueTextBlock.Text = $"Fav. color {GetSlider(FavoriteColorSlider) + 1}";
        HeightValueTextBlock.Text = $"Height {GetSlider(HeightSlider)}";
        WeightValueTextBlock.Text = $"Weight {GetSlider(WeightSlider)}";
        BirthMonthValueTextBlock.Text = $"Birth Month {GetSlider(BirthMonthSlider)}";
        BirthDayValueTextBlock.Text = $"Birth Day {GetSlider(BirthDaySlider)}";
        FaceShapeValueTextBlock.Text = $"Face shape {GetSlider(FaceShapeSlider) + 1}";
        SkinColorValueTextBlock.Text = $"Skin {GetSlider(SkinColorSlider) + 1}";
        FacialFeatureValueTextBlock.Text = $"Features {GetSlider(FacialFeatureSlider) + 1}";
        HairTypeValueTextBlock.Text = $"Hair style {GetSlider(HairTypeSlider) + 1}";
        HairColorValueTextBlock.Text = $"Hair color {GetSlider(HairColorSlider) + 1}";
        EyeTypeValueTextBlock.Text = $"Eye style {GetSlider(EyeTypeSlider) + 1}";
        EyeRotationValueTextBlock.Text = $"Rotation {GetSlider(EyeRotationSlider)}";
        EyeColorValueTextBlock.Text = $"Eye color {GetSlider(EyeColorSlider) + 1}";
        EyeSizeValueTextBlock.Text = $"Eye size {GetSlider(EyeSizeSlider)}";
        EyeSpacingValueTextBlock.Text = $"Eye spacing {GetSlider(EyeSpacingSlider)}";
        EyeVerticalValueTextBlock.Text = $"Eye position {GetSlider(EyeVerticalSlider)}";
        EyebrowTypeValueTextBlock.Text = $"Brow style {GetSlider(EyebrowTypeSlider) + 1}";
        EyebrowRotationValueTextBlock.Text = $"Brow rotation {GetSlider(EyebrowRotationSlider)}";
        EyebrowColorValueTextBlock.Text = $"Brow color {GetSlider(EyebrowColorSlider) + 1}";
        EyebrowSizeValueTextBlock.Text = $"Brow size {GetSlider(EyebrowSizeSlider)}";
        EyebrowSpacingValueTextBlock.Text = $"Brow spacing {GetSlider(EyebrowSpacingSlider)}";
        EyebrowVerticalValueTextBlock.Text = $"Brow position {GetSlider(EyebrowVerticalSlider)}";
        NoseTypeValueTextBlock.Text = $"Nose style {GetSlider(NoseTypeSlider) + 1}";
        NoseSizeValueTextBlock.Text = $"Nose size {GetSlider(NoseSizeSlider)}";
        NoseVerticalValueTextBlock.Text = $"Nose position {GetSlider(NoseVerticalSlider)}";
        MouthTypeValueTextBlock.Text = $"Mouth style {GetSlider(MouthTypeSlider) + 1}";
        MouthColorValueTextBlock.Text = $"Mouth color {GetSlider(MouthColorSlider) + 1}";
        MouthSizeValueTextBlock.Text = $"Mouth size {GetSlider(MouthSizeSlider)}";
        MouthVerticalValueTextBlock.Text = $"Mouth position {GetSlider(MouthVerticalSlider)}";
        MustacheTypeValueTextBlock.Text = $"Mustache {GetSlider(MustacheTypeSlider) + 1}";
        BeardTypeValueTextBlock.Text = $"Beard {GetSlider(BeardTypeSlider) + 1}";
        FacialHairColorValueTextBlock.Text = $"Hair color {GetSlider(FacialHairColorSlider) + 1}";
        MustacheSizeValueTextBlock.Text = $"Mustache size {GetSlider(MustacheSizeSlider)}";
        MustacheVerticalValueTextBlock.Text = $"Mustache position {GetSlider(MustacheVerticalSlider)}";
        GlassesTypeValueTextBlock.Text = $"Glasses style {GetSlider(GlassesTypeSlider) + 1}";
        GlassesColorValueTextBlock.Text = $"Glasses color {GetSlider(GlassesColorSlider) + 1}";
        GlassesSizeValueTextBlock.Text = $"Glasses size {GetSlider(GlassesSizeSlider)}";
        GlassesVerticalValueTextBlock.Text = $"Glasses position {GetSlider(GlassesVerticalSlider)}";
        MoleSizeValueTextBlock.Text = $"Mole size {GetSlider(MoleSizeSlider)}";
        MoleVerticalValueTextBlock.Text = $"Mole vertical {GetSlider(MoleVerticalSlider)}";
        MoleHorizontalValueTextBlock.Text = $"Mole horizontal {GetSlider(MoleHorizontalSlider)}";
    }

    private async Task RenderFeatureOptionAsync(Image image, FeatureOption option, CancellationToken cancellationToken)
    {
        try
        {
            var state = ReadState();
            option.Mutate?.Invoke(state, option.Value);
            var mii = _miiParser.CreateMii(state, "Editor option preview");
            await FeaturePreviewRenderGate.WaitAsync(cancellationToken);
            MiiAvatarRenderResult render;
            try
            {
                render = await _avatarRenderer.EnsureAvatarRenderAsync(mii, "face", 0, cancellationToken);
            }
            finally
            {
                FeaturePreviewRenderGate.Release();
            }

            if (render.IsReady)
            {
                await Dispatcher.InvokeAsync(() => SetImageSource(image, render.AvatarPath));
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch
        {
        }
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

        SetImageSource(PreviewImage, path);
    }

    private static void SetImageSource(Image image, string path)
    {
        var bitmap = new BitmapImage();
        bitmap.BeginInit();
        bitmap.CacheOption = BitmapCacheOption.OnLoad;
        bitmap.UriSource = new Uri(path, UriKind.Absolute);
        bitmap.EndInit();
        bitmap.Freeze();
        image.Source = bitmap;
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

    private void FaceViewButton_Click(object sender, RoutedEventArgs e)
    {
        _previewType = "face";
        UpdatePreviewTypeButtons();
        QueuePreviewRender(markDirty: false, renderImmediately: true);
    }

    private void BodyViewButton_Click(object sender, RoutedEventArgs e)
    {
        _previewType = "all_body";
        UpdatePreviewTypeButtons();
        QueuePreviewRender(markDirty: false, renderImmediately: true);
    }

    private void UpdatePreviewTypeButtons()
    {
        if (FaceViewButton != null && BodyViewButton != null)
        {
            FaceViewButton.Tag = _previewType == "face" ? "Selected" : null;
            BodyViewButton.Tag = _previewType == "all_body" ? "Selected" : null;
        }
    }



    private MessageBoxResult ShowCustomDialog(string title, string message, MessageBoxButton buttons = MessageBoxButton.OK)
    {
        var dialog = new CustomDialog(title, message, buttons)
        {
            Owner = this
        };

        var result = dialog.ShowDialog();
        if (buttons == MessageBoxButton.OK)
        {
            return result == MessageBoxResult.OK ? MessageBoxResult.OK : MessageBoxResult.None;
        }

        if (buttons == MessageBoxButton.YesNo)
        {
            return result == MessageBoxResult.Yes ? MessageBoxResult.Yes : MessageBoxResult.No;
        }

        return result ?? MessageBoxResult.OK;
    }

    private sealed record CategoryDefinition(string Key, string Label, string Title, string Hint);

    private sealed record FeatureOption(
        string Title,
        int Value,
        Func<bool> IsSelected,
        Action Apply,
        Action<MiiEditorState, int>? Mutate,
        bool RenderPreview,
        string Color,
        string? Label = null);
}
