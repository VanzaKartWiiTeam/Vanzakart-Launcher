//! Identificazione della piattaforma corrente.
//!
//! Il vocabolario è quello dell'updater di Tauri (`{{target}}-{{arch}}`), così
//! il manifest dell'installer e quello dell'aggiornamento automatico parlano
//! la stessa lingua e possono essere generati dallo stesso script.

use std::fmt;

/// Sistema operativo e architettura della macchina su cui gira l'installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub os: &'static str,
    pub arch: &'static str,
}

impl Target {
    /// La piattaforma su cui è stato compilato questo binario.
    pub const fn current() -> Self {
        Self {
            os: current_os(),
            arch: current_arch(),
        }
    }

    /// Chiave esatta, per esempio `windows-x86_64`.
    pub fn key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }

    /// Chiavi accettabili, dalla più specifica alla più generica.
    ///
    /// Su macOS un pacchetto universale vale per entrambe le architetture:
    /// è il formato che produce `--target universal-apple-darwin`, quindi
    /// `darwin-universal` è una ricaduta legittima di `darwin-aarch64`.
    pub fn candidates(&self) -> Vec<String> {
        let mut keys = vec![self.key()];
        if self.os == "darwin" {
            keys.push("darwin-universal".to_string());
        }
        keys.dedup();
        keys
    }

    /// Nome mostrato all'utente.
    pub fn display_name(&self) -> &'static str {
        match self.os {
            "windows" => "Windows",
            "darwin" => "macOS",
            _ => "Linux",
        }
    }

    pub fn is_windows(&self) -> bool {
        self.os == "windows"
    }

    pub fn is_macos(&self) -> bool {
        self.os == "darwin"
    }

    pub fn is_linux(&self) -> bool {
        self.os == "linux"
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

const fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

const fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "i686"
    } else if cfg!(target_arch = "arm") {
        "armv7"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_target_is_named() {
        let target = Target::current();
        assert!(["windows", "darwin", "linux"].contains(&target.os));
        assert_ne!(target.arch, "unknown");
        assert!(target.key().contains('-'));
    }

    #[test]
    fn the_exact_key_comes_first() {
        let target = Target::current();
        assert_eq!(target.candidates().first(), Some(&target.key()));
    }

    #[test]
    fn a_universal_package_is_acceptable_on_macos() {
        let target = Target {
            os: "darwin",
            arch: "aarch64",
        };
        assert_eq!(
            target.candidates(),
            vec!["darwin-aarch64".to_string(), "darwin-universal".to_string()]
        );
    }

    #[test]
    fn there_is_no_universal_fallback_elsewhere() {
        let target = Target {
            os: "linux",
            arch: "x86_64",
        };
        assert_eq!(target.candidates(), vec!["linux-x86_64".to_string()]);
    }

    #[test]
    fn each_platform_has_a_display_name() {
        assert_eq!(
            Target {
                os: "windows",
                arch: "x86_64"
            }
            .display_name(),
            "Windows"
        );
        assert_eq!(
            Target {
                os: "darwin",
                arch: "aarch64"
            }
            .display_name(),
            "macOS"
        );
        assert_eq!(
            Target {
                os: "linux",
                arch: "x86_64"
            }
            .display_name(),
            "Linux"
        );
    }
}
