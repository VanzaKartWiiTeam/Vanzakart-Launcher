//! Integrazione con Windows: collegamenti `.lnk`, chiave di disinstallazione,
//! rimozione differita della cartella.

use std::path::{Path, PathBuf};
use std::process::Command;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use super::{ShortcutRequest, UninstallRegistration, MENU_FOLDER_NAME, SHORTCUT_DESCRIPTION};
use crate::error::{InstallError, InstallResult};
use crate::record::{Artifact, ArtifactKind};

const UNINSTALL_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const APP_PATHS_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";

/// Chiave scritta dal setup del launcher legacy in C#.
const LEGACY_UNINSTALL_KEY: &str = "VanzaKartLauncher";

/// Nessuna finestra di console per i processi ausiliari.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Nome del collegamento, uguale a quello del launcher legacy: chi aggiorna
/// non si ritrova due icone diverse sul desktop.
const SHORTCUT_FILE_NAME: &str = "VanzaKart Launcher.lnk";
const UNINSTALL_SHORTCUT_FILE_NAME: &str = "Uninstall VanzaKart Launcher.lnk";

pub fn create_shortcuts(request: &ShortcutRequest) -> Vec<Artifact> {
    let mut artifacts = Vec::new();

    // Per tutto il PC: il desktop pubblico e il menu Start comune, che ogni
    // utente vede. Altrimenti quelli di chi installa — che, su un account
    // standard, è l'amministratore che ha digitato la password (§D-090).
    let (desktop_dir, programs_dir) = if request.machine_wide {
        (public_desktop(), common_start_menu_programs())
    } else {
        (dirs::desktop_dir(), start_menu_programs())
    };

    if request.desktop {
        if let Some(desktop) = desktop_dir {
            let link = desktop.join(SHORTCUT_FILE_NAME);
            if write_shortcut(&link, request.executable, request.working_dir, "").is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::DesktopShortcut, &link));
            }
        }
    }

    if request.start_menu {
        if let Some(programs) = programs_dir {
            let folder = programs.join(MENU_FOLDER_NAME);
            let link = folder.join(SHORTCUT_FILE_NAME);
            if write_shortcut(&link, request.executable, request.working_dir, "").is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::StartMenuShortcut, &link));
            }

            if request.uninstall_entry {
                if let Some(uninstaller) = request.uninstaller {
                    let link = folder.join(UNINSTALL_SHORTCUT_FILE_NAME);
                    if write_shortcut(&link, uninstaller, request.working_dir, "--uninstall")
                        .is_ok()
                    {
                        artifacts.push(Artifact::file(ArtifactKind::UninstallShortcut, &link));
                    }
                }
            }
        }
    }

    // L'avvio veloce è per utente per natura: in un'installazione per tutto
    // il PC finirebbe solo nel profilo di chi installa.
    if request.quick_launch && !request.machine_wide {
        if let Some(app_data) = dirs::config_dir() {
            let link = app_data
                .join("Microsoft")
                .join("Internet Explorer")
                .join("Quick Launch")
                .join(SHORTCUT_FILE_NAME);
            if write_shortcut(&link, request.executable, request.working_dir, "").is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::QuickLaunchShortcut, &link));
            }
        }
    }

    artifacts
}

pub fn remove_artifact(artifact: &Artifact) -> bool {
    match artifact.kind {
        ArtifactKind::RegistryKey => delete_key_tree(&artifact.path),
        ArtifactKind::Icon | ArtifactKind::Symlink => {
            crate::fsops::remove_path_best_effort(Path::new(&artifact.path))
        }
        _ => {
            let path = PathBuf::from(&artifact.path);
            let removed = crate::fsops::remove_path_best_effort(&path);
            super::remove_parent_if_empty(&path);
            removed
        }
    }
}

/// Radice del registro per un'installazione, con il prefisso con cui le sue
/// chiavi finiscono nel registro dell'installazione.
fn hive(machine_wide: bool) -> (RegKey, &'static str) {
    if machine_wide {
        (RegKey::predef(HKEY_LOCAL_MACHINE), "HKLM")
    } else {
        (RegKey::predef(HKEY_CURRENT_USER), "HKCU")
    }
}

/// Scrive la chiave che fa comparire il launcher in "App e funzionalità".
///
/// Il nome della chiave è l'identificatore del bundle, lo stesso che userebbe
/// l'installer NSIS di Tauri: così l'aggiornamento automatico riconosce
/// *questa* installazione invece di affiancargliene una seconda (§D-052).
/// Per tutto il PC sta in HKLM, dove la vede ogni utente (§D-090).
pub fn register_uninstall(registration: &UninstallRegistration) -> InstallResult<Vec<Artifact>> {
    let (root, prefix) = hive(registration.machine_wide);
    let key_path = format!(r"{UNINSTALL_ROOT}\{}", crate::BUNDLE_IDENTIFIER);
    let (key, _) = root
        .create_subkey(&key_path)
        .map_err(|error| InstallError::platform(format!("chiave di disinstallazione: {error}")))?;

    let uninstaller = registration
        .uninstaller
        .unwrap_or(registration.executable)
        .to_string_lossy()
        .to_string();
    let command = format!("\"{uninstaller}\" --uninstall");

    let write = |name: &str, value: &str| -> InstallResult<()> {
        key.set_value(name, &value.to_string())
            .map_err(|error| InstallError::platform(format!("{name}: {error}")))
    };

    write("DisplayName", crate::PRODUCT_NAME)?;
    write("DisplayVersion", registration.version.trim())?;
    write("Publisher", crate::PUBLISHER)?;
    write(
        "InstallLocation",
        &registration.install_dir.to_string_lossy(),
    )?;
    write("DisplayIcon", &registration.executable.to_string_lossy())?;
    write("UninstallString", &command)?;
    write("QuietUninstallString", &format!("{command} --quiet"))?;
    write("URLInfoAbout", "https://vwfc.vanzakart.net/")?;
    write("InstallDate", &install_date())?;

    let size_kb = u32::try_from(registration.size_bytes / 1024)
        .unwrap_or(u32::MAX)
        .max(1);
    for (name, value) in [
        ("NoModify", 1u32),
        ("NoRepair", 1),
        ("EstimatedSize", size_kb),
    ] {
        key.set_value(name, &value)
            .map_err(|error| InstallError::platform(format!("{name}: {error}")))?;
    }

    // Le installazioni fatte prima del rinominamento hanno la loro chiave:
    // lasciarla vorrebbe dire due voci per un programma solo (§D-083).
    for previous in crate::PREVIOUS_BUNDLE_IDENTIFIERS {
        if *previous != crate::BUNDLE_IDENTIFIER {
            delete_key_tree(&format!(r"{prefix}\{UNINSTALL_ROOT}\{previous}"));
        }
    }

    let mut artifacts = vec![Artifact::new(
        ArtifactKind::RegistryKey,
        format!(r"{prefix}\{key_path}"),
    )];

    // `App Paths`: fa funzionare "Esegui → vanzakart launcher".
    if let Some(file_name) = registration
        .executable
        .file_name()
        .and_then(|name| name.to_str())
    {
        let app_path_key = format!(r"{APP_PATHS_ROOT}\{file_name}");
        if let Ok((key, _)) = root.create_subkey(&app_path_key) {
            let _ = key.set_value("", &registration.executable.to_string_lossy().to_string());
            let _ = key.set_value(
                "Path",
                &registration.install_dir.to_string_lossy().to_string(),
            );
            artifacts.push(Artifact::new(
                ArtifactKind::RegistryKey,
                format!(r"{prefix}\{app_path_key}"),
            ));
        }
    }

    Ok(artifacts)
}

/// Cartella d'installazione registrata da **questo** installer o dal pacchetto
/// NSIS, se c'è.
///
/// Non guarda la chiave del setup legacy in C#: quella descrive un altro
/// programma, e chi la interroga per sapere "cosa devo disinstallare" avrebbe
/// come risposta il launcher vecchio. Per proporre una cartella esiste
/// [`legacy_install_dir`], che il disinstallatore non chiama mai.
pub fn registered_install_dir() -> Option<PathBuf> {
    // Prima l'utente, poi il PC: un'installazione per utente fatta fino alla
    // 2.1 resta quella che l'installer propone di aggiornare.
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|root| uninstall_key_names().find_map(|name| read_install_location(root, &name)))
}

/// I nomi sotto cui la chiave di disinstallazione può stare, dal più recente.
fn uninstall_key_names() -> impl Iterator<Item = String> {
    std::iter::once(crate::BUNDLE_IDENTIFIER.to_string()).chain(
        crate::PREVIOUS_BUNDLE_IDENTIFIERS
            .iter()
            .map(|k| (*k).to_string()),
    )
}

/// Cartella del launcher legacy in C#, dalla chiave che scriveva il suo setup.
///
/// Serve solo all'installer, per proporre la cartella che l'utente sta già
/// usando. Non è un'installazione nostra e non si rimuove.
pub fn legacy_install_dir() -> Option<PathBuf> {
    read_install_location(HKEY_CURRENT_USER, LEGACY_UNINSTALL_KEY)
}

fn read_install_location(root: winreg::HKEY, key_name: &str) -> Option<PathBuf> {
    let location = RegKey::predef(root)
        .open_subkey_with_flags(format!(r"{UNINSTALL_ROOT}\{key_name}"), KEY_READ)
        .ok()?
        .get_value::<String, _>("InstallLocation")
        .ok()?;
    let location = PathBuf::from(location.trim());
    location.is_dir().then_some(location)
}

/// Versione registrata dall'installazione corrente, se c'è.
pub fn registered_version() -> Option<String> {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|root| {
            uninstall_key_names().find_map(|name| {
                RegKey::predef(root)
                    .open_subkey_with_flags(format!(r"{UNINSTALL_ROOT}\{name}"), KEY_READ)
                    .ok()?
                    .get_value::<String, _>("DisplayVersion")
                    .ok()
            })
        })
}

/// Toglie la registrazione di **questa** installazione.
///
/// La chiave del launcher legacy resta dov'è anche quando la si è trovata:
/// descrive un programma che non abbiamo installato noi e che può essere
/// ancora sul disco. Cancellarla farebbe sparire il launcher vecchio da "App e
/// funzionalità" lasciandolo installato.
pub fn unregister_uninstall(executable_name: Option<&str>) -> bool {
    let mut removed = false;
    // Entrambe le radici: la chiave in HKLM la può togliere solo un
    // disinstallatore con i permessi di amministratore, che è quello che
    // rimuove un'installazione per tutto il PC.
    for prefix in ["HKCU", "HKLM"] {
        for name in uninstall_key_names() {
            removed |= delete_key_tree(&format!(r"{prefix}\{UNINSTALL_ROOT}\{name}"));
        }
        if let Some(name) = executable_name {
            removed |= delete_key_tree(&format!(r"{prefix}\{APP_PATHS_ROOT}\{name}"));
        }
    }
    removed
}

/// Cancella una chiave creata dall'installer.
///
/// Solo sotto `Uninstall` e `App Paths`, le due radici in cui l'installer
/// scrive: il disinstallatore di un'installazione per tutto il PC gira come
/// amministratore, e un registro manomesso non deve potergli far cancellare
/// qualunque altra chiave del sistema (§D-091).
fn delete_key_tree(qualified: &str) -> bool {
    let (root, path) = if let Some(path) = qualified.strip_prefix(r"HKCU\") {
        (HKEY_CURRENT_USER, path)
    } else if let Some(path) = qualified.strip_prefix(r"HKLM\") {
        (HKEY_LOCAL_MACHINE, path)
    } else {
        return false;
    };

    let lower = path.to_ascii_lowercase();
    let ours = !lower.contains("..")
        && [UNINSTALL_ROOT, APP_PATHS_ROOT].iter().any(|allowed| {
            let allowed = format!(r"{}\", allowed.to_ascii_lowercase());
            lower.starts_with(&allowed) && lower.len() > allowed.len()
        });
    if !ours {
        return false;
    }

    RegKey::predef(root).delete_subkey_all(path).is_ok()
}

/// Avvia il launcher e lascia che l'installer si chiuda.
///
/// L'installer gira come amministratore, e un processo avviato da lui lo
/// sarebbe a sua volta: il launcher avvierebbe Dolphin da amministratore, non
/// accetterebbe gli archivi trascinati da Esplora risorse e creerebbe la
/// cartella di WebView2 con permessi che gli avvii normali poi non hanno. Lo
/// si fa quindi avviare da Esplora risorse, che gira come l'utente del
/// desktop (§D-090).
pub fn launch_detached(executable: &Path) -> InstallResult<()> {
    let explorer = windows_dir().join("explorer.exe");
    if explorer.is_file() && Command::new(&explorer).arg(executable).spawn().is_ok() {
        return Ok(());
    }

    let working_dir = executable.parent().unwrap_or(Path::new("."));
    Command::new(executable)
        .current_dir(working_dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::io(executable, error))
}

/// Cartella di Windows, per chiamare i programmi di sistema per percorso.
///
/// Mai per nome: un processo avviato per nome viene cercato prima nella
/// cartella dell'eseguibile che lo chiama, e l'installer — che gira come
/// amministratore — sta di solito in Download, dove chiunque può mettere un
/// `cmd.exe` o un `icacls.exe` finto (§D-091).
fn windows_dir() -> PathBuf {
    ["SystemRoot", "windir"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_absolute() && path.join("System32").is_dir())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// Un programma di `System32`, per percorso.
fn system_tool(name: &str) -> PathBuf {
    windows_dir().join("System32").join(name)
}

/// SID che non dipendono dalla lingua di Windows: "Users" in italiano è
/// "Utenti", e un nome tradotto farebbe fallire `icacls`.
const SID_SYSTEM: &str = "*S-1-5-18";
const SID_ADMINISTRATORS: &str = "*S-1-5-32-544";
const SID_USERS: &str = "*S-1-5-32-545";

/// Permessi di un'installazione per tutto il PC (§D-091).
///
/// Prima si chiude la sottocartella del disinstallatore: solo SYSTEM e gli
/// amministratori la modificano, gli utenti la leggono ed eseguono. Poi si dà
/// agli utenti il permesso di modifica sul resto della cartella, che è ciò che
/// permette al launcher di aggiornarsi da sé senza chiedere la password, come
/// fa Steam. L'ordine conta: al contrario, per un istante anche la
/// sottocartella sarebbe stata modificabile da tutti.
pub fn secure_machine_install(install_dir: &Path, protected_dir: &Path) -> InstallResult<()> {
    if protected_dir != install_dir && protected_dir.is_dir() {
        run_icacls(
            protected_dir,
            &[
                "/inheritance:r",
                "/grant:r",
                &format!("{SID_SYSTEM}:(OI)(CI)F"),
                &format!("{SID_ADMINISTRATORS}:(OI)(CI)F"),
                &format!("{SID_USERS}:(OI)(CI)RX"),
                "/T",
                "/C",
                "/Q",
            ],
        )?;
    }

    run_icacls(
        install_dir,
        &["/grant", &format!("{SID_USERS}:(OI)(CI)M"), "/C", "/Q"],
    )
}

fn run_icacls(target: &Path, arguments: &[&str]) -> InstallResult<()> {
    use std::os::windows::process::CommandExt;

    let status = Command::new(system_tool("icacls.exe"))
        .arg(target)
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| InstallError::io(target, error))?;

    if status.success() {
        Ok(())
    } else {
        Err(InstallError::platform(format!(
            "permissions not applied to {} ({status})",
            target.display()
        )))
    }
}

/// Cancella i percorsi dopo l'uscita del processo.
///
/// Su Windows l'eseguibile in esecuzione è bloccato: il disinstallatore non
/// può togliere la cartella in cui si trova. Lascia quindi uno script che
/// aspetta la sua uscita e poi cancella. È la stessa tecnica del
/// disinstallatore legacy, con in più l'attesa in un ciclo invece di un
/// `timeout` a occhio.
pub fn schedule_removal(paths: &[PathBuf]) -> InstallResult<bool> {
    let removable: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|path| !path.is_empty())
        .collect();
    if removable.is_empty() {
        return Ok(false);
    }

    // `%` e `"` renderebbero lo script ambiguo: meglio dirlo all'utente che
    // cancellare la cartella sbagliata.
    if removable
        .iter()
        .any(|path| path.contains('%') || path.contains('"'))
    {
        return Err(InstallError::platform(
            "the path contains characters that cannot be passed to a removal script",
        ));
    }

    let self_path = std::env::current_exe()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut script = String::from("@echo off\r\nsetlocal\r\nset /a tries=0\r\n:wait\r\n");
    script.push_str("set /a tries+=1\r\n");
    if !self_path.is_empty() {
        script.push_str(&format!("del /f /q \"{self_path}\" >nul 2>&1\r\n"));
        script.push_str(&format!(
            "if exist \"{self_path}\" if %tries% lss 40 (ping -n 2 127.0.0.1 >nul & goto wait)\r\n"
        ));
    }
    for path in &removable {
        script.push_str(&format!("rd /s /q \"{path}\" >nul 2>&1\r\n"));
        script.push_str(&format!("del /f /q \"{path}\" >nul 2>&1\r\n"));
    }
    script.push_str("del /f /q \"%~f0\" >nul 2>&1\r\n");

    let script_path =
        std::env::temp_dir().join(format!("vanzakart_uninstall_{}.cmd", std::process::id()));
    std::fs::write(&script_path, script).map_err(|error| InstallError::io(&script_path, error))?;

    use std::os::windows::process::CommandExt;
    Command::new(system_tool("cmd.exe"))
        .arg("/c")
        .arg(&script_path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| InstallError::io(&script_path, error))?;

    Ok(true)
}

fn start_menu_programs() -> Option<PathBuf> {
    dirs::data_dir().map(|roaming| {
        roaming
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
    })
}

/// Il menu Start di tutti gli utenti.
fn common_start_menu_programs() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|data| {
            data.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
}

/// Il desktop pubblico, che compare sul desktop di ogni utente.
fn public_desktop() -> Option<PathBuf> {
    std::env::var_os("PUBLIC")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|public| public.join("Desktop"))
}

/// Crea un `.lnk`.
///
/// Windows non ha un'API per farlo che non passi da COM, e COM richiede
/// `unsafe`: il crate lo vieta (§D-051). Si usa quindi lo Windows Script Host,
/// come faceva `ShortcutService` del setup legacy, con PowerShell come
/// ripiego per le macchine in cui `wscript.exe` è disattivato.
fn write_shortcut(
    link: &Path,
    target: &Path,
    working_dir: &Path,
    arguments: &str,
) -> InstallResult<()> {
    if let Some(parent) = link.parent() {
        crate::fsops::ensure_dir(parent)?;
    }

    let link_text = quotable(link)?;
    let target_text = quotable(target)?;
    let working_text = quotable(working_dir)?;
    if arguments.contains('"') {
        return Err(InstallError::platform("invalid arguments"));
    }

    crate::fsops::remove_path_best_effort(link);

    if run_script_host(&link_text, &target_text, &working_text, arguments).is_ok() && link.is_file()
    {
        return Ok(());
    }
    run_powershell(&link_text, &target_text, &working_text, arguments)?;

    if link.is_file() {
        Ok(())
    } else {
        Err(InstallError::platform(format!(
            "shortcut not created: {}",
            link.display()
        )))
    }
}

fn run_script_host(
    link: &str,
    target: &str,
    working_dir: &str,
    arguments: &str,
) -> InstallResult<()> {
    let script = format!(
        "Set shell = CreateObject(\"WScript.Shell\")\r\n\
         Set lnk = shell.CreateShortcut(\"{link}\")\r\n\
         lnk.TargetPath = \"{target}\"\r\n\
         lnk.Arguments = \"{arguments}\"\r\n\
         lnk.WorkingDirectory = \"{working_dir}\"\r\n\
         lnk.IconLocation = \"{target},0\"\r\n\
         lnk.Description = \"{SHORTCUT_DESCRIPTION}\"\r\n\
         lnk.Save\r\n"
    );
    run_temp_script("vbs", &script, |path| {
        let mut command = Command::new(system_tool("wscript.exe"));
        command.arg("//B").arg("//Nologo").arg(path);
        command
    })
}

fn run_powershell(
    link: &str,
    target: &str,
    working_dir: &str,
    arguments: &str,
) -> InstallResult<()> {
    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell\r\n\
         $lnk = $shell.CreateShortcut('{link}')\r\n\
         $lnk.TargetPath = '{target}'\r\n\
         $lnk.Arguments = '{arguments}'\r\n\
         $lnk.WorkingDirectory = '{working_dir}'\r\n\
         $lnk.IconLocation = '{target},0'\r\n\
         $lnk.Description = '{SHORTCUT_DESCRIPTION}'\r\n\
         $lnk.Save()\r\n"
    );
    run_temp_script("ps1", &script, |path| {
        let mut command = Command::new(
            system_tool("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        );
        command
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(path);
        command
    })
}

fn run_temp_script(
    extension: &str,
    body: &str,
    build: impl Fn(&Path) -> Command,
) -> InstallResult<()> {
    use std::os::windows::process::CommandExt;

    let path = std::env::temp_dir().join(format!(
        "vanzakart_shortcut_{}.{extension}",
        std::process::id()
    ));
    std::fs::write(&path, body).map_err(|error| InstallError::io(&path, error))?;

    let status = build(&path)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| InstallError::io(&path, error));
    crate::fsops::remove_path_best_effort(&path);

    match status? {
        status if status.success() => Ok(()),
        status => Err(InstallError::platform(format!(
            "the shortcut script exited with {status}"
        ))),
    }
}

/// I percorsi di Windows non possono contenere virgolette: se ce ne sono,
/// qualcosa non torna e non si prosegue.
fn quotable(path: &Path) -> InstallResult<String> {
    let text = path.to_string_lossy().to_string();
    if text.contains('"') || text.contains('\'') || text.contains('\n') || text.contains('\r') {
        return Err(InstallError::platform(format!(
            "path not usable in a shortcut: {text}"
        )));
    }
    Ok(text)
}

fn install_date() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_with_a_quote_is_refused() {
        assert!(quotable(Path::new("C:\\a\"b")).is_err());
        assert!(quotable(Path::new("C:\\Program Files\\VanzaKart")).is_ok());
    }

    #[test]
    fn the_install_date_has_the_shape_windows_expects() {
        let date = install_date();
        assert_eq!(date.len(), 8, "{date}");
        assert!(date.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn registry_lookups_never_panic_when_nothing_is_installed() {
        let _ = registered_install_dir();
        let _ = registered_version();
        let _ = legacy_install_dir();
    }

    #[test]
    fn the_legacy_key_is_read_from_a_different_place() {
        // Le due funzioni non devono mai leggere la stessa chiave: se lo
        // facessero, disinstallare il launcher nuovo porterebbe via la
        // registrazione di quello vecchio (§D-055).
        assert_ne!(crate::BUNDLE_IDENTIFIER, LEGACY_UNINSTALL_KEY);
        assert!(!crate::PREVIOUS_BUNDLE_IDENTIFIERS.contains(&LEGACY_UNINSTALL_KEY));
    }

    #[test]
    fn the_current_key_is_tried_before_the_ones_it_replaced() {
        let names: Vec<String> = uninstall_key_names().collect();
        assert_eq!(
            names.first().map(String::as_str),
            Some(crate::BUNDLE_IDENTIFIER)
        );
        assert_eq!(names.len(), 1 + crate::PREVIOUS_BUNDLE_IDENTIFIERS.len());
    }

    #[test]
    fn removing_a_key_that_is_not_ours_is_refused() {
        assert!(!delete_key_tree(r"HKLM\Software\Qualcosa"));
        assert!(!delete_key_tree(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
        ));
        assert!(!delete_key_tree(r"HKLM\SYSTEM\CurrentControlSet"));
        // La radice stessa non è una chiave nostra: è di tutti i programmi.
        assert!(!delete_key_tree(&format!(r"HKLM\{UNINSTALL_ROOT}")));
        assert!(!delete_key_tree(&format!(r"HKCU\{UNINSTALL_ROOT}\..\Run")));
        assert!(!delete_key_tree(r"HKCR\qualcosa"));
    }

    #[test]
    fn system_tools_are_called_by_path_not_by_name() {
        for tool in ["icacls.exe", "cmd.exe", "wscript.exe"] {
            let path = system_tool(tool);
            assert!(path.is_absolute(), "{path:?}");
            assert!(path.is_file(), "{path:?}");
        }
        assert!(windows_dir().join("explorer.exe").is_file());
    }

    #[test]
    fn machine_wide_shortcuts_go_where_every_user_sees_them() {
        let desktop = public_desktop().expect("desktop pubblico");
        assert!(desktop.ends_with("Desktop"));
        assert_ne!(Some(desktop), dirs::desktop_dir());
        let programs = common_start_menu_programs().expect("menu Start comune");
        assert_ne!(Some(programs), start_menu_programs());
    }

    /// Ciò che conta davvero dei permessi: nella sottocartella protetta un
    /// utente normale non scrive, nel resto sì. Si prova su una cartella
    /// temporanea, di cui chi esegue i test è proprietario e su cui quindi
    /// può cambiare i permessi senza essere amministratore.
    #[test]
    fn the_uninstaller_folder_is_closed_and_the_rest_is_open() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("VanzaKart Launcher");
        let protected = install.join(crate::paths::PROTECTED_DIR_NAME);
        std::fs::create_dir_all(&protected).expect("cartelle");
        std::fs::write(protected.join("VanzaKart Uninstaller.exe"), b"MZ").expect("scritto");

        secure_machine_install(&install, &protected).expect("permessi applicati");

        // Da amministratore — un terminale elevato — si scrive ovunque: il
        // test non avrebbe niente da dimostrare.
        let elevated = Command::new(system_tool("whoami.exe"))
            .arg("/groups")
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("S-1-16-12288"));

        let open = std::fs::write(install.join("launcher-update.tmp"), b"ok");
        let closed = std::fs::write(protected.join("planted.dll"), b"MZ");
        let replaced = std::fs::write(protected.join("VanzaKart Uninstaller.exe"), b"XX");

        // Si rimettono i permessi ereditati, per poter cancellare la cartella.
        let _ = Command::new(system_tool("icacls.exe"))
            .arg(&install)
            .args(["/reset", "/T", "/C", "/Q"])
            .status();

        assert!(
            open.is_ok(),
            "il launcher deve potersi aggiornare: {open:?}"
        );
        if !elevated {
            assert!(closed.is_err(), "una DLL accanto al disinstallatore");
            assert!(replaced.is_err(), "il disinstallatore sostituito");
        }
    }

    #[test]
    fn a_shortcut_is_created_and_removed() {
        let temp = tempfile::tempdir().expect("temp");
        let target = temp.path().join("finto.exe");
        std::fs::write(&target, b"MZ").expect("scritto");
        let link = temp.path().join("collegamento.lnk");

        write_shortcut(&link, &target, temp.path(), "").expect("collegamento");
        assert!(link.is_file());

        assert!(remove_artifact(&Artifact::file(
            ArtifactKind::DesktopShortcut,
            &link
        )));
        assert!(!link.exists());
    }

    #[test]
    fn scheduling_a_removal_refuses_a_path_with_a_percent_sign() {
        let error = schedule_removal(&[PathBuf::from("C:\\temp\\%APPDATA%")]).expect_err("rifiuto");
        assert_eq!(error.code(), "platform");
    }

    #[test]
    fn scheduling_nothing_does_nothing() {
        assert!(!schedule_removal(&[]).expect("niente da fare"));
    }
}
