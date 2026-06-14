<#
.SYNOPSIS
    Genera il rilascio di un aggiornamento per VanzaKart (inclusi i file singoli, il manifest e lo zip intero).
.DESCRIPTION
    Questo script scansiona la cartella locale del modpack VanzaKart, calcola gli hash SHA-256 dei file
    escludendo quelli privati/protetti (es. salvataggi, mii, My Stuff), crea il manifest differenziale,
    genera l'archivio ZIP per le installazioni da zero e aggiorna versions.json.
.PARAMETER ModPath
    Il percorso della cartella "VanzaKart" locale contenente la mod aggiornata.
.PARAMETER Version
    La nuova versione della mod (es. 1.2.0).
.PARAMETER OutputDir
    La directory in cui generare i file da caricare sul server (default: .\dist).
#>

param (
    [Parameter(Mandatory=$true)]
    [string]$ModPath,

    [Parameter(Mandatory=$true)]
    [string]$Version,

    [Parameter(Mandatory=$false)]
    [string]$OutputDir = ".\dist"
)

$ErrorActionPreference = "Stop"

# Liste di esclusione (devono corrispondere a quelle del Launcher in C#)
$ProtectedDirs = @("My Stuff", "UserData", "userdata", "Saves", "Save", "Licenses", "License", "Patenti", "Profiles", "Miis", "Mii")
$ProtectedFiles = @("rksys.dat", "RFL_DB.dat", "active_mii.txt", "mii_profile.json")
$ProtectedExts = @(".mii", ".miigx", ".mae", ".vk-mii")
$ProtectedSubstrings = @("save", "license", "patent", "mii", "profile")

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
    foreach ($seg in $segments) {
        if ($ProtectedDirs -contains $seg) { return $true }
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

Write-Host "=== Generazione Rilascio VanzaKart v$Version ===" -ForegroundColor Cyan

# Risolvi percorsi assoluti
$absoluteModPath = [System.IO.Path]::GetFullPath($ModPath)
$absoluteOutputDir = [System.IO.Path]::GetFullPath($OutputDir)

if (-not (Test-Path $absoluteModPath -PathType Container)) {
    Write-Error "La cartella mod specificata non esiste: $absoluteModPath"
}

Write-Host "Mod Path: $absoluteModPath"
Write-Host "Output Dir: $absoluteOutputDir"

# Crea directory temporanee e di output
$tempZipFolder = Join-Path $env:TEMP "vanzakart_release_temp_$([guid]::NewGuid().ToString())"
$filesOutputDir = Join-Path $absoluteOutputDir "files"

if (Test-Path $absoluteOutputDir) {
    Write-Host "Rimozione vecchia cartella di output..." -ForegroundColor Yellow
    Remove-Item -Path $absoluteOutputDir -Recurse -Force -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Force -Path $absoluteOutputDir | Out-Null
New-Item -ItemType Directory -Force -Path $filesOutputDir | Out-Null
New-Item -ItemType Directory -Force -Path $tempZipFolder | Out-Null

Write-Host "Scansione dei file e calcolo degli hash..."

$manifestFiles = @()
$allowedFilesCount = 0
$skippedFilesCount = 0

# Recupera ricorsivamente tutti i file nella cartella mod
$files = Get-ChildItem -Path $absoluteModPath -Recurse -File

foreach ($file in $files) {
    # Ottieni il percorso relativo del file
    $relativePath = Get-RelativePath -BasePath $absoluteModPath -Path $file.FullName
    
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
    if (-not (Test-Path $destDir)) {
        New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    }
    Copy-Item -Path $file.FullName -Destination $destFile -Force
    
    # Copia nella cartella temporanea per lo ZIP
    $tempZipFile = Join-Path $tempZipFolder $relativePath
    $tempZipDir = [System.IO.Path]::GetDirectoryName($tempZipFile)
    if (-not (Test-Path $tempZipDir)) {
        New-Item -ItemType Directory -Force -Path $tempZipDir | Out-Null
    }
    Copy-Item -Path $file.FullName -Destination $tempZipFile -Force
}

Write-Host "Scansione completata." -ForegroundColor Green
Write-Host " - File inclusi: $allowedFilesCount"
Write-Host " - File privati esclusi (es. saves, My Stuff): $skippedFilesCount"

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

# 2. Compressione in VanzaKart.zip per installazioni da zero
$zipPath = Join-Path $absoluteOutputDir "VanzaKart.zip"
Write-Host "Compressione dello ZIP per installazioni complete..." -ForegroundColor Yellow
# Comprime il contenuto della cartella temporanea nel file zip di output
Compress-Archive -Path "$tempZipFolder\*" -DestinationPath $zipPath -Force
Write-Host "Creato archivio ZIP completo: VanzaKart.zip" -ForegroundColor Green

# Pulisci cartella temporanea
Remove-Item -Path $tempZipFolder -Recurse -Force -ErrorAction SilentlyContinue

# Calcola hash dello ZIP generato
$zipSha256 = Get-FileSha256 -FilePath $zipPath

# 3. Creazione o aggiornamento di versions.json
$versionsJsonPath = Join-Path $absoluteOutputDir "versions.json"
$baseVersionsObject = @{
    "mod_version" = $Version
    "launcher_version" = "1.1.0"
    "mod_url" = "https://sitodaking.it/Modpack/VanzaKart.zip"
    "mod_sha256" = $zipSha256
    "mod_manifest_url" = "https://sitodaking.it/Modpack/manifest_files.json"
    "mod_files_url" = "https://sitodaking.it/Modpack/files/"
    "mod_mirrors" = @()
    "mod_files_mirrors" = @()
    "launcher_url" = "https://sitodaking.it/Launcher/vanzakart_launcher.zip"
    "launcher_mirrors" = @()
    "changelog" = @("Rilascio versione $Version")
}

# Se esiste un versions.json locale o precedente, prova a leggerlo per preservare configurazioni avanzate
$localVersionsPath = ".\versions.json"
if (Test-Path $localVersionsPath) {
    Write-Host "Trovato versions.json esistente in locale, aggiorno solo le proprietà della mod..." -ForegroundColor Yellow
    try {
        $existingContent = Get-Content -Raw -Path $localVersionsPath | ConvertFrom-Json
        # Copia proprietà preservando quelle del launcher o changelog preesistente
        foreach ($prop in $existingContent.PSObject.Properties) {
            $baseVersionsObject[$prop.Name] = $prop.Value
        }
        # Sovrascrivi le proprietà relative all'aggiornamento appena fatto
        $baseVersionsObject["mod_version"] = $Version
        $baseVersionsObject["mod_sha256"] = $zipSha256
        $baseVersionsObject["mod_manifest_url"] = "https://sitodaking.it/Modpack/manifest_files.json"
        $baseVersionsObject["mod_files_url"] = "https://sitodaking.it/Modpack/files/"
    }
    catch {
        Write-Host "Impossibile leggere il versions.json locale esistente. Uso il modello standard." -ForegroundColor DarkYellow
    }
}

$versionsJsonContent = ConvertTo-Json -InputObject $baseVersionsObject -Depth 100
[System.IO.File]::WriteAllText($versionsJsonPath, $versionsJsonContent)
Write-Host "Creato/Aggiornato il file: versions.json" -ForegroundColor Green

Write-Host "`n=== PROCESSO COMPLETATO ===" -ForegroundColor Green
Write-Host "I file generati nella cartella '$OutputDir' sono pronti per essere caricati!"
Write-Host "Ecco le istruzioni per il rilascio:"
Write-Host "1. Carica il contenuto di '$OutputDir' sul tuo server web (es. dentro la cartella /Modpack/ e /Launcher/)."
Write-Host "   - Il file 'versions.json' deve risiedere all'URL configurato nel Launcher (di default: https://sitodaking.it/Launcher/versions.json)"
Write-Host "   - Il file 'manifest_files.json' e 'VanzaKart.zip' devono risiedere all'URL configurato (di default: https://sitodaking.it/Modpack/)"
Write-Host "   - La cartella 'files' deve risiedere all'URL configurato (di default: https://sitodaking.it/Modpack/files/)"
Write-Host "2. Assicurati che i permessi di lettura sui file sul server siano corretti."
