param(
    [string]$MusicPackPath = "",
    [string]$Version = "",
    [string]$OutputDir = (Join-Path $PSScriptRoot "MusicPackRelease"),
    [string]$VersionsJsonUrl = "https://sitodaking.it/Launcher/versions.json",
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

    $outputParent = [IO.Path]::GetDirectoryName($outputRoot)
    if ([string]::IsNullOrWhiteSpace($outputParent)) { $outputParent = $PSScriptRoot }
    New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
    $stagingRoot = Join-Path $outputParent ".musicpack-release-$([guid]::NewGuid().ToString('N'))"
    $backupRoot = "$outputRoot.backup-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null

    Write-Host "Creazione ZIP completo..." -ForegroundColor Yellow
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipPath = Join-Path $stagingRoot "vanzakart_musicpack.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($payloadRoot, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $false)

    Write-Host "Creazione aggiornamento differenziale..." -ForegroundColor Yellow
    $filesRoot = Join-Path $stagingRoot "files"
    New-Item -ItemType Directory -Force -Path $filesRoot | Out-Null
    $manifestFiles = [System.Collections.Generic.List[object]]::new()
    foreach ($file in $sourceFiles) {
        $relative = $file.FullName.Substring($payloadRoot.TrimEnd('\').Length).TrimStart('\')
        $webPath = $relative.Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifestFiles.Add([ordered]@{ path = $webPath; sha256 = $hash; size = $file.Length })
        $destination = Join-Path $filesRoot $relative
        New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($destination)) | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
    }
    Write-JsonNoBom -Value ([ordered]@{ mod_version = $Version; files = $manifestFiles }) -Path (Join-Path $stagingRoot "manifest_files.json")

    Write-Host "Creazione files.zip per il caricamento sul server..." -ForegroundColor Yellow
    $filesZipPath = Join-Path $stagingRoot "files.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory(
        $filesRoot,
        $filesZipPath,
        [IO.Compression.CompressionLevel]::Optimal,
        $false)

    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
    $canonical = [ordered]@{}
    foreach ($property in $versions.PSObject.Properties) { $canonical[$property.Name] = $property.Value }

    # Canonicalizza tutti i campi array, anche se il JSON online precedente conteneva una stringa.
    $canonical["mod_mirrors"] = As-StringArray $versions.mod_mirrors
    $canonical["mod_files_mirrors"] = As-StringArray $versions.mod_files_mirrors
    $canonical["launcher_mirrors"] = As-StringArray $versions.launcher_mirrors
    $canonical["changelog"] = As-StringArray $versions.changelog
    $canonical["music_pack_version"] = $Version
    $canonical["music_pack_url"] = "https://sitodaking.it/MusicPack/vanzakart_musicpack.zip"
    $canonical["music_pack_mirrors"] = As-StringArray $null
    $canonical["music_pack_sha256"] = $zipHash
    $canonical["music_pack_manifest_url"] = "https://sitodaking.it/MusicPack/manifest_files.json"
    $canonical["music_pack_files_url"] = "https://sitodaking.it/MusicPack/files/"
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
    Write-Host "Carica files.zip sul server ed estrailo dentro /MusicPack/files/."
    Write-Host "Carica vanzakart_musicpack.zip e manifest_files.json in /MusicPack/; versions.json in /Launcher/ per ultimo."
}
catch {
    if ($stagingRoot -and (Test-Path -LiteralPath $stagingRoot)) { Remove-Item -LiteralPath $stagingRoot -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Host "MUSIC PACK RELEASE FAILED: $($_.Exception.Message)" -ForegroundColor Red
    throw
}
