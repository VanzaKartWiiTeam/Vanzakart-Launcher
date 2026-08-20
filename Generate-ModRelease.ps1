<#
.SYNOPSIS
    Genera il rilascio di un aggiornamento per VanzaKart (inclusi i file singoli, il manifest e lo zip intero).
.DESCRIPTION
    Questo script scansiona la cartella locale del modpack VanzaKart, calcola gli hash SHA-256 dei file
    escludendo quelli privati/protetti (es. salvataggi, mii, My Stuff), crea il manifest differenziale,
    genera l'archivio ZIP per le installazioni da zero e aggiorna versions.json.
.PARAMETER ModPath
    Il percorso della cartella locale contenente la mod aggiornata (VanzaKart per Stable, VKBeta per Beta).
.PARAMETER Version
    La nuova versione della mod (es. 1.2.0).
.PARAMETER OutputDir
    La directory in cui generare i file da caricare sul server (default: .\dist).
.PARAMETER Channel
    Il canale da pubblicare: Stable usa /Modpack, Beta usa /VanzakartBeta.
.PARAMETER CreateFilesZip
    Crea files.zip e _by_sha256.zip come archivi di trasferimento delle cartelle files e _by_sha256 da caricare sul server.
    E' attivo per impostazione predefinita; il launcher non usa direttamente questi archivi.
#>

param (
    [string]$ModPath = "",

    [string]$Version = "",

    [Parameter(Mandatory=$false)]
    [string]$OutputDir = ".\dist",

    [string]$VersionsJsonUrl = "https://sitodaking.it:8443/Launcher/versions.json",
    [ValidateSet("Stable", "Beta")]
    [string]$Channel = "Stable",
    [string]$ServerBaseUrl = "https://sitodaking.it:8443",
    [string[]]$Changelog = @(),
    [switch]$CreateFilesZip = $true
)

$interactiveInvocation = -not $PSBoundParameters.ContainsKey("ModPath") -or
                         -not $PSBoundParameters.ContainsKey("Version")

# "Esegui con PowerShell" usa una console che Windows chiude appena lo script
# termina. Rilancia quindi l'uso interattivo in una console persistente: in
# questo modo anche gli errori di configurazione restano sempre leggibili.
if ($interactiveInvocation -and $env:VANZAKART_RELEASE_CONSOLE -ne "1") {
    $env:VANZAKART_RELEASE_CONSOLE = "1"
    $powerShellExe = Join-Path $PSHOME "powershell.exe"
    if (-not (Test-Path -LiteralPath $powerShellExe -PathType Leaf)) {
        $powerShellExe = "powershell.exe"
    }

    $quotedScriptPath = '"' + $PSCommandPath.Replace('"', '\"') + '"'
    Start-Process -FilePath $powerShellExe -ArgumentList @(
        "-NoExit",
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $quotedScriptPath)
    return
}

if ($env:VANZAKART_RELEASE_CONSOLE -eq "1") {
    Remove-Item Env:VANZAKART_RELEASE_CONSOLE -ErrorAction SilentlyContinue
}

$ErrorActionPreference = "Stop"
$interactiveLaunch = $interactiveInvocation

trap {
    Write-Host "`nMOD RELEASE FAILED: $($_.Exception.Message)" -ForegroundColor Red
    if ($interactiveLaunch) {
        [void](Read-Host "Premi INVIO; la console resterà aperta per leggere l'errore")
    }
    break
}

if ([string]::IsNullOrWhiteSpace($ModPath)) {
    $ModPath = Read-Host "Cartella della modpack (VanzaKart o VKBeta)"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = Read-Host "Versione della modpack (es. 1.2.0-beta.1)"
}
if (-not $PSBoundParameters.ContainsKey("Channel")) {
    $channelChoice = (Read-Host "Canale Stable o Beta? [Stable]").Trim()
    if ($channelChoice.Equals("B", [System.StringComparison]::OrdinalIgnoreCase) -or
        $channelChoice.Equals("Beta", [System.StringComparison]::OrdinalIgnoreCase)) {
        $Channel = "Beta"
    }
    else {
        $Channel = "Stable"
    }
}

$ModPath = $ModPath.Trim().Trim('"')
$Version = $Version.Trim().Trim('"')
if ([string]::IsNullOrWhiteSpace($ModPath)) { throw "La cartella della modpack non può essere vuota." }
if ([string]::IsNullOrWhiteSpace($Version)) { throw "La versione non può essere vuota." }
if (-not $PSBoundParameters.ContainsKey("OutputDir")) {
    $OutputDir = if ($Channel -eq "Beta") { ".\dist-beta" } else { ".\dist" }
}

$modDirectoryName = if ($Channel -eq "Beta") { "VKBeta" } else { "VanzaKart" }
$archiveName = "$modDirectoryName.zip"

# Liste di esclusione (devono corrispondere a quelle del Launcher in C#)
$ProtectedDirs = @("My Stuff", "UserData", "userdata", "Saves", "Save", "Licenses", "License", "Patenti", "Profiles", "Miis", "Mii", "private", "Patches", "patches")
$ProtectedFiles = @("rksys.dat", "RFL_DB.dat", "active_mii.txt", "mii_profile.json")
$ProtectedExts = @(".mii", ".miigx", ".mae", ".vk-mii")
$ProtectedSubstrings = @("save", "license", "patent", "mii", "profile")
$AlwaysIncludedDirs = @("CTBRSTM", "MiiOutfitC", "Race", "Language")
$AlwaysIncludedRelativeDirSuffixes = @("Scene/Model")

function Has-ProtectedDirectorySegment {
    param ([string]$RelativePath)

    $segments = $RelativePath.Split(
        [char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar),
        [System.StringSplitOptions]::RemoveEmptyEntries)

    foreach ($seg in $segments) {
        if ($ProtectedDirs -contains $seg) { return $true }
    }

    return $false
}

function Is-AlwaysIncludedRelativePath {
    param ([string]$RelativePath)

    $normalizedPath = $RelativePath.Replace('\', '/').Trim('/')
    foreach ($relativeDirSuffix in $AlwaysIncludedRelativeDirSuffixes) {
        $normalizedSuffix = $relativeDirSuffix.Replace('\', '/').Trim('/')
        if ($normalizedPath.EndsWith("/$normalizedSuffix", [System.StringComparison]::OrdinalIgnoreCase) -or
            $normalizedPath.IndexOf("/$normalizedSuffix/", [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        }
    }

    return $false
}

# Funzione helper per calcolare il percorso relativo (compatibile anche con PowerShell 5.1 su .NET Framework)
function Get-RelativePath {
    param (
        [string]$BasePath,
        [string]$Path
    )
    $BasePath = $BasePath.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if ($Path.StartsWith($BasePath, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $Path.Substring($BasePath.Length)
    }
    return $Path
}

# Funzione per verificare se un file relativo è protetto
function Is-FileProtected {
    param ([string]$RelativePath)
    
    # 1. Verifica segmenti cartella
    $segments = $RelativePath.Split([System.IO.Path]::DirectorySeparatorChar)

    # Le cartelle personali hanno sempre la precedenza: My Stuff deve essere
    # presente nello ZIP, ma nessun suo contenuto deve mai essere pubblicato.
    if (Has-ProtectedDirectorySegment -RelativePath $RelativePath) { return $true }

    # Le directory strutturali devono essere incluse integralmente nella release,
    # anche se un nome file al loro interno coincide con un filtro generico.
    if (Is-AlwaysIncludedRelativePath -RelativePath $RelativePath) { return $false }

    foreach ($requiredDir in $AlwaysIncludedDirs) {
        if ($segments -contains $requiredDir) { return $false }
    }
    
    # 2. Verifica nome file
    $fileName = [System.IO.Path]::GetFileName($RelativePath)
    if ($ProtectedFiles -contains $fileName) { return $true }
    
    # 3. Verifica estensione
    $ext = [System.IO.Path]::GetExtension($RelativePath)
    if ($ProtectedExts -contains $ext) { return $true }
    
    # 4. Verifica sottostringhe protette (case-insensitive)
    $lowerPath = $RelativePath.ToLower()
    foreach ($sub in $ProtectedSubstrings) {
        if ($lowerPath.Contains($sub)) { return $true }
    }
    
    return $false
}

# Funzione per calcolare l'hash SHA-256 in formato esadecimale minuscolo
function Get-FileSha256 {
    param ([string]$FilePath)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($FilePath)
    try {
        $hashBytes = $sha256.ComputeHash($stream)
        $hashHex = [System.BitConverter]::ToString($hashBytes).Replace("-", "").ToLowerInvariant()
        return $hashHex
    }
    finally {
        $stream.Close()
        $sha256.Dispose()
    }
}

function Test-ModPayloadRoot {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return $false }
    return (Test-Path -LiteralPath (Join-Path $Path "Riivolution") -PathType Container) -and
           (Test-Path -LiteralPath (Join-Path $Path $modDirectoryName) -PathType Container)
}

function Resolve-ModPayloadRoot {
    param([string]$SourcePath)

    $sourceFullPath = [System.IO.Path]::GetFullPath($SourcePath)
    if (Test-ModPayloadRoot -Path $sourceFullPath) {
        return $sourceFullPath
    }

    $nestedModpack = Join-Path $sourceFullPath $modDirectoryName
    if (Test-ModPayloadRoot -Path $nestedModpack) {
        return [System.IO.Path]::GetFullPath($nestedModpack)
    }

    throw "La cartella sorgente non sembra una release $modDirectoryName valida. Passa la cartella che contiene Riivolution/ e $modDirectoryName/, oppure la cartella padre che contiene $modDirectoryName/Riivolution/ e $modDirectoryName/$modDirectoryName/."
}

function Normalize-JsonText {
    param([string]$Text)
    if ($null -eq $Text) { return "" }
    return $Text.TrimStart([char[]]@(
        [char]0xFEFF, [char]0x200B,
        [char]0x00EF, [char]0x00BB, [char]0x00BF,
        [char]0x20, [char]0x09, [char]0x0D, [char]0x0A))
}

# Funzione per creare archivi ZIP conformi alle specifiche PKWARE (separatori '/', UTF-8, directory esplicite).
# Risolve l'incompatibilità su macOS, Linux e ChromeOS causata da Compress-Archive / .NET Framework su Windows.
function New-StandardZipArchive {
    param (
        [Parameter(Mandatory=$true)]
        [string]$SourceDirectory,
        [Parameter(Mandatory=$true)]
        [string]$DestinationZipPath,
        [System.IO.Compression.CompressionLevel]$CompressionLevel = [System.IO.Compression.CompressionLevel]::Optimal,
        [string[]]$IncludeEmptyDirs = @()
    )

    $sourceFullPath = [System.IO.Path]::GetFullPath($SourceDirectory).TrimEnd('\', '/')
    $destFullPath = [System.IO.Path]::GetFullPath($DestinationZipPath)

    $destDir = [System.IO.Path]::GetDirectoryName($destFullPath)
    if (-not (Test-Path -LiteralPath $destDir)) {
        [System.IO.Directory]::CreateDirectory($destDir) | Out-Null
    }

    if (Test-Path -LiteralPath $destFullPath) {
        Remove-Item -LiteralPath $destFullPath -Force -ErrorAction SilentlyContinue
    }

    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem

    $zipStream = [System.IO.File]::Open($destFullPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    $archive = [System.IO.Compression.ZipArchive]::new($zipStream, [System.IO.Compression.ZipArchiveMode]::Create, $false, [System.Text.Encoding]::UTF8)

    $addedEntries = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

    try {
        # 1. Aggiungi tutti i file usando tassativamente '/' come separatore
        $files = Get-ChildItem -LiteralPath $sourceFullPath -Recurse -File
        foreach ($file in $files) {
            $relative = $file.FullName.Substring($sourceFullPath.Length).TrimStart('\', '/')
            $entryName = $relative.Replace('\', '/')
            
            $entry = $archive.CreateEntry($entryName, $CompressionLevel)
            $entry.LastWriteTime = $file.LastWriteTime
            [void]$addedEntries.Add($entryName)
            
            $fileStream = [System.IO.File]::OpenRead($file.FullName)
            $entryStream = $entry.Open()
            try {
                $fileStream.CopyTo($entryStream)
            }
            finally {
                $entryStream.Dispose()
                $fileStream.Dispose()
            }
        }

        # 2. Aggiungi tutte le cartelle (incluse quelle vuote) con slash finale '/'
        $dirs = Get-ChildItem -LiteralPath $sourceFullPath -Recurse -Directory
        foreach ($dir in $dirs) {
            $relative = $dir.FullName.Substring($sourceFullPath.Length).TrimStart('\', '/')
            $dirEntryName = $relative.Replace('\', '/').TrimEnd('/') + '/'
            if (-not $addedEntries.Contains($dirEntryName)) {
                $null = $archive.CreateEntry($dirEntryName)
                [void]$addedEntries.Add($dirEntryName)
            }
        }

        # 3. Aggiungi eventuali cartelle vuote extra richieste
        if ($IncludeEmptyDirs) {
            foreach ($extraDir in $IncludeEmptyDirs) {
                if ([string]::IsNullOrWhiteSpace($extraDir)) { continue }
                $dirEntryName = $extraDir.Replace('\', '/').TrimStart('/').TrimEnd('/') + '/'
                if ($dirEntryName -ne '/' -and -not $addedEntries.Contains($dirEntryName)) {
                    $null = $archive.CreateEntry($dirEntryName)
                    [void]$addedEntries.Add($dirEntryName)
                }
            }
        }
    }
    finally {
        $archive.Dispose()
        $zipStream.Dispose()
    }
}

$serverDirectory = if ($Channel -eq "Beta") { "VanzakartBeta" } else { "Modpack" }
$modReleaseBaseUrl = "$($ServerBaseUrl.TrimEnd('/'))/$serverDirectory"

Write-Host "=== Generazione Rilascio VanzaKart v$Version ($Channel) ===" -ForegroundColor Cyan

# Risolvi percorsi assoluti
$absoluteModPath = [System.IO.Path]::GetFullPath($ModPath)
$absoluteOutputDir = [System.IO.Path]::GetFullPath($OutputDir)

if (-not (Test-Path -LiteralPath $absoluteModPath -PathType Container)) {
    Write-Error "La cartella mod specificata non esiste: $absoluteModPath"
}

Write-Host "Mod Path: $absoluteModPath"
$payloadRoot = Resolve-ModPayloadRoot -SourcePath $absoluteModPath
Write-Host "Payload Root: $payloadRoot"
Write-Host "Differential manifest root: contenuto di Payload Root, senza il prefisso $modDirectoryName/" -ForegroundColor DarkCyan
Write-Host "Full ZIP root: $modDirectoryName/" -ForegroundColor DarkCyan
Write-Host "Output Dir: $absoluteOutputDir"

# Scarica il versions.json attuale dal server prima di generare il rilascio.
Write-Host "Download del versions.json attuale da $versionsJsonUrl..." -ForegroundColor Yellow
try {
    $cacheBuster = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $versionsResponse = Invoke-WebRequest -Uri "${VersionsJsonUrl}?t=$cacheBuster" -UseBasicParsing -TimeoutSec 30
    $existingVersions = Normalize-JsonText $versionsResponse.Content | ConvertFrom-Json
}
catch {
    throw "Impossibile scaricare o leggere il versions.json da '$versionsJsonUrl': $($_.Exception.Message)"
}

$existingEndpoints = $null
$defaultEndpointsUrl = if ($existingVersions.endpoints_url) { [string]$existingVersions.endpoints_url } elseif ($existingVersions.endpoints_json_url) { [string]$existingVersions.endpoints_json_url } else { "https://sitodaking.it:8443/Launcher/endpoints.json" }
try {
    $endpointsResponse = Invoke-WebRequest -Uri "${defaultEndpointsUrl}?t=$cacheBuster" -UseBasicParsing -TimeoutSec 10
    $existingEndpoints = Normalize-JsonText $endpointsResponse.Content | ConvertFrom-Json
}
catch {
    # Non bloccante se endpoints.json non è ancora presente sul server
}

if (-not $existingVersions.launcher_version) {
    throw "Il versions.json scaricato da '$versionsJsonUrl' non contiene una launcher_version valida."
}
if (-not $existingVersions.music_pack_version) {
    throw "Il versions.json scaricato non contiene music_pack_version. Pubblica prima una release Music Pack con Generate-MusicPackRelease.ps1."
}

$currentLauncherVersion = [string]$existingVersions.launcher_version
Write-Host "Versione launcher attuale scaricata: $currentLauncherVersion" -ForegroundColor Green

# Crea directory temporanee e di output
$tempZipFolder = Join-Path $env:TEMP "vanzakart_release_temp_$([guid]::NewGuid().ToString())"
$filesOutputDir = Join-Path $absoluteOutputDir "files"
$hashFilesOutputDir = Join-Path $absoluteOutputDir "_by_sha256"

if (Test-Path -LiteralPath $absoluteOutputDir) {
    Write-Host "Rimozione vecchia cartella di output..." -ForegroundColor Yellow
    Remove-Item -LiteralPath $absoluteOutputDir -Recurse -Force -ErrorAction SilentlyContinue
}
[System.IO.Directory]::CreateDirectory($absoluteOutputDir) | Out-Null
[System.IO.Directory]::CreateDirectory($filesOutputDir) | Out-Null
[System.IO.Directory]::CreateDirectory($hashFilesOutputDir) | Out-Null
[System.IO.Directory]::CreateDirectory($tempZipFolder) | Out-Null

# Conserva le directory strutturali anche quando non contengono file. Verranno
# inoltre aggiunte esplicitamente come directory entry nello ZIP finale.
$alwaysIncludedRelativeDirs = @()
foreach ($requiredDir in $AlwaysIncludedDirs) {
    $matchingDirs = Get-ChildItem -LiteralPath $payloadRoot -Recurse -Directory |
        Where-Object { $_.Name.Equals($requiredDir, [System.StringComparison]::OrdinalIgnoreCase) }
    foreach ($directory in $matchingDirs) {
        $relativeDir = Get-RelativePath -BasePath $payloadRoot -Path $directory.FullName
        if (Has-ProtectedDirectorySegment -RelativePath $relativeDir) { continue }

        $alwaysIncludedRelativeDirs += $relativeDir
        [System.IO.Directory]::CreateDirectory((Join-Path $tempZipFolder (Join-Path $modDirectoryName $relativeDir))) | Out-Null
        [System.IO.Directory]::CreateDirectory((Join-Path $filesOutputDir $relativeDir)) | Out-Null
        Write-Host " - Directory obbligatoria preservata: $relativeDir" -ForegroundColor DarkCyan
    }
}

foreach ($relativeDirSuffix in $AlwaysIncludedRelativeDirSuffixes) {
    $requiredPath = Join-Path $payloadRoot (Join-Path $modDirectoryName ($relativeDirSuffix.Replace('/', [System.IO.Path]::DirectorySeparatorChar)))
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Container)) { continue }

    $relativeDir = Get-RelativePath -BasePath $payloadRoot -Path $requiredPath
    if (Has-ProtectedDirectorySegment -RelativePath $relativeDir) { continue }
    if ($alwaysIncludedRelativeDirs -contains $relativeDir) { continue }

    $alwaysIncludedRelativeDirs += $relativeDir
    [System.IO.Directory]::CreateDirectory((Join-Path $tempZipFolder (Join-Path $modDirectoryName $relativeDir))) | Out-Null
    [System.IO.Directory]::CreateDirectory((Join-Path $filesOutputDir $relativeDir)) | Out-Null
    Write-Host " - Directory obbligatoria preservata: $relativeDir" -ForegroundColor DarkCyan
}

# My Stuff deve esistere nella release, ma deve essere sempre vuota: i file
# personali al suo interno restano esclusi da Is-FileProtected.
$myStuffRelativeDir = Join-Path $modDirectoryName "My Stuff"
if ($alwaysIncludedRelativeDirs -notcontains $myStuffRelativeDir) {
    $alwaysIncludedRelativeDirs += $myStuffRelativeDir
    [System.IO.Directory]::CreateDirectory(
        (Join-Path $tempZipFolder (Join-Path $modDirectoryName $myStuffRelativeDir))) | Out-Null
    [System.IO.Directory]::CreateDirectory((Join-Path $filesOutputDir $myStuffRelativeDir)) | Out-Null
    Write-Host " - Directory vuota preservata: $myStuffRelativeDir" -ForegroundColor DarkCyan
}

Write-Host "Scansione dei file e calcolo degli hash..."

$manifestFiles = @()
$allowedFilesCount = 0
$skippedFilesCount = 0
$hashFallbackFilesCount = 0
$hashFallbackUniqueHashes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

# Recupera ricorsivamente tutti i file nella cartella payload del modpack.
# Il manifest differenziale deve essere relativo a questa cartella, mentre lo
# ZIP completo deve contenere un livello root specifico del canale perché viene estratto
# dentro Load/Riivolution.
$files = Get-ChildItem -LiteralPath $payloadRoot -Recurse -File

foreach ($file in $files) {
    # Ottieni il percorso relativo del file
    $relativePath = Get-RelativePath -BasePath $payloadRoot -Path $file.FullName
    
    # Controlla se il file è protetto/personale
    if (Is-FileProtected -RelativePath $relativePath) {
        $skippedFilesCount++
        continue
    }
    
    $allowedFilesCount++
    
    # Calcola SHA256 e dimensione
    $sha256 = Get-FileSha256 -FilePath $file.FullName
    $size = $file.Length
    
    # Normalizza il percorso con forward slashes per compatibilità web/cross-platform
    $webPath = $relativePath.Replace('\', '/')
    
    # Aggiungi all'array del manifest
    $manifestFiles += @{
        "path" = $webPath
        "sha256" = $sha256
        "size" = $size
    }
    
    # Copia nella cartella dei file singoli per l'aggiornamento differenziale
    $destFile = Join-Path $filesOutputDir $relativePath
    $destDir = [System.IO.Path]::GetDirectoryName($destFile)
    if (-not (Test-Path -LiteralPath $destDir)) {
        [System.IO.Directory]::CreateDirectory($destDir) | Out-Null
    }
    Copy-Item -LiteralPath $file.FullName -Destination $destFile -Force

    # Copia sempre anche in una posizione URL-safe basata sull'hash. Il launcher
    # la usa come fallback quando il path originale manca o viene servito male.
    $hashDestFile = Join-Path $hashFilesOutputDir $sha256
    Copy-Item -LiteralPath $file.FullName -Destination $hashDestFile -Force
    $hashFallbackFilesCount++
    [void]$hashFallbackUniqueHashes.Add($sha256)
    
    # Copia nella cartella temporanea per lo ZIP
    $tempZipFile = Join-Path $tempZipFolder (Join-Path $modDirectoryName $relativePath)
    $tempZipDir = [System.IO.Path]::GetDirectoryName($tempZipFile)
    if (-not (Test-Path -LiteralPath $tempZipDir)) {
        [System.IO.Directory]::CreateDirectory($tempZipDir) | Out-Null
    }
    Copy-Item -LiteralPath $file.FullName -Destination $tempZipFile -Force
}

Write-Host "Scansione completata." -ForegroundColor Green
Write-Host " - File inclusi: $allowedFilesCount"
Write-Host " - File privati esclusi (es. saves, My Stuff): $skippedFilesCount"
Write-Host " - File coperti da fallback _by_sha256: $hashFallbackFilesCount"
Write-Host " - File hash unici creati in _by_sha256: $($hashFallbackUniqueHashes.Count)"

Write-Host "Verifica coerenza manifest/cartella files..." -ForegroundColor Yellow
$missingDifferentialFiles = @()
foreach ($entry in $manifestFiles) {
    $relativeDiskPath = ([string]$entry.path).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
    $expectedPath = Join-Path $filesOutputDir $relativeDiskPath
    $expectedHashPath = Join-Path $hashFilesOutputDir ([string]$entry.sha256)
    if (-not (Test-Path -LiteralPath $expectedPath -PathType Leaf)) {
        $missingDifferentialFiles += [string]$entry.path
    }
    if (-not (Test-Path -LiteralPath $expectedHashPath -PathType Leaf)) {
        $missingDifferentialFiles += "_by_sha256/$([string]$entry.sha256) ($([string]$entry.path))"
    }
}

if ($missingDifferentialFiles.Count -gt 0) {
    $preview = ($missingDifferentialFiles | Select-Object -First 20) -join "`n - "
    throw "Release differenziale non valida: $($missingDifferentialFiles.Count) file sono nel manifest ma non esistono in dist/files.`n - $preview"
}
Write-Host " - OK: tutti i file del manifest esistono nella cartella files e hanno fallback _by_sha256." -ForegroundColor Green

# 1. Scrittura del file manifest_files.json
$manifestObject = @{
    "mod_version" = $Version
    "files" = $manifestFiles
}
$manifestJsonPath = Join-Path $absoluteOutputDir "manifest_files.json"
$manifestJsonContent = ConvertTo-Json -InputObject $manifestObject -Depth 100
# Forza la formattazione compatta o leggibile:
[System.IO.File]::WriteAllText($manifestJsonPath, $manifestJsonContent)
Write-Host "Creato manifest dei file: manifest_files.json" -ForegroundColor Green

# 2. Compressione dell'archivio completo per installazioni da zero (standard cross-platform)
$zipPath = Join-Path $absoluteOutputDir $archiveName
Write-Host "Compressione dello ZIP per installazioni complete (cross-platform)..." -ForegroundColor Yellow
New-StandardZipArchive -SourceDirectory $tempZipFolder -DestinationZipPath $zipPath
Write-Host "Creato archivio ZIP completo: $archiveName" -ForegroundColor Green

# 3. Compressione delle cartelle differenziali per il caricamento sul server.
# Il launcher scarica i file singoli da files/ e _by_sha256/; creare files.zip e _by_sha256.zip
# serve per trasferire e poi estrarre le strutture sul server.
if ($CreateFilesZip) {
    $filesZipPath = Join-Path $absoluteOutputDir "files.zip"
    Write-Host "Compressione dei file differenziali in files.zip (cross-platform)..." -ForegroundColor Yellow
    New-StandardZipArchive -SourceDirectory $filesOutputDir -DestinationZipPath $filesZipPath
    Write-Host "Creato archivio dei file differenziali: files.zip" -ForegroundColor Green

    $hashZipPath = Join-Path $absoluteOutputDir "_by_sha256.zip"
    Write-Host "Compressione dei file per hash in _by_sha256.zip (cross-platform)..." -ForegroundColor Yellow
    New-StandardZipArchive -SourceDirectory $hashFilesOutputDir -DestinationZipPath $hashZipPath
    Write-Host "Creato archivio dei file hash: _by_sha256.zip" -ForegroundColor Green
}

# Pulisci cartella temporanea
Remove-Item -LiteralPath $tempZipFolder -Recurse -Force -ErrorAction SilentlyContinue

# Calcola hash dello ZIP generato
$zipSha256 = Get-FileSha256 -FilePath $zipPath

# Inserisce l'hash dello ZIP anche nel manifest del canale. Il launcher usa questo
# valore per verificare i full download Beta senza dipendere dal versions.json Stable.
$manifestObject["archive_sha256"] = $zipSha256
$manifestJsonContent = ConvertTo-Json -InputObject $manifestObject -Depth 100
[System.IO.File]::WriteAllText($manifestJsonPath, $manifestJsonContent, [System.Text.UTF8Encoding]::new($false))

# 4. Creazione o aggiornamento di versions.json
$versionsJsonPath = Join-Path $absoluteOutputDir "versions.json"
$baseVersionsObject = @{
    "mod_version" = $Version
    "launcher_version" = $currentLauncherVersion
    "mod_url" = "$modReleaseBaseUrl/$archiveName"
    "mod_sha256" = $zipSha256
    "mod_manifest_url" = "$modReleaseBaseUrl/manifest_files.json"
    "mod_files_url" = "$modReleaseBaseUrl/files/"
    "mod_hash_files_url" = "$modReleaseBaseUrl/_by_sha256/"
    "mod_mirrors" = @()
    "mod_files_mirrors" = @()
    "mod_hash_files_mirrors" = @()
    "launcher_url" = "https://sitodaking.it/Launcher/vanzakart_launcher.zip"
    "launcher_mirrors" = @()
    "changelog" = @()
    "music_pack_version" = [string]$existingVersions.music_pack_version
    "music_pack_url" = "https://sitodaking.it/MusicPack/vanzakart_musicpack.zip"
    "music_pack_mirrors" = @()
    "music_pack_sha256" = [string]$existingVersions.music_pack_sha256
    "music_pack_changelog" = @()
}

# Preserva tutte le configurazioni del versions.json attuale.
foreach ($prop in $existingVersions.PSObject.Properties) {
    $baseVersionsObject[$prop.Name] = $prop.Value
}

# Aggiorna soltanto il gruppo di proprietà del canale pubblicato. In questo modo
# una release Beta non sostituisce mai i riferimenti Stable nel versions.json.
$baseVersionsObject["launcher_version"] = $currentLauncherVersion
if ($Channel -eq "Beta") {
    $baseVersionsObject["beta_mod_version"] = $Version
    $baseVersionsObject["beta_mod_sha256"] = $zipSha256
    $baseVersionsObject["beta_mod_manifest_url"] = "$modReleaseBaseUrl/manifest_files.json"
    $baseVersionsObject["beta_mod_files_url"] = "$modReleaseBaseUrl/files/"
    $baseVersionsObject["beta_mod_hash_files_url"] = "$modReleaseBaseUrl/_by_sha256/"
    $baseVersionsObject["beta_mod_url"] = "$modReleaseBaseUrl/$archiveName"
    $baseVersionsObject["beta_mod_mirrors"] = @()
    $baseVersionsObject["beta_mod_files_mirrors"] = @()
    $baseVersionsObject["beta_mod_hash_files_mirrors"] = @()
    $baseVersionsObject["beta_changelog"] = [string[]]@(if ($Changelog.Count -gt 0) { $Changelog } else { "VanzaKart Beta $Version" })
}
else {
    $baseVersionsObject["mod_version"] = $Version
    $baseVersionsObject["mod_sha256"] = $zipSha256
    $baseVersionsObject["mod_manifest_url"] = "$modReleaseBaseUrl/manifest_files.json"
    $baseVersionsObject["mod_files_url"] = "$modReleaseBaseUrl/files/"
    $baseVersionsObject["mod_hash_files_url"] = "$modReleaseBaseUrl/_by_sha256/"
    $baseVersionsObject["mod_url"] = "$modReleaseBaseUrl/$archiveName"
    $baseVersionsObject["mod_mirrors"] = @($existingVersions.mod_mirrors)
    $baseVersionsObject["mod_files_mirrors"] = @($existingVersions.mod_files_mirrors)
    $baseVersionsObject["mod_hash_files_mirrors"] = @($existingVersions.mod_hash_files_mirrors)
    $baseVersionsObject["changelog"] = [string[]]@(if ($Changelog.Count -gt 0) { $Changelog } else { "VanzaKart Modpack $Version" })
}

# Questi valori appartengono alle altre release e devono sempre restare invariati.
$baseVersionsObject["launcher_url"] = if ($existingVersions.launcher_url) { [string]$existingVersions.launcher_url } else { "https://sitodaking.it/Launcher/vanzakart_launcher.zip" }
$baseVersionsObject["launcher_mirrors"] = @($existingVersions.launcher_mirrors)
$baseVersionsObject["news_url"] = if ($existingVersions.news_url) { [string]$existingVersions.news_url } elseif ($existingVersions.news_json_url) { [string]$existingVersions.news_json_url } else { "https://sitodaking.it:8443/Launcher/news.json" }
$baseVersionsObject["leaderboard_api_url"] = if ($existingVersions.leaderboard_api_url) { [string]$existingVersions.leaderboard_api_url } else { "https://sitodaking.it:8443/api/vk_leaderboard.php" }
$baseVersionsObject["leaderboard_details_api_url"] = if ($existingVersions.leaderboard_details_api_url) { [string]$existingVersions.leaderboard_details_api_url } else { "https://sitodaking.it:8443/api/leaderboard/" }
$baseVersionsObject["rooms_api_url"] = if ($existingVersions.rooms_api_url) { [string]$existingVersions.rooms_api_url } else { "https://sitodaking.it:8443/api/vk_rooms.php" }
$baseVersionsObject["beta_token_verify_api_url"] = if ($existingVersions.beta_token_verify_api_url) { [string]$existingVersions.beta_token_verify_api_url } else { "https://sitodaking.it:8443/api/vk_beta_token.php" }
$baseVersionsObject["download_page_url"] = if ($existingVersions.download_page_url) { [string]$existingVersions.download_page_url } else { "https://vwfc.sitodaking.it/" }
$baseVersionsObject["mii_rendering_archive_url"] = if ($existingVersions.mii_rendering_archive_url) { [string]$existingVersions.mii_rendering_archive_url } else { "https://web.archive.org/web/20180502054513id_/http://download-cdn.miitomo.com/native/20180125111639/android/v2/asset_model_character_mii_AFLResHigh_2_3_dat.zip" }
$baseVersionsObject["music_pack_version"] = [string]$existingVersions.music_pack_version
$baseVersionsObject["music_pack_url"] = if ($existingVersions.music_pack_url) { [string]$existingVersions.music_pack_url } else { "https://sitodaking.it/MusicPack/vanzakart_musicpack.zip" }
$baseVersionsObject["music_pack_mirrors"] = @($existingVersions.music_pack_mirrors)
$baseVersionsObject["music_pack_sha256"] = [string]$existingVersions.music_pack_sha256
$baseVersionsObject["music_pack_changelog"] = @($existingVersions.music_pack_changelog)

$versionsJsonContent = ConvertTo-Json -InputObject $baseVersionsObject -Depth 100
[System.IO.File]::WriteAllText($versionsJsonPath, $versionsJsonContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "Creato/Aggiornato il file: versions.json" -ForegroundColor Green

# 5. Creazione o aggiornamento di endpoints.json
$endpointsJsonPath = Join-Path $absoluteOutputDir "endpoints.json"
$endpointsObject = [ordered]@{
    "mod_url" = if ($Channel -eq "Stable") { "$modReleaseBaseUrl/$archiveName" } elseif ($existingEndpoints.mod_url) { [string]$existingEndpoints.mod_url } elseif ($existingVersions.mod_url) { [string]$existingVersions.mod_url } else { "https://sitodaking.it:8443/Modpack/VanzaKart.zip" }
    "mod_manifest_url" = if ($Channel -eq "Stable") { "$modReleaseBaseUrl/manifest_files.json" } elseif ($existingEndpoints.mod_manifest_url) { [string]$existingEndpoints.mod_manifest_url } elseif ($existingVersions.mod_manifest_url) { [string]$existingVersions.mod_manifest_url } else { "https://sitodaking.it:8443/Modpack/manifest_files.json" }
    "mod_files_url" = if ($Channel -eq "Stable") { "$modReleaseBaseUrl/files/" } elseif ($existingEndpoints.mod_files_url) { [string]$existingEndpoints.mod_files_url } elseif ($existingVersions.mod_files_url) { [string]$existingVersions.mod_files_url } else { "https://sitodaking.it:8443/Modpack/files/" }
    "mod_hash_files_url" = if ($Channel -eq "Stable") { "$modReleaseBaseUrl/_by_sha256/" } elseif ($existingEndpoints.mod_hash_files_url) { [string]$existingEndpoints.mod_hash_files_url } elseif ($existingVersions.mod_hash_files_url) { [string]$existingVersions.mod_hash_files_url } else { "https://sitodaking.it:8443/Modpack/_by_sha256/" }
    "mod_mirrors" = if ($Channel -eq "Stable") { @($existingVersions.mod_mirrors) } elseif ($existingEndpoints.mod_mirrors) { @($existingEndpoints.mod_mirrors) } else { @() }
    "mod_files_mirrors" = if ($Channel -eq "Stable") { @($existingVersions.mod_files_mirrors) } elseif ($existingEndpoints.mod_files_mirrors) { @($existingEndpoints.mod_files_mirrors) } else { @() }
    "mod_hash_files_mirrors" = if ($Channel -eq "Stable") { @($existingVersions.mod_hash_files_mirrors) } elseif ($existingEndpoints.mod_hash_files_mirrors) { @($existingEndpoints.mod_hash_files_mirrors) } else { @() }

    "beta_mod_url" = if ($Channel -eq "Beta") { "$modReleaseBaseUrl/$archiveName" } elseif ($existingEndpoints.beta_mod_url) { [string]$existingEndpoints.beta_mod_url } elseif ($existingVersions.beta_mod_url) { [string]$existingVersions.beta_mod_url } else { "https://sitodaking.it:8443/VanzakartBeta/VKBeta.zip" }
    "beta_mod_manifest_url" = if ($Channel -eq "Beta") { "$modReleaseBaseUrl/manifest_files.json" } elseif ($existingEndpoints.beta_mod_manifest_url) { [string]$existingEndpoints.beta_mod_manifest_url } elseif ($existingVersions.beta_mod_manifest_url) { [string]$existingVersions.beta_mod_manifest_url } else { "https://sitodaking.it:8443/VanzakartBeta/manifest_files.json" }
    "beta_mod_files_url" = if ($Channel -eq "Beta") { "$modReleaseBaseUrl/files/" } elseif ($existingEndpoints.beta_mod_files_url) { [string]$existingEndpoints.beta_mod_files_url } elseif ($existingVersions.beta_mod_files_url) { [string]$existingVersions.beta_mod_files_url } else { "https://sitodaking.it:8443/VanzakartBeta/files/" }
    "beta_mod_hash_files_url" = if ($Channel -eq "Beta") { "$modReleaseBaseUrl/_by_sha256/" } elseif ($existingEndpoints.beta_mod_hash_files_url) { [string]$existingEndpoints.beta_mod_hash_files_url } elseif ($existingVersions.beta_mod_hash_files_url) { [string]$existingVersions.beta_mod_hash_files_url } else { "https://sitodaking.it:8443/VanzakartBeta/_by_sha256/" }
    "beta_mod_mirrors" = if ($existingEndpoints.beta_mod_mirrors) { @($existingEndpoints.beta_mod_mirrors) } else { @() }
    "beta_mod_files_mirrors" = if ($existingEndpoints.beta_mod_files_mirrors) { @($existingEndpoints.beta_mod_files_mirrors) } else { @() }
    "beta_mod_hash_files_mirrors" = if ($existingEndpoints.beta_mod_hash_files_mirrors) { @($existingEndpoints.beta_mod_hash_files_mirrors) } else { @() }

    "music_pack_url" = if ($existingEndpoints.music_pack_url) { [string]$existingEndpoints.music_pack_url } elseif ($existingVersions.music_pack_url) { [string]$existingVersions.music_pack_url } else { "https://sitodaking.it:8443/MusicPack/vanzakart_musicpack.zip" }
    "music_pack_manifest_url" = if ($existingEndpoints.music_pack_manifest_url) { [string]$existingEndpoints.music_pack_manifest_url } elseif ($existingVersions.music_pack_manifest_url) { [string]$existingVersions.music_pack_manifest_url } else { "https://sitodaking.it:8443/MusicPack/manifest_files.json" }
    "music_pack_files_url" = if ($existingEndpoints.music_pack_files_url) { [string]$existingEndpoints.music_pack_files_url } elseif ($existingVersions.music_pack_files_url) { [string]$existingVersions.music_pack_files_url } else { "https://sitodaking.it:8443/MusicPack/files/" }
    "music_pack_mirrors" = if ($existingEndpoints.music_pack_mirrors) { @($existingEndpoints.music_pack_mirrors) } else { @() }
    "music_pack_files_mirrors" = if ($existingEndpoints.music_pack_files_mirrors) { @($existingEndpoints.music_pack_files_mirrors) } else { @() }

    "launcher_url" = if ($existingEndpoints.launcher_url) { [string]$existingEndpoints.launcher_url } elseif ($existingVersions.launcher_url) { [string]$existingVersions.launcher_url } else { "https://sitodaking.it:8443/Launcher/vanzakart_launcher.zip" }
    "launcher_mirrors" = if ($existingEndpoints.launcher_mirrors) { @($existingEndpoints.launcher_mirrors) } else { @() }

    "news_url" = if ($existingEndpoints.news_url) { [string]$existingEndpoints.news_url } elseif ($existingVersions.news_url) { [string]$existingVersions.news_url } elseif ($existingVersions.news_json_url) { [string]$existingVersions.news_json_url } else { "https://sitodaking.it:8443/Launcher/news.json" }
    "leaderboard_api_url" = if ($existingEndpoints.leaderboard_api_url) { [string]$existingEndpoints.leaderboard_api_url } elseif ($existingVersions.leaderboard_api_url) { [string]$existingVersions.leaderboard_api_url } else { "https://sitodaking.it:8443/api/vk_leaderboard.php" }
    "leaderboard_details_api_url" = if ($existingEndpoints.leaderboard_details_api_url) { [string]$existingEndpoints.leaderboard_details_api_url } elseif ($existingVersions.leaderboard_details_api_url) { [string]$existingVersions.leaderboard_details_api_url } else { "https://sitodaking.it:8443/api/leaderboard/" }
    "rooms_api_url" = if ($existingEndpoints.rooms_api_url) { [string]$existingEndpoints.rooms_api_url } elseif ($existingVersions.rooms_api_url) { [string]$existingVersions.rooms_api_url } else { "https://sitodaking.it:8443/api/vk_rooms.php" }
    "beta_token_verify_api_url" = if ($existingEndpoints.beta_token_verify_api_url) { [string]$existingEndpoints.beta_token_verify_api_url } elseif ($existingVersions.beta_token_verify_api_url) { [string]$existingVersions.beta_token_verify_api_url } else { "https://sitodaking.it:8443/api/vk_beta_token.php" }
    "download_page_url" = if ($existingEndpoints.download_page_url) { [string]$existingEndpoints.download_page_url } elseif ($existingVersions.download_page_url) { [string]$existingVersions.download_page_url } else { "https://vwfc.sitodaking.it/" }
    "mii_rendering_archive_url" = if ($existingEndpoints.mii_rendering_archive_url) { [string]$existingEndpoints.mii_rendering_archive_url } elseif ($existingVersions.mii_rendering_archive_url) { [string]$existingVersions.mii_rendering_archive_url } else { "https://web.archive.org/web/20180502054513id_/http://download-cdn.miitomo.com/native/20180125111639/android/v2/asset_model_character_mii_AFLResHigh_2_3_dat.zip" }
    "server_base_url" = if ($existingEndpoints.server_base_url) { [string]$existingEndpoints.server_base_url } else { "https://sitodaking.it:8443/" }
    "rank_images_base_url" = if ($existingEndpoints.rank_images_base_url) { [string]$existingEndpoints.rank_images_base_url } else { "https://sitodaking.it:8443/FOOTAGE/ranks/" }
}

$endpointsJsonContent = ConvertTo-Json -InputObject $endpointsObject -Depth 100
[System.IO.File]::WriteAllText($endpointsJsonPath, $endpointsJsonContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "Creato/Aggiornato il file: endpoints.json" -ForegroundColor Green

Write-Host "`n=== PROCESSO COMPLETATO ===" -ForegroundColor Green
Write-Host "I file generati nella cartella '$OutputDir' sono pronti per essere caricati!"
Write-Host "Ecco le istruzioni per il rilascio:"
Write-Host "1. Carica il contenuto di '$OutputDir' nella cartella /$serverDirectory/ del server."
Write-Host "   - Carica 'versions.json' ed 'endpoints.json' in /Launcher/ per ultimi."
Write-Host "   - Il file 'manifest_files.json' e '$archiveName' devono risiedere in $modReleaseBaseUrl/"
Write-Host "   - La cartella 'files' deve risiedere in $modReleaseBaseUrl/files/"
Write-Host "   - La cartella '_by_sha256' deve risiedere in $modReleaseBaseUrl/_by_sha256/"
if ($CreateFilesZip) {
    Write-Host "   - 'files.zip' e '_by_sha256.zip' sono archivi di trasferimento per caricare ed estrarre le rispettive cartelle sul server; il launcher non li usa direttamente per gli update."
}
Write-Host "2. Assicurati che i permessi di lettura sui file sul server siano corretti."
if ($interactiveLaunch) {
    [void](Read-Host "`nPremi INVIO; la console resterà aperta")
}
