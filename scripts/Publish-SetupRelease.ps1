<#
.SYNOPSIS
    Prepara un rilascio completo: pacchetti, installer e i due manifest.

.DESCRIPTION
    Un rilascio è **un pacchetto per piattaforma e un manifest solo**.

    Il pacchetto è portabile — zip su Windows, `.app.tar.gz` su macOS,
    AppImage su Linux — e serve a tutti e due i pubblici:

      · chi installa da zero lo scarica con l'installer, che lo mette al suo
        posto, crea le scorciatoie e registra il programma;

      · chi ha già il launcher lo scarica dal launcher stesso, che lo srotola
        **nella cartella in cui è già installato** senza creare niente di
        nuovo (decisions.md §D-084).

    Non esistono più né il pacchetto NSIS né i manifest `updater/*.json`:
    erano un secondo installer che si installava per conto suo in
    `%LOCALAPPDATA%` e lasciava un secondo disinstallatore sul computer.

    Ogni esecuzione produce ciò che riguarda il **sistema su cui gira**, e poi
    rigenera `install.json` con tutto quello che trova nella cartella di
    uscita. Il giro completo per le tre piattaforme lo fa la CI
    (`.github/workflows/release.yml`), che esegue questo stesso script su
    Windows, macOS e Linux e poi lo riesegue una volta con `-SkipBuild` per
    unire i risultati.

    Le firme richiedono `TAURI_SIGNING_PRIVATE_KEY` e
    `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` nell'ambiente, e viaggiano come file
    `.sig` accanto al pacchetto: è così che il lavoro di tre macchine diverse
    si ricompone in un manifest solo. Senza chiave i pacchetti si costruiscono
    lo stesso e si installano lo stesso, ma nessuno può dimostrare chi li ha
    pubblicati.

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
    [string]$BaseUrl = 'https://vanzakart.net:8443/Launcher/releases',
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

# Copia un artefatto nella cartella di rilascio e lo firma. Restituisce il
# nome del file copiato.
#
# La firma sta accanto al pacchetto, in un `.sig`: è così che arriva alla
# macchina che scrive `install.json`, che nella CI è un'altra e non ha la
# chiave privata.
function Copy-Artifact([string]$source, [string]$destination) {
    Copy-Item $source $destination -Force
    Add-Signature $destination | Out-Null
    Split-Path -Leaf $destination
}

# Firma un pacchetto con la chiave dell'updater, se c'è.
#
# Senza chiave non è un errore: il pacchetto resta installabile, e a dirlo
# sarà `install.json`, che semplicemente non dichiarerà nessuna firma.
function Add-Signature([string]$path) {
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        return $false
    }

    Remove-Item "$path.sig" -Force -ErrorAction SilentlyContinue
    $global:LASTEXITCODE = 0
    try {
        npx tauri signer sign $path 2>&1 | Out-Null
    }
    catch {
        $global:LASTEXITCODE = 1
    }

    if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$path.sig")) {
        $global:LASTEXITCODE = 0
        Warn "firma non riuscita per $(Split-Path -Leaf $path): il pacchetto resta installabile ma non autenticato"
        return $false
    }

    Write-Host "  firmato:   $(Split-Path -Leaf $path).sig"
    return $true
}

# --- AppImage: quello che linuxdeploy si porta dietro e non dovrebbe -------

# Librerie che un AppImage non deve impacchettare.
#
# `libwayland-*` è legata al compositore e allo stack EGL del sistema. Se il
# processo ne carica due copie — quella dentro l'AppImage, per via del RPATH, e
# quella che `libEGL` di sistema tira dentro — la creazione del display EGL
# fallisce e WebKitGTK aborta prima ancora di aprire la finestra:
#
#     Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
#
# È il motivo per cui `libwayland-client.so.0` sta nella excludelist ufficiale
# di AppImage; il plugin GTK di linuxdeploy la copia lo stesso. Toglierla è la
# stessa cosa che fa a mano un `LD_PRELOAD` della libreria di sistema.
$LibrerieDaEscludere = @(
    'libwayland-client.so*',
    'libwayland-cursor.so*',
    'libwayland-egl.so*',
    'libwayland-server.so*'
)

# Trova `appimagetool`, che serve a riconfezionare l'AppDir corretto.
#
# Il bundler di Tauri lo ha già scaricato per costruire l'AppImage: si riusa
# quello, e solo se non c'è lo si prende dalla rete.
function Get-AppImageTool([string]$bundleDir) {
    $cercaIn = @(
        $bundleDir,
        (Join-Path $HOME '.cache/tauri'),
        'target/release/bundle/appimage'
    ) | Where-Object { $_ -and (Test-Path $_) }

    foreach ($cartella in $cercaIn) {
        $trovato = Get-ChildItem -Path $cartella -Recurse -Filter 'appimagetool*' -File -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($trovato) { return $trovato.FullName }
    }

    $comando = Get-Command 'appimagetool' -ErrorAction SilentlyContinue
    if ($comando) { return $comando.Source }

    $scaricato = Join-Path ([System.IO.Path]::GetTempPath()) 'appimagetool-x86_64.AppImage'
    if (-not (Test-Path $scaricato)) {
        Write-Host '  appimagetool non trovato in locale: lo scarico'
        curl -sSfL -o $scaricato 'https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage'
        if ($LASTEXITCODE -ne 0) { return $null }
        chmod +x $scaricato
    }
    return $scaricato
}

# Toglie dall'AppDir le librerie di sistema e rifà l'AppImage.
#
# Non fa fallire il rilascio: se qualcosa non va resta l'AppImage originale,
# con un avviso. Restituisce il percorso da pubblicare.
function Repair-AppImage([string]$appImagePath) {
    # Qualunque cosa vada storta qui dentro non deve fermare il rilascio: si
    # pubblica l'AppImage originale, con un avviso.
    try {
        return Repair-AppImageInner $appImagePath
    }
    catch {
        Warn "correzione dell AppImage non riuscita ($($_.Exception.Message)): pubblico quella originale"
        $global:LASTEXITCODE = 0
        return $appImagePath
    }
    finally {
        # Gli avvisi qui sopra non devono lasciare un codice d'uscita che il
        # passo successivo scambierebbe per suo.
        if ($LASTEXITCODE -ne 0) { $global:LASTEXITCODE = 0 }
    }
}

function Repair-AppImageInner([string]$appImagePath) {
    $bundleDir = Split-Path -Parent $appImagePath
    $appDir = Get-ChildItem -Path $bundleDir -Directory -Filter '*.AppDir' -ErrorAction SilentlyContinue |
        Select-Object -First 1

    if (-not $appDir) {
        Warn 'AppDir non trovato accanto all AppImage: la lascio com e'
        return $appImagePath
    }

    $libDir = Join-Path $appDir.FullName 'usr/lib'
    $rimosse = @()
    foreach ($schema in $LibrerieDaEscludere) {
        Get-ChildItem -Path $libDir -Filter $schema -File -ErrorAction SilentlyContinue | ForEach-Object {
            Remove-Item $_.FullName -Force
            $rimosse += $_.Name
        }
    }

    # Il backend GDK non si forza più su X11: in una sessione Wayland il
    # launcher deve girare su Wayland. Chi vuole X11 imposta la variabile e
    # vince lui; senza sessione Wayland resta il comportamento di prima.
    $hook = Join-Path $appDir.FullName 'apprun-hooks/linuxdeploy-plugin-gtk.sh'
    $backendCorretto = $false
    if (Test-Path $hook) {
        $testo = Get-Content $hook -Raw
        $nuovo = [regex]::Replace(
            $testo,
            '(?m)^export GDK_BACKEND=x11.*$',
            'if [ -z "${GDK_BACKEND:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then export GDK_BACKEND=x11; fi'
        )
        if ($nuovo -ne $testo) {
            Set-Content -Path $hook -Value $nuovo -NoNewline
            $backendCorretto = $true
        }
    }

    if ($rimosse.Count -eq 0 -and -not $backendCorretto) {
        Write-Host '  AppImage gia a posto: niente da correggere'
        return $appImagePath
    }

    if ($rimosse.Count -gt 0) {
        Write-Host "  tolte dall AppDir: $($rimosse -join ', ')"
    }
    if ($backendCorretto) {
        Write-Host '  GDK_BACKEND non e piu forzato su x11'
    }

    $tool = Get-AppImageTool $bundleDir
    if (-not $tool) {
        Warn 'appimagetool non disponibile: pubblico l AppImage non corretta'
        return $appImagePath
    }

    $nuovoFile = "$appImagePath.corretta"
    $env:ARCH = 'x86_64'
    chmod +x $tool 2>$null

    # `--appimage-extract-and-run` evita di dover montare: serve solo se lo
    # strumento è a sua volta un AppImage, altrimenti è un argomento che non
    # conosce.
    $global:LASTEXITCODE = 0
    try {
        if ($tool -like '*.AppImage') {
            & $tool --appimage-extract-and-run $appDir.FullName $nuovoFile 2>&1 | Out-Null
        }
        else {
            & $tool $appDir.FullName $nuovoFile 2>&1 | Out-Null
        }
    }
    catch {
        Warn "appimagetool non eseguibile: $($_.Exception.Message)"
        $global:LASTEXITCODE = 1
    }

    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $nuovoFile)) {
        Warn 'riconfezionamento non riuscito: pubblico l AppImage non corretta'
        Remove-Item $nuovoFile -Force -ErrorAction SilentlyContinue
        return $appImagePath
    }

    # La firma si fa dopo, sul file definitivo: qui non c'e' ancora niente da
    # rifare.
    Move-Item $nuovoFile $appImagePath -Force
    chmod +x $appImagePath
    Write-Host '  AppImage riconfezionata'
    return $appImagePath
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
    Warn 'TAURI_SIGNING_PRIVATE_KEY non impostata: i pacchetti non saranno firmati.'
    Warn 'Si installano e si aggiornano lo stesso, verificati con l impronta,'
    Warn 'ma nessuno puo dimostrare chi li ha pubblicati.'
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
        # Nessun bundle: su Windows il pacchetto è uno zip con dentro
        # l'eseguibile e le risorse, ed è l'installer (o l'aggiornamento in
        # loco) a metterlo al suo posto.
        npx tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { Fail 'build del launcher non riuscita' }

        $binary = 'target/release/vanzakart-launcher.exe'
        if (-not (Test-Path $binary)) { Fail "eseguibile non trovato: $binary" }

        $staging = Join-Path $releaseDir '_staging'
        Remove-Item -Recurse -Force $staging -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $staging | Out-Null

        # Il nome è quello del prodotto, lo stesso che il launcher già
        # installato si ritrova sul disco: l'aggiornamento sostituisce
        # *quel* file invece di affiancargliene un secondo (decisions.md
        # §D-052, §D-084).
        Copy-Item $binary (Join-Path $staging 'VanzaKart Launcher.exe')
        Copy-Item 'src-tauri/resources' (Join-Path $staging 'resources') -Recurse

        $payload = Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.zip"
        Remove-Item -Force $payload -ErrorAction SilentlyContinue
        Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $payload
        Remove-Item -Recurse -Force $staging
        Write-Host "  pacchetto: $(Split-Path -Leaf $payload)"
        Add-Signature $payload | Out-Null
    }
    elseif ($IsMacOS) {
        npx tauri build --target universal-apple-darwin --bundles app
        if ($LASTEXITCODE -ne 0) { Fail 'build del launcher non riuscita' }

        $payload = Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.tar.gz"
        $bundle = Get-ChildItem -Path 'target' -Recurse -Filter 'VanzaKart Launcher.app' -Directory |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $bundle) { Fail 'bundle .app non trovato' }

        # `tar` di sistema: preserva permessi e collegamenti simbolici del
        # bundle, che uno zip perderebbe rendendo l'app non avviabile.
        Remove-Item -Force $payload -ErrorAction SilentlyContinue
        tar -czf $payload -C $bundle.Parent.FullName $bundle.Name
        if ($LASTEXITCODE -ne 0) { Fail 'creazione del tar.gz non riuscita' }
        Write-Host "  pacchetto: $(Split-Path -Leaf $payload)"
        Add-Signature $payload | Out-Null
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

        # Prima di pubblicarla: via le librerie di sistema che linuxdeploy si
        # porta dietro e che rompono EGL, e niente X11 forzato (§D-072).
        $corretta = Repair-AppImage $appimage.FullName

        $name = Copy-Artifact $corretta (Join-Path $releaseDir "VanzaKart-Launcher_${version}_$targetKey.AppImage")
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

    $voce = [ordered]@{
        url        = "$($BaseUrl.TrimEnd('/'))/$version/$($file.Name)"
        sha256     = (Get-FileHash -Algorithm SHA256 -Path $file.FullName).Hash.ToLowerInvariant()
        size       = $file.Length
        format     = $format
        executable = $eseguibili[$osName]
    }

    # La firma è nel `.sig` accanto al pacchetto, prodotto dalla macchina che
    # lo ha compilato. Senza, il pacchetto resta installabile: si verifica
    # l'impronta e basta.
    $firma = "$($file.FullName).sig"
    if (Test-Path $firma) {
        $voce['signature'] = (Get-Content $firma -Raw).Trim()
    }
    else {
        Warn "$($file.Name) non ha la firma: il pacchetto non sara autenticato"
    }

    $platforms[$key] = $voce
    $firmato = if ($voce.Contains('signature')) { 'firmato' } else { 'NON firmato' }
    Write-Host "  $key → $($file.Name) ($([math]::Round($file.Length / 1MB, 1)) MB, $firmato)"
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

# Gli installer: non servono all'installer stesso, servono al sito, che cosi
# legge un file solo invece di avere tre link scritti a mano.
$installer = [ordered]@{}
Get-ChildItem -Path $releaseDir -File |
    Where-Object { $_.Name -like "VanzaKart-Setup_${version}_*" -and $_.Extension -ne '.sig' } |
    Sort-Object Name |
    ForEach-Object {
        if ($_.Name -match "^VanzaKart-Setup_${version}_(?<target>[a-z0-9]+-[a-z0-9_]+)\.[A-Za-z]+$") {
            $installer[$Matches['target']] = [ordered]@{
                url    = "$($BaseUrl.TrimEnd('/'))/$version/$($_.Name)"
                sha256 = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
                size   = $_.Length
            }
            Write-Host "  installer $($Matches['target']) → $($_.Name)"
        }
    }

$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = $platforms
    setup     = $installer
}

$manifestPath = Join-Path $OutputDir 'install.json'
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path $manifestPath -Encoding utf8NoBOM
Write-Host "  scritto: $manifestPath"

# --- 6. Cosa caricare, e in che ordine ------------------------------------
Write-Host ''
Write-Host 'Da caricare sul server, in questo ordine:' -ForegroundColor Green
Write-Host "  1. $releaseDir/*  →  /Launcher/releases/$version/"
Write-Host "  2. $manifestPath  →  /Launcher/install.json"
Write-Host ''
Write-Host 'Prima i pacchetti, poi il manifest: al contrario, chi controlla nel'
Write-Host 'mezzo leggerebbe un indirizzo che non esiste ancora.'
Write-Host ''
Write-Host 'Lo stesso install.json serve a chi installa da zero e a chi aggiorna'
Write-Host 'dal launcher: non c e un secondo manifest da ricordarsi.'

$nonFirmati = @($platforms.Values | Where-Object { -not $_.Contains('signature') }).Count
if ($nonFirmati -gt 0) {
    Write-Host ''
    Warn "$nonFirmati pacchetti su $($platforms.Count) non sono firmati."
    Write-Host 'Si installano e si aggiornano lo stesso, verificati con l impronta,'
    Write-Host 'ma nessuno puo dimostrare chi li ha pubblicati. Imposta'
    Write-Host 'TAURI_SIGNING_PRIVATE_KEY e rilancia (release.md §5).'
}

if ($platforms.Count -lt 3) {
    $parola = if ($platforms.Count -eq 1) { 'piattaforma' } else { 'piattaforme' }
    Write-Host ''
    Warn "Il manifest copre $($platforms.Count) $parola su 3."
    Write-Host "Sulle altre l'installer dira che non c'e un pacchetto per questo"
    Write-Host 'sistema. Gli artefatti mancanti li produce la CI: scaricali,'
    Write-Host "mettili in $releaseDir e rilancia con -SkipBuild."
}
