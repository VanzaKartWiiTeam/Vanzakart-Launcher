# VanzaKart Launcher — Tauri 2

Riscrittura del launcher VanzaKart da C#/.NET 8/WPF a **Rust + Tauri 2** con
frontend **TypeScript + Svelte 5 + Vite**.

Il launcher legacy resta il riferimento per gli utenti finché questa versione
non raggiunge la parità funzionale: vedi [`docs/status.md`](docs/status.md).

---

## Cosa cambia rispetto al launcher C#

| | Legacy | Questo |
| --- | --- | --- |
| Piattaforme | Windows | Windows, macOS, Linux |
| UI | WPF | Svelte 5 in una webview, stessa identità grafica |
| Aggiornamento del launcher | script PowerShell non firmato | updater Tauri con firma Ed25519 |
| Installazione | Setup e Uninstaller in WPF, solo Windows | stessa procedura guidata su Windows, macOS e Linux (`setup/`), più i pacchetti nativi NSIS, DMG, AppImage, deb, rpm |
| Configurazione | registro di Windows + AppDir | un file JSON per OS, scritture atomiche |
| Log | percorsi e URL in chiaro | sanitizzati in scrittura e in lettura |
| Avvio di Dolphin | `UseShellExecute = true` | `std::process` con argomenti separati |
| Lingua | solo italiano | inglese di default, italiano a un clic dalle impostazioni |

Contratti server, formato dei manifest, layout delle cartelle della modpack e
file di versione **non cambiano**: i due launcher possono convivere sulla
stessa macchina.

---

## Struttura

```
tauri-launcher/
  crates/
    vk-core/        HTTP con resume e mirror, manifest, hash, ZIP sicuro,
                    protezione dei dati utente, update transazionale
    vk-dolphin/     INI format-preserving, percorsi, Riivolution, controller
    vk-install/     installazione e rimozione: manifest dei pacchetti,
                    scorciatoie, registrazione, registro dell'installazione
    vk-save/        rksys.dat, RFL_DB.dat, blocchi Mii, friend code
  src-tauri/
    src/domain/     tipi scambiati con il frontend
    src/storage/    persistenza, migrazioni, import legacy
    src/platform/   unico punto con API di sistema
    src/services/   casi d'uso, indipendenti da Tauri
    src/commands/   guscio IPC
  src/
    lib/api/        wrapper IPC tipizzati
    lib/components/ componenti riusabili
    lib/i18n/       dizionari italiano (riferimento) e inglese
    lib/styles/     design system estratto dallo XAML
    routes/         una pagina per voce di menu
  setup/            installer e disinstallatore: stessa finestra, stesso
                    design, un solo binario in due vesti
  docs/
```

I tre crate di dominio non dipendono da Tauri e non chiamano API di sistema:
si compilano e si testano da soli.

```bash
cargo test -p vk-core
```

`vk-install` è l'eccezione consapevole: installare *è* un'operazione di
piattaforma, e le API di sistema stanno tutte in `vk-install::platform`
([`docs/installer.md`](docs/installer.md)).

---

## Sviluppo

```bash
npm install
npm run tauri dev
```

Con log verbosi:

```bash
VK_LOG=debug npm run tauri dev
```

### Verifiche

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

npm run lint
npm run check
npm run build
```

Sono le stesse che gira la CI (`.github/workflows/ci.yml`).

---

## Avvio su Linux e macOS

**Non serve `sudo`, e con `sudo` non parte.** Un'applicazione grafica avviata
come root non ha il cookie di autorizzazione del server grafico dell'utente:
GTK non riesce ad aprire nessuna finestra e il processo muore. Il launcher se
ne accorge da solo e lo dice, invece di lasciar morire GTK con un panic.

```bash
chmod +x VanzaKart-Launcher_*_linux-x86_64.AppImage
./VanzaKart-Launcher_*_linux-x86_64.AppImage
```

Su Ubuntu 24.04 e su altre distribuzioni recenti gli AppImage hanno bisogno di
FUSE 2, che non è più preinstallato: `sudo apt install libfuse2t64`, oppure si
avvia con `--appimage-extract-and-run` senza installare niente.

In una sessione **Wayland** il launcher gira sul backend nativo: l'AppImage non
forza più X11 e non si porta dietro le librerie Wayland del sistema, che erano
la causa di un crash di WebKitGTK all'avvio (`decisions.md` §D-072). All'avvio
prova comunque da sola quale configurazione grafica regge, e se un avvio non
arriva alla finestra il successivo riparte in modalità conservativa.

Se manca qualche libreria di sistema, il messaggio d'errore indica il pacchetto
da installare (`libwebkit2gtk-4.1-0` e `libgtk-3-0` su Debian e Ubuntu,
`webkit2gtk4.1` e `gtk3` su Fedora, `webkit2gtk-4.1` e `gtk3` su Arch). Con i
driver proprietari NVIDIA il launcher disattiva da solo il rendering DMA-BUF di
WebKit, che su quelle schede disegna una finestra bianca.

Su **macOS** il pacchetto non è notarizzato: al primo avvio il sistema lo
blocca. Si apre con il tasto destro → *Apri*, oppure togliendo la quarantena:

```bash
xattr -dr com.apple.quarantine "/Applications/VanzaKart Launcher.app"
```

Anche qui niente `sudo`: servirebbe solo a far fallire la connessione a
WindowServer.

---

## Dove finiscono i dati

| OS | Cartella |
| --- | --- |
| Windows | `%APPDATA%\VanzaKart\Launcher\` |
| macOS | `~/Library/Application Support/VanzaKart/Launcher/` |
| Linux | `~/.local/share/VanzaKart/Launcher/` |

Al primo avvio, su Windows, i dati del launcher legacy vengono importati:
**copia integrale prima di tradurre, originali mai toccati**. Il dettaglio è
in [`docs/migration.md`](docs/migration.md), con la procedura per annullare
l'import.

---

## Sicurezza

- SHA-256 verificato su ogni file scaricato e sull'archivio completo.
- Estrazione ZIP con difesa Zip-Slip: path assoluti, `..`, drive letter,
  symlink e zip-bomb vengono rifiutati.
- Staging + apply atomico: nessun file viene applicato finché l'intero
  aggiornamento non è stato scaricato e verificato.
- Backup e rollback verificato per hash prima di ogni aggiornamento.
- Il frontend **non** ha accesso al filesystem: nessuna capability `fs:*`.
  Ogni operazione su file passa da un comando Rust che valida i percorsi.
- Gli endpoint remoti sono accettati solo su `https`, senza credenziali
  nell'URL, campo per campo.
- Il token beta non attraversa mai l'IPC dopo essere stato salvato.
- Log sanitizzati: home directory → `~`, query string rimosse, token mascherati.

---

## Documentazione

| Documento | Contenuto |
| --- | --- |
| [`docs/handoff.md`](docs/handoff.md) | **Parti da qui**: contesto completo, regole, cosa manca e come procedere |
| [`docs/parity-matrix.md`](docs/parity-matrix.md) | 68 funzioni legacy mappate sui nuovi moduli |
| [`docs/status.md`](docs/status.md) | cosa è pronto, cosa è parziale, cosa manca |
| [`docs/migration.md`](docs/migration.md) | compatibilità file, import non distruttivo, rollback |
| [`docs/decisions.md`](docs/decisions.md) | 55 decisioni tecniche con l'alternativa scartata |
| [`docs/ui-parity.md`](docs/ui-parity.md) | token di design estratti dallo XAML e differenze motivate |
| [`docs/release.md`](docs/release.md) | build, firma, updater, pubblicazione |
| [`docs/installer.md`](docs/installer.md) | installer e disinstallatore: cosa fanno, contratto `install.json`, come si pubblicano |
