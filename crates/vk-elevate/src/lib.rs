//! # vk-elevate
//!
//! Avvia un programma con i permessi di amministratore e ne aspetta l'uscita.
//!
//! Serve a una cosa sola: aggiornare il launcher quando è installato in una
//! cartella che l'utente non può scrivere, cioè in `Programmi` (§D-093). Il
//! launcher resta un processo normale; per il solo scambio dei file chiede a
//! Windows di avviare **sé stesso** elevato, e Windows mostra la finestra UAC.
//!
//! È l'unico crate del progetto con codice `unsafe`, ed è per questo che vive
//! da solo: tutti gli altri restano `#![forbid(unsafe_code)]`. Il crate
//! `runas` fa la stessa chiamata, ma non distingue "l'utente ha detto no" da
//! un guasto e non chiude l'handle del processo; qui le due cose servono.
//!
//! Fuori da Windows non c'è niente da elevare: le funzioni rispondono
//! [`ElevationError::Unsupported`].

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations)]

use std::ffi::OsString;
use std::path::Path;

/// Perché il processo elevato non è partito o non ha finito.
#[derive(Debug, thiserror::Error)]
pub enum ElevationError {
    /// L'utente ha risposto "No" alla richiesta di Windows.
    #[error("the administrator request was declined")]
    Cancelled,

    /// Questo sistema non ha un meccanismo di elevazione gestito qui.
    #[error("elevation is not supported on this platform")]
    Unsupported,

    /// Windows non ha potuto avviare il processo, o non se ne è potuto
    /// leggere l'esito.
    #[error("the elevated process could not be run: {0}")]
    Failed(#[source] std::io::Error),
}

/// Avvia `program` con `args` come amministratore e ne restituisce il codice
/// d'uscita.
///
/// Blocca finché il processo non termina: va chiamata da un thread che può
/// aspettare (in un runtime asincrono, dentro `spawn_blocking`).
///
/// `owner` è l'handle della finestra che chiede l'elevazione: Windows lo usa
/// per mettere la richiesta davanti alla finestra giusta invece di lasciarla
/// lampeggiare nella barra delle applicazioni.
pub fn run_elevated(
    program: &Path,
    args: &[OsString],
    owner: Option<isize>,
) -> Result<u32, ElevationError> {
    #[cfg(windows)]
    {
        windows::run_elevated(program, &command_line(args), owner)
    }
    #[cfg(not(windows))]
    {
        let _ = (program, args, owner);
        Err(ElevationError::Unsupported)
    }
}

/// Unisce gli argomenti in una riga di comando che `CommandLineToArgvW` — e
/// quindi `std::env::args` del processo avviato — spezza di nuovo negli
/// stessi argomenti.
///
/// Le regole sono quelle di Microsoft: le barre rovesciate contano solo
/// davanti a un doppio apice, dove vanno raddoppiate, e l'argomento va fra
/// apici se contiene spazi, tabulazioni o apici, o se è vuoto.
pub fn command_line(args: &[OsString]) -> String {
    let mut line = String::new();

    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            line.push(' ');
        }
        let arg = arg.to_string_lossy();
        let needs_quotes = arg.is_empty() || arg.contains([' ', '\t', '\n', '\u{0B}', '"']);
        if !needs_quotes {
            line.push_str(&arg);
            continue;
        }

        line.push('"');
        let mut backslashes = 0usize;
        for ch in arg.chars() {
            match ch {
                '\\' => backslashes += 1,
                '"' => {
                    // Le barre davanti a un apice si raddoppiano, e l'apice
                    // si protegge con una barra in più.
                    line.push_str(&"\\".repeat(backslashes * 2 + 1));
                    line.push('"');
                    backslashes = 0;
                }
                other => {
                    line.push_str(&"\\".repeat(backslashes));
                    line.push(other);
                    backslashes = 0;
                }
            }
        }
        // Le barre finali stanno davanti all'apice di chiusura: raddoppiate.
        line.push_str(&"\\".repeat(backslashes * 2));
        line.push('"');
    }

    line
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    use super::ElevationError;

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub(super) fn run_elevated(
        program: &Path,
        parameters: &str,
        owner: Option<isize>,
    ) -> Result<u32, ElevationError> {
        let verb = wide("runas".as_ref());
        let file = wide(program.as_os_str());
        let parameters = wide(parameters.as_ref());
        let directory = program.parent().map(|dir| wide(dir.as_os_str()));

        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // NOCLOSEPROCESS: ci serve l'handle per aspettare. NOASYNC: la
            // chiamata torna solo a processo avviato. FLAG_NO_UI: un errore
            // si racconta nella finestra del launcher, non in una di sistema.
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            hwnd: owner.unwrap_or(0) as *mut core::ffi::c_void,
            lpVerb: verb.as_ptr(),
            lpFile: file.as_ptr(),
            lpParameters: parameters.as_ptr(),
            lpDirectory: directory
                .as_ref()
                .map_or(std::ptr::null(), |directory| directory.as_ptr()),
            nShow: SW_HIDE,
            ..Default::default()
        };

        // SAFETY: `info` è inizializzata per intero; le stringhe a cui punta
        // vivono fino alla fine della funzione e terminano con uno zero.
        let started = unsafe { ShellExecuteExW(&mut info) };
        if started == 0 {
            // SAFETY: nessun requisito; legge l'errore del thread corrente.
            let code = unsafe { GetLastError() };
            return Err(if code == ERROR_CANCELLED {
                ElevationError::Cancelled
            } else {
                ElevationError::Failed(std::io::Error::from_raw_os_error(code as i32))
            });
        }

        let process = info.hProcess;
        if process.is_null() {
            return Err(ElevationError::Failed(std::io::Error::other(
                "Windows did not return the elevated process",
            )));
        }

        // SAFETY: `process` è un handle valido restituito da ShellExecuteExW
        // con SEE_MASK_NOCLOSEPROCESS, e lo chiudiamo una volta sola.
        unsafe {
            WaitForSingleObject(process, INFINITE);
            let mut code = 0u32;
            let read = GetExitCodeProcess(process, &mut code);
            let error = GetLastError();
            CloseHandle(process);
            if read == 0 {
                Err(ElevationError::Failed(std::io::Error::from_raw_os_error(
                    error as i32,
                )))
            } else {
                Ok(code)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(args: &[&str]) -> String {
        command_line(&args.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn plain_arguments_are_left_alone() {
        assert_eq!(
            line(&["--vk-apply-update", "job.json"]),
            "--vk-apply-update job.json"
        );
    }

    #[test]
    fn a_path_with_spaces_is_quoted_and_its_backslashes_kept() {
        assert_eq!(
            line(&[r"C:\Users\Mario Rossi\AppData\Local\Temp\vk\job.json"]),
            r#""C:\Users\Mario Rossi\AppData\Local\Temp\vk\job.json""#
        );
    }

    #[test]
    fn trailing_backslashes_are_doubled_before_the_closing_quote() {
        assert_eq!(line(&[r"C:\Program Files\"]), r#""C:\Program Files\\""#);
    }

    #[test]
    fn embedded_quotes_are_escaped() {
        assert_eq!(line(&[r#"a "b" c"#]), r#""a \"b\" c""#);
        assert_eq!(line(&[r#"x\"y z"#]), r#""x\\\"y z""#);
    }

    #[test]
    fn an_empty_argument_survives() {
        assert_eq!(line(&["a", "", "b"]), r#"a "" b"#);
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_there_is_nothing_to_elevate() {
        assert!(matches!(
            run_elevated(Path::new("/bin/true"), &[], None),
            Err(ElevationError::Unsupported)
        ));
    }
}
