using System.Globalization;
using System.IO;
using VanzaKartLauncher.Models;

namespace VanzaKartLauncher.Services;

public sealed class MarioKartControllerConfigurationService
{
    private readonly DolphinControllerProfileManager _profiles = new();
    private readonly DolphinIniService _ini = new();

    public DolphinControllerMode DetectMode(string userFolder)
    {
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            return DolphinControllerMode.LauncherConfiguration;
        }

        var dolphinPath = Path.Combine(userFolder, "Config", "Dolphin.ini");
        var dolphin = _ini.ReadIni(dolphinPath);
        if (!dolphin.TryGetValue("Core", out var core))
        {
            return DolphinControllerMode.LauncherConfiguration;
        }

        var emulatedWiimoteEnabled =
            core.TryGetValue("WiimoteSource0", out var wiimoteSource) &&
            string.Equals(wiimoteSource, "1", StringComparison.OrdinalIgnoreCase);
        var gameCubeEnabled =
            core.TryGetValue("SIDevice0", out var siDevice) &&
            string.Equals(siDevice, "6", StringComparison.OrdinalIgnoreCase);

        return emulatedWiimoteEnabled && !gameCubeEnabled
            ? DolphinControllerMode.ConfigureWithDolphin
            : DolphinControllerMode.LauncherConfiguration;
    }

    public void ActivateMode(string userFolder, DolphinControllerMode mode)
    {
        if (mode == DolphinControllerMode.ConfigureWithDolphin)
        {
            // Dolphin remains the sole owner of its controller files in this mode.
            return;
        }
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            throw new InvalidOperationException("Select the Dolphin User folder first.");
        }

        var configDir = Path.Combine(userFolder, "Config");
        Directory.CreateDirectory(configDir);
        var dolphinPath = Path.Combine(configDir, "Dolphin.ini");
        var backup = Backup(dolphinPath);

        try
        {
            _ini.UpdateIniOrThrow(
                dolphinPath,
                new Dictionary<string, Dictionary<string, string>>
                {
                    ["Core"] = BuildControllerSourceSettings(mode)
                });
            VerifyControllerSourceSettings(dolphinPath, mode);
        }
        catch
        {
            Restore(dolphinPath, backup);
            throw;
        }
        finally
        {
            DeleteBackup(backup);
        }
    }

    public static bool IsAllowedSharedBinding(IEnumerable<string> actionIds)
    {
        var ids = actionIds
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        return ids.Count == 2 && ids.Contains("brake") && ids.Contains("drift");
    }

    public string GetConfiguredDevice(string userFolder)
    {
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            return string.Empty;
        }

        var current = ReadEffectiveBindings(userFolder);
        return current.TryGetValue("Device", out var device)
            ? device.Trim()
            : string.Empty;
    }

    public static bool IsSameDolphinDevice(string configuredDevice, ControllerDeviceInfo candidate)
    {
        if (string.IsNullOrWhiteSpace(configuredDevice))
        {
            return false;
        }

        if (string.Equals(configuredDevice, candidate.DolphinDevice, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        var configured = ParseDevice(configuredDevice);
        var detected = ParseDevice(candidate.DolphinDevice);

        if (configured.Backend.Equals("XInput", StringComparison.OrdinalIgnoreCase) &&
            candidate.XInputSlot >= 0)
        {
            return configured.Index == candidate.XInputSlot;
        }

        if (configured.Index != detected.Index)
        {
            return false;
        }

        var configuredName = NormalizeDeviceName(configured.Name);
        var detectedName = NormalizeDeviceName(candidate.DisplayName);
        if (configuredName.Length > 2 &&
            detectedName.Length > 2 &&
            (configuredName.Equals(detectedName, StringComparison.OrdinalIgnoreCase) ||
             configuredName.Contains(detectedName, StringComparison.OrdinalIgnoreCase) ||
             detectedName.Contains(configuredName, StringComparison.OrdinalIgnoreCase)))
        {
            return true;
        }

        var configuredKind = InferKind(configured.Name);
        return configuredKind != ControllerDeviceKind.Generic &&
               configuredKind == candidate.Kind &&
               (configuredName.Length <= 2 || detectedName.Length <= 2);
    }

    public static ControllerDeviceInfo CreateDisconnectedDevice(string configuredDevice)
    {
        var parsed = ParseDevice(configuredDevice);
        var displayName = string.IsNullOrWhiteSpace(parsed.Name)
            ? "Dolphin controller"
            : parsed.Name;
        return new ControllerDeviceInfo
        {
            Id = $"dolphin:{configuredDevice}",
            DisplayName = displayName,
            DolphinDevice = configuredDevice,
            Kind = InferKind(displayName),
            UsesRawInputLayout = parsed.Backend.Equals("DInput", StringComparison.OrdinalIgnoreCase),
            IsConnected = false
        };
    }

    public MarioKartControllerProfile Load(string userFolder, ControllerDeviceInfo device)
    {
        var profile = CreateDefault(device);
        if (string.IsNullOrWhiteSpace(userFolder)) return profile;

        var current = ReadEffectiveBindings(userFolder);
        if (!current.TryGetValue("Device", out var configuredDevice) ||
            !IsSameDolphinDevice(configuredDevice, device))
        {
            return profile;
        }

        profile.LoadedFromDolphin = true;
        var configuredIdentity = ParseDevice(configuredDevice);
        profile.ConfiguredDolphinDevice =
            device.Kind == ControllerDeviceKind.PlayStation &&
            configuredIdentity.Name.Equals("Wireless Controller", StringComparison.OrdinalIgnoreCase)
                ? device.DolphinDevice
                : configuredDevice;
        profile.Deadzone = ReadNumber(current, "Main Stick/Dead Zone", 10);
        profile.Sensitivity = ReadNumber(current, "VanzaKart/Sensitivity", 100);
        profile.Vibration = current.TryGetValue("Rumble/Motor", out var rumble) &&
                            !string.IsNullOrWhiteSpace(rumble);

        foreach (var action in profile.Actions)
        {
            action.Values.Clear();
            foreach (var key in action.DolphinKeys)
            {
                if (!current.TryGetValue(key, out var value)) continue;
                var normalizedValue = NormalizeLegacyControllerBinding(value, device);
                action.Values[key] = IsSteeringKey(key)
                    ? RemoveSensitivityWrapper(normalizedValue, profile.Sensitivity)
                    : normalizedValue;
            }
            UpdateDisplay(action, device.Kind);
        }

        return profile;
    }

    public void Save(string userFolder, MarioKartControllerProfile profile)
    {
        if (string.IsNullOrWhiteSpace(userFolder))
            throw new InvalidOperationException("Select the Dolphin User folder first.");
        if (!profile.Device.IsConnected)
            throw new InvalidOperationException("The selected controller is not connected.");

        var configDir = Path.Combine(userFolder, "Config");
        Directory.CreateDirectory(configDir);
        var padPath = Path.Combine(configDir, "GCPadNew.ini");
        var dolphinPath = Path.Combine(configDir, "Dolphin.ini");
        var padBackup = Backup(padPath);
        var dolphinBackup = Backup(dolphinPath);

        try
        {
            var existing = ReadEffectiveBindings(userFolder);
            var merged = new Dictionary<string, string>(existing, StringComparer.OrdinalIgnoreCase)
            {
                ["Device"] = string.IsNullOrWhiteSpace(profile.ConfiguredDolphinDevice)
                    ? profile.Device.DolphinDevice
                    : profile.ConfiguredDolphinDevice,
                ["Main Stick/Dead Zone"] = FormatNumber(profile.Deadzone),
                ["C-Stick/Dead Zone"] = FormatNumber(profile.Deadzone),
                ["VanzaKart/Sensitivity"] = FormatNumber(profile.Sensitivity),
                ["Rumble/Motor"] = profile.Vibration
                    ? profile.Device.Kind == ControllerDeviceKind.Xbox
                        ? "`Motor L` | `Motor R`"
                        : "`Motor`"
                    : ""
            };

            foreach (var action in profile.Actions)
            {
                foreach (var key in action.DolphinKeys)
                {
                    action.Values.TryGetValue(key, out var value);
                    merged[key] = IsSteeringKey(key)
                        ? AddSensitivityWrapper(value ?? "", profile.Sensitivity)
                        : value ?? "";
                }
            }

            _ini.UpdateIniOrThrow(
                padPath,
                new Dictionary<string, Dictionary<string, string>>
                {
                    ["GCPad1"] = merged
                });
            _ini.UpdateIniOrThrow(
                dolphinPath,
                new Dictionary<string, Dictionary<string, string>>
                {
                    ["Core"] = BuildControllerSourceSettings(DolphinControllerMode.LauncherConfiguration)
                });
            VerifyControllerSourceSettings(
                dolphinPath,
                DolphinControllerMode.LauncherConfiguration);

            profile.LoadedFromDolphin = true;
        }
        catch
        {
            Restore(padPath, padBackup);
            Restore(dolphinPath, dolphinBackup);
            throw;
        }
        finally
        {
            DeleteBackup(padBackup);
            DeleteBackup(dolphinBackup);
        }
    }

    public MarioKartControllerProfile CreateDefault(ControllerDeviceInfo device)
    {
        var profile = new MarioKartControllerProfile { Device = device };
        profile.Actions.AddRange(CreateActions());
        var defaults = DefaultsFor(device);

        foreach (var action in profile.Actions)
        {
            foreach (var key in action.DolphinKeys)
            {
                if (defaults.TryGetValue(key, out var value)) action.Values[key] = value;
            }
            UpdateDisplay(action, device.Kind);
        }

        return profile;
    }

    public void ResetRecommended(MarioKartControllerProfile profile)
    {
        var defaults = CreateDefault(profile.Device);
        profile.Deadzone = defaults.Deadzone;
        profile.Sensitivity = defaults.Sensitivity;
        profile.Vibration = defaults.Vibration;

        foreach (var action in profile.Actions)
        {
            var source = defaults.Actions.First(a => a.Id == action.Id);
            action.Values.Clear();
            foreach (var item in source.Values) action.Values[item.Key] = item.Value;
            action.DisplayBinding = source.DisplayBinding;
            action.HasConflict = false;
        }
    }

    public static IReadOnlyList<MarioKartActionBinding> CreateActions() =>
    [
        Action("drive", "RACING", "⚡", "Drive", "Accelerate and launch from the starting line", MarioKartBindingKind.Single, "Buttons/A"),
        Action("brake", "RACING", "◼", "Brake / Reverse", "Brake and reverse · can share a button with Drift", MarioKartBindingKind.Single, "Buttons/B"),
        Action("drift", "RACING", "↗", "Drift / Hop", "Hop and drift · can share a button with Brake", MarioKartBindingKind.Trigger, "Triggers/R", "Triggers/R-Analog"),
        Action("item", "RACING", "◆", "Item", "Use or hold an item behind the kart", MarioKartBindingKind.Trigger, "Triggers/L", "Triggers/L-Analog"),
        Action("look_back", "RACING", "◉", "Look Back", "Look behind the vehicle", MarioKartBindingKind.Single, "Buttons/X"),
        Action("pause", "RACING", "Ⅱ", "Pause", "Pause the current race", MarioKartBindingKind.Single, "Buttons/Start"),
        Action("steering", "MOVEMENT", "↔", "Steering", "Turn the kart and navigate menus", MarioKartBindingKind.Steering,
            "Main Stick/Up", "Main Stick/Down", "Main Stick/Left", "Main Stick/Right"),
        Action("trick_up", "MOVEMENT", "↑", "Trick Up", "Wheelie on bikes or perform an upward trick", MarioKartBindingKind.Single, "D-Pad/Up"),
        Action("trick_down", "MOVEMENT", "↓", "Trick Down", "Perform a downward trick", MarioKartBindingKind.Single, "D-Pad/Down"),
        Action("trick_left", "MOVEMENT", "←", "Trick Left", "Perform a left trick", MarioKartBindingKind.Single, "D-Pad/Left"),
        Action("trick_right", "MOVEMENT", "→", "Trick Right", "Perform a right trick", MarioKartBindingKind.Single, "D-Pad/Right")
    ];

    public static void UpdateDisplay(MarioKartActionBinding action, ControllerDeviceKind kind)
    {
        if (action.Values.Count == 0 || action.Values.Values.All(string.IsNullOrWhiteSpace))
        {
            action.DisplayBinding = "Unassigned";
            return;
        }

        if (action.Kind == MarioKartBindingKind.Steering)
        {
            action.DisplayBinding = DetectDirectionalFamily(action.Values, kind, steering: true);
            return;
        }

        action.DisplayBinding = FriendlyInput(action.Values.Values.FirstOrDefault(v => !string.IsNullOrWhiteSpace(v)) ?? "", kind);
    }

    public static string FriendlyInput(string raw, ControllerDeviceKind kind)
    {
        var value = raw.Trim().Trim('`');
        if (string.IsNullOrWhiteSpace(value)) return "Unassigned";

        if (kind == ControllerDeviceKind.PlayStation)
        {
            return value switch
            {
                "Button S" => "✕ Cross",
                "Button E" => "○ Circle",
                "Button W" => "□ Square",
                "Button N" => "△ Triangle",
                "Button 0" => "□ Square",
                "Button 1" => "✕ Cross",
                "Button 2" => "○ Circle",
                "Button 3" => "△ Triangle",
                "Button 4" => "L1",
                "Button 5" => "R1",
                "Button 6" => "L2",
                "Button 7" => "R2",
                "Button 8" => "Create / Share",
                "Button 9" => "Options",
                "Button 10" => "L3",
                "Button 11" => "R3",
                "Button 12" => "PS",
                "Button 13" => "Touch Pad",
                "Start" or "Menu" => "Options",
                "Back" => "Create",
                "Shoulder L" => "L1",
                "Shoulder R" => "R1",
                "Trigger L" => "L2",
                "Trigger R" => "R2",
                _ => FriendlyCommon(value)
            };
        }

        if (kind == ControllerDeviceKind.Switch)
        {
            return value switch
            {
                "Button S" => "B",
                "Button E" => "A",
                "Button W" => "Y",
                "Button N" => "X",
                "Start" or "Menu" => "+",
                "Back" => "−",
                "Shoulder L" => "L",
                "Shoulder R" => "R",
                "Trigger L" => "ZL",
                "Trigger R" => "ZR",
                _ => FriendlyCommon(value)
            };
        }

        return value switch
        {
            "Button A" => "A",
            "Button B" => "B",
            "Button X" => "X",
            "Button Y" => "Y",
            "Menu" or "Start" => "Menu",
            "Back" => "View",
            "Shoulder L" => "LB",
            "Shoulder R" => "RB",
            "Trigger L" => "LT",
            "Trigger R" => "RT",
            _ => FriendlyCommon(value)
        };
    }

    private static string FriendlyCommon(string value)
    {
        if (value.StartsWith("Button ", StringComparison.OrdinalIgnoreCase))
        {
            return value switch
            {
                "Button 0" => "Primary button",
                "Button 1" => "Secondary button",
                "Button 2" => "Left face button",
                "Button 3" => "Top face button",
                _ => "Extra button"
            };
        }
        if (value.StartsWith("Axis ", StringComparison.OrdinalIgnoreCase))
        {
            return value switch
            {
                "Axis 0-" => "Left stick ←",
                "Axis 0+" => "Left stick →",
                "Axis 1-" => "Left stick ↑",
                "Axis 1+" => "Left stick ↓",
                "Axis 2-" => "Right stick ←",
                "Axis 2+" => "Right stick →",
                "Axis 3-" => "Right stick ↑",
                "Axis 3+" => "Right stick ↓",
                _ => value.EndsWith("-", StringComparison.Ordinal)
                    ? "Analog axis −"
                    : "Analog axis +"
            };
        }

        return value switch
        {
            "Left X-" => "Left stick ←",
            "Left X+" => "Left stick →",
            "Left Y-" => "Left stick ↓",
            "Left Y+" => "Left stick ↑",
            "Right X-" => "Right stick ←",
            "Right X+" => "Right stick →",
            "Right Y-" => "Right stick ↓",
            "Right Y+" => "Right stick ↑",
            "D-Pad Up" => "D-pad ↑",
            "D-Pad Down" => "D-pad ↓",
            "D-Pad Left" => "D-pad ←",
            "D-Pad Right" => "D-pad →",
            "Pad N" => "D-pad ↑",
            "Pad S" => "D-pad ↓",
            "Pad W" => "D-pad ←",
            "Pad E" => "D-pad →",
            "UP" => "Arrow ↑",
            "DOWN" => "Arrow ↓",
            "LEFT" => "Arrow ←",
            "RIGHT" => "Arrow →",
            "RETURN" => "Enter",
            "CONTROL" => "Ctrl",
            "SHIFT" => "Shift",
            "SPACE" => "Space",
            _ => value.Replace("KEY_", "", StringComparison.OrdinalIgnoreCase)
        };
    }

    private static string DetectDirectionalFamily(
        IReadOnlyDictionary<string, string> values,
        ControllerDeviceKind kind,
        bool steering)
    {
        var normalized = values.Values.Select(v => v.Trim().Trim('`')).ToArray();
        if (normalized.Any(v => v.StartsWith("Left ", StringComparison.OrdinalIgnoreCase))) return "Left stick · 4 directions";
        if (normalized.Any(v => v.StartsWith("Right ", StringComparison.OrdinalIgnoreCase))) return "Right stick · 4 directions";
        if (normalized.Any(v => v.StartsWith("Axis 0", StringComparison.OrdinalIgnoreCase))) return "Left stick · 4 directions";
        if (normalized.Any(v =>
                v.StartsWith("D-Pad", StringComparison.OrdinalIgnoreCase) ||
                v.StartsWith("Pad ", StringComparison.OrdinalIgnoreCase)))
            return "D-pad · 4 directions";
        if (normalized.Contains("W", StringComparer.OrdinalIgnoreCase) &&
            normalized.Contains("A", StringComparer.OrdinalIgnoreCase)) return "WASD";
        if (normalized.Contains("UP", StringComparer.OrdinalIgnoreCase) &&
            normalized.Contains("LEFT", StringComparer.OrdinalIgnoreCase)) return "Arrow keys";
        if (normalized.Distinct(StringComparer.OrdinalIgnoreCase).Count() == 1)
            return FriendlyInput(values.Values.First(), kind);
        return steering ? "Custom steering" : "Custom directions";
    }

    private static IReadOnlyDictionary<string, string> DefaultsFor(ControllerDeviceInfo device)
    {
        var kind = device.Kind;
        if (kind == ControllerDeviceKind.Keyboard)
        {
            return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Buttons/A"] = "`SPACE`",
                ["Buttons/B"] = "`C`",
                ["Triggers/R"] = "`SHIFT`",
                ["Triggers/R-Analog"] = "`SHIFT`",
                ["Triggers/L"] = "`E`",
                ["Triggers/L-Analog"] = "`E`",
                ["Buttons/X"] = "`Q`",
                ["Buttons/Start"] = "`RETURN`",
                ["Main Stick/Up"] = "`W`",
                ["Main Stick/Down"] = "`S`",
                ["Main Stick/Left"] = "`A`",
                ["Main Stick/Right"] = "`D`",
                ["D-Pad/Up"] = "`UP`",
                ["D-Pad/Down"] = "`DOWN`",
                ["D-Pad/Left"] = "`LEFT`",
                ["D-Pad/Right"] = "`RIGHT`"
            };
        }

        if (device.UsesRawInputLayout)
        {
            if (kind == ControllerDeviceKind.PlayStation)
            {
                return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
                {
                    ["Buttons/A"] = "`Button S`",
                    ["Buttons/B"] = "`Button E`",
                    ["Triggers/R"] = "`Trigger R`",
                    ["Triggers/R-Analog"] = "`Trigger R`",
                    ["Triggers/L"] = "`Trigger L`",
                    ["Triggers/L-Analog"] = "`Trigger L`",
                    ["Buttons/X"] = "`Button N`",
                    ["Buttons/Start"] = "`Start`",
                    ["Main Stick/Up"] = "`Left Y+`",
                    ["Main Stick/Down"] = "`Left Y-`",
                    ["Main Stick/Left"] = "`Left X-`",
                    ["Main Stick/Right"] = "`Left X+`",
                    ["D-Pad/Up"] = "`Pad N`",
                    ["D-Pad/Down"] = "`Pad S`",
                    ["D-Pad/Left"] = "`Pad W`",
                    ["D-Pad/Right"] = "`Pad E`"
                };
            }

            return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["Buttons/A"] = "`Button 0`",
                ["Buttons/B"] = "`Button 1`",
                ["Triggers/R"] = "`Button 5`",
                ["Triggers/R-Analog"] = "`Button 5`",
                ["Triggers/L"] = "`Button 4`",
                ["Triggers/L-Analog"] = "`Button 4`",
                ["Buttons/X"] = "`Button 3`",
                ["Buttons/Start"] = "`Button 9`",
                ["Main Stick/Up"] = "`Axis 1-`",
                ["Main Stick/Down"] = "`Axis 1+`",
                ["Main Stick/Left"] = "`Axis 0-`",
                ["Main Stick/Right"] = "`Axis 0+`",
                ["D-Pad/Up"] = "`Pad N`",
                ["D-Pad/Down"] = "`Pad S`",
                ["D-Pad/Left"] = "`Pad W`",
                ["D-Pad/Right"] = "`Pad E`"
            };
        }

        var xbox = kind == ControllerDeviceKind.Xbox;
        return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["Buttons/A"] = xbox ? "`Button A`" : "`Button S`",
            ["Buttons/B"] = xbox ? "`Button B`" : "`Button E`",
            ["Triggers/R"] = "`Trigger R`",
            ["Triggers/R-Analog"] = "`Trigger R`",
            ["Triggers/L"] = "`Trigger L`",
            ["Triggers/L-Analog"] = "`Trigger L`",
            ["Buttons/X"] = xbox ? "`Button X`" : "`Button N`",
            ["Buttons/Start"] = xbox ? "`Menu`" : "`Start`",
            ["Main Stick/Up"] = "`Left Y+`",
            ["Main Stick/Down"] = "`Left Y-`",
            ["Main Stick/Left"] = "`Left X-`",
            ["Main Stick/Right"] = "`Left X+`",
            ["D-Pad/Up"] = "`Pad N`",
            ["D-Pad/Down"] = "`Pad S`",
            ["D-Pad/Left"] = "`Pad W`",
            ["D-Pad/Right"] = "`Pad E`"
        };
    }

    private static string NormalizeLegacyControllerBinding(
        string binding,
        ControllerDeviceInfo device)
    {
        var value = binding.Trim().Trim('`');
        var normalized = value switch
        {
            "D-Pad Up" => "Pad N",
            "D-Pad Down" => "Pad S",
            "D-Pad Left" => "Pad W",
            "D-Pad Right" => "Pad E",
            _ => value
        };

        if (device.Kind == ControllerDeviceKind.PlayStation &&
            device.UsesRawInputLayout)
        {
            normalized = normalized switch
            {
                "Button 0" => "Button W",
                "Button 1" => "Button S",
                "Button 2" => "Button E",
                "Button 3" => "Button N",
                "Button 4" => "Shoulder L",
                "Button 5" => "Shoulder R",
                "Button 6" => "Trigger L",
                "Button 7" => "Trigger R",
                "Button 8" => "Back",
                "Button 9" => "Start",
                "Axis 0-" => "Left X-",
                "Axis 0+" => "Left X+",
                "Axis 1-" => "Left Y+",
                "Axis 1+" => "Left Y-",
                _ => normalized
            };
        }

        return binding.StartsWith('`') && binding.EndsWith('`')
            ? $"`{normalized}`"
            : normalized;
    }

    private static MarioKartActionBinding Action(
        string id,
        string section,
        string icon,
        string title,
        string description,
        MarioKartBindingKind kind,
        params string[] keys) =>
        new()
        {
            Id = id,
            Section = section,
            Icon = icon,
            Title = title,
            Description = description,
            Kind = kind,
            DolphinKeys = keys
        };

    private static bool IsSteeringKey(string key) =>
        key.StartsWith("Main Stick/", StringComparison.OrdinalIgnoreCase) &&
        (key.EndsWith("/Up", StringComparison.OrdinalIgnoreCase) ||
         key.EndsWith("/Down", StringComparison.OrdinalIgnoreCase) ||
         key.EndsWith("/Left", StringComparison.OrdinalIgnoreCase) ||
         key.EndsWith("/Right", StringComparison.OrdinalIgnoreCase));

    private Dictionary<string, string> ReadEffectiveBindings(string userFolder)
    {
        var active = _profiles.ReadActiveBindings(userFolder, isWiimote: false, portIndex: 1);
        if (!active.TryGetValue("Profile", out var profileName) ||
            string.IsNullOrWhiteSpace(profileName))
        {
            return active;
        }

        var profile = _profiles.ReadProfile(
            userFolder,
            isWiimote: false,
            profileName.Trim().Trim('"', '`'));
        foreach (var item in profile)
        {
            active.TryAdd(item.Key, item.Value);
        }
        return active;
    }

    private static (string Backend, int Index, string Name) ParseDevice(string value)
    {
        var parts = value.Split('/', 3, StringSplitOptions.TrimEntries);
        var backend = parts.Length > 0 ? parts[0] : string.Empty;
        var index = parts.Length > 1 &&
                    int.TryParse(parts[1], NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed)
            ? parsed
            : 0;
        var name = parts.Length > 2 ? parts[2] : value;
        return (backend, index, name);
    }

    private static ControllerDeviceKind InferKind(string name)
    {
        if (name.Contains("DualSense", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("DualShock", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PlayStation", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PS4", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("PS5", StringComparison.OrdinalIgnoreCase) ||
            name.Equals("Wireless Controller", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.PlayStation;
        }

        if (name.Contains("Nintendo", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("Switch", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("Joy-Con", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.Switch;
        }

        if (name.Contains("Xbox", StringComparison.OrdinalIgnoreCase) ||
            name.Contains("XInput", StringComparison.OrdinalIgnoreCase) ||
            name.Equals("Gamepad", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.Xbox;
        }

        if (name.Contains("Keyboard", StringComparison.OrdinalIgnoreCase))
        {
            return ControllerDeviceKind.Keyboard;
        }

        return ControllerDeviceKind.Generic;
    }

    private static string NormalizeDeviceName(string name)
    {
        var chars = name
            .Where(char.IsLetterOrDigit)
            .Select(char.ToLowerInvariant)
            .ToArray();
        var normalized = new string(chars);
        foreach (var genericWord in new[]
                 {
                     "wireless", "controller", "gamepad", "input", "usb",
                     "sonyinteractiveentertainment"
                 })
        {
            normalized = normalized.Replace(genericWord, string.Empty, StringComparison.OrdinalIgnoreCase);
        }
        return normalized;
    }

    private static Dictionary<string, string> BuildControllerSourceSettings(DolphinControllerMode mode) =>
        mode == DolphinControllerMode.ConfigureWithDolphin
            ? new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["SIDevice0"] = "0",
                ["WiimoteSource0"] = "1"
            }
            : new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["SIDevice0"] = "6",
                ["WiimoteSource0"] = "0"
            };

    private void VerifyControllerSourceSettings(string dolphinIniPath, DolphinControllerMode mode)
    {
        var expected = BuildControllerSourceSettings(mode);
        var ini = _ini.ReadIni(dolphinIniPath);
        if (!ini.TryGetValue("Core", out var core))
        {
            throw new IOException("Dolphin.ini does not contain the Core section after saving.");
        }

        foreach (var item in expected)
        {
            if (!core.TryGetValue(item.Key, out var actual) ||
                !string.Equals(actual, item.Value, StringComparison.OrdinalIgnoreCase))
            {
                throw new IOException(
                    $"Dolphin rejected controller setting {item.Key}. Expected {item.Value}, found {actual ?? "missing"}.");
            }
        }
    }

    private static double ReadNumber(
        IReadOnlyDictionary<string, string> values,
        string key,
        double fallback)
    {
        if (!values.TryGetValue(key, out var text)) return fallback;
        return double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out var value)
            ? value
            : fallback;
    }

    private static string FormatNumber(double value) =>
        Math.Clamp(value, 0, 200).ToString("0.0", CultureInfo.InvariantCulture);

    private static string AddSensitivityWrapper(string binding, double sensitivity)
    {
        if (string.IsNullOrWhiteSpace(binding) || Math.Abs(sensitivity - 100) < 0.01) return binding;
        var factor = (sensitivity / 100d).ToString("0.###", CultureInfo.InvariantCulture);
        return $"({binding} * {factor})";
    }

    private static string RemoveSensitivityWrapper(string binding, double sensitivity)
    {
        var factor = (sensitivity / 100d).ToString("0.###", CultureInfo.InvariantCulture);
        var suffix = $" * {factor})";
        return binding.StartsWith("(", StringComparison.Ordinal) &&
               binding.EndsWith(suffix, StringComparison.Ordinal)
            ? binding[1..^suffix.Length]
            : binding;
    }

    private static string? Backup(string path)
    {
        if (!File.Exists(path)) return null;
        var backup = $"{path}.{Guid.NewGuid():N}.bak";
        File.Copy(path, backup);
        return backup;
    }

    private static void Restore(string path, string? backup)
    {
        if (backup is null)
        {
            if (File.Exists(path)) File.Delete(path);
            return;
        }
        File.Copy(backup, path, overwrite: true);
    }

    private static void DeleteBackup(string? path)
    {
        if (path is not null && File.Exists(path)) File.Delete(path);
    }
}
