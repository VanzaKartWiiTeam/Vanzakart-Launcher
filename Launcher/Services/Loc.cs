// Services/Loc.cs
namespace VanzaKartLauncher.Services;

/// <summary>Short entry point used from code-behind: <c>Loc.T("Key")</c>.</summary>
public static class Loc
{
    public static LocalizationService Service => LocalizationService.Instance;

    public static string T(string key) => LocalizationService.Instance.Get(key);

    public static string Format(string key, params object?[] args) => LocalizationService.Instance.Format(key, args);
}
