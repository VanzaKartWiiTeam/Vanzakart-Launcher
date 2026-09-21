// Services/LocExtension.cs
using System.Windows.Data;
using System.Windows.Markup;

namespace VanzaKartLauncher.Services;

/// <summary>
/// XAML markup extension used as <c>{s:Loc Some_Key}</c>. It resolves to a one-way
/// binding against <see cref="LocalizationService.Instance"/>, which is what makes
/// switching language update the whole window live.
/// </summary>
[MarkupExtensionReturnType(typeof(object))]
public sealed class LocExtension : MarkupExtension
{
    public LocExtension()
    {
    }

    public LocExtension(string key)
    {
        Key = key;
    }

    [ConstructorArgument("key")]
    public string Key { get; set; } = string.Empty;

    /// <summary>Optional text appended after the translated value (e.g. a trailing separator).</summary>
    public string Suffix { get; set; } = string.Empty;

    /// <summary>Optional text prepended before the translated value.</summary>
    public string Prefix { get; set; } = string.Empty;

    public override object ProvideValue(IServiceProvider serviceProvider)
    {
        var binding = new Binding($"[{Key}]")
        {
            Source = LocalizationService.Instance,
            Mode = BindingMode.OneWay,
            FallbackValue = Key
        };

        if (!string.IsNullOrEmpty(Prefix) || !string.IsNullOrEmpty(Suffix))
        {
            binding.StringFormat = $"{EscapeFormat(Prefix)}{{0}}{EscapeFormat(Suffix)}";
        }

        return binding.ProvideValue(serviceProvider);
    }

    private static string EscapeFormat(string value)
        => value.Replace("{", "{{", StringComparison.Ordinal).Replace("}", "}}", StringComparison.Ordinal);
}
