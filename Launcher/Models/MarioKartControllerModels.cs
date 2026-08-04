using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace VanzaKartLauncher.Models;

public enum DolphinControllerMode
{
    LauncherConfiguration,
    ConfigureWithDolphin
}

public enum MarioKartBindingKind
{
    Single,
    Trigger,
    Steering
}

public sealed class MarioKartActionBinding : INotifyPropertyChanged
{
    private string _displayBinding = "Unassigned";
    private bool _isListening;
    private bool _hasConflict;

    public required string Id { get; init; }
    public required string Section { get; init; }
    public required string Icon { get; init; }
    public required string Title { get; init; }
    public required string Description { get; init; }
    public required MarioKartBindingKind Kind { get; init; }
    public required IReadOnlyList<string> DolphinKeys { get; init; }
    public Dictionary<string, string> Values { get; } = new(StringComparer.OrdinalIgnoreCase);

    public string DisplayBinding
    {
        get => _displayBinding;
        set
        {
            if (_displayBinding == value) return;
            _displayBinding = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(BindingButtonText));
        }
    }

    public string BindingButtonText => IsListening ? "Premi un input…" : DisplayBinding;

    public bool IsListening
    {
        get => _isListening;
        set
        {
            if (_isListening == value) return;
            _isListening = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(BindingButtonText));
        }
    }

    public bool HasConflict
    {
        get => _hasConflict;
        set
        {
            if (_hasConflict == value) return;
            _hasConflict = value;
            OnPropertyChanged();
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public void SetValue(string key, string value)
    {
        Values[key] = value;
        OnPropertyChanged(nameof(Values));
    }

    public void Clear()
    {
        Values.Clear();
        DisplayBinding = "Unassigned";
        OnPropertyChanged(nameof(Values));
    }

    private void OnPropertyChanged([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}

public sealed class MarioKartControllerProfile
{
    public required ControllerDeviceInfo Device { get; init; }
    public List<MarioKartActionBinding> Actions { get; } = new();
    public double Deadzone { get; set; } = 10;
    public double Sensitivity { get; set; } = 100;
    public bool Vibration { get; set; } = true;
    public bool LoadedFromDolphin { get; set; }
    public string? ConfiguredDolphinDevice { get; set; }
}
