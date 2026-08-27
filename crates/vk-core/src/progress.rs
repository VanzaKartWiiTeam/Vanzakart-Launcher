//! Progressi e annullamento.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::error::{CoreError, CoreResult};

/// Fase di un'operazione, con gli stessi nomi mostrati dalla UI legacy
/// (`SetUpdateState("Connecting" | "Backup" | …)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Connecting,
    Backup,
    Download,
    Verifying,
    Installing,
    Updating,
    Recovery,
    Rollback,
    Completed,
    Error,
    Idle,
}

impl Phase {
    /// Etichetta identica a quella del launcher legacy.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Backup => "Backup",
            Self::Download => "Download",
            Self::Verifying => "Verifying",
            Self::Installing => "Installing",
            Self::Updating => "Updating",
            Self::Recovery => "Recovery",
            Self::Rollback => "Rollback",
            Self::Completed => "Completed",
            Self::Error => "Error",
            Self::Idle => "Idle",
        }
    }
}

/// Un aggiornamento di stato emesso durante un'operazione lunga.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressUpdate {
    pub phase: Phase,
    pub detail: String,
    /// 0–100. `None` quando la fase è indeterminata.
    pub percent: Option<f64>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u32,
    pub files_total: u32,
}

impl ProgressUpdate {
    pub fn new(phase: Phase, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
            percent: None,
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
        }
    }

    pub fn with_percent(mut self, percent: f64) -> Self {
        self.percent = Some(percent.clamp(0.0, 100.0));
        self
    }

    pub fn with_bytes(mut self, done: u64, total: u64) -> Self {
        self.bytes_done = done;
        self.bytes_total = total;
        if total > 0 {
            self.percent = Some((done as f64 / total as f64 * 100.0).clamp(0.0, 100.0));
        }
        self
    }

    pub fn with_files(mut self, done: u32, total: u32) -> Self {
        self.files_done = done;
        self.files_total = total;
        self
    }
}

/// Destinazione dei progressi. `Arc` perché viene condiviso fra i task di
/// download concorrenti.
pub type ProgressSink = Arc<dyn Fn(ProgressUpdate) + Send + Sync>;

/// Sink che scarta tutto: utile nei test.
pub fn noop_sink() -> ProgressSink {
    Arc::new(|_| {})
}

/// Token di annullamento cooperativo.
///
/// Non usa `tokio_util` per non aggiungere una dipendenza: il modello a
/// flag atomico è sufficiente, perché ogni loop del core controlla il token a
/// ogni chunk letto o a ogni file processato.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Errore [`CoreError::Cancelled`] se il token è stato attivato.
    pub fn check(&self) -> CoreResult<()> {
        if self.is_cancelled() {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Formatta un numero di byte come nel launcher legacy (`FormatBytes`).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_derived_from_bytes() {
        let update = ProgressUpdate::new(Phase::Download, "x").with_bytes(50, 200);
        assert_eq!(update.percent, Some(25.0));
    }

    #[test]
    fn percent_is_clamped() {
        let update = ProgressUpdate::new(Phase::Download, "x").with_bytes(300, 200);
        assert_eq!(update.percent, Some(100.0));
        assert_eq!(
            ProgressUpdate::new(Phase::Download, "x")
                .with_percent(-5.0)
                .percent,
            Some(0.0)
        );
    }

    #[test]
    fn zero_total_leaves_percent_unset() {
        let update = ProgressUpdate::new(Phase::Download, "x").with_bytes(10, 0);
        assert_eq!(update.percent, None);
    }

    #[test]
    fn cancel_token_is_observable_across_clones() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(token.check().is_ok());
        clone.cancel();
        assert!(token.is_cancelled());
        assert!(matches!(token.check(), Err(CoreError::Cancelled)));
    }

    #[test]
    fn formats_bytes_like_the_legacy_ui() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
