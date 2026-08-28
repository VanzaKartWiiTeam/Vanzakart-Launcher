//! Adapter Linux.

use std::path::PathBuf;

/// Controlli d'ambiente prima di aprire la finestra.
///
/// L'interfaccia gira su GTK, e GTK non parte se non trova un display a cui
/// connettersi: quello che si vede allora è un panic dentro `gtk::rt::init`
/// — *"Failed to initialize GTK"* — che non dice né la causa né il rimedio.
/// Le due cause pratiche sono queste, e si riconoscono prima (§D-067).
pub fn preflight() -> Result<(), String> {
    if super::launched_as_root() {
        return Err(ROOT_MESSAGE.into());
    }

    if !has_display() {
        return Err(DISPLAY_MESSAGE.into());
    }

    start_gtk()
}

/// Avvia GTK qui, dove un fallimento si può ancora raccontare.
///
/// Dentro `tao` la stessa chiamata è un `expect`: quello che l'utente vede è
/// un panic su un file del registry di cargo, senza una riga che dica cosa
/// fare. Facendolo prima si ottengono due cose: il **ripiego su X11**, che
/// rimette in piedi i casi in cui il backend Wayland della GTK impacchettata
/// non parte, e un messaggio che porta con sé lo stato dell'ambiente, così una
/// segnalazione basta a sé stessa (§D-070).
///
/// La seconda `gtk::init()` di `tao` non ripete niente: GTK inizializzata
/// risponde subito che lo è.
fn start_gtk() -> Result<(), String> {
    if gtk::init().is_ok() {
        return Ok(());
    }

    // Sessione Wayland con XWayland disponibile: il backend X11 è la strada
    // che funziona quando quello Wayland non parte.
    let can_try_x11 = std::env::var_os("GDK_BACKEND").is_none()
        && std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty());

    if can_try_x11 {
        std::env::set_var("GDK_BACKEND", "x11");
        if gtk::init().is_ok() {
            tracing::warn!("GTK avviata sul backend X11: quello nativo non è partito");
            return Ok(());
        }
        std::env::remove_var("GDK_BACKEND");
    }

    Err(format!(
        "{GTK_MESSAGE}

{}",
        environment()
    ))
}

/// Fotografia dell'ambiente grafico, da incollare in una segnalazione.
fn environment() -> String {
    let value = |name: &str| std::env::var(name).unwrap_or_else(|_| "(non impostata)".into());

    format!(
        "Ambiente:
  XDG_SESSION_TYPE = {}
  DISPLAY          = {}
           WAYLAND_DISPLAY  = {}
  GDK_BACKEND      = {}
  APPIMAGE         = {}",
        value("XDG_SESSION_TYPE"),
        value("DISPLAY"),
        value("WAYLAND_DISPLAY"),
        value("GDK_BACKEND"),
        value("APPIMAGE"),
    )
}

const ROOT_MESSAGE: &str = "Il launcher è stato avviato come root (sudo).

Un'applicazione grafica avviata con sudo non riesce ad aprire nessuna finestra:
il cookie di autorizzazione del server grafico appartiene al tuo utente, non a
root — è l'errore \"Authorization required, but no authorization protocol
specified\" che compare appena prima. In più le impostazioni e i salvataggi
finirebbero nella cartella di root invece che nella tua.

Riavvialo senza sudo:

    ./VanzaKart-Launcher_*.AppImage

Il launcher non ha bisogno di privilegi di amministratore. Se sai quello che
fai e vuoi comunque proseguire, imposta VK_ALLOW_ROOT=1.";

const DISPLAY_MESSAGE: &str =
    "Nessun server grafico disponibile: né DISPLAY né WAYLAND_DISPLAY sono impostate.

Il launcher ha una finestra e non può partire in una sessione solo testuale o
in un collegamento SSH senza inoltro grafico. Avvialo dalla sessione desktop,
oppure con \"ssh -X\" se stai lavorando da remoto.";

const GTK_MESSAGE: &str = "GTK non è riuscita ad aprire il display.

Il server grafico c'è, ma la libreria non riesce a usarlo. Le cause abituali,
in ordine di frequenza:

  · librerie di sistema mancanti. Su Debian e Ubuntu:
        sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libgdk-pixbuf-2.0-0
    su Fedora:
        sudo dnf install webkit2gtk4.1 gtk3
    su Arch:
        sudo pacman -S webkit2gtk-4.1 gtk3

  · l'AppImage e le librerie di sistema non vanno d'accordo. Prova ad
    avviarla senza montarla:
        ./VanzaKart-Launcher_*.AppImage --appimage-extract-and-run
    oppure installa il pacchetto .deb o .rpm, che usa le librerie del sistema.

  · sessione Wayland senza XWayland. Il launcher ha già provato a ripiegare
    su X11 e non c'è riuscito: installa xorg-xwayland, oppure avvia la
    sessione in modalità X11.";

fn has_display() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Aggira il rendering DMA-BUF di WebKit sulle schede NVIDIA.
///
/// Con i driver proprietari NVIDIA WebKitGTK disegna una finestra bianca o va
/// in crash appena carica la pagina: è un difetto noto del percorso DMA-BUF, e
/// il rimedio che usano tutte le applicazioni Tauri è disattivarlo. Si tocca
/// solo se la variabile non è già impostata e solo dove il driver c'è davvero,
/// perché altrove quel percorso funziona ed è più veloce (§D-067).
pub fn prepare_graphics() {
    const VARIABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

    if std::env::var_os(VARIABLE).is_some() || !nvidia_driver_loaded() {
        return;
    }

    std::env::set_var(VARIABLE, "1");
    tracing::info!("driver NVIDIA rilevato: rendering DMA-BUF di WebKit disattivato");
}

fn nvidia_driver_loaded() -> bool {
    if std::path::Path::new("/sys/module/nvidia").exists() {
        return true;
    }

    std::fs::read_to_string("/proc/modules")
        .is_ok_and(|modules| modules.lines().any(|line| line.starts_with("nvidia ")))
}

/// Su Linux Dolphin non usa il registro: nessun percorso da leggere.
pub fn dolphin_registry_user_path() -> Option<String> {
    None
}

/// Il launcher legacy è Windows-only: non c'è nulla da importare.
pub fn legacy_install_dir() -> Option<PathBuf> {
    None
}

/// Radici aggiuntive: i percorsi in cui finiscono AppImage e build portable.
pub fn extra_search_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("bin"));
        roots.push(home.join("Applications"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_messages_say_what_to_do() {
        assert!(ROOT_MESSAGE.contains("senza sudo"));
        assert!(ROOT_MESSAGE.contains("VK_ALLOW_ROOT"));
        assert!(DISPLAY_MESSAGE.contains("WAYLAND_DISPLAY"));
        assert!(GTK_MESSAGE.contains("appimage-extract-and-run"));
        assert!(GTK_MESSAGE.contains("libwebkit2gtk-4.1-0"));
    }

    #[test]
    fn the_environment_snapshot_names_every_variable_that_matters() {
        let snapshot = environment();
        for name in [
            "XDG_SESSION_TYPE",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "GDK_BACKEND",
            "APPIMAGE",
        ] {
            assert!(snapshot.contains(name), "manca {name}");
        }
    }

    #[test]
    fn a_display_is_needed_to_open_a_window() {
        // Uno dei due basta, e una variabile vuota non conta.
        assert_eq!(
            has_display(),
            ["DISPLAY", "WAYLAND_DISPLAY"]
                .iter()
                .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        );
    }

    #[test]
    fn there_is_nothing_to_read_from_a_registry() {
        assert!(dolphin_registry_user_path().is_none());
        assert!(legacy_install_dir().is_none());
    }

    #[test]
    fn common_install_prefixes_are_searched() {
        assert!(extra_search_roots().contains(&PathBuf::from("/opt")));
    }
}
