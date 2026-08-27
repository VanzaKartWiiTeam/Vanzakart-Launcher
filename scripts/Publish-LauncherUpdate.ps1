<#
.SYNOPSIS
    Compila il launcher firmato e produce il manifest dell'updater.

.DESCRIPTION
    Un rilascio del launcher sono tre cose, e devono essere coerenti:

      1. il pacchetto (`*-setup.exe`);
      2. la sua firma (`*-setup.exe.sig`), che l'updater verifica prima di
         installare qualunque cosa;
      3. il manifest `<target>-<arch>.json`, che dice ai launcher installati
         dove trovare il pacchetto e con quale firma confrontarlo.

    Questo script le produce tutte e tre. La chiave privata resta dov'è: non
    viene copiata, non viene stampata, non entra nel repository.

.PARAMETER PrivateKey
    Percorso della chiave privata dell'updater. Se non esiste, lo script si
    ferma e dice come generarla — non la crea da sé, perché una chiave di firma
    la si genera una volta e la si conserva, non la si trova per caso.

.PARAMETER OutputDir
    Dove scrivere ciò che va caricato sul server.

.EXAMPLE
    # Una volta sola, e poi conserva il file:
    npx tauri signer generate --write-keys "$HOME\.tauri\vanzakart.key"

    # A ogni rilascio:
    .\scripts\Publish-LauncherUpdate.ps1 -PrivateKey "$HOME\.tauri\vanzakart.key" -Notes "Correzioni all'aggiornamento differenziale."
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PrivateKey,
    [string]$KeyPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD,
    [string]$Notes = '',
    [string]$OutputDir = './dist-launcher',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail([string]$message) {
    Write-Host "  $message" -ForegroundColor Red
    exit 1
}

# --- 1. Chiave -------------------------------------------------------------
if (-not (Test-Path $PrivateKey)) {
    Fail @"
Chiave privata non trovata: $PrivateKey

Generane una (una volta sola, poi conservala fuori dal repository):
    npx tauri signer generate --write-keys "$PrivateKey"

Il comando stampa la chiave PUBBLICA: incollala in
src-tauri/tauri.conf.json, sotto plugins.updater.pubkey.
"@
}

$conf = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
$pubkey = $conf.plugins.updater.pubkey
if ([string]::IsNullOrWhiteSpace($pubkey) -or $pubkey -eq 'REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY') {
    Fail @"
La chiave pubblica in src-tauri/tauri.conf.json e ancora il segnaposto.

Senza, i launcher gia installati rifiutano ogni aggiornamento: la firma non
puo essere verificata contro niente. Incolla li la chiave pubblica stampata
da 'tauri signer generate'.
"@
}

$version = $conf.version
Write-Host "  VanzaKart Launcher $version" -ForegroundColor Magenta

# --- 2. Build firmata ------------------------------------------------------
if (-not $SkipBuild) {
    # La password non finisce ne' sulla riga di comando ne' nella cronologia
    # della shell: se non arriva dall'ambiente la si chiede qui, mascherata.
    if (-not $KeyPassword) {
        $secure = Read-Host -Prompt '  Password della chiave privata (vuoto se non ne ha)' -AsSecureString
        $KeyPassword = [System.Net.NetworkCredential]::new('', $secure).Password
    }

    Write-Host '  Build firmata in corso...' -ForegroundColor Cyan
    $env:TAURI_SIGNING_PRIVATE_KEY = (Resolve-Path $PrivateKey).Path
    # Sempre impostata, anche vuota: senza la variabile la CLI chiede la
    # password a video e la build si blocca in attesa.
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $KeyPassword

    try {
        # npx e non 'npm run tauri -- ...': npm interpreta i flag anche dopo
        # il '--' e si mangia --bundles, lasciando a cargo un 'nsis' solitario.
        npx tauri build --bundles nsis
        if ($LASTEXITCODE -ne 0) { Fail 'La build e fallita.' }
    }
    finally {
        # La password non resta nell'ambiente della sessione.
        Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
        Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
        $KeyPassword = $null
    }
}

# --- 3. Pacchetto e firma --------------------------------------------------
$bundle = 'src-tauri/target/release/bundle/nsis'
if (-not (Test-Path $bundle)) { $bundle = 'target/release/bundle/nsis' }

$setup = Get-ChildItem $bundle -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { Fail "Nessun installer in $bundle." }

$signature = "$($setup.FullName).sig"

# Una firma piu' vecchia del pacchetto e' di una build precedente: il manifest
# risulterebbe valido e ogni launcher rifiuterebbe l'aggiornamento con un
# errore di verifica. Meglio fermarsi qui che scoprirlo dagli utenti.
if ((Test-Path $signature) -and
    ((Get-Item $signature).LastWriteTime -lt $setup.LastWriteTime.AddSeconds(-30))) {
    Fail @"
La firma e' piu' vecchia del pacchetto:

  $($setup.Name)      $($setup.LastWriteTime)
  $($setup.Name).sig  $((Get-Item $signature).LastWriteTime)

E' rimasta da una build precedente. Rilancia senza -SkipBuild: un manifest
costruito con questa firma farebbe fallire la verifica su ogni launcher.
"@
}

if (-not (Test-Path $signature)) {
    Fail @"
Manca il file .sig accanto a $($setup.Name).

La build non ha firmato. Le cause, in ordine di probabilita':

  1. password della chiave sbagliata: la CLI dice
     'incorrect updater private key password' e produce comunque
     l'installer, ma senza .sig;
  2. -SkipBuild su una build precedente alla firma: rilancia senza;
  3. bundle.createUpdaterArtifacts non e' true in tauri.conf.json.
"@
}

# --- 4. Manifest -----------------------------------------------------------
New-Item -ItemType Directory -Force $OutputDir | Out-Null
$releaseDir = Join-Path $OutputDir "releases/$version"
New-Item -ItemType Directory -Force $releaseDir | Out-Null
$updaterDir = Join-Path $OutputDir 'updater'
New-Item -ItemType Directory -Force $updaterDir | Out-Null

Copy-Item $setup.FullName $releaseDir -Force
Copy-Item $signature $releaseDir -Force

$serverBase = $conf.plugins.updater.endpoints[0] -replace '/updater/.*$', ''
$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = (Get-Content $signature -Raw).Trim()
            url       = "$serverBase/releases/$version/$([uri]::EscapeDataString($setup.Name))"
        }
    }
}

$manifestPath = Join-Path $updaterDir 'windows-x86_64.json'
$manifest | ConvertTo-Json -Depth 6 | Set-Content $manifestPath -Encoding utf8

Write-Host ''
Write-Host '  Pronto da caricare:' -ForegroundColor Green
Write-Host "    $releaseDir\$($setup.Name)"
Write-Host "    $releaseDir\$($setup.Name).sig"
Write-Host "    $manifestPath"
Write-Host ''
Write-Host '  ORDINE DI CARICAMENTO: prima il pacchetto e il .sig,' -ForegroundColor Yellow
Write-Host '  il manifest PER ULTIMO. Al contrario, un launcher che' -ForegroundColor Yellow
Write-Host '  controlla nel mezzo scarica un URL che non esiste ancora.' -ForegroundColor Yellow
