//! Integrazione con Linux: file `.desktop`, icona nel tema `hicolor`,
//! collegamento nel `PATH`.
//!
//! L'equivalente del menu Start è `~/.local/share/applications`: un file di
//! testo per ogni voce. Non serve nessun privilegio, e la disinstallazione è
//! la cancellazione degli stessi file.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{ShortcutRequest, UninstallRegistration, SHORTCUT_DESCRIPTION};
use crate::error::{InstallError, InstallResult};
use crate::record::{Artifact, ArtifactKind};

const DESKTOP_FILE_NAME: &str = "vanzakart-launcher.desktop";
const UNINSTALL_DESKTOP_FILE_NAME: &str = "vanzakart-uninstaller.desktop";
const ICON_NAME: &str = "vanzakart-launcher";
const SYMLINK_NAME: &str = "vanzakart-launcher";

pub fn create_shortcuts(request: &ShortcutRequest) -> Vec<Artifact> {
    let mut artifacts = Vec::new();

    // L'icona va installata per prima: le voci del menu la citano per nome e
    // il tema la risolve solo se il file esiste già.
    let icon = request.icon.and_then(|source| match install_icon(source) {
        Ok(path) => {
            artifacts.push(Artifact::file(ArtifactKind::Icon, &path));
            Some(ICON_NAME.to_string())
        }
        Err(error) => {
            tracing::warn!(%error, "icona non installata");
            None
        }
    });
    let icon = icon.unwrap_or_else(|| ICON_NAME.to_string());

    let entry = DesktopEntry {
        name: crate::PRODUCT_NAME,
        comment: SHORTCUT_DESCRIPTION,
        executable: request.executable,
        arguments: "",
        icon: &icon,
        categories: "Game;",
    };

    if request.start_menu {
        if let Some(applications) = applications_dir() {
            let path = applications.join(DESKTOP_FILE_NAME);
            if write_desktop_entry(&path, &entry).is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::StartMenuShortcut, &path));
            }

            if request.uninstall_entry {
                if let Some(uninstaller) = request.uninstaller {
                    let path = applications.join(UNINSTALL_DESKTOP_FILE_NAME);
                    let uninstall_entry = DesktopEntry {
                        name: "Uninstall VanzaKart Launcher",
                        comment: "Removes the VanzaKart launcher from this computer",
                        executable: uninstaller,
                        arguments: "--uninstall",
                        icon: &icon,
                        categories: "Game;Settings;",
                    };
                    if write_desktop_entry(&path, &uninstall_entry).is_ok() {
                        artifacts.push(Artifact::file(ArtifactKind::UninstallShortcut, &path));
                    }
                }
            }

            refresh_desktop_database(&applications);
        }
    }

    if request.desktop {
        if let Some(desktop) = dirs::desktop_dir() {
            let path = desktop.join(DESKTOP_FILE_NAME);
            if write_desktop_entry(&path, &entry).is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::DesktopShortcut, &path));
            }
        }
    }

    if request.path_symlink {
        if let Some(bin) = dirs::home_dir().map(|home| home.join(".local").join("bin")) {
            let link = bin.join(SYMLINK_NAME);
            if symlink(request.executable, &link).is_ok() {
                artifacts.push(Artifact::file(ArtifactKind::Symlink, &link));
            }
        }
    }

    artifacts
}

pub fn remove_artifact(artifact: &Artifact) -> bool {
    let path = PathBuf::from(&artifact.path);
    let removed = crate::fsops::remove_path_best_effort(&path);

    if artifact.kind == ArtifactKind::StartMenuShortcut
        || artifact.kind == ArtifactKind::UninstallShortcut
    {
        if let Some(parent) = path.parent() {
            refresh_desktop_database(parent);
        }
    } else {
        super::remove_parent_if_empty(&path);
    }

    removed
}

/// Su Linux non esiste un registro dei programmi installati: il registro
/// dell'installazione è l'unica traccia.
pub fn register_uninstall(_registration: &UninstallRegistration) -> InstallResult<Vec<Artifact>> {
    Ok(Vec::new())
}

pub fn unregister_uninstall(_executable_name: Option<&str>) -> bool {
    false
}

pub fn registered_install_dir() -> Option<PathBuf> {
    None
}

/// Il launcher legacy in C# era solo per Windows: non c'è nessuna
/// installazione vecchia da riconoscere.
pub fn legacy_install_dir() -> Option<PathBuf> {
    None
}

pub fn registered_version() -> Option<String> {
    None
}

pub fn launch_detached(executable: &Path) -> InstallResult<()> {
    let working_dir = executable.parent().unwrap_or(Path::new("."));
    Command::new(executable)
        .current_dir(working_dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::io(executable, error))
}

/// Come su macOS: cancellare un eseguibile in esecuzione è lecito, quindi la
/// rimozione avviene subito e non resta nessuno script in giro.
pub fn schedule_removal(paths: &[PathBuf]) -> InstallResult<bool> {
    for path in paths {
        crate::fsops::remove_path(path)?;
    }
    Ok(false)
}

/// Contenuto di una voce del menu applicazioni.
struct DesktopEntry<'a> {
    name: &'a str,
    comment: &'a str,
    executable: &'a Path,
    arguments: &'a str,
    icon: &'a str,
    categories: &'a str,
}

impl DesktopEntry<'_> {
    fn render(&self) -> String {
        let mut exec = exec_argument(self.executable);
        if !self.arguments.is_empty() {
            exec.push(' ');
            exec.push_str(self.arguments);
        }

        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Version=1.0\n\
             Name={name}\n\
             Comment={comment}\n\
             Exec={exec}\n\
             Icon={icon}\n\
             Terminal=false\n\
             Categories={categories}\n\
             StartupNotify=true\n\
             StartupWMClass={wm_class}\n",
            name = escape_value(self.name),
            comment = escape_value(self.comment),
            icon = escape_value(self.icon),
            categories = self.categories,
            wm_class = escape_value(crate::PRODUCT_NAME),
        )
    }
}

fn write_desktop_entry(path: &Path, entry: &DesktopEntry) -> InstallResult<()> {
    if let Some(parent) = path.parent() {
        crate::fsops::ensure_dir(parent)?;
    }
    std::fs::write(path, entry.render()).map_err(|error| InstallError::io(path, error))?;
    // Un `.desktop` sul desktop viene eseguito solo se è eseguibile: senza
    // questo, GNOME mostra un file di testo.
    crate::fsops::set_executable(path)
}

fn install_icon(source: &Path) -> InstallResult<PathBuf> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "png".to_string());
    let target = dirs::data_dir()
        .ok_or_else(|| InstallError::platform("the user data folder could not be determined"))?
        .join("icons")
        .join("hicolor")
        .join("256x256")
        .join("apps")
        .join(format!("{ICON_NAME}.{extension}"));

    if let Some(parent) = target.parent() {
        crate::fsops::ensure_dir(parent)?;
    }
    std::fs::copy(source, &target).map_err(|error| InstallError::io(&target, error))?;
    Ok(target)
}

fn applications_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|data| data.join("applications"))
}

/// Aggiorna la cache del menu. Se il comando non c'è, i desktop moderni
/// rileggono comunque la cartella: è un miglioramento, non un requisito.
fn refresh_desktop_database(applications: &Path) {
    let _ = Command::new("update-desktop-database")
        .arg(applications)
        .status();
}

fn symlink(target: &Path, link: &Path) -> InstallResult<()> {
    if let Some(parent) = link.parent() {
        crate::fsops::ensure_dir(parent)?;
    }
    crate::fsops::remove_path_best_effort(link);
    std::os::unix::fs::symlink(target, link).map_err(|error| InstallError::io(link, error))
}

/// Percorso citabile dentro `Exec=`.
///
/// Due livelli di quoting da rispettare: quello del file `.desktop`, dove la
/// barra rovesciata è il carattere di escape, e quello degli argomenti, dove
/// vanno protetti `"`, `$`, `` ` `` e la barra stessa. Una cartella con uno
/// spazio nel nome è normalissima e senza virgolette il menu non avvia nulla.
fn exec_argument(path: &Path) -> String {
    let mut quoted = String::from("\"");
    for character in path.to_string_lossy().chars() {
        match character {
            '"' => quoted.push_str("\\\\\""),
            '\\' => quoted.push_str("\\\\\\\\"),
            '$' => quoted.push_str("\\\\$"),
            '`' => quoted.push_str("\\\\`"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// Escape dei valori semplici: solo la barra rovesciata e gli a capo.
fn escape_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_for(path: &Path) -> DesktopEntry<'_> {
        DesktopEntry {
            name: crate::PRODUCT_NAME,
            comment: SHORTCUT_DESCRIPTION,
            executable: path,
            arguments: "",
            icon: ICON_NAME,
            categories: "Game;",
        }
    }

    #[test]
    fn a_path_with_spaces_is_quoted() {
        let rendered = entry_for(Path::new(
            "/home/tizio/.local/opt/VanzaKart Launcher/app.AppImage",
        ))
        .render();
        assert!(
            rendered.contains("Exec=\"/home/tizio/.local/opt/VanzaKart Launcher/app.AppImage\""),
            "{rendered}"
        );
    }

    #[test]
    fn the_dangerous_characters_are_escaped_twice() {
        // `$` deve arrivare all'esecuzione come `\$`, e nel file si scrive
        // `\\$`: altrimenti la shell del menu proverebbe a espanderlo.
        let quoted = exec_argument(Path::new("/home/tizio/$HOME/`whoami`/app"));
        assert!(quoted.contains("\\\\$HOME"), "{quoted}");
        assert!(quoted.contains("\\\\`whoami\\\\`"), "{quoted}");
    }

    #[test]
    fn a_backslash_survives_the_round_trip() {
        let quoted = exec_argument(Path::new("/home/tizio/a\\b"));
        assert!(quoted.contains("a\\\\\\\\b"), "{quoted}");
    }

    #[test]
    fn the_entry_declares_everything_a_menu_needs() {
        let rendered = entry_for(Path::new("/opt/app")).render();
        for required in [
            "[Desktop Entry]",
            "Type=Application",
            "Name=VanzaKart Launcher",
            "Icon=vanzakart-launcher",
            "Terminal=false",
            "Categories=Game;",
        ] {
            assert!(
                rendered.contains(required),
                "manca {required} in {rendered}"
            );
        }
    }

    #[test]
    fn an_entry_is_written_and_is_executable() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join(DESKTOP_FILE_NAME);
        write_desktop_entry(&path, &entry_for(Path::new("/opt/app"))).expect("scritto");

        assert!(path.is_file());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    #[test]
    fn an_entry_is_removed_again() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join(DESKTOP_FILE_NAME);
        write_desktop_entry(&path, &entry_for(Path::new("/opt/app"))).expect("scritto");

        assert!(remove_artifact(&Artifact::file(
            ArtifactKind::StartMenuShortcut,
            &path
        )));
        assert!(!path.exists());
    }

    #[test]
    fn removal_happens_at_once() {
        let temp = tempfile::tempdir().expect("temp");
        let app = temp.path().join("app");
        std::fs::create_dir_all(&app).expect("app");
        assert!(!schedule_removal(std::slice::from_ref(&app)).expect("rimosso"));
        assert!(!app.exists());
    }

    #[test]
    fn a_newline_never_breaks_a_value() {
        assert_eq!(escape_value("uno\ndue"), "uno due");
        assert_eq!(escape_value("c:\\x"), "c:\\\\x");
    }
}
