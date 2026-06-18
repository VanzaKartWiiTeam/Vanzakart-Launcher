using System.ComponentModel;
using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;

namespace VanzaKartLauncher;

public sealed class AddonDownloadDialog : Window
{
    private readonly TextBlock _stageText;
    private readonly TextBlock _detailText;
    private readonly TextBlock _percentText;
    private readonly ProgressBar _progressBar;
    private readonly Button _actionButton;
    private readonly Stopwatch _stopwatch = Stopwatch.StartNew();
    private bool _canClose;
    private long _lastBytes;
    private TimeSpan _lastSample;
    private double _smoothedBytesPerSecond;

    public event Action? CancelRequested;

    public AddonDownloadDialog(string addonName, string fileName)
    {
        Title = $"Installing {addonName}";
        Width = 590;
        Height = 330;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        Background = Brushes.Transparent;
        ShowInTaskbar = false;

        var accent = CreateRainbowBrush(animate: false);
        var card = new Border
        {
            Margin = new Thickness(20),
            Padding = new Thickness(28),
            CornerRadius = new CornerRadius(18),
            Background = new SolidColorBrush(Color.FromRgb(0x11, 0x18, 0x27)),
            BorderBrush = CreateRainbowBrush(animate: true),
            BorderThickness = new Thickness(1.8),
            Effect = new DropShadowEffect { BlurRadius = 36, ShadowDepth = 0, Opacity = 0.72, Color = Color.FromRgb(0x00, 0xF2, 0xFF) }
        };
        card.MouseLeftButtonDown += DragWindow;

        var layout = new Grid();
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var heading = new TextBlock
        {
            Text = "Installing addon",
            FontSize = 12,
            FontWeight = FontWeights.Bold,
            Foreground = new SolidColorBrush(Color.FromRgb(0x66, 0xA6, 0xFF))
        };
        layout.Children.Add(heading);

        var name = new TextBlock
        {
            Text = addonName,
            FontSize = 22,
            FontWeight = FontWeights.Black,
            Foreground = Brushes.White,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 6, 0, 3)
        };
        Grid.SetRow(name, 1);
        layout.Children.Add(name);

        var file = new TextBlock
        {
            Text = fileName,
            FontSize = 12,
            Foreground = new SolidColorBrush(Color.FromRgb(0x93, 0xA1, 0xBF)),
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 0, 0, 20)
        };
        Grid.SetRow(file, 2);
        layout.Children.Add(file);

        var progressGrid = new Grid();
        progressGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        progressGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        _stageText = new TextBlock
        {
            Text = "Preparing download...",
            FontSize = 13,
            FontWeight = FontWeights.SemiBold,
            Foreground = Brushes.White
        };
        _percentText = new TextBlock
        {
            Text = "0%",
            FontSize = 13,
            FontWeight = FontWeights.Bold,
            Foreground = new SolidColorBrush(Color.FromRgb(0x66, 0xA6, 0xFF))
        };
        Grid.SetColumn(_percentText, 1);
        progressGrid.Children.Add(_stageText);
        progressGrid.Children.Add(_percentText);

        var progressStack = new StackPanel();
        progressStack.Children.Add(progressGrid);
        _progressBar = new ProgressBar
        {
            Height = 9,
            Minimum = 0,
            Maximum = 100,
            Value = 0,
            Foreground = accent,
            Background = Brushes.White,
            BorderThickness = new Thickness(0),
            Margin = new Thickness(0, 9, 0, 9),
            Template = CreateProgressBarTemplate()
        };
        progressStack.Children.Add(_progressBar);
        _detailText = new TextBlock
        {
            Text = "Waiting for GameBanana...",
            FontSize = 12,
            Foreground = new SolidColorBrush(Color.FromRgb(0x93, 0xA1, 0xBF))
        };
        progressStack.Children.Add(_detailText);
        Grid.SetRow(progressStack, 3);
        layout.Children.Add(progressStack);

        _actionButton = new Button
        {
            Content = "Cancel",
            MinWidth = 100,
            Height = 36,
            Padding = new Thickness(16, 0, 16, 0),
            HorizontalAlignment = HorizontalAlignment.Right,
            Margin = new Thickness(0, 22, 0, 0),
            Foreground = Brushes.White,
            Background = new SolidColorBrush(Color.FromRgb(0x21, 0x2B, 0x43)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0x43, 0x51, 0x70)),
            BorderThickness = new Thickness(1),
            FontWeight = FontWeights.Bold,
            Cursor = Cursors.Hand,
            Template = CreateButtonTemplate()
        };
        _actionButton.Click += (_, _) =>
        {
            if (_canClose) Close();
            else
            {
                _actionButton.IsEnabled = false;
                SetStage("Cancelling...");
                CancelRequested?.Invoke();
            }
        };
        Grid.SetRow(_actionButton, 4);
        layout.Children.Add(_actionButton);

        card.Child = layout;
        Content = card;
    }

    private void DragWindow(object sender, MouseButtonEventArgs e)
    {
        if (e.LeftButton != MouseButtonState.Pressed || FindAncestor<Button>(e.OriginalSource as DependencyObject) != null)
        {
            return;
        }

        try { DragMove(); }
        catch (InvalidOperationException) { }
    }

    private static T? FindAncestor<T>(DependencyObject? element) where T : DependencyObject
    {
        while (element != null)
        {
            if (element is T match) return match;
            element = VisualTreeHelper.GetParent(element);
        }
        return null;
    }

    public void UpdateDownload(long current, long total)
    {
        var elapsed = _stopwatch.Elapsed;
        var sampleSeconds = (elapsed - _lastSample).TotalSeconds;
        if (sampleSeconds >= 0.25)
        {
            var instantSpeed = Math.Max(0, current - _lastBytes) / sampleSeconds;
            _smoothedBytesPerSecond = _smoothedBytesPerSecond <= 0
                ? instantSpeed
                : (_smoothedBytesPerSecond * 0.7) + (instantSpeed * 0.3);
            _lastBytes = current;
            _lastSample = elapsed;
        }

        _progressBar.IsIndeterminate = total <= 0;
        var percent = total > 0 ? Math.Clamp(current * 100d / total, 0, 100) : 0;
        _progressBar.Value = percent;
        _percentText.Text = total > 0 ? $"{percent:0}%" : "—";
        _stageText.Text = "Downloading addon...";

        var speed = _smoothedBytesPerSecond > 0 ? $" • {FormatBytes((long)_smoothedBytesPerSecond)}/s" : string.Empty;
        var eta = total > current && _smoothedBytesPerSecond > 0
            ? $" • about {FormatDuration(TimeSpan.FromSeconds((total - current) / _smoothedBytesPerSecond))} remaining"
            : string.Empty;
        _detailText.Text = total > 0
            ? $"{FormatBytes(current)} of {FormatBytes(total)}{speed}{eta}"
            : $"{FormatBytes(current)} downloaded{speed}";
    }

    public void SetStage(string stage)
    {
        _stageText.Text = stage;
        _detailText.Text = stage;
        _progressBar.IsIndeterminate = true;
        _percentText.Text = "";
    }

    public void MarkCompleted()
    {
        _canClose = true;
        _stageText.Text = "Addon installed";
        _detailText.Text = "The addon is enabled and ready to use.";
        _progressBar.IsIndeterminate = false;
        _progressBar.Value = 100;
        _percentText.Text = "100%";
        _actionButton.IsEnabled = true;
        _actionButton.Content = "Close";
    }

    public void MarkFailed(string message)
    {
        _canClose = true;
        _stageText.Text = "Installation failed";
        _detailText.Text = message;
        _detailText.Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x6B, 0x6B));
        _progressBar.IsIndeterminate = false;
        _actionButton.IsEnabled = true;
        _actionButton.Content = "Close";
    }

    public void MarkCancelled()
    {
        _canClose = true;
        _stageText.Text = "Installation cancelled";
        _detailText.Text = "No addon was installed.";
        _progressBar.IsIndeterminate = false;
        _actionButton.IsEnabled = true;
        _actionButton.Content = "Close";
    }

    protected override void OnClosing(CancelEventArgs e)
    {
        if (!_canClose)
        {
            e.Cancel = true;
            CancelRequested?.Invoke();
            return;
        }
        base.OnClosing(e);
    }

    private static string FormatBytes(long bytes)
    {
        string[] units = { "B", "KB", "MB", "GB" };
        var value = (double)Math.Max(0, bytes);
        var unit = 0;
        while (value >= 1024 && unit < units.Length - 1) { value /= 1024; unit++; }
        return $"{value:0.#} {units[unit]}";
    }

    private static string FormatDuration(TimeSpan value)
    {
        if (value.TotalSeconds < 60) return $"{Math.Max(1, value.TotalSeconds):0}s";
        if (value.TotalMinutes < 60) return $"{value.TotalMinutes:0}m";
        return $"{value.TotalHours:0.#}h";
    }

    private static LinearGradientBrush CreateRainbowBrush(bool animate)
    {
        var rotate = new RotateTransform(0, 0.5, 0.5);
        var brush = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(1, 0),
            RelativeTransform = rotate,
            GradientStops =
            {
                new GradientStop(Color.FromRgb(0xFF, 0x00, 0x66), 0.00),
                new GradientStop(Color.FromRgb(0xFF, 0x88, 0x00), 0.18),
                new GradientStop(Color.FromRgb(0xFF, 0xEA, 0x00), 0.34),
                new GradientStop(Color.FromRgb(0x00, 0xFF, 0x66), 0.50),
                new GradientStop(Color.FromRgb(0x00, 0xF2, 0xFF), 0.67),
                new GradientStop(Color.FromRgb(0x33, 0x00, 0xFF), 0.84),
                new GradientStop(Color.FromRgb(0xB0, 0x00, 0xFF), 1.00)
            }
        };
        if (animate)
            rotate.BeginAnimation(RotateTransform.AngleProperty, new DoubleAnimation(0, 360, TimeSpan.FromSeconds(6)) { RepeatBehavior = RepeatBehavior.Forever });
        return brush;
    }

    private static ControlTemplate CreateProgressBarTemplate()
    {
        var template = new ControlTemplate(typeof(ProgressBar));
        var track = new FrameworkElementFactory(typeof(Border), "PART_Track");
        track.SetValue(Border.BackgroundProperty, Brushes.White);
        track.SetValue(Border.CornerRadiusProperty, new CornerRadius(4.5));
        track.SetValue(Border.ClipToBoundsProperty, true);

        var indicator = new FrameworkElementFactory(typeof(Border), "PART_Indicator");
        indicator.SetValue(Border.BackgroundProperty, CreateRainbowBrush(animate: true));
        indicator.SetValue(Border.CornerRadiusProperty, new CornerRadius(4.5));
        indicator.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Left);
        track.AppendChild(indicator);
        template.VisualTree = track;
        return template;
    }

    private static ControlTemplate CreateButtonTemplate()
    {
        var template = new ControlTemplate(typeof(Button));
        var border = new FrameworkElementFactory(typeof(Border), "ButtonBorder");
        border.SetBinding(Border.BackgroundProperty, new System.Windows.Data.Binding(nameof(Button.Background))
        {
            RelativeSource = new System.Windows.Data.RelativeSource(System.Windows.Data.RelativeSourceMode.TemplatedParent)
        });
        border.SetBinding(Border.BorderBrushProperty, new System.Windows.Data.Binding(nameof(Button.BorderBrush))
        {
            RelativeSource = new System.Windows.Data.RelativeSource(System.Windows.Data.RelativeSourceMode.TemplatedParent)
        });
        border.SetBinding(Border.BorderThicknessProperty, new System.Windows.Data.Binding(nameof(Button.BorderThickness))
        {
            RelativeSource = new System.Windows.Data.RelativeSource(System.Windows.Data.RelativeSourceMode.TemplatedParent)
        });
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(9));
        border.SetValue(Border.RenderTransformOriginProperty, new Point(0.5, 0.5));
        border.SetValue(Border.RenderTransformProperty, new ScaleTransform(1, 1));

        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
        border.AppendChild(presenter);
        template.VisualTree = border;

        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0x66, 0xA6, 0xFF)), "ButtonBorder"));
        template.Triggers.Add(hover);
        var pressed = new Trigger { Property = Button.IsPressedProperty, Value = true };
        pressed.Setters.Add(new Setter(UIElement.OpacityProperty, 0.72, "ButtonBorder"));
        pressed.Setters.Add(new Setter(UIElement.RenderTransformProperty, new ScaleTransform(0.97, 0.97), "ButtonBorder"));
        template.Triggers.Add(pressed);
        return template;
    }
}
