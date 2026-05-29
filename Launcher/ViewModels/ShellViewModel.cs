namespace VanzaKartLauncher.ViewModels;

public sealed class ShellViewModel : BaseViewModel
{
    private string _currentTab = "Home";
    private bool _isBusy;
    private string _status = "Ready";

    public string CurrentTab
    {
        get => _currentTab;
        set => SetProperty(ref _currentTab, value);
    }

    public bool IsBusy
    {
        get => _isBusy;
        set => SetProperty(ref _isBusy, value);
    }

    public string Status
    {
        get => _status;
        set => SetProperty(ref _status, value);
    }
}
