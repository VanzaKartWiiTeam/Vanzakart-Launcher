param(
    [string]$MusicPackPath = "",
    [string]$Version = "",
    [string]$OutputDir = (Join-Path $PSScriptRoot "MusicPackRelease"),
    [string]$VersionsJsonUrl = "https://sitodaking.it:8443/Launcher/versions.json",
    [string]$ServerBaseUrl = "https://sitodaking.it:8443",
    [string]$BetaManifestUrl = "",
    [string[]]$Changelog = @()
)

$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$stagingRoot = $null
$backupRoot = $null

function Write-JsonNoBom {
    param([object]$Value, [string]$Path)
    $json = $Value | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, $json, $utf8NoBom)
}

function As-StringArray {
    param($Value)
    $items = [System.Collections.Generic.List[string]]::new()
    if ($null -ne $Value) {
        foreach ($entry in @($Value)) {
            $text = [string]$entry
            if (-not [string]::IsNullOrWhiteSpace($text)) { $items.Add($text) }
        }
    }
    return ,$items
}

function Normalize-JsonText {
    param([string]$Text)
    if ($null -eq $Text) { return "" }
    # Gestisce BOM Unicode normale e BOM UTF-8 decodificato come "ï»¿" da Windows PowerShell 5.1.
    return $Text.TrimStart([char[]]@(
        [char]0xFEFF, [char]0x200B,
        [char]0x00EF, [char]0x00BB, [char]0x00BF,
        [char]0x20, [char]0x09, [char]0x0D, [char]0x0A))
}

# Funzione per creare archivi ZIP conformi alle specifiche PKWARE (separatori '/', UTF-8, directory esplicite).
# Risolve l'incompatibilità su macOS, Linux e ChromeOS causata da ZipFile / .NET Framework su Windows.
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

function Needs-HashAddressedFallback {
    param([string]$WebPath)
    return $WebPath -match '[^A-Za-z0-9._~/-]'
}

try {
    if ([string]::IsNullOrWhiteSpace($MusicPackPath)) { $MusicPackPath = Read-Host "Cartella Music Pack (o My Stuff)" }
    if ([string]::IsNullOrWhiteSpace($Version)) { $Version = Read-Host "Versione Music Pack (es. 1.0.0)" }
    $MusicPackPath = $MusicPackPath.Trim().Trim('"')
    $Version = $Version.Trim().Trim('"')

    if (-not (Test-Path -LiteralPath $MusicPackPath -PathType Container)) {
        throw "Cartella Music Pack non trovata: $MusicPackPath"
    }
    if ([string]::IsNullOrWhiteSpace($Version)) { throw "La versione non può essere vuota." }

    $sourceRoot = [System.IO.Path]::GetFullPath($MusicPackPath).TrimEnd('\', '/')
    $outputRoot = [System.IO.Path]::GetFullPath($OutputDir).TrimEnd('\', '/')
    if ($outputRoot.Equals($sourceRoot, [StringComparison]::OrdinalIgnoreCase) -or
        $outputRoot.StartsWith($sourceRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "OutputDir non può trovarsi dentro la cartella sorgente."
    }

    if ([IO.Path]::GetFileName($sourceRoot).Equals("My Stuff", [StringComparison]::OrdinalIgnoreCase)) {
        $payloadRoot = $sourceRoot
    }
    else {
        $myStuff = Get-ChildItem -LiteralPath $sourceRoot -Directory -Recurse |
            Where-Object { $_.Name.Equals("My Stuff", [StringComparison]::OrdinalIgnoreCase) } |
            Sort-Object { $_.FullName.Length } | Select-Object -First 1
        $payloadRoot = if ($myStuff) { $myStuff.FullName } else { $sourceRoot }
    }
    $sourceFiles = @(Get-ChildItem -LiteralPath $payloadRoot -File -Recurse)
    if ($sourceFiles.Count -eq 0) { throw "La cartella Music Pack non contiene file." }

    Write-Host "Download versions.json attuale..." -ForegroundColor Yellow
    $cacheBuster = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $currentJson = Normalize-JsonText ((Invoke-WebRequest -UseBasicParsing -Uri "${VersionsJsonUrl}?t=$cacheBuster" -TimeoutSec 30).Content)
    $versions = $currentJson | ConvertFrom-Json
    if (-not $versions.mod_version) { throw "versions.json non contiene mod_version." }
    if (-not $versions.launcher_version) { throw "versions.json non contiene launcher_version." }

    $serverBaseUrl = $ServerBaseUrl.TrimEnd('/')
    $betaBaseUrl = "$serverBaseUrl/VanzakartBeta"
    if ([string]::IsNullOrWhiteSpace($BetaManifestUrl)) {
        $BetaManifestUrl = "$betaBaseUrl/manifest_files.json"
    }

    # Il Music Pack rigenera il versions.json centrale: aggiorna i dati Beta dal
    # manifest del canale quando è online, oppure conserva quelli già pubblicati.
    $betaManifest = $null
    try {
        Write-Host "Lettura metadati canale Beta da $BetaManifestUrl..." -ForegroundColor Yellow
        $betaCacheBuster = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $betaJson = Normalize-JsonText ((Invoke-WebRequest -UseBasicParsing -Uri "${BetaManifestUrl}?t=$betaCacheBuster" -TimeoutSec 30).Content)
        $betaManifest = $betaJson | ConvertFrom-Json
        if (-not $betaManifest.mod_version -or @($betaManifest.files).Count -eq 0) {
            throw "Il manifest Beta non contiene mod_version o files."
        }
        if ($betaManifest.archive_sha256 -and ([string]$betaManifest.archive_sha256) -notmatch '^[0-9a-fA-F]{64}$') {
            throw "archive_sha256 del manifest Beta non è valido."
        }
        Write-Host "Beta rilevata: $($betaManifest.mod_version) ($(@($betaManifest.files).Count) file)." -ForegroundColor Green
    }
    catch {
        $betaManifest = $null
        if ($versions.beta_mod_version) {
            Write-Host "Manifest Beta non disponibile; mantengo i metadati Beta già presenti in versions.json. Dettaglio: $($_.Exception.Message)" -ForegroundColor DarkYellow
        }
        else {
            Write-Host "Manifest Beta non disponibile e nessun metadato Beta precedente da conservare. Dettaglio: $($_.Exception.Message)" -ForegroundColor DarkYellow
        }
    }

    $outputParent = [IO.Path]::GetDirectoryName($outputRoot)
    if ([string]::IsNullOrWhiteSpace($outputParent)) { $outputParent = $PSScriptRoot }
    New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
    $stagingRoot = Join-Path $outputParent ".musicpack-release-$([guid]::NewGuid().ToString('N'))"
    $backupRoot = "$outputRoot.backup-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null

    Write-Host "Creazione ZIP completo (cross-platform)..." -ForegroundColor Yellow
    $zipPath = Join-Path $stagingRoot "vanzakart_musicpack.zip"
    New-StandardZipArchive -SourceDirectory $payloadRoot -DestinationZipPath $zipPath

    Write-Host "Creazione aggiornamento differenziale..." -ForegroundColor Yellow
    $filesRoot = Join-Path $stagingRoot "files"
    $hashFilesRoot = Join-Path $filesRoot "_by_sha256"
    New-Item -ItemType Directory -Force -Path $filesRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $hashFilesRoot | Out-Null
    $manifestFiles = [System.Collections.Generic.List[object]]::new()
    $hashFallbackFilesCount = 0
    $hashFallbackUniqueHashes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $sourceFiles) {
        $relative = $file.FullName.Substring($payloadRoot.TrimEnd('\').Length).TrimStart('\')
        $webPath = $relative.Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifestFiles.Add([ordered]@{ path = $webPath; sha256 = $hash; size = $file.Length })
        $destination = Join-Path $filesRoot $relative
        New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($destination)) | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
        if (Needs-HashAddressedFallback -WebPath $webPath) {
            Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $hashFilesRoot $hash) -Force
            $hashFallbackFilesCount++
            [void]$hashFallbackUniqueHashes.Add($hash)
        }
    }

    $missingDifferentialFiles = [System.Collections.Generic.List[string]]::new()
    foreach ($entry in $manifestFiles) {
        $relativeDiskPath = ([string]$entry.path).Replace('/', [IO.Path]::DirectorySeparatorChar)
        $expectedPath = Join-Path $filesRoot $relativeDiskPath
        $expectedHashPath = Join-Path $hashFilesRoot ([string]$entry.sha256)
        if (-not (Test-Path -LiteralPath $expectedPath -PathType Leaf)) {
            $missingDifferentialFiles.Add([string]$entry.path)
        }
        if ((Needs-HashAddressedFallback -WebPath ([string]$entry.path)) -and
            -not (Test-Path -LiteralPath $expectedHashPath -PathType Leaf)) {
            $missingDifferentialFiles.Add("_by_sha256/$([string]$entry.sha256) ($([string]$entry.path))")
        }
    }
    if ($missingDifferentialFiles.Count -gt 0) {
        $preview = ($missingDifferentialFiles | Select-Object -First 20) -join "`n - "
        throw "Release differenziale Music Pack non valida: $($missingDifferentialFiles.Count) file sono nel manifest ma non esistono nella cartella files.`n - $preview"
    }

    Write-Host "Path sensibili coperti da fallback _by_sha256: $hashFallbackFilesCount" -ForegroundColor DarkCyan
    Write-Host "File hash unici creati in _by_sha256: $($hashFallbackUniqueHashes.Count)" -ForegroundColor DarkCyan

    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-JsonNoBom -Value ([ordered]@{ mod_version = $Version; archive_sha256 = $zipHash; files = $manifestFiles }) -Path (Join-Path $stagingRoot "manifest_files.json")

    Write-Host "Creazione files.zip per il caricamento sul server (cross-platform)..." -ForegroundColor Yellow
    $filesZipPath = Join-Path $stagingRoot "files.zip"
    New-StandardZipArchive -SourceDirectory $filesRoot -DestinationZipPath $filesZipPath

    $canonical = [ordered]@{}
    foreach ($property in $versions.PSObject.Properties) { $canonical[$property.Name] = $property.Value }

    # Canonicalizza tutti i campi array, anche se il JSON online precedente conteneva una stringa.
    $canonical["mod_mirrors"] = As-StringArray $versions.mod_mirrors
    $canonical["mod_files_mirrors"] = As-StringArray $versions.mod_files_mirrors
    $canonical["launcher_mirrors"] = As-StringArray $versions.launcher_mirrors
    $canonical["changelog"] = As-StringArray $versions.changelog
    $canonical["beta_mod_mirrors"] = As-StringArray $versions.beta_mod_mirrors
    $canonical["beta_mod_files_mirrors"] = As-StringArray $versions.beta_mod_files_mirrors
    $canonical["beta_changelog"] = As-StringArray $versions.beta_changelog

    if ($null -ne $betaManifest) {
        $betaVersion = [string]$betaManifest.mod_version
        $canonical["beta_mod_version"] = $betaVersion
        $canonical["beta_mod_url"] = "$betaBaseUrl/VKBeta.zip"
        $canonical["beta_mod_manifest_url"] = $BetaManifestUrl
        $canonical["beta_mod_files_url"] = "$betaBaseUrl/files/"
        $canonical["beta_mod_mirrors"] = As-StringArray $null
        $canonical["beta_mod_files_mirrors"] = As-StringArray $null
        $canonical["beta_mod_sha256"] = if ($betaManifest.archive_sha256) {
            ([string]$betaManifest.archive_sha256).ToLowerInvariant()
        }
        elseif ($versions.beta_mod_version -eq $betaVersion) {
            [string]$versions.beta_mod_sha256
        }
        else {
            ""
        }
        if (-not $versions.beta_mod_version -or
            $versions.beta_mod_version -ne $betaVersion -or
            @($versions.beta_changelog).Count -eq 0) {
            $canonical["beta_changelog"] = As-StringArray "VanzaKart Beta $betaVersion"
        }
    }

    $canonical["music_pack_version"] = $Version
    $canonical["music_pack_url"] = "$serverBaseUrl/MusicPack/vanzakart_musicpack.zip"
    $canonical["music_pack_mirrors"] = As-StringArray $null
    $canonical["music_pack_sha256"] = $zipHash
    $canonical["music_pack_manifest_url"] = "$serverBaseUrl/MusicPack/manifest_files.json"
    $canonical["music_pack_files_url"] = "$serverBaseUrl/MusicPack/files/"
    $canonical["music_pack_files_mirrors"] = As-StringArray $null
    $canonical["music_pack_changelog"] = if ($Changelog.Count -gt 0) { As-StringArray $Changelog } else { As-StringArray "VanzaKart Music Pack $Version" }
    Write-JsonNoBom -Value $canonical -Path (Join-Path $stagingRoot "versions.json")

    if (Test-Path -LiteralPath $outputRoot) { Move-Item -LiteralPath $outputRoot -Destination $backupRoot }
    try {
        Move-Item -LiteralPath $stagingRoot -Destination $outputRoot
        $stagingRoot = $null
        if (Test-Path -LiteralPath $backupRoot) { Remove-Item -LiteralPath $backupRoot -Recurse -Force }
        $backupRoot = $null
    }
    catch {
        if (Test-Path -LiteralPath $outputRoot) { Remove-Item -LiteralPath $outputRoot -Recurse -Force }
        if (Test-Path -LiteralPath $backupRoot) { Move-Item -LiteralPath $backupRoot -Destination $outputRoot }
        throw
    }

    Write-Host "Release Music Pack $Version completata: $outputRoot" -ForegroundColor Green
    Write-Host "Carica files.zip sul server ed estrailo dentro /MusicPack/files/ includendo anche la cartella _by_sha256."
    Write-Host "Carica vanzakart_musicpack.zip e manifest_files.json in /MusicPack/; versions.json in /Launcher/ per ultimo."
}
catch {
    if ($stagingRoot -and (Test-Path -LiteralPath $stagingRoot)) { Remove-Item -LiteralPath $stagingRoot -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Host "MUSIC PACK RELEASE FAILED: $($_.Exception.Message)" -ForegroundColor Red
    throw
}
