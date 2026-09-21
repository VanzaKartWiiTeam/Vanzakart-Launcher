using System.Windows;
using VanzaKartLauncher.Services;
using System.IO;
using System;

namespace VanzaKartLauncher;

public partial class App : System.Windows.Application
{
    public App()
    {
        AppDomain.CurrentDomain.UnhandledException += (s, e) =>
        {
            LogCrash(e.ExceptionObject?.ToString() ?? "Unknown AppDomain error");
        };

        DispatcherUnhandledException += (s, e) =>
        {
            LogCrash(e.Exception.ToString());
            e.Handled = true;
            System.Windows.MessageBox.Show(e.Exception.ToString(), "VanzaKart Launcher Crash", MessageBoxButton.OK, MessageBoxImage.Error);
        };
    }

    private static void LogCrash(string error)
    {
        try
        {
            var logPath = Path.Combine(AppContext.BaseDirectory, "crash.log");
            File.WriteAllText(logPath, $"[{DateTime.UtcNow:O}] CRASH:\n{error}\n");
        }
        catch { }
    }

    protected override void OnStartup(StartupEventArgs e)
    {
        try
        {
            WindowsInstallRegistryService.SynchronizeRegistration(LauncherConfig.CurrentLauncherVersion);
            base.OnStartup(e);
        }
        catch (Exception ex)
        {
            LogCrash(ex.ToString());
            System.Windows.MessageBox.Show(ex.ToString(), "Startup error", MessageBoxButton.OK, MessageBoxImage.Error);
            throw;
        }
    }
}
