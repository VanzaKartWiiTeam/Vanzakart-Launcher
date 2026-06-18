using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher;

public sealed class GameBananaFilePickerDialog : Window
{
    private readonly ListBox _filesList;

    public GameBananaFile? SelectedFile { get; private set; }

    public GameBananaFilePickerDialog(GameBananaMod mod)
    {
        Title = $"Choose download — {mod.Name}";
        Width = 700;
        Height = Math.Min(640, 310 + (mod.Files.Count * 112));
        MinHeight = 450;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        WindowStyle = WindowStyle.None;
        AllowsTransparency = true;
        Background = Brushes.Transparent;
        ShowInTaskbar = false;

        var card = new Border
        {
            Margin = new Thickness(20),
            Padding = new Thickness(26),
            CornerRadius = new CornerRadius(18),
            Background = new SolidColorBrush(Color.FromRgb(0x11, 0x18, 0x27)),
            BorderBrush = CreateRainbowBrush(animate: true),
            BorderThickness = new Thickness(1.8),
            Effect = new DropShadowEffect { BlurRadius = 36, ShadowDepth = 0, Opacity = 0.72, Color = Color.FromRgb(0x00, 0xF2, 0xFF) }
        };
        card.MouseLeftButtonDown += DragWindow;

        var layout = new DockPanel();
        var heading = new StackPanel { Margin = new Thickness(0, 0, 0, 20) };
        heading.Children.Add(new TextBlock
        {
            Text = "SELECT A VERSION",
            FontSize = 11,
            FontWeight = FontWeights.Bold,
            Foreground = new SolidColorBrush(Color.FromRgb(0x66, 0xA6, 0xFF))
        });
        heading.Children.Add(new TextBlock
        {
            Text = mod.Name,
            FontSize = 24,
            FontWeight = FontWeights.Black,
            Foreground = Brushes.White,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 5, 0, 0)
        });
        heading.Children.Add(new TextBlock
        {
            Text = $"The creator provides {mod.Files.Count} download options. Compare their description, size and release date, then choose the one to install.",
            FontSize = 13,
            Foreground = new SolidColorBrush(Color.FromRgb(0x93, 0xA1, 0xBF)),
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(0, 6, 0, 0)
        });
        DockPanel.SetDock(heading, Dock.Top);
        layout.Children.Add(heading);

        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right,
            Margin = new Thickness(0, 18, 0, 0)
        };
        var cancel = CreateButton("Cancel", false);
        cancel.Click += (_, _) => { DialogResult = false; Close(); };
        var install = CreateButton("Install selected", true);
        install.Margin = new Thickness(10, 0, 0, 0);
        install.Click += (_, _) => ConfirmSelection();
        buttons.Children.Add(cancel);
        buttons.Children.Add(install);
        DockPanel.SetDock(buttons, Dock.Bottom);
        layout.Children.Add(buttons);

        _filesList = new ListBox
        {
            ItemsSource = mod.Files,
            SelectedIndex = 0,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Foreground = Brushes.White,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            ItemContainerStyle = BuildItemContainerStyle(),
            ItemTemplate = BuildFileTemplate(),
            Padding = new Thickness(0)
        };
        _filesList.MouseDoubleClick += (_, _) => ConfirmSelection();
        layout.Children.Add(_filesList);

        card.Child = layout;
        Content = card;
    }

    private void ConfirmSelection()
    {
        if (_filesList.SelectedItem is not GameBananaFile selected) return;
        SelectedFile = selected;
        DialogResult = true;
        Close();
    }

    private static DataTemplate BuildFileTemplate()
    {
        var template = new DataTemplate(typeof(GameBananaFile));
        var grid = new FrameworkElementFactory(typeof(Grid));
        grid.AppendChild(CreateFileTextBlock(nameof(GameBananaFile.FileName), 15, FontWeights.Bold, Brushes.White, new Thickness(0, 0, 44, 0)));

        var description = CreateFileTextBlock(
            nameof(GameBananaFile.VariantDescription),
            12,
            FontWeights.Normal,
            new SolidColorBrush(Color.FromRgb(0xB3, 0xC0, 0xD8)),
            new Thickness(0, 27, 44, 0));
        description.SetValue(TextBlock.TextTrimmingProperty, TextTrimming.CharacterEllipsis);
        grid.AppendChild(description);

        var metadata = CreateFileTextBlock(
            nameof(GameBananaFile.MetadataText),
            11,
            FontWeights.SemiBold,
            new SolidColorBrush(Color.FromRgb(0x7E, 0xE8, 0xFF)),
            new Thickness(0, 52, 44, 0));
        grid.AppendChild(metadata);

        var arrow = new FrameworkElementFactory(typeof(TextBlock));
        arrow.SetValue(TextBlock.TextProperty, "›");
        arrow.SetValue(TextBlock.FontSizeProperty, 26d);
        arrow.SetValue(TextBlock.FontWeightProperty, FontWeights.Light);
        arrow.SetValue(TextBlock.ForegroundProperty, new SolidColorBrush(Color.FromRgb(0x66, 0xA6, 0xFF)));
        arrow.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Right);
        arrow.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
        grid.AppendChild(arrow);
        template.VisualTree = grid;
        return template;
    }

    private static FrameworkElementFactory CreateFileTextBlock(
        string property,
        double fontSize,
        FontWeight fontWeight,
        Brush foreground,
        Thickness margin)
    {
        var text = new FrameworkElementFactory(typeof(TextBlock));
        text.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding(property));
        text.SetValue(TextBlock.FontSizeProperty, fontSize);
        text.SetValue(TextBlock.FontWeightProperty, fontWeight);
        text.SetValue(TextBlock.ForegroundProperty, foreground);
        text.SetValue(TextBlock.MarginProperty, margin);
        text.SetValue(TextBlock.TextTrimmingProperty, TextTrimming.CharacterEllipsis);
        return text;
    }

    private static Style BuildItemContainerStyle()
    {
        var style = new Style(typeof(ListBoxItem));
        style.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(16, 13, 16, 13)));
        style.Setters.Add(new Setter(Control.MarginProperty, new Thickness(0, 0, 0, 10)));
        style.Setters.Add(new Setter(Control.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x17, 0x21, 0x38))));
        style.Setters.Add(new Setter(Control.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0x2E, 0x3C, 0x5A))));
        style.Setters.Add(new Setter(Control.BorderThicknessProperty, new Thickness(1)));
        style.Setters.Add(new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Stretch));
        style.Setters.Add(new Setter(Control.CursorProperty, Cursors.Hand));

        var template = new ControlTemplate(typeof(ListBoxItem));
        var border = new FrameworkElementFactory(typeof(Border), "ItemBorder");
        border.SetBinding(Border.BackgroundProperty, TemplateBinding(nameof(Control.Background)));
        border.SetBinding(Border.BorderBrushProperty, TemplateBinding(nameof(Control.BorderBrush)));
        border.SetBinding(Border.BorderThicknessProperty, TemplateBinding(nameof(Control.BorderThickness)));
        border.SetBinding(Border.PaddingProperty, TemplateBinding(nameof(Control.Padding)));
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(10));
        border.SetValue(Border.RenderTransformOriginProperty, new Point(0.5, 0.5));
        border.SetValue(Border.RenderTransformProperty, new ScaleTransform(1, 1));
        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        border.AppendChild(presenter);
        template.VisualTree = border;

        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x1D, 0x2A, 0x46)), "ItemBorder"));
        hover.Setters.Add(new Setter(Border.BorderBrushProperty, new SolidColorBrush(Color.FromRgb(0x55, 0x70, 0x9B)), "ItemBorder"));
        template.Triggers.Add(hover);

        var selected = new Trigger { Property = ListBoxItem.IsSelectedProperty, Value = true };
        selected.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x20, 0x2D, 0x4A)), "ItemBorder"));
        selected.Setters.Add(new Setter(Border.BorderBrushProperty, CreateRainbowBrush(animate: false), "ItemBorder"));
        selected.Setters.Add(new Setter(Border.BorderThicknessProperty, new Thickness(2), "ItemBorder"));
        selected.Setters.Add(new Setter(UIElement.EffectProperty, new DropShadowEffect
        {
            BlurRadius = 18,
            ShadowDepth = 0,
            Opacity = 0.32,
            Color = Color.FromRgb(0x00, 0xF2, 0xFF)
        }, "ItemBorder"));
        template.Triggers.Add(selected);
        style.Setters.Add(new Setter(Control.TemplateProperty, template));
        return style;
    }

    private static System.Windows.Data.Binding TemplateBinding(string property) => new(property)
    {
        RelativeSource = new System.Windows.Data.RelativeSource(System.Windows.Data.RelativeSourceMode.TemplatedParent)
    };

    private static Button CreateButton(string text, bool primary)
    {
        return new Button
        {
            Content = text,
            MinWidth = 110,
            Height = 38,
            Padding = new Thickness(15, 0, 15, 0),
            Foreground = Brushes.White,
            Background = new SolidColorBrush(primary
                ? Color.FromRgb(0x15, 0x1E, 0x33)
                : Color.FromRgb(0x21, 0x2B, 0x43)),
            BorderBrush = primary
                ? CreateRainbowBrush(animate: false)
                : new SolidColorBrush(Color.FromRgb(0x43, 0x51, 0x70)),
            BorderThickness = new Thickness(1),
            FontWeight = FontWeights.Bold,
            Cursor = Cursors.Hand,
            Template = CreateButtonTemplate(primary)
        };
    }

    private void DragWindow(object sender, MouseButtonEventArgs e)
    {
        if (e.LeftButton != MouseButtonState.Pressed ||
            FindAncestor<Button>(e.OriginalSource as DependencyObject) != null ||
            FindAncestor<ListBoxItem>(e.OriginalSource as DependencyObject) != null ||
            FindAncestor<ScrollBar>(e.OriginalSource as DependencyObject) != null)
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

    private static ControlTemplate CreateButtonTemplate(bool primary)
    {
        var template = new ControlTemplate(typeof(Button));
        var root = new FrameworkElementFactory(typeof(Grid));

        if (primary)
        {
            var glow = new FrameworkElementFactory(typeof(Border), "GlowBorder");
            glow.SetValue(Border.BackgroundProperty, CreateRainbowBrush(animate: false));
            glow.SetValue(Border.CornerRadiusProperty, new CornerRadius(10));
            glow.SetValue(UIElement.OpacityProperty, 0.25);
            glow.SetValue(FrameworkElement.MarginProperty, new Thickness(-3));
            glow.SetValue(UIElement.EffectProperty, new BlurEffect { Radius = 9 });
            root.AppendChild(glow);
        }

        var border = new FrameworkElementFactory(typeof(Border), "ButtonBorder");
        border.SetBinding(Border.BackgroundProperty, TemplateBinding(nameof(Button.Background)));
        border.SetBinding(Border.BorderBrushProperty, TemplateBinding(nameof(Button.BorderBrush)));
        border.SetBinding(Border.BorderThicknessProperty, TemplateBinding(nameof(Button.BorderThickness)));
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(9));
        border.SetValue(Border.RenderTransformOriginProperty, new Point(0.5, 0.5));
        border.SetValue(Border.RenderTransformProperty, new ScaleTransform(1, 1));
        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
        border.AppendChild(presenter);
        root.AppendChild(border);
        template.VisualTree = root;

        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BackgroundProperty, new SolidColorBrush(Color.FromRgb(0x1F, 0x2C, 0x4C)), "ButtonBorder"));
        if (primary)
        {
            hover.Setters.Add(new Setter(UIElement.OpacityProperty, 0.65, "GlowBorder"));
        }
        template.Triggers.Add(hover);
        var pressed = new Trigger { Property = Button.IsPressedProperty, Value = true };
        pressed.Setters.Add(new Setter(UIElement.RenderTransformProperty, new ScaleTransform(0.97, 0.97), "ButtonBorder"));
        template.Triggers.Add(pressed);
        return template;
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
}
