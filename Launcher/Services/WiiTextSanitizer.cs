// Services/WiiTextSanitizer.cs
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace VanzaKartLauncher.Services;

/// <summary>
/// Cleans strings decoded from Wii save data (Mii names, license names, creator
/// names). Those fields are fixed-size UTF-16BE buffers, so they routinely carry
/// NUL padding, leftover control bytes and half-written surrogate pairs, all of
/// which WPF draws as an empty box.
/// </summary>
public static class WiiTextSanitizer
{
    public static string Clean(string? value)
    {
        if (string.IsNullOrEmpty(value))
        {
            return string.Empty;
        }

        var builder = new StringBuilder(value.Length);

        for (var i = 0; i < value.Length; i++)
        {
            var c = value[i];

            // NUL padding plus every other C0/C1 control character.
            if (char.IsControl(c))
            {
                continue;
            }

            // Replacement / non-characters produced by a bad decode.
            if (c is '\uFFFD' or '\uFFFE' or '\uFFFF')
            {
                continue;
            }

            if (char.IsHighSurrogate(c))
            {
                // Keep the pair only when it is complete, otherwise drop the orphan.
                if (i + 1 < value.Length && char.IsLowSurrogate(value[i + 1]))
                {
                    builder.Append(c).Append(value[i + 1]);
                    i++;
                }

                continue;
            }

            if (char.IsLowSurrogate(c))
            {
                continue;
            }

            builder.Append(c);
        }

        return builder.ToString().Trim();
    }

    /// <summary>
    /// Prepares a Wii string for the screen.
    /// <para>
    /// The Wii name keyboard has a "symbols" page whose glyphs live in the Unicode
    /// private use area (0xF000 + the symbol index, e.g. U+F043). Those code points
    /// mean nothing outside the Wii system font, so Windows draws an empty box no
    /// matter which font is used. Known symbols are mapped to a Unicode look-alike
    /// through <see cref="WiiSymbols"/>; the rest are dropped, which reads better
    /// than tofu. This is display-only: the value written back to a save still goes
    /// through <see cref="Clean"/>, so nothing is lost on disk.
    /// </para>
    /// </summary>
    public static string ToDisplay(string? value)
    {
        var cleaned = Clean(value);
        if (cleaned.Length == 0 || !cleaned.Any(IsPrivateUse))
        {
            return cleaned;
        }

        var builder = new StringBuilder(cleaned.Length);
        foreach (var c in cleaned)
        {
            if (!IsPrivateUse(c))
            {
                builder.Append(c);
                continue;
            }

            if (WiiSymbols.TryGetValue(c, out var replacement))
            {
                builder.Append(replacement);
            }
        }

        return builder.ToString().Trim();
    }

    private static bool IsPrivateUse(char c) => c >= '\uE000' && c <= '\uF8FF';

    /// <summary>
    /// Wii private-use symbols that have a reasonable Unicode equivalent.
    /// Extend this as more of the Wii symbol page is identified; anything missing
    /// is simply removed from the displayed name.
    /// </summary>
    private static readonly Dictionary<char, string> WiiSymbols = new()
    {
        // Intentionally sparse: only add an entry when the Wii glyph is known.
    };

    /// <summary>Returns the first character of the cleaned value, for avatar placeholders.</summary>
    public static string Initial(string? value, string fallback = "M")
    {
        var cleaned = ToDisplay(value);
        if (cleaned.Length == 0)
        {
            return fallback;
        }

        // Take the whole surrogate pair when the name starts with an astral character.
        var length = char.IsHighSurrogate(cleaned[0]) && cleaned.Length > 1 ? 2 : 1;
        return cleaned[..length].ToUpperInvariant();
    }
}
