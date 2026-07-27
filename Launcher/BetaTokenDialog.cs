using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher;

public sealed class BetaTokenDialog : Window
{
    private readonly BetaAccessService _betaAccessService = new();
    private readonly Border _rootCard;
    private readonly ScaleTransform _cardScaleTransform;
    private readonly TranslateTransform _cardTranslateTransform;
    
    private readonly TextBox _tokenTextBox;
    private readonly TextBlock _placeholderTextBlock;
    private readonly Button _pasteButton;
    private readonly Button _unlockButton;
    private readonly Button _cancelButton;

    private readonly Border _errorCard;
    private readonly TextBlock _errorTextBlock;
    private readonly TranslateTransform _errorTranslateTransform;

    private readonly StackPanel _loadingPanel;
    private readonly ProgressBar _progressBar;

    public string VerifiedToken { get; private set; } = string.Empty;

    public BetaTokenDialog(string initialToken = "")
    {
        Title = "Unlock Beta Access";
        Width = 560;
        SizeToContent = SizeToContent.Height;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        Background = Brushes.Transparent;
        ShowInTaskbar = false;
        Topmost = true;

        // Force Ultra-Crisp ClearType Text & Pixel Snapping (Fixes sgranato/blurry text)
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        TextOptions.SetTextRenderingMode(this, TextRenderingMode.ClearType);
        RenderOptions.SetClearTypeHint(this, ClearTypeHint.Enabled);
        UseLayoutRounding = true;
        SnapsToDevicePixels = true;

        // --- Animations & Transforms Setup ---
        _cardScaleTransform = new ScaleTransform(0.92, 0.92);
        _cardTranslateTransform = new TranslateTransform(0, 20);
        var transformGroup = new TransformGroup();
        transformGroup.Children.Add(_cardScaleTransform);
        transformGroup.Children.Add(_cardTranslateTransform);

        // Beta Amber/Gold Gradient Border Stroke
        var borderBrush = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(1, 1),
            GradientStops =
            {
                new GradientStop(Color.FromRgb(0xFF, 0xB3, 0x02), 0.0), // Beta Gold
                new GradientStop(Color.FromRgb(0xFF, 0x9F, 0x43), 0.5), // Beta Amber Accent
                new GradientStop(Color.FromRgb(0xFF, 0x6B, 0x00), 1.0)  // Deep Orange
            }
        };

        // Root Glass Card (Matching Launcher GlassCard style)
        _rootCard = new Border
        {
            Width = 500,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(24),
            Opacity = 0,
            Padding = new Thickness(26),
            CornerRadius = new CornerRadius(16),
            Background = new SolidColorBrush(Color.FromRgb(0x13, 0x1B, 0x2E)), // Launcher Panel Glass
            BorderBrush = borderBrush,
            BorderThickness = new Thickness(1.8),
            RenderTransformOrigin = new Point(0.5, 0.5),
            RenderTransform = transformGroup,
            SnapsToDevicePixels = true,
            Effect = new DropShadowEffect
            {
                BlurRadius = 32,
                ShadowDepth = 0,
                Opacity = 0.6,
                Color = Color.FromRgb(0xFF, 0x9F, 0x43)
            }
        };
        _rootCard.MouseLeftButtonDown += (_, e) => { if (e.ButtonState == MouseButtonState.Pressed) DragMove(); };

        var mainLayout = new StackPanel();

        // --- Header Row with Key Badge (🔑) ---
        var headerGrid = new Grid { Margin = new Thickness(0, 0, 0, 18) };
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

        // Glowing Key Badge Icon Container
        var keyBadgeBorder = new Border
        {
            Width = 48,
            Height = 48,
            CornerRadius = new CornerRadius(14),
            Background = new RadialGradientBrush(
                Color.FromRgb(0x3B, 0x2A, 0x10),
                Color.FromRgb(0x1F, 0x17, 0x0A)),
            BorderBrush = new SolidColorBrush(Color.FromArgb(0xAA, 0xFF, 0x9F, 0x43)),
            BorderThickness = new Thickness(1.5),
            Margin = new Thickness(0, 0, 16, 0),
            SnapsToDevicePixels = true,
            Effect = new DropShadowEffect { BlurRadius = 12, ShadowDepth = 0, Opacity = 0.5, Color = Color.FromRgb(0xFF, 0x9F, 0x43) }
        };
        keyBadgeBorder.Child = new TextBlock
        {
            Text = "🔑",
            FontSize = 22,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center
        };
        Grid.SetColumn(keyBadgeBorder, 0);
        headerGrid.Children.Add(keyBadgeBorder);

        // Title Stack
        var titleStack = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        var badgeTitleRow = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 0, 0, 5) };

        var betaBadge = new Border
        {
            Background = new SolidColorBrush(Color.FromArgb(0x33, 0xFF, 0x9F, 0x43)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(7, 2, 7, 2),
            Margin = new Thickness(0, 0, 10, 0),
            VerticalAlignment = VerticalAlignment.Center,
            SnapsToDevicePixels = true
        };
        betaBadge.Child = new TextBlock
        {
            Text = "BETA CHANNEL",
            FontSize = 10.5,
            FontWeight = FontWeights.Bold,
            Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0xB3, 0x02))
        };

        var titleText = new TextBlock
        {
            Text = "Access Token Required",
            FontSize = 19,
            FontWeight = FontWeights.Black,
            Foreground = Brushes.White,
            VerticalAlignment = VerticalAlignment.Center
        };

        badgeTitleRow.Children.Add(betaBadge);
        badgeTitleRow.Children.Add(titleText);

        var subTitleText = new TextBlock
        {
            Text = "Enter your Access Token to join and activate the VKBeta channel.",
            FontSize = 13,
            Foreground = new SolidColorBrush(Color.FromRgb(0x9C, 0xA3, 0xAF)),
            TextWrapping = TextWrapping.Wrap
        };

        titleStack.Children.Add(badgeTitleRow);
        titleStack.Children.Add(subTitleText);
        Grid.SetColumn(titleStack, 1);
        headerGrid.Children.Add(titleStack);

        mainLayout.Children.Add(headerGrid);

        // --- Error Banner ---
        _errorTranslateTransform = new TranslateTransform(0, 0);
        _errorCard = new Border
        {
            Background = new SolidColorBrush(Color.FromArgb(0x33, 0xFF, 0x52, 0x52)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0xFF, 0x52, 0x52)),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(12, 10, 12, 10),
            Margin = new Thickness(0, 0, 0, 16),
            Visibility = Visibility.Collapsed,
            RenderTransform = _errorTranslateTransform,
            SnapsToDevicePixels = true
        };

        var errorStack = new StackPanel { Orientation = Orientation.Horizontal };
        errorStack.Children.Add(new TextBlock { Text = "⚠️ ", FontSize = 13, VerticalAlignment = VerticalAlignment.Center });
        _errorTextBlock = new TextBlock
        {
            Text = string.Empty,
            FontSize = 12.5,
            FontWeight = FontWeights.SemiBold,
            Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x6B, 0x6B)),
            TextWrapping = TextWrapping.Wrap,
            VerticalAlignment = VerticalAlignment.Center
        };
        errorStack.Children.Add(_errorTextBlock);
        _errorCard.Child = errorStack;
        mainLayout.Children.Add(_errorCard);

        // --- Token Input Row ---
        var inputGrid = new Grid { Margin = new Thickness(0, 0, 0, 20) };
        inputGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        inputGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        var textStackGrid = new Grid();
        
        _placeholderTextBlock = new TextBlock
        {
            Text = "Paste or enter your Beta token...",
            FontSize = 13,
            Foreground = new SolidColorBrush(Color.FromRgb(0x4B, 0x55, 0x63)),
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(14, 0, 14, 0),
            IsHitTestVisible = false,
            Visibility = string.IsNullOrEmpty(initialToken) ? Visibility.Visible : Visibility.Collapsed
        };

        _tokenTextBox = new TextBox
        {
            Text = initialToken,
            Height = 42,
            FontSize = 13,
            FontWeight = FontWeights.SemiBold,
            Foreground = Brushes.White,
            CaretBrush = new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)),
            Template = CreateTextBoxTemplate()
        };

        _tokenTextBox.TextChanged += (_, _) =>
        {
            _placeholderTextBlock.Visibility = string.IsNullOrEmpty(_tokenTextBox.Text) ? Visibility.Visible : Visibility.Collapsed;
            HideError();
        };
        _tokenTextBox.KeyDown += TokenTextBox_OnKeyDown;

        textStackGrid.Children.Add(_tokenTextBox);
        textStackGrid.Children.Add(_placeholderTextBlock);

        _pasteButton = new Button
        {
            Content = "📋 Paste",
            Height = 42,
            MinWidth = 76,
            Padding = new Thickness(12, 0, 12, 0),
            Margin = new Thickness(8, 0, 0, 0),
            FontWeight = FontWeights.Bold,
            FontSize = 12.5,
            Foreground = Brushes.White,
            Background = new SolidColorBrush(Color.FromRgb(0x1B, 0x26, 0x40)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0x33, 0x40, 0x5D)),
            BorderThickness = new Thickness(1),
            Cursor = Cursors.Hand,
            Template = CreateButtonTemplate(isPrimary: false)
        };
        _pasteButton.Click += PasteButton_OnClick;

        Grid.SetColumn(textStackGrid, 0);
        Grid.SetColumn(_pasteButton, 1);
        inputGrid.Children.Add(textStackGrid);
        inputGrid.Children.Add(_pasteButton);
        mainLayout.Children.Add(inputGrid);

        // --- Loader Bar ---
        _loadingPanel = new StackPanel
        {
            Orientation = Orientation.Vertical,
            Margin = new Thickness(0, 0, 0, 16),
            Visibility = Visibility.Collapsed
        };

        _progressBar = new ProgressBar
        {
            IsIndeterminate = true,
            Height = 4,
            Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)),
            Background = new SolidColorBrush(Color.FromRgb(0x1B, 0x26, 0x40)),
            BorderThickness = new Thickness(0),
            Margin = new Thickness(0, 0, 0, 8)
        };

        var loadingText = new TextBlock
        {
            Text = "Verifying token with VanzaKart server...",
            FontSize = 12,
            FontWeight = FontWeights.SemiBold,
            Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)),
            HorizontalAlignment = HorizontalAlignment.Center
        };

        _loadingPanel.Children.Add(_progressBar);
        _loadingPanel.Children.Add(loadingText);
        mainLayout.Children.Add(_loadingPanel);

        // --- Action Buttons ---
        var buttonGrid = new Grid();
        buttonGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        buttonGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1.3, GridUnitType.Star) });

        _cancelButton = new Button
        {
            Content = "Cancel",
            Height = 42,
            Margin = new Thickness(0, 0, 6, 0),
            FontWeight = FontWeights.Bold,
            FontSize = 13,
            Foreground = new SolidColorBrush(Color.FromRgb(0xD1, 0xD5, 0xDB)),
            Background = new SolidColorBrush(Color.FromRgb(0x1B, 0x26, 0x40)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0x33, 0x40, 0x5D)),
            BorderThickness = new Thickness(1),
            Cursor = Cursors.Hand,
            Template = CreateButtonTemplate(isPrimary: false)
        };
        _cancelButton.Click += (_, _) => CloseDialog(false);

        _unlockButton = new Button
        {
            Content = "🚀 Unlock Beta",
            Height = 42,
            Margin = new Thickness(6, 0, 0, 0),
            FontWeight = FontWeights.Black,
            FontSize = 13.5,
            Foreground = new SolidColorBrush(Color.FromRgb(0x0D, 0x11, 0x1A)), // Crisp dark legibility
            Background = new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)), // Beta Amber Accent
            BorderBrush = new SolidColorBrush(Color.FromRgb(0xFF, 0xC1, 0x07)),
            BorderThickness = new Thickness(1.5),
            Cursor = Cursors.Hand,
            Template = CreateButtonTemplate(isPrimary: true)
        };
        _unlockButton.Click += UnlockButton_OnClick;

        Grid.SetColumn(_cancelButton, 0);
        Grid.SetColumn(_unlockButton, 1);
        buttonGrid.Children.Add(_cancelButton);
        buttonGrid.Children.Add(_unlockButton);
        mainLayout.Children.Add(buttonGrid);

        _rootCard.Child = mainLayout;
        Content = _rootCard;

        // --- Entrance Animation ---
        Loaded += (_, _) =>
        {
            var easeOut = new CubicEase { EasingMode = EasingMode.EaseOut };
            var duration = TimeSpan.FromMilliseconds(220);

            _rootCard.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, duration) { EasingFunction = easeOut });
            _cardScaleTransform.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(0.92, 1.0, duration) { EasingFunction = easeOut });
            _cardScaleTransform.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(0.92, 1.0, duration) { EasingFunction = easeOut });
            _cardTranslateTransform.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(20, 0, duration) { EasingFunction = easeOut });

            _tokenTextBox.Focus();
            if (!string.IsNullOrEmpty(_tokenTextBox.Text))
            {
                _tokenTextBox.SelectAll();
            }
        };

        KeyDown += (_, e) =>
        {
            if (e.Key == Key.Escape && _loadingPanel.Visibility != Visibility.Visible)
            {
                CloseDialog(false);
            }
        };
    }

    // --- Custom WPF Control Templates (Overriding WPF Defaults) ---

    private static ControlTemplate CreateButtonTemplate(bool isPrimary)
    {
        var template = new ControlTemplate(typeof(Button));
        var gridFactory = new FrameworkElementFactory(typeof(Grid));

        if (isPrimary)
        {
            var glowFactory = new FrameworkElementFactory(typeof(Border));
            glowFactory.Name = "GlowBorder";
            glowFactory.SetValue(Border.CornerRadiusProperty, new CornerRadius(10));
            glowFactory.SetValue(Border.MarginProperty, new Thickness(-3));
            glowFactory.SetValue(Border.OpacityProperty, 0.3);
            glowFactory.SetValue(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)));
            glowFactory.SetValue(UIElement.EffectProperty, new BlurEffect { Radius = 10 });
            gridFactory.AppendChild(glowFactory);
        }

        var cardFactory = new FrameworkElementFactory(typeof(Border));
        cardFactory.Name = "CardBorder";
        cardFactory.SetValue(Border.CornerRadiusProperty, new CornerRadius(8));
        cardFactory.SetValue(Border.BackgroundProperty, new TemplateBindingExtension(Button.BackgroundProperty));
        cardFactory.SetValue(Border.BorderBrushProperty, new TemplateBindingExtension(Button.BorderBrushProperty));
        cardFactory.SetValue(Border.BorderThicknessProperty, new TemplateBindingExtension(Button.BorderThicknessProperty));
        cardFactory.SetValue(Border.SnapsToDevicePixelsProperty, true);

        var presenterFactory = new FrameworkElementFactory(typeof(ContentPresenter));
        presenterFactory.SetValue(ContentPresenter.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        presenterFactory.SetValue(ContentPresenter.VerticalAlignmentProperty, VerticalAlignment.Center);
        presenterFactory.SetValue(ContentPresenter.MarginProperty, new TemplateBindingExtension(Button.PaddingProperty));
        presenterFactory.SetValue(TextOptions.TextFormattingModeProperty, TextFormattingMode.Display);
        presenterFactory.SetValue(TextOptions.TextRenderingModeProperty, TextRenderingMode.ClearType);

        cardFactory.AppendChild(presenterFactory);
        gridFactory.AppendChild(cardFactory);

        template.VisualTree = gridFactory;

        // Hover & Focus Triggers
        var hoverTrigger = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        if (isPrimary)
        {
            hoverTrigger.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0xFF, 0xB3, 0x02)), "CardBorder"));
            hoverTrigger.Setters.Add(new Setter(Border.OpacityProperty, 0.7, "GlowBorder"));
        }
        else
        {
            hoverTrigger.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x25, 0x35, 0x5C)), "CardBorder"));
            hoverTrigger.Setters.Add(new Setter(Border.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0x4A, 0x5E, 0x8C)), "CardBorder"));
        }
        template.Triggers.Add(hoverTrigger);

        var disabledTrigger = new Trigger { Property = UIElement.IsEnabledProperty, Value = false };
        disabledTrigger.Setters.Add(new Setter(UIElement.OpacityProperty, 0.45, "CardBorder"));
        template.Triggers.Add(disabledTrigger);

        return template;
    }

    private static ControlTemplate CreateTextBoxTemplate()
    {
        var template = new ControlTemplate(typeof(TextBox));
        var borderFactory = new FrameworkElementFactory(typeof(Border));
        borderFactory.Name = "RootBorder";
        borderFactory.SetValue(Border.CornerRadiusProperty, new CornerRadius(8));
        borderFactory.SetValue(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x0B, 0x10, 0x1D)));
        borderFactory.SetValue(Border.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0x33, 0x40, 0x5D)));
        borderFactory.SetValue(Border.BorderThicknessProperty, new Thickness(1.5));
        borderFactory.SetValue(Border.PaddingProperty, new Thickness(12, 10, 12, 10));
        borderFactory.SetValue(Border.SnapsToDevicePixelsProperty, true);

        var scrollViewerFactory = new FrameworkElementFactory(typeof(ScrollViewer));
        scrollViewerFactory.Name = "PART_ContentHost";
        scrollViewerFactory.SetValue(ScrollViewer.MarginProperty, new Thickness(0));
        scrollViewerFactory.SetValue(ScrollViewer.VerticalAlignmentProperty, VerticalAlignment.Center);
        scrollViewerFactory.SetValue(TextOptions.TextFormattingModeProperty, TextFormattingMode.Display);
        scrollViewerFactory.SetValue(TextOptions.TextRenderingModeProperty, TextRenderingMode.ClearType);

        borderFactory.AppendChild(scrollViewerFactory);
        template.VisualTree = borderFactory;

        var focusTrigger = new Trigger { Property = UIElement.IsKeyboardFocusedProperty, Value = true };
        focusTrigger.Setters.Add(new Setter(Border.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0xFF, 0x9F, 0x43)), "RootBorder"));
        focusTrigger.Setters.Add(new Setter(UIElement.EffectProperty, new DropShadowEffect { BlurRadius = 10, ShadowDepth = 0, Opacity = 0.5, Color = Color.FromRgb(0xFF, 0x9F, 0x43) }, "RootBorder"));
        template.Triggers.Add(focusTrigger);

        return template;
    }

    // --- Actions & Helpers ---

    private void PasteButton_OnClick(object sender, RoutedEventArgs e)
    {
        try
        {
            if (Clipboard.ContainsText())
            {
                var text = Clipboard.GetText()?.Trim();
                if (!string.IsNullOrEmpty(text))
                {
                    _tokenTextBox.Text = text;
                    _tokenTextBox.SelectAll();
                    HideError();
                }
            }
        }
        catch
        {
            // Clipboard access error ignored
        }
    }

    private void TokenTextBox_OnKeyDown(object sender, KeyEventArgs e)
    {
        HideError();
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            VerifyAndSubmitAsync();
        }
    }

    private void UnlockButton_OnClick(object sender, RoutedEventArgs e)
    {
        VerifyAndSubmitAsync();
    }

    private async void VerifyAndSubmitAsync()
    {
        var token = _tokenTextBox.Text?.Trim();
        if (string.IsNullOrWhiteSpace(token))
        {
            TriggerErrorShake();
            ShowError("Please enter an Access Token.");
            _tokenTextBox.Focus();
            return;
        }

        SetLoadingState(true);
        HideError();

        var result = await _betaAccessService.VerifyTokenAsync(token);

        SetLoadingState(false);

        if (result.IsSuccess)
        {
            VerifiedToken = token;
            CloseDialog(true);
        }
        else
        {
            TriggerErrorShake();
            ShowError(result.DisplayErrorMessage);
            _tokenTextBox.Focus();
            _tokenTextBox.SelectAll();
        }
    }

    private void ShowError(string message)
    {
        _errorTextBlock.Text = message;
        _errorCard.Visibility = Visibility.Visible;
    }

    private void HideError()
    {
        _errorCard.Visibility = Visibility.Collapsed;
        _errorTextBlock.Text = string.Empty;
    }

    private void TriggerErrorShake()
    {
        var shakeAnimation = new DoubleAnimationUsingKeyFrames();
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(0, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(0))));
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(-8, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(50))));
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(8, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(100))));
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(-5, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(150))));
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(5, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(200))));
        shakeAnimation.KeyFrames.Add(new LinearDoubleKeyFrame(0, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(250))));

        _errorTranslateTransform.BeginAnimation(TranslateTransform.XProperty, shakeAnimation);
    }

    private void SetLoadingState(bool isLoading)
    {
        _tokenTextBox.IsEnabled = !isLoading;
        _pasteButton.IsEnabled = !isLoading;
        _unlockButton.IsEnabled = !isLoading;
        _cancelButton.IsEnabled = !isLoading;
        _loadingPanel.Visibility = isLoading ? Visibility.Visible : Visibility.Collapsed;
    }

    private void CloseDialog(bool success)
    {
        DialogResult = success;
        Close();
    }
}
