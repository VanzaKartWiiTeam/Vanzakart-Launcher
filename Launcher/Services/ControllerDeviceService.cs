using System.Runtime.InteropServices;
using VanzaKartLauncher.Models;
using Windows.Gaming.Input;

namespace VanzaKartLauncher.Services;

public sealed class ControllerDeviceService
{
    private readonly object _lock = new();
    private readonly Dictionary<string, ControllerDeviceInfo> _knownDevices =
        new(StringComparer.OrdinalIgnoreCase);

    public event EventHandler? DevicesChanged;

    public ControllerDeviceService()
    {
        try
        {
            RawGameController.RawGameControllerAdded += (_, _) => DevicesChanged?.Invoke(this, EventArgs.Empty);
            RawGameController.RawGameControllerRemoved += (_, _) => DevicesChanged?.Invoke(this, EventArgs.Empty);
            Gamepad.GamepadAdded += (_, _) => DevicesChanged?.Invoke(this, EventArgs.Empty);
            Gamepad.GamepadRemoved += (_, _) => DevicesChanged?.Invoke(this, EventArgs.Empty);
        }
        catch
        {
            // WinRT hardware events may not be available on all execution contexts
        }
    }

    public IReadOnlyList<ControllerDeviceInfo> Scan()
    {
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var result = new List<ControllerDeviceInfo>();

        AddOrUpdate(
            new ControllerDeviceInfo
            {
                Id = "keyboard",
                DisplayName = "Tastiera",
                DolphinDevice = "DInput/0/Keyboard Mouse",
                Kind = ControllerDeviceKind.Keyboard,
                IsConnected = true
            },
            seen,
            result);

        for (var slot = 0; slot < 4; slot++)
        {
            if (!XInputReader.TryRead(slot, out _)) continue;
            AddOrUpdate(
                new ControllerDeviceInfo
                {
                    Id = $"xinput:{slot}",
                    DisplayName = $"Controller Xbox {slot + 1}",
                    DolphinDevice = $"XInput/{slot}/Gamepad",
                    Kind = ControllerDeviceKind.Xbox,
                    XInputSlot = slot,
                    IsConnected = true
                },
                seen,
                result);
        }

        try
        {
            var rawControllers = RawGameController.RawGameControllers;
            var indexesByName = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
            foreach (var raw in rawControllers)
            {
                var name = string.IsNullOrWhiteSpace(raw.DisplayName) ? "Controller generico" : raw.DisplayName.Trim();
                var kind = Classify(raw, name);
                var dolphinName = GetDolphinDeviceName(raw, name, kind);

                // XInput exposes the same physical Xbox pad through both APIs.
                if (kind == ControllerDeviceKind.Xbox && result.Any(d => d.Kind == ControllerDeviceKind.Xbox))
                {
                    continue;
                }

                indexesByName.TryGetValue(dolphinName, out var sameNameIndex);
                indexesByName[dolphinName] = sameNameIndex + 1;

                var id = string.IsNullOrWhiteSpace(raw.NonRoamableId)
                    ? $"raw:{raw.HardwareVendorId:X4}:{raw.HardwareProductId:X4}:{sameNameIndex}"
                    : $"raw:{raw.NonRoamableId}";
                var gamepad = Gamepad.FromGameController(raw);

                AddOrUpdate(
                    new ControllerDeviceInfo
                    {
                        Id = id,
                        DisplayName = name,
                        DolphinDevice = $"SDL/{sameNameIndex}/{dolphinName}",
                        Kind = kind,
                        UsesRawInputLayout = gamepad is null,
                        RuntimeDevice = (object?)gamepad ?? raw,
                        IsConnected = true
                    },
                    seen,
                    result);
            }
        }
        catch
        {
            // Handle transient device enumeration errors during PnP initialization
        }

        lock (_lock)
        {
            foreach (var known in _knownDevices.Values.Where(d => !seen.Contains(d.Id)))
            {
                known.IsConnected = false;
                known.RuntimeDevice = null;
            }
        }

        return result
            .OrderBy(d => d.Kind == ControllerDeviceKind.Keyboard ? 1 : 0)
            .ThenBy(d => d.DisplayName, StringComparer.CurrentCultureIgnoreCase)
            .ToArray();
    }

    public ControllerInputSnapshot Read(ControllerDeviceInfo? device)
    {
        if (device is null || !device.IsConnected) return ControllerInputSnapshot.Empty;
        if (device.Kind == ControllerDeviceKind.Keyboard) return ControllerInputSnapshot.Empty;

        if (device.XInputSlot >= 0 && XInputReader.TryRead(device.XInputSlot, out var xinput))
        {
            return xinput;
        }

        if (device.RuntimeDevice is Gamepad gamepad)
        {
            return FromGamepad(gamepad.GetCurrentReading());
        }

        if (device.RuntimeDevice is RawGameController raw)
        {
            return FromRawController(raw);
        }

        return ControllerInputSnapshot.Empty;
    }

    public void SetVibration(ControllerDeviceInfo device, double strength)
    {
        strength = Math.Clamp(strength, 0, 1);
        if (device.XInputSlot >= 0)
        {
            XInputReader.SetVibration(device.XInputSlot, strength);
            return;
        }

        if (device.RuntimeDevice is Gamepad gamepad)
        {
            gamepad.Vibration = new GamepadVibration
            {
                LeftMotor = strength,
                RightMotor = strength,
                LeftTrigger = strength * 0.35,
                RightTrigger = strength * 0.35
            };
            return;
        }

        throw new NotSupportedException("Il dispositivo non espone un motore di vibrazione standard.");
    }

    private void AddOrUpdate(
        ControllerDeviceInfo candidate,
        HashSet<string> seen,
        ICollection<ControllerDeviceInfo> output)
    {
        seen.Add(candidate.Id);
        lock (_lock)
        {
            if (_knownDevices.TryGetValue(candidate.Id, out var existing))
            {
                existing.IsConnected = true;
                existing.RuntimeDevice = candidate.RuntimeDevice;
                output.Add(existing);
                return;
            }

            _knownDevices[candidate.Id] = candidate;
            output.Add(candidate);
        }
    }

    private static ControllerDeviceKind Classify(RawGameController raw, string name)
    {
        if (raw.HardwareVendorId == 0x054C ||
            name.Contains("DualSense", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("DualShock", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PlayStation", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PS4", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PS5", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.PlayStation;
        }

        if (raw.HardwareVendorId == 0x057E ||
            name.Contains("Nintendo", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("Switch", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.Switch;
        }

        if (raw.HardwareVendorId == 0x045E ||
            name.Contains("Xbox", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("XInput", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.Xbox;
        }

        return ControllerDeviceKind.Generic;
    }

    private static string GetDolphinDeviceName(
        RawGameController raw,
        string detectedName,
        ControllerDeviceKind kind)
    {
        if (kind != ControllerDeviceKind.PlayStation)
        {
            return detectedName;
        }

        // Windows often reports Sony pads simply as "Wireless Controller",
        // while Dolphin/SDL uses the model-specific device qualifier.
        return raw.HardwareProductId switch
        {
            0x0CE6 or 0x0DF2 => "DualSense Wireless Controller",
            0x05C4 or 0x09CC or 0x0BA0 or 0x0CDA => "PS4 Controller",
            _ when detectedName.Contains("DualSense", StringComparison.OrdinalIgnoreCase) =>
                "DualSense Wireless Controller",
            _ when detectedName.Equals("Wireless Controller", StringComparison.OrdinalIgnoreCase) =>
                "PS4 Controller",
            _ => detectedName
        };
    }

    private static ControllerInputSnapshot FromGamepad(GamepadReading reading)
    {
        var pressed = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        AddButton(pressed, reading.Buttons, GamepadButtons.A, "Button S");
        AddButton(pressed, reading.Buttons, GamepadButtons.B, "Button E");
        AddButton(pressed, reading.Buttons, GamepadButtons.X, "Button W");
        AddButton(pressed, reading.Buttons, GamepadButtons.Y, "Button N");
        AddButton(pressed, reading.Buttons, GamepadButtons.LeftShoulder, "Shoulder L");
        AddButton(pressed, reading.Buttons, GamepadButtons.RightShoulder, "Shoulder R");
        AddButton(pressed, reading.Buttons, GamepadButtons.Menu, "Start");
        AddButton(pressed, reading.Buttons, GamepadButtons.View, "Back");
        AddButton(pressed, reading.Buttons, GamepadButtons.DPadUp, "D-Pad Up");
        AddButton(pressed, reading.Buttons, GamepadButtons.DPadDown, "D-Pad Down");
        AddButton(pressed, reading.Buttons, GamepadButtons.DPadLeft, "D-Pad Left");
        AddButton(pressed, reading.Buttons, GamepadButtons.DPadRight, "D-Pad Right");
        AddButton(pressed, reading.Buttons, GamepadButtons.LeftThumbstick, "Left Stick Click");
        AddButton(pressed, reading.Buttons, GamepadButtons.RightThumbstick, "Right Stick Click");
        AddAxes(
            pressed,
            reading.LeftThumbstickX,
            reading.LeftThumbstickY,
            reading.RightThumbstickX,
            reading.RightThumbstickY,
            reading.LeftTrigger,
            reading.RightTrigger);

        return new ControllerInputSnapshot(
            reading.LeftThumbstickX,
            reading.LeftThumbstickY,
            reading.RightThumbstickX,
            reading.RightThumbstickY,
            reading.LeftTrigger,
            reading.RightTrigger,
            pressed);
    }

    private static ControllerInputSnapshot FromRawController(RawGameController raw)
    {
        var buttons = new bool[raw.ButtonCount];
        var switches = new GameControllerSwitchPosition[raw.SwitchCount];
        var axes = new double[raw.AxisCount];
        raw.GetCurrentReading(buttons, switches, axes);

        var pressed = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        for (var i = 0; i < buttons.Length; i++)
        {
            if (buttons[i]) pressed.Add($"Button {i}");
        }

        for (var i = 0; i < switches.Length; i++)
        {
            var value = switches[i];
            if (value is GameControllerSwitchPosition.Up or GameControllerSwitchPosition.UpLeft or GameControllerSwitchPosition.UpRight)
                pressed.Add("D-Pad Up");
            if (value is GameControllerSwitchPosition.Down or GameControllerSwitchPosition.DownLeft or GameControllerSwitchPosition.DownRight)
                pressed.Add("D-Pad Down");
            if (value is GameControllerSwitchPosition.Left or GameControllerSwitchPosition.UpLeft or GameControllerSwitchPosition.DownLeft)
                pressed.Add("D-Pad Left");
            if (value is GameControllerSwitchPosition.Right or GameControllerSwitchPosition.UpRight or GameControllerSwitchPosition.DownRight)
                pressed.Add("D-Pad Right");
        }

        double Axis(int index, double center = 0.5) =>
            axes.Length > index ? Math.Clamp((axes[index] - center) * 2, -1, 1) : 0;
        var lx = Axis(0);
        var ly = -Axis(1);
        var rx = Axis(2);
        var ry = -Axis(3);
        var lt = axes.Length > 4 ? Math.Clamp(axes[4], 0, 1) : 0;
        var rt = axes.Length > 5 ? Math.Clamp(axes[5], 0, 1) : 0;
        // Raw/DirectInput devices do not expose the standardized SDL gamepad
        // names. Keep the physical axis number so the captured expression is
        // also valid for Dolphin's generic SDL input backend.
        const double threshold = 0.55;
        for (var i = 0; i < axes.Length; i++)
        {
            var normalized = Math.Clamp((axes[i] - 0.5) * 2, -1, 1);
            if (normalized <= -threshold) pressed.Add($"Axis {i}-");
            if (normalized >= threshold) pressed.Add($"Axis {i}+");
        }

        return new ControllerInputSnapshot(lx, ly, rx, ry, lt, rt, pressed);
    }

    private static void AddButton(
        ISet<string> pressed,
        GamepadButtons current,
        GamepadButtons flag,
        string name)
    {
        if ((current & flag) != 0) pressed.Add(name);
    }

    private static void AddAxes(
        ISet<string> pressed,
        double lx,
        double ly,
        double rx,
        double ry,
        double lt,
        double rt)
    {
        const double threshold = 0.55;
        if (lx <= -threshold) pressed.Add("Left X-");
        if (lx >= threshold) pressed.Add("Left X+");
        if (ly <= -threshold) pressed.Add("Left Y-");
        if (ly >= threshold) pressed.Add("Left Y+");
        if (rx <= -threshold) pressed.Add("Right X-");
        if (rx >= threshold) pressed.Add("Right X+");
        if (ry <= -threshold) pressed.Add("Right Y-");
        if (ry >= threshold) pressed.Add("Right Y+");
        if (lt >= threshold) pressed.Add("Trigger L");
        if (rt >= threshold) pressed.Add("Trigger R");
    }

    private static class XInputReader
    {
        private const int Success = 0;

        public static bool TryRead(int slot, out ControllerInputSnapshot snapshot)
        {
            snapshot = ControllerInputSnapshot.Empty;
            XInputState state;
            int result;
            try
            {
                result = XInputGetState14(slot, out state);
            }
            catch (DllNotFoundException)
            {
                result = XInputGetState13(slot, out state);
            }
            catch (EntryPointNotFoundException)
            {
                result = XInputGetState13(slot, out state);
            }

            if (result != Success) return false;

            var gamepad = state.Gamepad;
            var pressed = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            AddXButton(pressed, gamepad.Buttons, 0x1000, "Button A");
            AddXButton(pressed, gamepad.Buttons, 0x2000, "Button B");
            AddXButton(pressed, gamepad.Buttons, 0x4000, "Button X");
            AddXButton(pressed, gamepad.Buttons, 0x8000, "Button Y");
            AddXButton(pressed, gamepad.Buttons, 0x0100, "Shoulder L");
            AddXButton(pressed, gamepad.Buttons, 0x0200, "Shoulder R");
            AddXButton(pressed, gamepad.Buttons, 0x0010, "Menu");
            AddXButton(pressed, gamepad.Buttons, 0x0020, "Back");
            AddXButton(pressed, gamepad.Buttons, 0x0001, "D-Pad Up");
            AddXButton(pressed, gamepad.Buttons, 0x0002, "D-Pad Down");
            AddXButton(pressed, gamepad.Buttons, 0x0004, "D-Pad Left");
            AddXButton(pressed, gamepad.Buttons, 0x0008, "D-Pad Right");
            AddXButton(pressed, gamepad.Buttons, 0x0040, "Left Stick Click");
            AddXButton(pressed, gamepad.Buttons, 0x0080, "Right Stick Click");

            var lx = NormalizeThumb(gamepad.LeftThumbX);
            var ly = NormalizeThumb(gamepad.LeftThumbY);
            var rx = NormalizeThumb(gamepad.RightThumbX);
            var ry = NormalizeThumb(gamepad.RightThumbY);
            var lt = gamepad.LeftTrigger / 255d;
            var rt = gamepad.RightTrigger / 255d;
            AddAxes(pressed, lx, ly, rx, ry, lt, rt);

            snapshot = new ControllerInputSnapshot(lx, ly, rx, ry, lt, rt, pressed);
            return true;
        }

        public static void SetVibration(int slot, double strength)
        {
            var value = (ushort)Math.Round(Math.Clamp(strength, 0, 1) * ushort.MaxValue);
            var vibration = new XInputVibration
            {
                LeftMotorSpeed = value,
                RightMotorSpeed = value
            };

            try
            {
                XInputSetState14(slot, ref vibration);
            }
            catch (DllNotFoundException)
            {
                XInputSetState13(slot, ref vibration);
            }
            catch (EntryPointNotFoundException)
            {
                XInputSetState13(slot, ref vibration);
            }
        }

        private static double NormalizeThumb(short value) =>
            value < 0 ? value / 32768d : value / 32767d;

        private static void AddXButton(ISet<string> pressed, ushort current, ushort flag, string name)
        {
            if ((current & flag) != 0) pressed.Add(name);
        }

        [DllImport("xinput1_4.dll", EntryPoint = "XInputGetState")]
        private static extern int XInputGetState14(int userIndex, out XInputState state);

        [DllImport("xinput1_3.dll", EntryPoint = "XInputGetState")]
        private static extern int XInputGetState13(int userIndex, out XInputState state);

        [DllImport("xinput1_4.dll", EntryPoint = "XInputSetState")]
        private static extern int XInputSetState14(int userIndex, ref XInputVibration vibration);

        [DllImport("xinput1_3.dll", EntryPoint = "XInputSetState")]
        private static extern int XInputSetState13(int userIndex, ref XInputVibration vibration);

        [StructLayout(LayoutKind.Sequential)]
        private struct XInputState
        {
            public uint PacketNumber;
            public XInputGamepad Gamepad;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct XInputGamepad
        {
            public ushort Buttons;
            public byte LeftTrigger;
            public byte RightTrigger;
            public short LeftThumbX;
            public short LeftThumbY;
            public short RightThumbX;
            public short RightThumbY;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct XInputVibration
        {
            public ushort LeftMotorSpeed;
            public ushort RightMotorSpeed;
        }
    }
}
