using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

namespace VanzaKartLauncher.Services;

public static class MarkdownHelper
{
    public static readonly DependencyProperty MarkdownTextProperty =
        DependencyProperty.RegisterAttached(
            "MarkdownText",
            typeof(string),
            typeof(MarkdownHelper),
            new PropertyMetadata(string.Empty, OnMarkdownTextChanged));

    public static string GetMarkdownText(DependencyObject obj) => (string)obj.GetValue(MarkdownTextProperty);
    public static void SetMarkdownText(DependencyObject obj, string value) => obj.SetValue(MarkdownTextProperty, value);

    private static void OnMarkdownTextChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is TextBlock textBlock)
        {
            var markdown = e.NewValue as string;
            textBlock.Inlines.Clear();
            if (string.IsNullOrWhiteSpace(markdown))
            {
                return;
            }

            // Standardize linebreaks
            markdown = markdown.Replace("\r\n", "\n").Replace("\r", "\n");
            var lines = markdown.Split('\n');

            for (int i = 0; i < lines.Length; i++)
            {
                var line = lines[i];

                if (string.IsNullOrWhiteSpace(line))
                {
                    textBlock.Inlines.Add(new LineBreak());
                    continue;
                }

                Inline inlineLine;
                if (line.StartsWith("# "))
                {
                    var run = new Run(line.Substring(2))
                    {
                        FontWeight = FontWeights.Bold,
                        FontSize = textBlock.FontSize + 5,
                        Foreground = SafeFindResource(textBlock, "TextPrimary", Brushes.White)
                    };
                    inlineLine = new Span(run);
                }
                else if (line.StartsWith("## "))
                {
                    var run = new Run(line.Substring(3))
                    {
                        FontWeight = FontWeights.Bold,
                        FontSize = textBlock.FontSize + 2,
                        Foreground = SafeFindResource(textBlock, "TextPrimary", Brushes.White)
                    };
                    inlineLine = new Span(run);
                }
                else if (line.StartsWith("- "))
                {
                    var span = new Span();
                    span.Inlines.Add(new Run("• ")
                    {
                        FontWeight = FontWeights.Bold,
                        Foreground = SafeFindResource(textBlock, "RainbowGradient", Brushes.DeepSkyBlue)
                    });
                    span.Inlines.Add(ParseInlineFormatting(line.Substring(2)));
                    inlineLine = span;
                }
                else
                {
                    inlineLine = ParseInlineFormatting(line);
                }

                textBlock.Inlines.Add(inlineLine);

                if (i < lines.Length - 1)
                {
                    textBlock.Inlines.Add(new LineBreak());
                }
            }
        }
    }

    private static Brush SafeFindResource(FrameworkElement element, string resourceKey, Brush fallback)
    {
        return element.TryFindResource(resourceKey) as Brush ?? fallback;
    }

    private static Inline ParseInlineFormatting(string text)
    {
        var span = new Span();
        // Parse bold **bold**
        var parts = text.Split(new[] { "**" }, StringSplitOptions.None);
        for (int i = 0; i < parts.Length; i++)
        {
            var part = parts[i];
            if (i % 2 == 1) // Odd index means inside **
            {
                span.Inlines.Add(new Run(part) { FontWeight = FontWeights.Bold });
            }
            else
            {
                // Parse italic *italic*
                var italicParts = part.Split('*');
                for (int j = 0; j < italicParts.Length; j++)
                {
                    var ip = italicParts[j];
                    if (j % 2 == 1) // Odd index means inside *
                    {
                        span.Inlines.Add(new Run(ip) { FontStyle = FontStyles.Italic });
                    }
                    else
                    {
                        span.Inlines.Add(new Run(ip));
                    }
                }
            }
        }
        return span;
    }
}
