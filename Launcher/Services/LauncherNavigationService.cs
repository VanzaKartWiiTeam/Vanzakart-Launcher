namespace VanzaKartLauncher.Services;

public sealed class LauncherNavigationService
{
    public event Action<string>? Navigated;

    public string CurrentTab { get; set; } = "Home";

    public void Navigate(string tab)
    {
        if (string.IsNullOrWhiteSpace(tab) || tab == CurrentTab)
        {
            return;
        }

        CurrentTab = tab;
        Navigated?.Invoke(tab);
    }
}
