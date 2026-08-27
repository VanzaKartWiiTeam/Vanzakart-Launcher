<#
.SYNOPSIS
    Prepara un rilascio completo: pacchetti, installer e i due manifest.

.DESCRIPTION
    Un rilascio serve due pubblici, e questo script prepara tutto per
    entrambi:

      · chi installa da zero legge `install.json` e scarica un pacchetto
        **portabile** (zip su Windows, `.app.tar.gz` su macOS, AppImage su
        Linux). A metterlo al suo posto ci pensa l'installer, che è l'unico
        a creare scorciatoie e registrazioni;

      · chi ha già il launcher legge `updater/<target>-<arch>.json` e scarica
        il pacchetto **nativo firmato** (NSIS su Windows; su macOS e Linux è
        lo stesso file del portabile).

    Ogni esecuzione produce ciò che riguarda il **sistema su cui gira**, e poi
    rigenera i manifest con tutto quello che trova nella cartella di uscita.
    Il giro completo per le tre piattaforme lo fa la CI
    (`.github/workflows/release.yml`), che esegue questo stesso script su
    Windows, macOS e Linux e poi lo riesegue una volta con `-SkipBuild` per
    unire i risultati.

    Le firme dell'updater richiedono `TAURI_SIGNING_PRIVATE_KEY` e
    `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` nell'ambiente. Senza, i pacchetti si
    costruiscono lo stesso ma non sono aggiornabili: lo script lo dice e
    `install.json` resta valido comunque.

.PARAMETER OutputDir
    Dove scrivere ciò che va caricato sul server.

.PARAMETER BaseUrl
    Cartella pubblica in cui finiranno i pacchetti. I manifest ci costruiscono
    sopra gli URL, quindi dev'essere l'indirizzo vero.

.PARAMETER SkipBuild
    Non compila niente: rigenera solo i manifest da ciò che c'è già.

.PARAMETER SkipSetupApp
    Non compila l'installer, solo i pacchetti del launcher.

.PARAMETER Notes
    Riga di novità, mostrata dall'installer e dall'aggiornamento in-app.

.EXAMPLE
    .\scripts\Publish-SetupRelease.ps1 -Notes "Prima versione Tauri."

.EXAMPLE
    # Dopo aver messo in dist-launcher/releases/<versione>/ gli artefatti
    # macOS e Linux scaricati dalla CI:
    .\scripts\Publish-SetupRelease.ps1 -SkipBuild -Notes "Prima versione Tauri."
#>
[CmdletBinding()]
param(
    [string]$OutputDir = './dist-launcher',
    [string]$BaseUrl = 'https://sitodaking.it:8443/Launcher/releases',
    [string]$Notes = '',
    [switch]$SkipBuild,
    [switch]$SkipSetupApp
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail([string]$message) {
    Write-Host "  $message" -ForegroundColor Red
    exit 1
}

function Step([string]$message) {
    Write-Host "→ $message" -ForegroundColor Cyan
}

function Warn([string]$message) {
    Write-Host "  $message" -ForegroundColor Yellow
}

# Copia un artefatto nella cartella di rilascio, portandosi dietro la firma
# `.sig` se c'è. Restituisce il nome del file copiato.
function Copy-Artifact([string]$source, [string]$destination) {
    Copy-Item $source $destination -Force
    if (Test-Path "$source.sig") {
        Copy-Item "$source.sig" "$destination.sig" -Force
    }
    Split-Path -Leaf $destination
}

# --- 1. Versione: deve coincidere in tutti i punti che la dichiarano -------
$conf = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
$version = $conf.version
$package = (Get-Content 'package.json' -Raw | ConvertFrom-Json).version
$cargo = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

if ($version -ne $package -or $version -ne $cargo) {
    Fail @"
Le versioni non coincidono:
  src-tauri/tauri.conf.json : $version
  package.json              : $package
  Cargo.toml                : $cargo

Allineale prima di pubblicare: i manifest dichiarano una versione sola, e
l'installer la mostra all'utente prima di scaricare.
"@
}

# --- 1-bis. Variabili di firma vuote: meglio assenti ------------------------
#
# `${{ secrets.NOME }}`, quando il segreto non esiste, non sparisce: diventa
# una stringa vuota. Tauri vede la variabile e conclude che si vuole firmare,
# poi passa a `security import` un certificato di zero byte e la build muore
# con "SecKeychainItemImport: parameters not valid". Toglierle di mezzo rende
# "segreto non configurato" identico a "variabile assente", che è quello che
# Tauri si aspetta.
foreach ($variabile in @(
        'APPLE_CERTIFICATE',
        'APPLE_CERTIFICATE_PASSWORD',
        'APPLE_SIGNING_IDENTITY',
        'APPLE_ID',
        'APPLE_PASSWORD',
        'APPLE_TEAM_ID',
        'TAURI_SIGNING_PRIVATE_KEY',
        'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')) {
    $percorso = "Env:\$variabile"
    if ((Test-Path $percorso) -and [string]::IsNullOrWhiteSpace((Get-Item $percorso).Value)) {
        Remove-Item $percorso
        Write-Host "  $variabile era vuota: la tolgo" -ForegroundColor DarkGray
    }
}

$releaseDir = Join-Path (Join-Path $OutputDir 'releases') $version
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Write-Host "VanzaKart Launcher $version → $releaseDir"

if (-not $SkipBuild -and -not $env:TAURI_SIGNING_PRIVATE_KEY) {
    Warn 'TAURI_SIGNING_PRIVATE_KEY non impostata: i pacchetti non saranno firmati'
    Warn "e l'aggiornamento dal launcher non li accettera. L'installazione da zero"
    Warn 'funziona lo stesso.'
}

# --- 2. Piattaforma corrente ----------------------------------------------
$targetKey = if ($IsWindows) { 'windows-x86_64' }
elseif ($IsMacOS) { 'darwin-universal' }
else { 'linux-x86_64' }

if ($IsWindows -and $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { $targetKey = 'windows-aarch64' }
if ($IsLinux -and (uname -m) -eq 'aarch64') { $targetKey = 'linux-aarch64' }

# --- 3. Pacchetti del launcher --------------------------------------------
if (-not $SkipBuild) {
    Step 'Compilazione del launcher'
    npm run build:only
    if ($LASTEXITCODE -ne 0) { Fail 'build del frontend non riuscita' }

    if ($IsWindows) {
        # NSIS serve all'aggiornamento in-app; il portabile lo si costruisce
        # qui sotto dall'eseguibile che la stessa build ha prodotto.
        npx tauri build --bundles nsis
        if ($LASTEXITCODE -ne 0) { Fail 'build del launcher non riuscita' }

        $binary = 'target/release/vanzakart-launcher.exe'
        if (-not (Test-Path $binary)) { Fail "eseguibile non trovato: $binary" }

        $staging = Join-Path $releaseDir '_staging'
        Remove-Item -Recurse -Force $staging -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $staging | Out-Null

        # Il nome è quello del prodotto, lo stesso che installa il pacchetto
        # NSIS: chi aggiorna dall'updater sovrascrive questo file invece di
        # affiancargliene un secondo (decisions.md §D-052).
        Copy-Item $binary (Join-Path $staging 'VanzaKart Launcher.exe')
        Copy-Item 'src-tauri/resources' (Join-Path $staging 'resources') -Recurse

        $payload = Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.zip"
        Remove-Item -Force $payload -ErrorAction SilentlyContinue
        Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $payload
        Remove-Item -Recurse -Force $staging
        Write-Host "  portabile: $(Split-Path -Leaf $payload)"

        $nsis = Get-ChildItem -Path 'target' -Recurse -Filter '*-setup.exe' -File |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($nsis) {
            $name = Copy-Artifact $nsis.FullName (Join-Path $releaseDir "VanzaKart-Update_${version}_$targetKey.exe")
            Write-Host "  nativo:    $name"
        }
        else {
            Warn 'pacchetto NSIS non trovato: nessun aggiornamento in-app per Windows'
        }
    }
    elseif ($IsMacOS) {
        # `app` produce il bundle e il suo `.app.tar.gz`, che è insieme il
        # pacchetto dell'updater e quello che scarica l'installer: su macOS i
        # due canali usano lo stesso file.
        npx tauri build --target universal-apple-darwin --bundles app
        if ($LASTEXITCODE -ne 0) { Fail 'build del launcher non riuscita' }

        $payload = Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.tar.gz"
        $tarball = Get-ChildItem -Path 'target' -Recurse -Filter '*.app.tar.gz' -File |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1

        if ($tarball) {
            $name = Copy-Artifact $tarball.FullName $payload
            Write-Host "  pacchetto: $name"
        }
        else {
            # Tauri produce il `.app.tar.gz` solo quando genera gli artefatti
            # dell'updater. Se non c'e, il bundle si impacchetta qui: serve
            # comunque all'installer, che non chiede nessuna firma.
            Warn 'archivio .app.tar.gz non prodotto dalla build: lo creo dal bundle'
            $bundle = Get-ChildItem -Path 'target' -Recurse -Filter 'VanzaKart Launcher.app' -Directory |
                Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if (-not $bundle) { Fail 'bundle .app non trovato' }

            # `tar` di sistema: preserva permessi e collegamenti simbolici del
            # bundle, che uno zip perderebbe rendendo l'app non avviabile.
            tar -czf $payload -C $bundle.Parent.FullName $bundle.Name
            if ($LASTEXITCODE -ne 0) { Fail 'creazione del tar.gz non riuscita' }
            Write-Host "  pacchetto: $(Split-Path -Leaf $payload)"
        }
    }
    else {
        npx tauri build --bundles appimage
        if ($LASTEXITCODE -ne 0) { Fail 'build del launcher non riuscita' }

        $appimage = Get-ChildItem -Path 'target' -Recurse -Filter '*.AppImage' -File |
            Where-Object { $_.Name -notlike '*setup*' } |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $appimage) {
            Fail @"
AppImage non trovata sotto target/.

Se la build e finita senza errori, il bundler AppImage puo aver fallito il
download di linuxdeploy: succede quando la rete del runner e lenta. Rilancia
il workflow.
"@
        }

        $name = Copy-Artifact $appimage.FullName (Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.AppImage")
        Write-Host "  pacchetto: $name"
    }
}

# --- 4. L'installer stesso -------------------------------------------------
if (-not $SkipBuild -and -not $SkipSetupApp) {
    Step "Compilazione dell'installer"
    npm run build:setup:only
    if ($LASTEXITCODE -ne 0) { Fail "build del frontend dell'installer non riuscita" }

    Push-Location 'setup'
    try {
        if ($IsWindows) { npx tauri build --no-bundle }
        elseif ($IsMacOS) { npx tauri build --target universal-apple-darwin --bundles dmg }
        else { npx tauri build --bundles appimage }
        if ($LASTEXITCODE -ne 0) { Fail "build dell'installer non riuscita" }
    }
    finally {
        Pop-Location
    }

    $setupArtifact = if ($IsWindows) {
        Get-Item 'target/release/vanzakart-setup.exe' -ErrorAction SilentlyContinue
    }
    elseif ($IsMacOS) {
        Get-ChildItem -Path 'target' -Recurse -Filter '*.dmg' -File |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    else {
        Get-ChildItem -Path 'target' -Recurse -Filter 'vanzakart-setup*.AppImage' -File |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }

    if ($setupArtifact) {
        $extension = [System.IO.Path]::GetExtension($setupArtifact.Name)
        $destination = Join-Path $releaseDir "VanzaKart-Setup_${version}_$targetKey$extension"
        Copy-Item $setupArtifact.FullName $destination -Force
        Write-Host "  installer: $(Split-Path -Leaf $destination)"
    }
    else {
        Warn 'installer non trovato: salto la copia'
    }
}

# --- 5. Manifest dell'installazione ---------------------------------------
Step 'Manifest install.json'

$eseguibili = @{
    'windows' = 'VanzaKart Launcher.exe'
    'darwin'  = 'VanzaKart Launcher.app'
    'linux'   = 'vanzakart-launcher.AppImage'
}

$platforms = [ordered]@{}
$payloads = Get-ChildItem -Path $releaseDir -File |
    Where-Object { $_.Name -like "VanzaKart-Launcher_${version}_*" -and $_.Extension -ne '.sig' } |
    Sort-Object Name

foreach ($file in $payloads) {
    if ($file.Name -notmatch "^VanzaKart-Launcher_${version}_(?<target>[a-z0-9]+-[a-z0-9_]+)\.(?<ext>zip|tar\.gz|AppImage)$") {
        Warn "ignoro $($file.Name): il nome non dice per quale piattaforma è"
        continue
    }

    $key = $Matches['target']
    $osName = $key.Split('-')[0]
    $format = switch ($Matches['ext']) {
        'zip' { 'zip' }
        'tar.gz' { 'tar-gz' }
        'AppImage' { 'app-image' }
    }

    $platforms[$key] = [ordered]@{
        url        = "$($BaseUrl.TrimEnd('/'))/$version/$($file.Name)"
        sha256     = (Get-FileHash -Algorithm SHA256 -Path $file.FullName).Hash.ToLowerInvariant()
        size       = $file.Length
        format     = $format
        executable = $eseguibili[$osName]
    }

    Write-Host "  $key → $($file.Name) ($([math]::Round($file.Length / 1MB, 1)) MB)"
}

if ($platforms.Count -eq 0) {
    Fail @"
Nessun pacchetto in $releaseDir.

I file devono chiamarsi VanzaKart-Launcher_${version}_<target>.<estensione>,
per esempio:
  VanzaKart-Launcher_${version}_windows-x86_64.zip
  VanzaKart-Launcher_${version}_darwin-universal.tar.gz
  VanzaKart-Launcher_${version}_linux-x86_64.AppImage
"@
}

$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = $platforms
}

$manifestPath = Join-Path $OutputDir 'install.json'
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path $manifestPath -Encoding utf8NoBOM
Write-Host "  scritto: $manifestPath"

# --- 6. Manifest dell'aggiornamento ---------------------------------------
Step 'Manifest updater'

$updaterDir = Join-Path $OutputDir 'updater'
New-Item -ItemType Directory -Force -Path $updaterDir | Out-Null

# Su Windows l'updater installa il pacchetto NSIS; su macOS e Linux lo stesso
# file che scarica l'installer. Si guarda quindi in due posti.
$updatable = @()
$updatable += Get-ChildItem -Path $releaseDir -File |
    Where-Object { $_.Name -like "VanzaKart-Update_${version}_*" -and $_.Extension -ne '.sig' }
$updatable += $payloads | Where-Object { $_.Name -notlike '*windows*' }

$written = 0
foreach ($file in $updatable) {
    if ($file.Name -notmatch "_(?<target>[a-z0-9]+-[a-z0-9_]+)\.(?:exe|zip|tar\.gz|AppImage)$") {
        continue
    }
    $key = $Matches['target']

    $signaturePath = "$($file.FullName).sig"
    if (-not (Test-Path $signaturePath)) {
        Warn "$($file.Name) non ha la firma: niente aggiornamento in-app per $key"
        continue
    }
    $signature = (Get-Content $signaturePath -Raw).Trim()

    # `darwin-universal` non è un bersaglio che l'updater conosce: costruisce
    # l'indirizzo con l'architettura della macchina, quindi lo stesso pacchetto
    # va dichiarato per tutte e due.
    $targets = if ($key -eq 'darwin-universal') { @('darwin-aarch64', 'darwin-x86_64') } else { @($key) }

    foreach ($target in $targets) {
        $document = [ordered]@{
            version   = $version
            notes     = $Notes
            pub_date  = $manifest.pub_date
            platforms = [ordered]@{
                $target = [ordered]@{
                    signature = $signature
                    url       = "$($BaseUrl.TrimEnd('/'))/$version/$($file.Name)"
                }
            }
        }
        $path = Join-Path $updaterDir "$target.json"
        $document | ConvertTo-Json -Depth 6 | Set-Content -Path $path -Encoding utf8NoBOM
        Write-Host "  $target → $($file.Name)"
        $written++
    }
}

if ($written -eq 0) {
    Warn 'nessun manifest di aggiornamento: mancano le firme (§5 di release.md)'
}

# --- 7. Cosa caricare, e in che ordine ------------------------------------
Write-Host ''
Write-Host 'Da caricare sul server, in questo ordine:' -ForegroundColor Green
Write-Host "  1. $releaseDir/*  →  /Launcher/releases/$version/"
if ($written -gt 0) {
    Write-Host "  2. $updaterDir/*.json  →  /Launcher/updater/     (aggiornamento dal launcher)"
    Write-Host "  3. $manifestPath  →  /Launcher/install.json      (installazione da zero)"
}
else {
    Write-Host "  2. $manifestPath  →  /Launcher/install.json      (installazione da zero)"
}
Write-Host ''
Write-Host 'Prima i pacchetti, poi i manifest: al contrario, chi controlla nel'
Write-Host 'mezzo leggerebbe un indirizzo che non esiste ancora.'

if ($platforms.Count -lt 3) {
    $parola = if ($platforms.Count -eq 1) { 'piattaforma' } else { 'piattaforme' }
    Write-Host ''
    Warn "Il manifest copre $($platforms.Count) $parola su 3."
    Write-Host "Sulle altre l'installer dira che non c'e un pacchetto per questo"
    Write-Host 'sistema. Gli artefatti mancanti li produce la CI: scaricali,'
    Write-Host "mettili in $releaseDir e rilancia con -SkipBuild."
}
