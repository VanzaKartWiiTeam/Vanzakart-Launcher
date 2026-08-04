using System.Diagnostics;
using System.IO;
using System.Text;

namespace VanzaKartLauncher.Services;

public static class LauncherUpdateHostService
{
    public static void Start(
        string archivePath,
        string installDirectory,
        string launcherPath,
        string targetVersion)
    {
        var scriptPath = Path.Combine(installDirectory, "VanzaKart_UpdateHost.ps1");
        File.WriteAllText(scriptPath, UpdateScript, new UTF8Encoding(encoderShouldEmitUTF8Identifier: true));

        var powershellPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.System),
            @"WindowsPowerShell\v1.0\powershell.exe");
        if (!File.Exists(powershellPath)) powershellPath = "powershell.exe";

        var startInfo = new ProcessStartInfo
        {
            FileName = powershellPath,
            WorkingDirectory = installDirectory,
            UseShellExecute = true,
            WindowStyle = ProcessWindowStyle.Normal
        };
        startInfo.ArgumentList.Add("-NoProfile");
        startInfo.ArgumentList.Add("-ExecutionPolicy");
        startInfo.ArgumentList.Add("Bypass");
        startInfo.ArgumentList.Add("-File");
        startInfo.ArgumentList.Add(scriptPath);
        startInfo.ArgumentList.Add("-ArchivePath");
        startInfo.ArgumentList.Add(archivePath);
        startInfo.ArgumentList.Add("-InstallDirectory");
        startInfo.ArgumentList.Add(installDirectory);
        startInfo.ArgumentList.Add("-LauncherPath");
        startInfo.ArgumentList.Add(launcherPath);
        startInfo.ArgumentList.Add("-TargetVersion");
        startInfo.ArgumentList.Add(targetVersion);
        startInfo.ArgumentList.Add("-ParentProcessId");
        startInfo.ArgumentList.Add(Environment.ProcessId.ToString());

        if (Process.Start(startInfo) == null)
            throw new InvalidOperationException("The PowerShell updater could not be started.");
    }

    private const string UpdateScript = """
param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$InstallDirectory,
    [Parameter(Mandatory = $true)][string]$LauncherPath,
    [Parameter(Mandatory = $true)][string]$TargetVersion,
    [Parameter(Mandatory = $true)][int]$ParentProcessId
)

$ErrorActionPreference = 'Stop'
$Host.UI.RawUI.WindowTitle = 'VanzaKart Launcher Update'
$Host.UI.RawUI.BackgroundColor = 'Black'
$Host.UI.RawUI.ForegroundColor = 'White'
Clear-Host

function Show-Stage([string]$Title, [string]$Detail, [int]$Percent) {
    Write-Progress -Activity 'VanzaKart Launcher Update' -Status $Title -CurrentOperation $Detail -PercentComplete $Percent
    Write-Host "[$Percent%] $Title" -ForegroundColor Cyan
    if (-not [string]::IsNullOrWhiteSpace($Detail)) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

try {
    Write-Host ''
    Write-Host '  VANZAKART LAUNCHER UPDATE' -ForegroundColor Magenta
    Write-Host "  Installing version $TargetVersion" -ForegroundColor White
    Write-Host '  This window will close when the launcher restarts.' -ForegroundColor DarkGray
    Write-Host ''

    Show-Stage 'Waiting for the launcher to close...' 'Please keep this window open.' 2
    $parent = Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
    if ($null -ne $parent -and -not $parent.WaitForExit(20000)) {
        throw 'The launcher did not close in time. Close it manually and try again.'
    }
    if (-not (Test-Path -LiteralPath $ArchivePath)) { throw 'The downloaded update package could not be found.' }

    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    Show-Stage 'Reading update package...' 'Checking archive contents.' 5
    $archive = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $totalBytes = [Math]::Max(1, ($archive.Entries | Measure-Object -Property Length -Sum).Sum)
        $completedBytes = 0L
        $installRoot = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        $buffer = New-Object byte[] (1024 * 1024)
        $protectedUserFiles = @(
            'launcher_settings.json',
            'user_preferences.json',
            'mod_version.txt',
            'mod_beta_version.txt',
            'musicpack_version.txt',
            'musicpack_beta_version.txt',
            'mod_install_state.json',
            'VanzaKart_launcher.json',
            'VKBeta_launcher.json'
        )

        foreach ($entry in $archive.Entries) {
            $destination = [IO.Path]::GetFullPath((Join-Path $installRoot $entry.FullName))
            if (-not $destination.StartsWith($installRoot, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Unsafe path in update package: $($entry.FullName)"
            }
            if ([string]::IsNullOrEmpty($entry.Name)) {
                [IO.Directory]::CreateDirectory($destination) | Out-Null
                continue
            }

            $relativeEntry = $entry.FullName.Replace('\', '/').TrimStart('/')
            if (-not $relativeEntry.Contains('/') -and $protectedUserFiles -contains $relativeEntry) {
                $completedBytes += $entry.Length
                Write-Host "       Preserved user file: $relativeEntry" -ForegroundColor DarkGray
                continue
            }

            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
            $source = $entry.Open()
            $target = [IO.File]::Open($destination, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
            try {
                while (($read = $source.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $target.Write($buffer, 0, $read)
                    $completedBytes += $read
                    $percent = [int](8 + (($completedBytes / $totalBytes) * 84))
                    Write-Progress -Activity 'VanzaKart Launcher Update' -Status 'Installing files...' -CurrentOperation $entry.Name -PercentComplete $percent
                }
            }
            finally { $target.Dispose(); $source.Dispose() }
        }
    }
    finally { $archive.Dispose() }

    Show-Stage 'Registering in Windows...' 'Updating installed app information.' 95
    $keyPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VanzaKartLauncher'
    New-Item -Path $keyPath -Force | Out-Null
    New-ItemProperty -Path $keyPath -Name DisplayName -Value 'VanzaKart Launcher' -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $keyPath -Name DisplayVersion -Value $TargetVersion -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $keyPath -Name Publisher -Value 'VanzaKart' -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $keyPath -Name InstallLocation -Value $InstallDirectory -PropertyType String -Force | Out-Null

    Show-Stage 'Restarting VanzaKart...' 'Update completed successfully.' 100
    Remove-Item -LiteralPath $ArchivePath -Force -ErrorAction SilentlyContinue
    Start-Process -FilePath $LauncherPath -WorkingDirectory $InstallDirectory
    Start-Sleep -Seconds 2
}
catch {
    Write-Progress -Activity 'VanzaKart Launcher Update' -Completed
    Write-Host ''
    Write-Host '  UPDATE FAILED' -ForegroundColor Red
    Write-Host "  $($_.Exception.Message)" -ForegroundColor Yellow
    Write-Host ''
    Read-Host 'Press Enter to close this window'
}
finally {
    Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
}
""";
}
