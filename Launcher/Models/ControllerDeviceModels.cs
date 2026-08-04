using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace VanzaKartLauncher.Models;

public enum ControllerDeviceKind
{
    Keyboard,
    Xbox,
    PlayStation,
    Switch,
    Generic
}

public sealed class ControllerDeviceInfo : INotifyPropertyChanged
{
    private bool _isConnected;
    private object? _runtimeDevice;

    public required string Id { get; init; }
    public required string DisplayName { get; init; }
    public required string DolphinDevice { get; init; }
    public required ControllerDeviceKind Kind { get; init; }
    public bool UsesRawInputLayout { get; init; }
    public int XInputSlot { get; init; } = -1;
    public string Icon => Kind switch
    {
        ControllerDeviceKind.Keyboard => "⌨",
        ControllerDeviceKind.PlayStation => "△",
        ControllerDeviceKind.Switch => "◆",
        _ => "🎮"
    };
    public string TypeLabel => Kind switch
    {
        ControllerDeviceKind.Keyboard => "Keyboard",
        ControllerDeviceKind.Xbox => "Xbox / XInput",
        ControllerDeviceKind.PlayStation => "PlayStation",
        ControllerDeviceKind.Switch => "Nintendo",
        ControllerDeviceKind.Generic when UsesRawInputLayout => "Generic / DirectInput",
        _ => "Gamepad"
    };
    public string ConnectionLabel => IsConnected ? "Connected" : "Disconnected";

    public bool IsConnected
    {
        get => _isConnected;
        set
        {
            if (_isConnected == value) return;
            _isConnected = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(ConnectionLabel));
        }
    }

    public object? RuntimeDevice
    {
        get => _runtimeDevice;
        set => _runtimeDevice = value;
    }

    public override string ToString() => $"{DisplayName} · {ConnectionLabel}";

    public event PropertyChangedEventHandler? PropertyChanged;

    private void OnPropertyChanged([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}

public sealed record ControllerInputSnapshot(
    double LeftX,
    double LeftY,
    double RightX,
    double RightY,
    double LeftTrigger,
    double RightTrigger,
    IReadOnlySet<string> PressedInputs)
{
    public static ControllerInputSnapshot Empty { get; } =
        new(0, 0, 0, 0, 0, 0, new HashSet<string>(StringComparer.OrdinalIgnoreCase));
}
