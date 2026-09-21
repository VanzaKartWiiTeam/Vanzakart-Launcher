// Services/LocalizationService.cs
using System.Collections.Generic;
using System.ComponentModel;
using System.Globalization;
using VanzaKartLauncher.Localization;

namespace VanzaKartLauncher.Services;

/// <summary>
/// Central string catalogue for the launcher UI.
/// Bindings created by <see cref="LocExtension"/> point at the indexer of this
/// singleton, so changing <see cref="CurrentLanguage"/> re-renders every
/// localized element without restarting the launcher.
/// </summary>
public sealed class LocalizationService : INotifyPropertyChanged
{
    public const string English = "en";
    public const string Italian = "it";

    private static readonly IReadOnlyList<LanguageOption> Languages = new[]
    {
        new LanguageOption(English, "English", "EN"),
        new LanguageOption(Italian, "Italiano", "IT")
    };

    private Dictionary<string, string> _strings = StringsEn.Values;
    private string _currentLanguage = English;

    public static LocalizationService Instance { get; } = new();

    private LocalizationService()
    {
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>Raised after the active language changed, once the catalogue is swapped.</summary>
    public event EventHandler? LanguageChanged;

    public static IReadOnlyList<LanguageOption> AvailableLanguages => Languages;

    public string CurrentLanguage => _currentLanguage;

    public string this[string key] => Get(key);

    public static bool IsSupported(string? language)
        => !string.IsNullOrWhiteSpace(language)
           && Languages.Any(l => string.Equals(l.Code, language, StringComparison.OrdinalIgnoreCase));

    /// <summary>Picks the closest supported language for the current Windows UI culture.</summary>
    public static string DetectSystemLanguage()
    {
        try
        {
            var culture = CultureInfo.CurrentUICulture;
            while (culture != null && !string.IsNullOrEmpty(culture.Name))
            {
                if (culture.TwoLetterISOLanguageName.Equals(Italian, StringComparison.OrdinalIgnoreCase))
                {
                    return Italian;
                }

                if (ReferenceEquals(culture, culture.Parent))
                {
                    break;
                }

                culture = culture.Parent;
            }
        }
        catch
        {
            // Fall through to English.
        }

        return English;
    }

    public void SetLanguage(string? language)
    {
        var normalized = IsSupported(language) ? language!.ToLowerInvariant() : English;
        if (string.Equals(normalized, _currentLanguage, StringComparison.Ordinal) && _strings.Count > 0)
        {
            return;
        }

        _currentLanguage = normalized;
        _strings = normalized == Italian ? StringsIt.Values : StringsEn.Values;

        // "Item[]" tells WPF that every indexer value changed at once.
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs("Item[]"));
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(CurrentLanguage)));
        LanguageChanged?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>
    /// Returns the translated string, falling back to English and finally to the
    /// key itself so a missing entry is visible instead of blanking the UI.
    /// </summary>
    public string Get(string key)
    {
        if (string.IsNullOrEmpty(key))
        {
            return string.Empty;
        }

        if (_strings.TryGetValue(key, out var value))
        {
            return value;
        }

        return StringsEn.Values.TryGetValue(key, out var fallback) ? fallback : key;
    }

    public string Format(string key, params object?[] args)
    {
        var template = Get(key);
        if (args.Length == 0)
        {
            return template;
        }

        try
        {
            return string.Format(CultureInfo.CurrentCulture, template, args);
        }
        catch (FormatException)
        {
            return template;
        }
    }
}

/// <summary>
/// A selectable UI language. <paramref name="Badge"/> is a short code rather than a
/// flag emoji: Windows desktop fonts render regional indicators as bare letters.
/// </summary>
public sealed record LanguageOption(string Code, string DisplayName, string Badge)
{
    public string Label => $"{Badge}  —  {DisplayName}";
}
