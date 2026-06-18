using System.Windows;
using VanzaKartLauncher.Services;

namespace VanzaKartLauncher;

public partial class App : System.Windows.Application
{
    protected override void OnStartup(StartupEventArgs e)
    {
        try
        {
            WindowsInstallRegistryService.SynchronizeRegistration(LauncherConfig.CurrentLauncherVersion);
            base.OnStartup(e);
        }
        catch (Exception ex)
        {
            System.Windows.MessageBox.Show(ex.ToString(), "Startup error");
            throw;
        }
    }
}
