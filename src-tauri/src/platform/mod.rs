//! Adapter di piattaforma.
//!
//! È l'unico punto del progetto in cui compaiono API specifiche di un sistema
//! operativo. I crate di dominio ricevono i risultati come dati inerti
//! (vedi `docs/decisions.md` §D-001).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(all(unix, not(target_os = "macos")))]
pub use linux::*;

use std::path::Path;

/// `true` se il processo gira da root, con l'utente che non ha chiesto di
/// forzare.
///
/// Serve solo a dirlo all'utente, non a impedire qualcosa: un'applicazione
/// grafica avviata con `sudo` non riesce a parlare con il server grafico, e
/// scriverebbe impostazioni e salvataggi nella cartella di root.
///
/// L'UID effettivo si legge da `/proc/self/status`, che è l'unico modo di
/// saperlo senza `unsafe` (`geteuid` è una chiamata C). Dove `/proc` non c'è —
/// macOS — resta la variabile che `sudo` imposta sempre.
#[cfg(unix)]
pub(crate) fn launched_as_root() -> bool {
    if std::env::var_os("VK_ALLOW_ROOT").is_some() {
        return false;
    }

    // Dove l'UID effettivo si può leggere quella è la risposta esatta: la
    // variabile di `sudo` da sola direbbe di sì anche a `sudo -u altroutente`,
    // che di root non ha niente.
    match effective_uid() {
        Some(uid) => uid == 0,
        None => std::env::var_os("SUDO_USER").is_some(),
    }
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;

    // `Uid:	<reale>	<effettivo>	<salvato>	<filesystem>`
    status
        .lines()
        .find(|line| line.starts_with("Uid:"))?
        .split_whitespace()
        .nth(2)?
        .parse()
        .ok()
}

/// Nome della piattaforma, mostrato nella pagina Debug.
pub const fn platform_name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// `true` se un eseguibile con quel percorso è in esecuzione.
///
/// Equivalente di `MainWindow.xaml.cs::IsExecutableRunning`: serve a impedire
/// l'avvio quando Dolphin è già aperto, perché non rileggerebbe i binding.
///
/// Il confronto è **sul percorso**, non sul nome: due programmi possono
/// chiamarsi allo stesso modo, e su Linux il nome che il sistema espone è
/// troncato (vedi [`matches_process_name`]). Il nome resta come ripiego per i
/// processi di cui non si riesce a leggere il percorso.
pub fn is_executable_running(executable: &Path) -> bool {
    let Some(file_name) = executable
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return false;
    };
    if file_name.trim().is_empty() {
        return false;
    }

    let stem = executable
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.clone());
    let target = std::fs::canonicalize(executable).ok();

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    system.processes().values().any(|process| {
        if let (Some(target), Some(path)) = (target.as_deref(), process.exe()) {
            // `starts_with` copre i bundle di macOS, dove il processo vive in
            // `Contents/MacOS/` dentro l'applicazione che si sta cercando.
            let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if resolved == target || resolved.starts_with(target) {
                return true;
            }
        }

        let name = process.name().to_string_lossy().to_string();
        matches_process_name(&name, &file_name) || matches_process_name(&name, &stem)
    })
}

/// Confronta il nome di un processo con quello atteso.
///
/// Su Linux il nome arriva da `/proc/<pid>/comm`, che il kernel **tronca a 15
/// caratteri**: `vanzakart-launcher.AppImage` si presenta come
/// `vanzakart-launc`, e un confronto per intero non lo riconoscerebbe mai.
fn matches_process_name(process_name: &str, expected: &str) -> bool {
    /// Lunghezza di `/proc/<pid>/comm`, senza il terminatore.
    const COMM_LIMIT: usize = 15;

    if process_name.eq_ignore_ascii_case(expected) {
        return true;
    }

    process_name.len() == COMM_LIMIT
        && expected.len() > COMM_LIMIT
        && expected.is_char_boundary(COMM_LIMIT)
        && expected[..COMM_LIMIT].eq_ignore_ascii_case(process_name)
}

/// `true` se il file può essere eseguito.
///
/// Su Unix un AppImage o un binario appena scaricato arriva spesso senza il
/// bit di esecuzione, e l'unico segnale sarebbe un "Permission denied" al
/// momento dell'avvio: meglio dirlo prima, con il comando da dare (§D-068).
/// Su Windows il concetto non esiste e basta che il file ci sia.
pub fn is_executable_file(path: &Path) -> bool {
    !path.as_os_str().is_empty() && has_execute_permission(path)
}

/// Il bit di esecuzione, per chiunque lo abbia.
///
/// Un bundle `.app` di macOS è una directory: eseguibile è ciò che sta dentro,
/// e il chiamante risolve il percorso prima di chiedere. La directory di per
/// sé passa, così un percorso risolto male non blocca l'avvio da solo.
#[cfg(unix)]
fn has_execute_permission(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata.permissions().mode() & 0o111 != 0,
        Ok(metadata) => metadata.is_dir(),
        Err(_) => false,
    }
}

/// Su Windows il permesso di esecuzione non esiste: basta che il file ci sia.
#[cfg(not(unix))]
fn has_execute_permission(path: &Path) -> bool {
    path.is_file()
}

// ---------------------------------------------------------------------------
// Ambiente dei processi figli
// ---------------------------------------------------------------------------

/// Toglie da un elenco di percorsi quelli che stanno dentro l'AppImage.
///
/// `None` quando non resta niente: la variabile va rimossa, non svuotata.
///
/// Serve solo dentro un AppImage, cioè solo su Linux, ma resta compilata
/// ovunque perché è la parte che si può provare con un test.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
fn without_appdir(value: &str, appdir: &str) -> Option<String> {
    let rimasti: Vec<&str> = value
        .split(':')
        .filter(|voce| !voce.is_empty() && !voce.starts_with(appdir))
        .collect();

    (!rimasti.is_empty()).then(|| rimasti.join(":"))
}

/// Ripulisce l'ambiente di un processo figlio dalle variabili dell'AppImage.
///
/// Il launcher, dentro un AppImage, gira con `LD_LIBRARY_PATH` che punta alle
/// librerie impacchettate: sono di Ubuntu 22.04 e servono a lui. Un programma
/// di sistema avviato da qui le eredita e ci muore sopra —
///
/// ```text
/// /usr/bin/dolphin: /tmp/.mount_xxx/usr/lib/liblzma.so.5: version `XZ_5.4'
/// not found (required by /usr/lib/libKF6Archive.so.6)
/// ```
///
/// — perché la sua `liblzma` di sistema è più nuova di quella nell'AppImage
/// (§D-074). Fuori da un AppImage non c'è niente da ripulire.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn clean_child_environment(command: &mut std::process::Command) {
    /// Variabili che valgono elenchi di percorsi: si ripuliscono, non si
    /// tolgono, perché contengono anche percorsi di sistema che servono.
    const PATH_LISTS: &[&str] = &[
        "LD_LIBRARY_PATH",
        "XDG_DATA_DIRS",
        "GTK_PATH",
        "GDK_PIXBUF_MODULEDIR",
        "GIO_EXTRA_MODULES",
        "QT_PLUGIN_PATH",
        "PYTHONPATH",
        "PERLLIB",
    ];

    /// Variabili che l'AppImage imposta per sé: un figlio non deve vederle.
    const OWN_VARIABLES: &[&str] = &[
        "APPDIR",
        "APPIMAGE",
        "ARGV0",
        "OWD",
        "GTK_EXE_PREFIX",
        "GTK_DATA_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
        "GSETTINGS_SCHEMA_DIR",
        "GTK_THEME",
        "GDK_BACKEND",
        "LD_PRELOAD",
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ];

    let Ok(appdir) = std::env::var("APPDIR") else {
        return;
    };
    if appdir.is_empty() {
        return;
    }

    for name in PATH_LISTS {
        let Ok(value) = std::env::var(name) else {
            continue;
        };

        match without_appdir(&value, &appdir) {
            Some(rimasto) => command.env(name, rimasto),
            None => command.env_remove(name),
        };
    }

    for name in OWN_VARIABLES {
        command.env_remove(name);
    }
}

/// Fuori da Linux nessuno inietta niente nell'ambiente.
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn clean_child_environment(_command: &mut std::process::Command) {}

/// Apre un percorso o un URL con il gestore di sistema, con l'ambiente pulito.
///
/// `false` quando non se ne occupa: chi chiama usa il plugin di Tauri, che è
/// la strada giusta ovunque tranne che dentro un AppImage, dove `xdg-open`
/// erediterebbe le librerie impacchettate (§D-074).
#[cfg(all(unix, not(target_os = "macos")))]
pub fn open_with_system_handler(target: &str) -> bool {
    if std::env::var_os("APPDIR").is_none() {
        return false;
    }

    let mut command = std::process::Command::new("xdg-open");
    command.arg(target);
    clean_child_environment(&mut command);

    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Altrove se ne occupa il plugin.
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn open_with_system_handler(_target: &str) -> bool {
    false
}

/// Radici in cui cercare installazioni portable di Dolphin.
pub fn dolphin_search_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Downloads"));
        roots.push(home.join("Desktop"));
    }
    if let Some(desktop) = dirs::desktop_dir() {
        roots.push(desktop);
    }
    roots.extend(extra_search_roots());

    roots.retain(|path| path.is_dir());
    roots.sort();
    roots.dedup();
    roots
}

/// Byte casuali dal generatore del sistema operativo.
///
/// È l'unica sorgente di entropia del progetto: i crate di dominio restano
/// deterministici e ricevono i valori casuali come argomenti (per esempio
/// `vk_save::mii::generate_mii_id`).
///
/// Se il generatore del sistema non risponde si ripiega sull'orologio ad alta
/// risoluzione: non è entropia crittografica, ma qui serve solo a non far
/// collidere due Mii creati sulla stessa macchina, e restituire zeri
/// garantirebbe la collisione.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    if getrandom::fill(&mut buffer).is_ok() {
        return buffer;
    }

    tracing::warn!("generatore di numeri casuali del sistema non disponibile");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = ((nanos >> ((index % 16) * 8)) & 0xFF) as u8;
    }
    buffer
}

/// Dati d'ambiente per la risoluzione dei percorsi di Dolphin.
pub fn path_probe() -> vk_dolphin::paths::PathProbe {
    vk_dolphin::paths::PathProbe {
        home: dirs::home_dir(),
        documents: dirs::document_dir(),
        app_data: dirs::config_dir(),
        local_app_data: dirs::data_local_dir(),
        search_roots: dolphin_search_roots(),
        registry_user_path: dolphin_registry_user_path(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn forcing_root_is_always_possible() {
        // La variabile è la via d'uscita documentata nel messaggio d'errore.
        std::env::set_var("VK_ALLOW_ROOT", "1");
        assert!(!launched_as_root());
        std::env::remove_var("VK_ALLOW_ROOT");
    }

    #[test]
    fn the_appimage_paths_are_stripped_and_the_others_kept() {
        let appdir = "/tmp/.mount_VanzaK";

        assert_eq!(
            without_appdir("/tmp/.mount_VanzaK/usr/lib:/usr/lib", appdir),
            Some("/usr/lib".to_string())
        );
        assert_eq!(
            without_appdir(
                "/tmp/.mount_VanzaK/usr/share:/usr/share:/usr/local/share",
                appdir
            ),
            Some("/usr/share:/usr/local/share".to_string())
        );

        // Solo percorsi dell'AppImage: la variabile va tolta, non svuotata.
        assert_eq!(without_appdir("/tmp/.mount_VanzaK/usr/lib", appdir), None);
        assert_eq!(without_appdir("", appdir), None);

        // Un percorso che comincia allo stesso modo ma non è dentro l'AppImage
        // non c'entra niente.
        assert_eq!(
            without_appdir("/usr/lib:/opt/altro", appdir),
            Some("/usr/lib:/opt/altro".to_string())
        );
    }

    #[test]
    fn cleaning_a_command_outside_an_appimage_changes_nothing() {
        // La suite non gira dentro un AppImage: la funzione non deve toccare
        // niente né andare in panico.
        let mut command = std::process::Command::new("echo");
        clean_child_environment(&mut command);
        assert_eq!(command.get_program(), "echo");
    }

    #[test]
    fn the_platform_is_named() {
        assert!(["Windows", "macOS", "Linux"].contains(&platform_name()));
    }

    #[test]
    fn a_missing_file_cannot_be_executed() {
        assert!(!is_executable_file(Path::new("")));
        assert!(!is_executable_file(Path::new(
            "/percorso/inesistente/QuestoNonEsisteDavvero"
        )));
    }

    #[test]
    fn the_current_executable_can_be_executed() {
        let current = std::env::current_exe().expect("eseguibile corrente");
        assert!(is_executable_file(&current));
    }

    #[test]
    fn a_missing_executable_is_not_running() {
        assert!(!is_executable_running(Path::new("")));
        assert!(!is_executable_running(Path::new(
            "/percorso/inesistente/QuestoNonEsisteDavvero.exe"
        )));
    }

    #[test]
    fn a_truncated_process_name_is_still_recognised() {
        // È il caso di Linux: il kernel tiene 15 caratteri di `comm`.
        assert!(matches_process_name(
            "vanzakart-launc",
            "vanzakart-launcher.AppImage"
        ));
        assert!(matches_process_name("Dolphin.exe", "dolphin.exe"));
        // Un nome corto troncato non esiste: niente falsi positivi.
        assert!(!matches_process_name("vanzakart-launc", "vanzakart-lau"));
        assert!(!matches_process_name(
            "altro-programma",
            "vanzakart-launcher.AppImage"
        ));
    }

    #[test]
    fn the_current_process_is_detected_as_running() {
        let current = std::env::current_exe().expect("eseguibile corrente");
        assert!(is_executable_running(&current));
    }

    #[test]
    fn every_search_root_exists() {
        assert!(dolphin_search_roots().iter().all(|path| path.is_dir()));
    }

    #[test]
    fn random_bytes_are_not_all_zero() {
        // Due chiamate consecutive che coincidono segnalerebbero un
        // generatore fermo: con 16 byte la collisione è impossibile in pratica.
        let first = random_bytes::<16>();
        let second = random_bytes::<16>();
        assert_ne!(first, [0u8; 16]);
        assert_ne!(first, second);
    }

    #[test]
    fn the_probe_is_populated() {
        let probe = path_probe();
        assert!(probe.home.is_some() || probe.local_app_data.is_some());
    }
}
