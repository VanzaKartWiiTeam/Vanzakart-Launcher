using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;
using System.Windows.Threading;
using VanzaKartLauncher.Models;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher.Views;

public partial class MarioKartControllerPanel : UserControl, INotifyPropertyChanged
{
    private readonly ControllerDeviceService _deviceService = new();
    private readonly MarioKartControllerConfigurationService _configurationService = new();
    private readonly SettingsService _settingsService = new();
    private readonly DispatcherTimer _inputTimer;
    private readonly DispatcherTimer _deviceTimer;
    private ControllerDeviceInfo? _selectedDevice;
    private MarioKartControllerProfile? _profile;
    private MarioKartActionBinding? _captureAction;
    private MarioKartActionBinding? _captureDisplayAction;
    private HashSet<string> _captureBaseline = new(StringComparer.OrdinalIgnoreCase);
    private string _pageStatus = "Detecting controller";
    private double _deadzone = 10;
    private double _sensitivity = 100;
    private bool _vibration = true;
    private bool _isLoading;
    private bool _isDirty;
    private bool _hasConflicts;
    private string _lastDeviceSignature = "";
    private DolphinControllerMode _controllerMode = DolphinControllerMode.LauncherConfiguration;
    private int _scanInProgress;

    public MarioKartControllerPanel()
    {
        _isLoading = true;
        InitializeComponent();
        DataContext = this;

        _inputTimer = new DispatcherTimer(DispatcherPriority.Input)
        {
            Interval = TimeSpan.FromMilliseconds(16)
        };
        _inputTimer.Tick += InputTimer_OnTick;

        _deviceTimer = new DispatcherTimer(DispatcherPriority.Background)
        {
            Interval = TimeSpan.FromSeconds(2.5)
        };
        _deviceTimer.Tick += (_, _) => _ = RefreshDevicesAsync(showMessage: false);

        _deviceService.DevicesChanged += (_, _) =>
        {
            _ = Dispatcher.InvokeAsync(() => _ = RefreshDevicesAsync(showMessage: false));
        };
        _isLoading = false;
    }

    public Func<string>? UserFolderResolver { get; set; }
    public ObservableCollection<ControllerDeviceInfo> Devices { get; } = new();
    public ObservableCollection<MarioKartActionBinding> RaceActions { get; } = new();
    public ObservableCollection<MarioKartActionBinding> MovementActions { get; } = new();

    public ControllerDeviceInfo? SelectedDevice
    {
        get => _selectedDevice;
        set
        {
            if (ReferenceEquals(_selectedDevice, value)) return;
            _selectedDevice = value;
            OnPropertyChanged();
            UpdateConnectionStatus();
        }
    }

    public string PageStatus
    {
        get => _pageStatus;
        private set => SetField(ref _pageStatus, value);
    }

    public double Deadzone
    {
        get => _deadzone;
        set => SetField(ref _deadzone, Math.Clamp(value, 0, 35));
    }

    public double Sensitivity
    {
        get => _sensitivity;
        set => SetField(ref _sensitivity, Math.Clamp(value, 60, 140));
    }

    public bool Vibration
    {
        get => _vibration;
        set => SetField(ref _vibration, value);
    }

    public bool IsDirty => _isDirty;

    public event EventHandler? ConfigurationChanged;
    public event EventHandler<string>? StatusChanged;
    public event EventHandler? PlayRequested;
    public event PropertyChangedEventHandler? PropertyChanged;

    public void ReloadFromDolphin()
    {
        var userFolder = ResolveUserFolder();
        var configuredDevice = _configurationService.GetConfiguredDevice(userFolder);
        _ = RefreshDevicesAsync(
            showMessage: false,
            preferredDolphinDevice: configuredDevice,
            loadChangedSelection: false);
        ApplyControllerMode(
            LoadPreferredControllerMode(),
            markDirty: false);
        if (SelectedDevice is not null) LoadSelectedDevice();
    }

    public bool SaveToDolphin()
    {
        var userFolder = ResolveUserFolder();
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            SetStatus(Loc.T("Msg_SelectTheDolphinUserFolderFirst2"), isError: true);
            return false;
        }

        if (_controllerMode == DolphinControllerMode.ConfigureWithDolphin)
        {
            try
            {
                PersistControllerMode();
                _isDirty = false;
                SaveHintText.Text = Loc.T("Msg_DolphinRemainsInControlNoController");
                SaveHintText.Foreground = BrushFrom("#72E6B4");
                PageStatus = Loc.T("Msg_ManagedByDolphin");
                PageStatusDot.Fill = BrushFrom("#50E7A7");
                SetStatus(Loc.T("Msg_ControllerConfigurationLeftEntirely"));
                return true;
            }
            catch (Exception ex)
            {
                SetStatus(Loc.Format("Msg_CouldNotSaveTheControllerMode", ex.Message), isError: true);
                return false;
            }
        }

        if (_profile is null || SelectedDevice is null)
        {
            SetStatus(Loc.T("Msg_ConnectAndSelectAControllerBefore"), isError: true);
            return false;
        }

        ValidateMappings();
        if (_hasConflicts)
        {
            SetStatus(Loc.T("Msg_ResolveTheHighlightedDuplicate"), isError: true);
            return false;
        }

        if (!SelectedDevice.IsConnected)
        {
            SetStatus(Loc.T("Msg_TheSelectedControllerIsNotConnected"), isError: true);
            return false;
        }

        try
        {
            _profile.Deadzone = Deadzone;
            _profile.Sensitivity = Sensitivity;
            _profile.Vibration = Vibration;
            _configurationService.Save(userFolder, _profile);
            PersistControllerMode();
            _isDirty = false;
            SaveHintText.Text = Loc.T("Msg_ConfigurationAppliedMarioKart");
            SaveHintText.Foreground = BrushFrom("#72E6B4");
            PageStatus = Loc.T("Msg_ReadyToRace");
            PageStatusDot.Fill = BrushFrom("#50E7A7");
            SetStatus(Loc.T("Msg_ControllerConfiguredForMario"));
            return true;
        }
        catch (Exception ex)
        {
            SetStatus(Loc.Format("Msg_CouldNotSaveTheController", ex.Message), isError: true);
            return false;
        }
    }

    private void Panel_OnLoaded(object sender, RoutedEventArgs e)
    {
        Root.BeginAnimation(
            OpacityProperty,
            new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(280))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            });
        if (IsVisible)
        {
            _inputTimer.Start();
            _deviceTimer.Start();
        }
        ReloadFromDolphin();
    }

    private void Panel_OnUnloaded(object sender, RoutedEventArgs e)
    {
        _inputTimer.Stop();
        _deviceTimer.Stop();
        CancelCapture();
    }

    private void Panel_OnIsVisibleChanged(object sender, DependencyPropertyChangedEventArgs e)
    {
        if (IsVisible)
        {
            _inputTimer.Start();
            _deviceTimer.Start();
            _ = RefreshDevicesAsync(showMessage: false);
        }
        else
        {
            _inputTimer.Stop();
            _deviceTimer.Stop();
            CancelCapture();
        }
    }

    private void RefreshButton_OnClick(object sender, RoutedEventArgs e)
    {
        RefreshButton.BeginAnimation(
            OpacityProperty,
            new DoubleAnimation(0.55, 1, TimeSpan.FromMilliseconds(180))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            });
        _ = RefreshDevicesAsync(showMessage: true);
    }

    public bool PrepareControllerModeForLaunch()
    {
        var userFolder = ResolveUserFolder();
        if (string.IsNullOrWhiteSpace(userFolder))
        {
            SetStatus(Loc.T("Msg_SelectTheDolphinUserFolderFirst2"), isError: true);
            return false;
        }

        var selectedMode = LoadPreferredControllerMode();
        if (selectedMode == DolphinControllerMode.ConfigureWithDolphin)
        {
            return true;
        }

        try
        {
            _configurationService.ActivateMode(
                userFolder,
                DolphinControllerMode.LauncherConfiguration);
            return true;
        }
        catch (Exception ex)
        {
            SetStatus(Loc.Format("Msg_CouldNotActivateTheLauncherController", ex.Message), isError: true);
            return false;
        }
    }

    private async Task RefreshDevicesAsync(
        bool showMessage,
        string? preferredDolphinDevice = null,
        bool loadChangedSelection = true)
    {
        if (Interlocked.CompareExchange(ref _scanInProgress, 1, 0) != 0)
        {
            return;
        }

        try
        {
            IReadOnlyList<ControllerDeviceInfo> detected;
            try
            {
                detected = await Task.Run(() => _deviceService.Scan());
            }
            catch (Exception ex)
            {
                if (showMessage) SetStatus(Loc.Format("Msg_CouldNotDetectControllers", ex.Message), isError: true);
                return;
            }

            var previousId = SelectedDevice?.Id;
            var previousDevice = SelectedDevice;
            var reconnectedEquivalent = previousDevice is null
                ? null
                : detected.FirstOrDefault(device =>
                    device.IsConnected &&
                    MarioKartControllerConfigurationService.IsSameDolphinDevice(
                        previousDevice.DolphinDevice,
                        device));
            var wasLoading = _isLoading;
            _isLoading = true;
            try
            {
                var desired = detected.ToList();
                var configuredMatch = string.IsNullOrWhiteSpace(preferredDolphinDevice)
                    ? null
                    : desired.FirstOrDefault(device =>
                        MarioKartControllerConfigurationService.IsSameDolphinDevice(
                            preferredDolphinDevice,
                            device));

                if (!string.IsNullOrWhiteSpace(preferredDolphinDevice) && configuredMatch is null)
                {
                    configuredMatch = MarioKartControllerConfigurationService.CreateDisconnectedDevice(
                        preferredDolphinDevice);
                    desired.Add(configuredMatch);
                }

                if (previousDevice is not null &&
                    desired.All(d => !string.Equals(d.Id, previousDevice.Id, StringComparison.OrdinalIgnoreCase)))
                {
                    previousDevice.IsConnected = false;
                    desired.Add(previousDevice);
                }

                var currentIds = Devices.Select(d => $"{d.Id}:{d.IsConnected}").ToArray();
                var newSorted = desired
                    .OrderByDescending(d => d.IsConnected && d.Kind != ControllerDeviceKind.Keyboard)
                    .ThenBy(d => d.Kind == ControllerDeviceKind.Keyboard)
                    .ThenBy(d => d.DisplayName, StringComparer.CurrentCultureIgnoreCase)
                    .ToArray();
                var newIds = newSorted.Select(d => $"{d.Id}:{d.IsConnected}").ToArray();

                if (!currentIds.SequenceEqual(newIds))
                {
                    Devices.Clear();
                    foreach (var device in newSorted)
                    {
                        Devices.Add(device);
                    }
                }

                SelectedDevice = !string.IsNullOrWhiteSpace(preferredDolphinDevice)
                    ? Devices.FirstOrDefault(d => ReferenceEquals(d, configuredMatch)) ??
                      Devices.FirstOrDefault(d =>
                          MarioKartControllerConfigurationService.IsSameDolphinDevice(
                              preferredDolphinDevice,
                              d))
                    : reconnectedEquivalent is not null
                        ? Devices.FirstOrDefault(d => ReferenceEquals(d, reconnectedEquivalent))
                    : previousId is null
                        ? Devices.FirstOrDefault(d => d.IsConnected && d.Kind != ControllerDeviceKind.Keyboard) ??
                          Devices.FirstOrDefault(d => d.IsConnected)
                    : Devices.FirstOrDefault(d => string.Equals(d.Id, previousId, StringComparison.OrdinalIgnoreCase)) ??
                      Devices.FirstOrDefault(d => d.IsConnected && d.Kind != ControllerDeviceKind.Keyboard) ??
                      Devices.FirstOrDefault(d => d.IsConnected);
            }
            finally
            {
                _isLoading = wasLoading;
            }

            var selectionChanged = !ReferenceEquals(previousDevice, SelectedDevice);
            if (selectionChanged && loadChangedSelection && !wasLoading && SelectedDevice is not null)
            {
                LoadSelectedDevice();
            }

            UpdateConnectionStatus();
            var signature = string.Join("|", Devices.Where(d => d.IsConnected).Select(d => d.Id));
            if (!string.Equals(signature, _lastDeviceSignature, StringComparison.Ordinal))
            {
                if (!string.IsNullOrEmpty(_lastDeviceSignature) && SelectedDevice is not null)
                {
                    SetStatus(SelectedDevice.IsConnected
                        ? Loc.Format("Msg_IsReady", SelectedDevice.DisplayName)
                        : Loc.T("Msg_ControllerDisconnectedConnect"));
                }
                _lastDeviceSignature = signature;
            }
            else if (showMessage)
            {
                SetStatus(Loc.T("Msg_ControllerListRefreshed"));
            }
        }
        finally
        {
            Interlocked.Exchange(ref _scanInProgress, 0);
        }
    }

    private void DeviceComboBox_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isLoading || SelectedDevice is null) return;
        LoadSelectedDevice();
    }

    private void LauncherModeButton_OnClick(object sender, RoutedEventArgs e) =>
        ApplyControllerMode(DolphinControllerMode.LauncherConfiguration, markDirty: true);

    private void WiimoteModeButton_OnClick(object sender, RoutedEventArgs e) =>
        ApplyControllerMode(DolphinControllerMode.ConfigureWithDolphin, markDirty: true);

    private void ApplyControllerMode(DolphinControllerMode mode, bool markDirty)
    {
        var changed = _controllerMode != mode;
        _controllerMode = mode;
        CancelCapture();
        UpdateControllerModeUi();
        ValidateMappings();

        if (changed && markDirty)
        {
            MarkDirty();
        }
        else
        {
            UpdateConnectionStatus();
        }
    }

    private void UpdateControllerModeUi()
    {
        var isManaged = _controllerMode == DolphinControllerMode.LauncherConfiguration;
        ManagedDeviceCard.Visibility = isManaged ? Visibility.Visible : Visibility.Collapsed;
        ManagedConfigurationGrid.Visibility = isManaged ? Visibility.Visible : Visibility.Collapsed;
        WiimoteModeNotice.Visibility = isManaged ? Visibility.Collapsed : Visibility.Visible;
        RecommendedSetupButton.Visibility = isManaged ? Visibility.Visible : Visibility.Collapsed;

        LauncherModeButton.Background = BrushFrom(isManaged ? "#203A61" : "#1B2A45");
        LauncherModeButton.BorderBrush = BrushFrom(isManaged ? "#6EA8FF" : "#344B70");
        WiimoteModeButton.Background = BrushFrom(isManaged ? "#1B2A45" : "#302B55");
        WiimoteModeButton.BorderBrush = BrushFrom(isManaged ? "#344B70" : "#887CFF");
        LauncherModeDot.Fill = BrushFrom(isManaged ? "#50E7A7" : "#40516D");
        WiimoteModeDot.Fill = BrushFrom(isManaged ? "#40516D" : "#887CFF");

        if (!isManaged && !_isDirty)
        {
            SaveHintText.Text = Loc.T("Msg_DolphinOwnsTheControllerConfiguration");
            SaveHintText.Foreground = BrushFrom("#AAA9D1");
        }
    }

    private DolphinControllerMode LoadPreferredControllerMode()
    {
        var settings = _settingsService.Load();
        if (Enum.TryParse<DolphinControllerMode>(
                settings.ControllerConfigurationMode,
                ignoreCase: true,
                out var savedMode))
        {
            return savedMode;
        }

        // First-run migration: preserve an existing emulated Wii Remote setup;
        // everyone else starts with the recommended launcher workflow.
        return _configurationService.DetectMode(ResolveUserFolder());
    }

    private void PersistControllerMode()
    {
        var settings = _settingsService.Load();
        settings.ControllerConfigurationMode = _controllerMode.ToString();
        _settingsService.Save(settings);
    }

    private void LoadSelectedDevice()
    {
        if (SelectedDevice is null) return;
        _isLoading = true;
        try
        {
            var userFolder = ResolveUserFolder();
            _profile = _configurationService.Load(userFolder, SelectedDevice);
            Deadzone = _profile.Deadzone;
            Sensitivity = _profile.Sensitivity;
            Vibration = _profile.Vibration;

            RaceActions.Clear();
            MovementActions.Clear();
            foreach (var action in _profile.Actions)
            {
                if (action.Section == "RACING") RaceActions.Add(action);
                else MovementActions.Add(action);
            }

            _isDirty = false;
            SaveHintText.Text = _profile.LoadedFromDolphin
                ? Loc.T("Msg_ExistingDolphinAssignmentsLoaded")
                : Loc.T("Msg_RecommendedSetupAppliedAutomatically");
            SaveHintText.Foreground = BrushFrom("#91A3C2");
            ValidateMappings();
            UpdateConnectionStatus();
            UpdateFaceButtonLabels();
            UpdateControllerModeUi();
            if (_profile.LoadedFromDolphin)
            {
                SetStatus(Loc.Format("Msg_LoadedTheExistingDolphinConfiguration", SelectedDevice.DisplayName));
            }
        }
        catch (Exception ex)
        {
            SetStatus(Loc.Format("Msg_CouldNotLoadTheConfiguration", ex.Message), isError: true);
        }
        finally
        {
            _isLoading = false;
        }
    }

    private void BindingButton_OnClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not MarioKartActionBinding clicked ||
            _profile is null)
        {
            return;
        }

        CancelCapture();
        _captureDisplayAction = clicked;
        _captureAction = clicked;

        _captureDisplayAction.IsListening = true;
        _captureAction.IsListening = true;
        _captureBaseline = _deviceService.Read(SelectedDevice)
            .PressedInputs.ToHashSet(StringComparer.OrdinalIgnoreCase);
        PageStatus = Loc.T("Msg_Listening");
        PageStatusDot.Fill = BrushFrom("#58E7FF");
        PageStatusDot.BeginAnimation(
            OpacityProperty,
            new DoubleAnimation(0.3, 1, TimeSpan.FromMilliseconds(420))
            {
                AutoReverse = true,
                RepeatBehavior = RepeatBehavior.Forever
            });
        Focus();
        Keyboard.Focus(this);
        SetStatus(_captureAction.Kind switch
        {
            MarioKartBindingKind.Steering => Loc.T("Msg_MoveTheStickYouWantToUseForSteering"),
            _ => Loc.Format("Msg_PressTheButtonForEscCancels", clicked.Title)
        });
    }

    private void ClearBindingButton_OnClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not MarioKartActionBinding clicked ||
            _profile is null)
        {
            return;
        }

        var target = clicked;
        target.Clear();
        MarkDirty();
        ValidateMappings();
        SetStatus(Loc.Format("Msg_AssignmentForCleared", clicked.Title));
    }

    private void InputTimer_OnTick(object? sender, EventArgs e)
    {
        var snapshot = _deviceService.Read(SelectedDevice);
        UpdateTester(snapshot);
        UpdateAnalogPreview(snapshot);

        if (_captureAction is null) return;
        var newInput = snapshot.PressedInputs.FirstOrDefault(input => !_captureBaseline.Contains(input));
        if (!string.IsNullOrWhiteSpace(newInput))
        {
            var dolphinInput = NormalizeCapturedInput(newInput, SelectedDevice);
            TryCompleteCapture($"`{dolphinInput}`", dolphinInput);
        }
        else
        {
            _captureBaseline = snapshot.PressedInputs.ToHashSet(StringComparer.OrdinalIgnoreCase);
        }
    }

    private void Panel_OnPreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (_captureAction is null) return;
        e.Handled = true;
        if (e.Key == Key.Escape)
        {
            CancelCapture();
            SetStatus(Loc.T("Msg_AssignmentCancelled"));
            return;
        }

        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        var dolphinKey = ToDolphinKey(key);
        TryCompleteCapture($"`{dolphinKey}`", dolphinKey);
    }

    private void TryCompleteCapture(string binding, string rawInput)
    {
        if (_captureAction is null || _profile is null || SelectedDevice is null) return;
        var target = _captureAction;

        if (target.Kind == MarioKartBindingKind.Steering)
        {
            if (!TryAssignDirectionalFamily(target, rawInput, steering: true))
            {
                SetStatus(Loc.T("Msg_ForSteeringMoveAStickOrPress"), isError: true);
                return;
            }
        }
        else
        {
            foreach (var key in target.DolphinKeys) target.Values[key] = binding;
        }

        MarioKartControllerConfigurationService.UpdateDisplay(target, SelectedDevice.Kind);
        var display = target.DisplayBinding;
        var actionTitle = _captureDisplayAction?.Title ?? target.Title;
        CancelCapture();
        MarkDirty();
        ValidateMappings();
        SetStatus(Loc.Format("Msg_AssignedTo", actionTitle, display));
    }

    private static bool TryAssignDirectionalFamily(
        MarioKartActionBinding action,
        string rawInput,
        bool steering)
    {
        var value = rawInput.Trim().Trim('`');
        string up;
        string down;
        string left;
        string right;

        if (value.StartsWith("Left ", StringComparison.OrdinalIgnoreCase))
        {
            up = "`Left Y+`";
            down = "`Left Y-`";
            left = "`Left X-`";
            right = "`Left X+`";
        }
        else if (value.StartsWith("Right ", StringComparison.OrdinalIgnoreCase))
        {
            up = "`Right Y+`";
            down = "`Right Y-`";
            left = "`Right X-`";
            right = "`Right X+`";
        }
        else if (value.StartsWith("D-Pad", StringComparison.OrdinalIgnoreCase))
        {
            up = "`Pad N`";
            down = "`Pad S`";
            left = "`Pad W`";
            right = "`Pad E`";
        }
        else if (value.StartsWith("Pad ", StringComparison.OrdinalIgnoreCase))
        {
            up = "`Pad N`";
            down = "`Pad S`";
            left = "`Pad W`";
            right = "`Pad E`";
        }
        else if (value.StartsWith("Axis 0", StringComparison.OrdinalIgnoreCase) ||
                 value.StartsWith("Axis 1", StringComparison.OrdinalIgnoreCase))
        {
            up = "`Axis 1-`";
            down = "`Axis 1+`";
            left = "`Axis 0-`";
            right = "`Axis 0+`";
        }
        else if (value is "W" or "A" or "S" or "D")
        {
            up = "`W`";
            down = "`S`";
            left = "`A`";
            right = "`D`";
        }
        else if (value is "UP" or "DOWN" or "LEFT" or "RIGHT")
        {
            up = "`UP`";
            down = "`DOWN`";
            left = "`LEFT`";
            right = "`RIGHT`";
        }
        else
        {
            return false;
        }

        action.Values[action.DolphinKeys[0]] = up;
        action.Values[action.DolphinKeys[1]] = down;
        action.Values[action.DolphinKeys[2]] = left;
        action.Values[action.DolphinKeys[3]] = right;
        return true;
    }

    private static string NormalizeCapturedInput(
        string input,
        ControllerDeviceInfo? device)
    {
        input = input switch
        {
            "D-Pad Up" => "Pad N",
            "D-Pad Down" => "Pad S",
            "D-Pad Left" => "Pad W",
            "D-Pad Right" => "Pad E",
            _ => input
        };

        if (device?.Kind != ControllerDeviceKind.PlayStation ||
            !device.UsesRawInputLayout)
        {
            return input;
        }

        return input switch
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
            "Button 10" => "Left Stick Click",
            "Button 11" => "Right Stick Click",
            "Axis 0-" => "Left X-",
            "Axis 0+" => "Left X+",
            "Axis 1-" => "Left Y+",
            "Axis 1+" => "Left Y-",
            "Axis 2-" => "Right X-",
            "Axis 2+" => "Right X+",
            "Axis 3-" => "Right Y+",
            "Axis 3+" => "Right Y-",
            _ => input
        };
    }

    private void CancelCapture()
    {
        if (_captureAction is not null) _captureAction.IsListening = false;
        if (_captureDisplayAction is not null) _captureDisplayAction.IsListening = false;
        _captureAction = null;
        _captureDisplayAction = null;
        _captureBaseline.Clear();
        PageStatusDot.BeginAnimation(OpacityProperty, null);
        PageStatusDot.Opacity = 1;
        UpdateConnectionStatus();
    }

    private void UpdateTester(ControllerInputSnapshot snapshot)
    {
        MoveKnob(TesterLeftKnob, snapshot.LeftX, snapshot.LeftY, 87, 65, 12);
        MoveKnob(TesterRightKnob, snapshot.RightX, snapshot.RightY, 207, 121, 10);
        TesterLeftTrigger.Value = snapshot.LeftTrigger;
        TesterRightTrigger.Value = snapshot.RightTrigger;

        var inputs = snapshot.PressedInputs;
        var playStationRaw = SelectedDevice?.Kind == ControllerDeviceKind.PlayStation &&
                             SelectedDevice.UsesRawInputLayout;
        SetActive(FaceButtonSouth, HasAny(inputs, "Button A", "Button S") ||
                                   (playStationRaw && inputs.Contains("Button 1")));
        SetActive(FaceButtonEast, HasAny(inputs, "Button B", "Button E") ||
                                  (playStationRaw && inputs.Contains("Button 2")));
        SetActive(FaceButtonWest, HasAny(inputs, "Button X", "Button W") ||
                                  (playStationRaw && inputs.Contains("Button 0")));
        SetActive(FaceButtonNorth, HasAny(inputs, "Button Y", "Button N") ||
                                   (playStationRaw && inputs.Contains("Button 3")));
        SetDpadActive(DpadUp, HasDirection(inputs, "Up"));
        SetDpadActive(DpadDown, HasDirection(inputs, "Down"));
        SetDpadActive(DpadLeft, HasDirection(inputs, "Left"));
        SetDpadActive(DpadRight, HasDirection(inputs, "Right"));

        PressedInputsText.Text = inputs.Count == 0
            ? Loc.T("Msg_PressAButtonToTestIt")
            : string.Join(
                "  ·  ",
                inputs.Take(5).Select(i =>
                    MarioKartControllerConfigurationService.FriendlyInput(
                        $"`{i}`",
                        SelectedDevice?.Kind ?? ControllerDeviceKind.Generic)));
    }

    private void UpdateAnalogPreview(ControllerInputSnapshot snapshot)
    {
        Canvas.SetLeft(AnalogRawDot, 47.5 + snapshot.LeftX * 38);
        Canvas.SetTop(AnalogRawDot, 47.5 - snapshot.LeftY * 38);

        var (x, y) = ApplyAnalogResponse(snapshot.LeftX, snapshot.LeftY);
        Canvas.SetLeft(AnalogOutputDot, 46 + x * 40);
        Canvas.SetTop(AnalogOutputDot, 46 - y * 40);
    }

    private (double X, double Y) ApplyAnalogResponse(double x, double y)
    {
        var magnitude = Math.Sqrt(x * x + y * y);
        var deadzone = Deadzone / 100d;
        if (magnitude <= deadzone || magnitude <= double.Epsilon) return (0, 0);

        var normalized = Math.Clamp((magnitude - deadzone) / (1 - deadzone), 0, 1);
        var scaled = Math.Clamp(normalized * (Sensitivity / 100d), 0, 1);
        return (x / magnitude * scaled, y / magnitude * scaled);
    }

    private void AnalogSlider_OnChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (!_isLoading) MarkDirty();
    }

    private void ResetAnalogButton_OnClick(object sender, RoutedEventArgs e)
    {
        Deadzone = 10;
        Sensitivity = 100;
        MarkDirty();
        SetStatus(Loc.T("Msg_AnalogResponseResetToTheRecommended"));
    }

    private void RecommendedSetupButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (_profile is null) return;
        _configurationService.ResetRecommended(_profile);
        Deadzone = _profile.Deadzone;
        Sensitivity = _profile.Sensitivity;
        Vibration = _profile.Vibration;
        MarkDirty();
        ValidateMappings();
        SetStatus(Loc.T("Msg_RecommendedSetupAppliedTestIt"));
    }

    private void VibrationSwitch_OnChanged(object sender, RoutedEventArgs e)
    {
        if (!_isLoading) MarkDirty();
    }

    private async void TestVibrationButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (SelectedDevice is null || !SelectedDevice.IsConnected || !Vibration)
        {
            SetStatus(Loc.T("Msg_SelectAControllerAndEnableVibration"), isError: true);
            return;
        }

        try
        {
            _deviceService.SetVibration(SelectedDevice, 0.5);
            await Task.Delay(260);
            _deviceService.SetVibration(SelectedDevice, 0);
            SetStatus(Loc.T("Msg_VibrationTestCompleted"));
        }
        catch (Exception ex)
        {
            SetStatus(Loc.Format("Msg_VibrationIsNotSupported", ex.Message), isError: true);
        }
    }

    private void SaveButton_OnClick(object sender, RoutedEventArgs e)
    {
        if (SaveToDolphin()) PlayRequested?.Invoke(this, EventArgs.Empty);
    }

    private void ValidateMappings()
    {
        if (_controllerMode == DolphinControllerMode.ConfigureWithDolphin)
        {
            _hasConflicts = false;
            ConflictBanner.Visibility = Visibility.Collapsed;
            SaveButton.IsEnabled = true;
            return;
        }

        if (_profile is null) return;
        foreach (var action in _profile.Actions) action.HasConflict = false;

        var canonical = _profile.Actions.ToArray();
        var conflicts = canonical
            .SelectMany(action => action.Values.Values
                .Where(value => !string.IsNullOrWhiteSpace(value))
                .Select(NormalizeBinding)
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .Select(value => (Action: action, Value: value)))
            .GroupBy(item => item.Value, StringComparer.OrdinalIgnoreCase)
            .Where(group =>
            {
                var actionIds = group
                    .Select(item => item.Action.Id)
                    .Distinct(StringComparer.OrdinalIgnoreCase)
                    .ToHashSet(StringComparer.OrdinalIgnoreCase);
                return actionIds.Count > 1 &&
                       !MarioKartControllerConfigurationService.IsAllowedSharedBinding(actionIds);
            })
            .ToArray();

        foreach (var conflict in conflicts)
        {
            foreach (var item in conflict) item.Action.HasConflict = true;
        }
        _hasConflicts = conflicts.Length > 0;
        if (_hasConflicts)
        {
            ConflictText.Text = string.Join(
                "  ·  ",
                conflicts.Select(group =>
                    $"{MarioKartControllerConfigurationService.FriendlyInput($"`{group.Key}`", SelectedDevice?.Kind ?? ControllerDeviceKind.Generic)}: " +
                    string.Join(", ", group.Select(item => item.Action.Title).Distinct())));
            ConflictBanner.Visibility = Visibility.Visible;
            PageStatus = Loc.T("Msg_ConflictDetected");
            PageStatusDot.Fill = BrushFrom("#FF7088");
        }
        else
        {
            ConflictBanner.Visibility = Visibility.Collapsed;
            UpdateConnectionStatus();
        }

        SaveButton.IsEnabled = !_hasConflicts && SelectedDevice?.IsConnected == true;
    }

    private void UpdateConnectionStatus()
    {
        if (_captureAction is not null) return;

        if (_controllerMode == DolphinControllerMode.ConfigureWithDolphin)
        {
            PageStatus = Loc.T(_isDirty ? "Msg_UnsavedModeChange" : "Msg_ManagedByDolphin");
            PageStatusDot.Fill = BrushFrom(_isDirty ? "#FFB74D" : "#887CFF");
            return;
        }

        if (SelectedDevice?.IsConnected == true)
        {
            ConnectionText.Text = Loc.T("Msg_Connected");
            ConnectionDot.Fill = BrushFrom("#50E7A7");
            PageStatus = _isDirty ? Loc.T("Msg_UnsavedChanges") : Loc.T("Phase_Ready");
            PageStatusDot.Fill = BrushFrom(_isDirty ? "#FFB74D" : "#50E7A7");
        }
        else
        {
            ConnectionText.Text = Loc.T("Msg_NotConnected");
            ConnectionDot.Fill = BrushFrom("#FF7088");
            PageStatus = Loc.T("Msg_ControllerMissing");
            PageStatusDot.Fill = BrushFrom("#FF7088");
        }
    }

    private void UpdateFaceButtonLabels()
    {
        switch (SelectedDevice?.Kind)
        {
            case ControllerDeviceKind.PlayStation:
                FaceLabelNorth.Text = "△";
                FaceLabelWest.Text = "□";
                FaceLabelEast.Text = "○";
                FaceLabelSouth.Text = "×";
                break;
            case ControllerDeviceKind.Switch:
                FaceLabelNorth.Text = "X";
                FaceLabelWest.Text = "Y";
                FaceLabelEast.Text = "A";
                FaceLabelSouth.Text = "B";
                break;
            default:
                FaceLabelNorth.Text = "Y";
                FaceLabelWest.Text = "X";
                FaceLabelEast.Text = "B";
                FaceLabelSouth.Text = "A";
                break;
        }
    }

    private void MarkDirty()
    {
        if (_isLoading) return;
        _isDirty = true;
        SaveHintText.Text = Loc.T("Msg_ChangesHaveNotBeenAppliedToDolphin");
        SaveHintText.Foreground = BrushFrom("#FFBF69");
        UpdateConnectionStatus();
        ConfigurationChanged?.Invoke(this, EventArgs.Empty);
    }

    private void SetStatus(string message, bool isError = false)
    {
        StatusChanged?.Invoke(this, message);
        if (isError)
        {
            PageStatus = "Check configuration";
            PageStatusDot.Fill = BrushFrom("#FF7088");
        }
    }

    private string ResolveUserFolder() => UserFolderResolver?.Invoke()?.Trim() ?? "";

    private static void MoveKnob(FrameworkElement knob, double x, double y, double originX, double originY, double range)
    {
        Canvas.SetLeft(knob, originX + x * range);
        Canvas.SetTop(knob, originY - y * range);
    }

    private static void SetActive(Shape shape, bool active)
    {
        shape.Fill = BrushFrom(active ? "#58E7FF" : "#203451");
        shape.Stroke = BrushFrom(active ? "#C4F7FF" : "#496488");
    }

    private static void SetDpadActive(Shape direction, bool active) =>
        direction.Fill = BrushFrom(active ? "#58E7FF" : "#253A59");

    private static bool HasDirection(IReadOnlySet<string> inputs, string direction) =>
        inputs.Any(input =>
            input.Contains("D-Pad", StringComparison.OrdinalIgnoreCase) &&
            input.Contains(direction, StringComparison.OrdinalIgnoreCase));

    private static bool HasAny(IReadOnlySet<string> inputs, params string[] candidates) =>
        candidates.Any(candidate => inputs.Contains(candidate));

    private static string NormalizeBinding(string binding) =>
        binding.Trim().Trim('`').Trim().ToUpperInvariant();

    private static SolidColorBrush BrushFrom(string hex) =>
        new((Color)ColorConverter.ConvertFromString(hex));

    private static string ToDolphinKey(Key key) => key switch
    {
        Key.Enter => "RETURN",
        Key.LeftShift or Key.RightShift => "SHIFT",
        Key.LeftCtrl or Key.RightCtrl => "CONTROL",
        Key.LeftAlt or Key.RightAlt => "ALT",
        Key.Back => "BACKSPACE",
        Key.Space => "SPACE",
        Key.OemPlus => "EQUALS",
        Key.OemMinus => "MINUS",
        Key.OemComma => "COMMA",
        Key.OemPeriod => "PERIOD",
        _ => key.ToString().ToUpperInvariant()
    };

    private void SetField<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return;
        field = value;
        OnPropertyChanged(name);
    }

    private void OnPropertyChanged([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
