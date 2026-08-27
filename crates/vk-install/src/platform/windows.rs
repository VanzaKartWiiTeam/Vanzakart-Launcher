//! Integrazione con Windows: collegamenti `.lnk`, chiave di disinstallazione,
//! rimozione differita della cartella.

use std::path::{Path, PathBuf};
use std::process::Command;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
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
const UNINSTALL_SHORTCUT_FILE_NAME: &str = "Disinstalla VanzaKart Launcher.lnk";

pub fn create_shortcuts(request: &ShortcutRequest) -> Vec<Artifact> {
    let mut artifacts = Vec::new();

    if request.desktop {
        if let Some(desktop) = dirs::desktop_dir() {
            let link = desktop.join(SHORTCUT_FILE_NAME);
            if write_shortcut(&link, request.executable, request.working_dir, "").is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::DesktopShortcut, &link));
            }
        }
    }

    if request.start_menu {
        if let Some(programs) = start_menu_programs() {
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

    if request.quick_launch {
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

/// Scrive la chiave che fa comparire il launcher in "App e funzionalità".
///
/// Il nome della chiave è l'identificatore del bundle, lo stesso che userebbe
/// l'installer NSIS di Tauri: così l'aggiornamento automatico riconosce
/// *questa* installazione invece di affiancargliene una seconda (§D-052).
pub fn register_uninstall(registration: &UninstallRegistration) -> InstallResult<Vec<Artifact>> {
    let key_path = format!(r"{UNINSTALL_ROOT}\{}", crate::BUNDLE_IDENTIFIER);
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
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
    write("URLInfoAbout", "https://vwfc.sitodaking.it/")?;
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

    let mut artifacts = vec![Artifact::new(
        ArtifactKind::RegistryKey,
        format!(r"HKCU\{key_path}"),
    )];

    // `App Paths`: fa funzionare "Esegui → vanzakart launcher".
    if let Some(file_name) = registration
        .executable
        .file_name()
        .and_then(|name| name.to_str())
    {
        let app_path_key = format!(r"{APP_PATHS_ROOT}\{file_name}");
        if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&app_path_key) {
            let _ = key.set_value("", &registration.executable.to_string_lossy().to_string());
            let _ = key.set_value(
                "Path",
                &registration.install_dir.to_string_lossy().to_string(),
            );
            artifacts.push(Artifact::new(
                ArtifactKind::RegistryKey,
                format!(r"HKCU\{app_path_key}"),
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
    read_install_location(crate::BUNDLE_IDENTIFIER)
}

/// Cartella del launcher legacy in C#, dalla chiave che scriveva il suo setup.
///
/// Serve solo all'installer, per proporre la cartella che l'utente sta già
/// usando. Non è un'installazione nostra e non si rimuove.
pub fn legacy_install_dir() -> Option<PathBuf> {
    read_install_location(LEGACY_UNINSTALL_KEY)
}

fn read_install_location(key_name: &str) -> Option<PathBuf> {
    let location = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(format!(r"{UNINSTALL_ROOT}\{key_name}"), KEY_READ)
        .ok()?
        .get_value::<String, _>("InstallLocation")
        .ok()?;
    let location = PathBuf::from(location.trim());
    location.is_dir().then_some(location)
}

/// Versione registrata dall'installazione corrente, se c'è.
pub fn registered_version() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            format!(r"{UNINSTALL_ROOT}\{}", crate::BUNDLE_IDENTIFIER),
            KEY_READ,
        )
        .ok()?
        .get_value::<String, _>("DisplayVersion")
        .ok()
}

/// Toglie la registrazione di **questa** installazione.
///
/// La chiave del launcher legacy resta dov'è anche quando la si è trovata:
/// descrive un programma che non abbiamo installato noi e che può essere
/// ancora sul disco. Cancellarla farebbe sparire il launcher vecchio da "App e
/// funzionalità" lasciandolo installato.
pub fn unregister_uninstall(executable_name: Option<&str>) -> bool {
    let mut removed = delete_key_tree(&format!(
        r"HKCU\{UNINSTALL_ROOT}\{}",
        crate::BUNDLE_IDENTIFIER
    ));
    if let Some(name) = executable_name {
        removed |= delete_key_tree(&format!(r"HKCU\{APP_PATHS_ROOT}\{name}"));
    }
    removed
}

fn delete_key_tree(qualified: &str) -> bool {
    let Some(path) = qualified.strip_prefix(r"HKCU\") else {
        return false;
    };
    RegKey::predef(HKEY_CURRENT_USER)
        .delete_subkey_all(path)
        .is_ok()
}

/// Avvia il launcher e lascia che l'installer si chiuda.
pub fn launch_detached(executable: &Path) -> InstallResult<()> {
    let working_dir = executable.parent().unwrap_or(Path::new("."));
    Command::new(executable)
        .current_dir(working_dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::io(executable, error))
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
            "il percorso contiene caratteri che non si possono passare a uno script di rimozione",
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
    Command::new("cmd.exe")
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
        return Err(InstallError::platform("argomenti non validi"));
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
            "collegamento non creato: {}",
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
        let mut command = Command::new("wscript.exe");
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
        let mut command = Command::new("powershell.exe");
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
            "lo script di collegamento è uscito con {status}"
        ))),
    }
}

/// I percorsi di Windows non possono contenere virgolette: se ce ne sono,
/// qualcosa non torna e non si prosegue.
fn quotable(path: &Path) -> InstallResult<String> {
    let text = path.to_string_lossy().to_string();
    if text.contains('"') || text.contains('\'') || text.contains('\n') || text.contains('\r') {
        return Err(InstallError::platform(format!(
            "percorso non utilizzabile in un collegamento: {text}"
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
    }

    #[test]
    fn removing_a_key_that_is_not_ours_is_refused() {
        assert!(!delete_key_tree(r"HKLM\Software\Qualcosa"));
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
