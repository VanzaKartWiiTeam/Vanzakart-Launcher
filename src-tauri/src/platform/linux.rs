//! Adapter Linux.

use std::path::PathBuf;

/// Bandiera con cui il launcher richiama sé stesso per provare GTK.
///
/// Non è un'opzione della riga di comando: chi la passa a mano ottiene un
/// processo che prova ad aprire il display e si chiude subito.
pub const GTK_PROBE_FLAG: &str = "--vk-gtk-probe";

/// Se questo processo è una sonda, prova GTK e si chiude con il suo esito.
///
/// Va chiamata per prima cosa, prima di qualunque altra inizializzazione: la
/// sonda non deve fare altro che aprire il display.
pub fn handle_probe_if_requested() {
    if std::env::args_os().any(|argument| argument == GTK_PROBE_FLAG) {
        std::process::exit(i32::from(gtk::init().is_err()));
    }
}

/// Controlli d'ambiente prima di aprire la finestra.
///
/// L'interfaccia gira su GTK, e quando GTK non parte quello che si vede è un
/// panic dentro `gtk::rt::init` — o, peggio, un `abort()` di GDK che nemmeno
/// arriva al panic. Qui si riconoscono prima le cause note e si sceglie una
/// configurazione grafica che funzioni (§D-067, §D-071).
pub fn preflight() -> Result<(), String> {
    if super::launched_as_root() {
        return Err(ROOT_MESSAGE.into());
    }

    if !has_display() {
        return Err(DISPLAY_MESSAGE.into());
    }

    start_gtk()
}

const ROOT_MESSAGE: &str = "\
Il launcher è stato avviato come root (sudo).

Un'applicazione grafica avviata con sudo non riesce ad aprire nessuna finestra:
il cookie di autorizzazione del server grafico appartiene al tuo utente, non a
root — è l'errore \"Authorization required, but no authorization protocol
specified\" che compare appena prima. In più le impostazioni e i salvataggi
finirebbero nella cartella di root invece che nella tua.

Riavvialo senza sudo:

    ./VanzaKart-Launcher_*.AppImage

Il launcher non ha bisogno di privilegi di amministratore. Se sai quello che
fai e vuoi comunque proseguire, imposta VK_ALLOW_ROOT=1.";

const DISPLAY_MESSAGE: &str = "\
Nessun server grafico disponibile: né DISPLAY né WAYLAND_DISPLAY sono impostate.

Il launcher ha una finestra e non può partire in una sessione solo testuale o
in un collegamento SSH senza inoltro grafico. Avvialo dalla sessione desktop,
oppure con \"ssh -X\" se stai lavorando da remoto.";

const GTK_MESSAGE: &str = "\
GTK non è riuscita ad aprire il display, in nessuna delle configurazioni
provate.

Le cause abituali, in ordine di frequenza:

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

  · sessione Wayland senza XWayland e senza backend Wayland utilizzabile:
    installa xorg-xwayland, oppure avvia la sessione in modalità X11.";

fn has_display() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

// ---------------------------------------------------------------------------
// Scelta della configurazione grafica
// ---------------------------------------------------------------------------

/// Una configurazione grafica da provare, con le variabili che la definiscono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    /// Come si chiama, per il log e per il messaggio d'errore.
    pub label: &'static str,
    pub variables: &'static [(&'static str, &'static str)],
}

/// Le configurazioni da provare, nell'ordine in cui conviene provarle.
///
/// La prima è l'ambiente così com'è: se funziona non si tocca niente. Le altre
/// sono le vie di scampo note, dalla meno invasiva alla più lenta:
///
/// - **Wayland nativo**: l'`AppRun` che Tauri mette dentro l'AppImage esporta
///   `GDK_BACKEND=x11`, e in una sessione Wayland questo manda GDK su XWayland,
///   dove l'inizializzazione EGL fallisce con `EGL_BAD_PARAMETER` e un
///   `abort()`. Tornare al backend nativo è il rimedio esatto per quel caso;
/// - **GLX al posto di EGL**: da GTK 3.24.30 il percorso predefinito su X11 è
///   EGL, e dove la libreria EGL impacchettata non va d'accordo con il driver
///   di sistema GLX funziona ancora;
/// - **OpenGL software**: rinuncia all'accelerazione, ma parte ovunque.
fn attempts() -> Vec<Attempt> {
    let mut out = vec![Attempt {
        label: "ambiente di sistema",
        variables: &[],
    }];

    if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty()) {
        out.push(Attempt {
            label: "backend Wayland nativo",
            variables: &[("GDK_BACKEND", "wayland")],
        });
    }

    if std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()) {
        out.push(Attempt {
            label: "backend X11 con GLX",
            variables: &[("GDK_BACKEND", "x11"), ("GDK_DEBUG", "gl-glx")],
        });
    }

    out.push(Attempt {
        label: "OpenGL software",
        variables: &[("LIBGL_ALWAYS_SOFTWARE", "1")],
    });
    out.push(Attempt {
        label: "OpenGL software senza GL in GDK",
        variables: &[("LIBGL_ALWAYS_SOFTWARE", "1"), ("GDK_GL", "disable")],
    });

    out
}

/// Prima configurazione che la sonda accetta.
///
/// Separata dall'esecuzione perché la scelta si possa provare senza aprire
/// nessun display.
fn choose<P>(attempts: &[Attempt], mut probe: P) -> Option<Attempt>
where
    P: FnMut(&Attempt) -> bool,
{
    attempts.iter().copied().find(|attempt| probe(attempt))
}

/// Avvia GTK, cercando una configurazione che regga.
///
/// La prova la fa un processo figlio: un GDK che non riesce a inizializzare
/// EGL non restituisce un errore, chiama `abort()`, e un processo che aborta
/// non lo si può recuperare dall'interno. Il figlio invece muore per conto suo
/// e qui resta solo il suo esito (§D-071).
fn start_gtk() -> Result<(), String> {
    let Ok(executable) = std::env::current_exe() else {
        // Senza il percorso di sé stessi non c'è sonda: si prova diretto, che
        // è quello che succedeva prima.
        return direct_start();
    };

    let candidates = attempts();
    let mut first_failure = String::new();

    let chosen = choose(&candidates, |attempt| {
        match probe_with(&executable, attempt.variables) {
            Ok(Probe { started: true, .. }) => true,
            Ok(Probe { message, .. }) => {
                if first_failure.is_empty() {
                    first_failure = message;
                }
                false
            }
            // La sonda non è partita per motivi suoi: meglio non fidarsi del
            // suo verdetto e provare direttamente.
            Err(_) => true,
        }
    });

    let Some(chosen) = chosen else {
        return Err(failure_message(&first_failure));
    };

    for (name, value) in chosen.variables {
        std::env::set_var(name, value);
    }

    if !chosen.variables.is_empty() {
        tracing::warn!(
            configurazione = chosen.label,
            "GTK avviata con una configurazione di ripiego"
        );
    }

    direct_start()
}

/// Esito di una sonda, con quello che ha scritto morendo.
struct Probe {
    started: bool,
    message: String,
}

/// Quanto si aspetta una sonda prima di considerarla persa.
///
/// Aprire un display richiede millisecondi; un server che non risponde può
/// invece restare lì, e l'avvio del launcher non deve dipenderne.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Esegue la sonda con le variabili indicate.
fn probe_with(executable: &std::path::Path, variables: &[(&str, &str)]) -> std::io::Result<Probe> {
    use std::io::Read;

    let mut command = std::process::Command::new(executable);
    command
        .arg(GTK_PROBE_FLAG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    for (name, value) in variables {
        command.env(name, value);
    }

    let mut child = command.spawn()?;
    let deadline = std::time::Instant::now() + PROBE_TIMEOUT;

    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    };

    let mut message = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut message);
    }

    Ok(Probe {
        started: status.is_some_and(|status| status.success()),
        message: match status {
            Some(_) => message.trim().to_string(),
            None => "la prova di apertura del display non ha risposto entro dieci secondi".into(),
        },
    })
}

/// Inizializza GTK in questo processo.
fn direct_start() -> Result<(), String> {
    if gtk::init().is_ok() {
        Ok(())
    } else {
        Err(failure_message(""))
    }
}

fn failure_message(reason: &str) -> String {
    let reason = if reason.is_empty() {
        String::new()
    } else {
        format!("\n\nQuello che ha detto il sistema:\n{reason}")
    };

    format!("{GTK_MESSAGE}{reason}\n\n{}", environment())
}

/// Fotografia dell'ambiente grafico, da incollare in una segnalazione.
fn environment() -> String {
    let value = |name: &str| std::env::var(name).unwrap_or_else(|_| "(non impostata)".into());

    format!(
        "Ambiente:\n  XDG_SESSION_TYPE = {}\n  DISPLAY          = {}\n  \
         WAYLAND_DISPLAY  = {}\n  GDK_BACKEND      = {}\n  APPIMAGE         = {}",
        value("XDG_SESSION_TYPE"),
        value("DISPLAY"),
        value("WAYLAND_DISPLAY"),
        value("GDK_BACKEND"),
        value("APPIMAGE"),
    )
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

/// Riparte con le impostazioni grafiche più prudenti che WebKitGTK accetta.
///
/// Si chiama solo dopo un avvio che non è arrivato alla finestra. Il percorso
/// DMA-BUF è il primo sospetto: è dove WebKit crea il display EGL, e dove
/// aborta quando lo stack grafico non gli va a genio. Perdere quel percorso
/// costa qualche fotogramma su una pagina che è quasi tutta statica, e vale
/// molto meno di un launcher che non parte (§D-072).
pub fn degrade_graphics() {
    for (name, value) in [
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
    ] {
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, value);
        }
    }

    tracing::warn!("avvio in modalità grafica conservativa dopo un avvio non riuscito");
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
        assert_eq!(
            has_display(),
            ["DISPLAY", "WAYLAND_DISPLAY"]
                .iter()
                .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        );
    }

    #[test]
    fn the_untouched_environment_is_tried_first() {
        let candidates = attempts();
        assert!(candidates[0].variables.is_empty());

        let chosen = choose(&candidates, |_| true).expect("una configurazione");
        assert!(chosen.variables.is_empty());
    }

    /// È il caso della segnalazione: sessione Wayland, AppImage che forza
    /// `GDK_BACKEND=x11`, EGL che aborta sotto XWayland.
    #[test]
    fn on_wayland_the_native_backend_comes_before_the_others() {
        let candidates = vec![
            Attempt {
                label: "ambiente di sistema",
                variables: &[],
            },
            Attempt {
                label: "backend Wayland nativo",
                variables: &[("GDK_BACKEND", "wayland")],
            },
            Attempt {
                label: "OpenGL software",
                variables: &[("LIBGL_ALWAYS_SOFTWARE", "1")],
            },
        ];

        // La prima aborta, la seconda parte: si sceglie la seconda.
        let chosen = choose(&candidates, |attempt| !attempt.variables.is_empty())
            .expect("una configurazione");
        assert_eq!(chosen.label, "backend Wayland nativo");
    }

    #[test]
    fn nothing_is_chosen_when_no_configuration_starts() {
        assert!(choose(&attempts(), |_| false).is_none());
    }

    #[test]
    fn the_failure_message_carries_what_the_system_said() {
        let message = failure_message("Could not create default EGL display: EGL_BAD_PARAMETER.");
        assert!(message.contains("EGL_BAD_PARAMETER"));
        assert!(message.contains("Ambiente:"));

        // Senza motivo non si inventa una sezione vuota.
        assert!(!failure_message("").contains("Quello che ha detto il sistema"));
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
