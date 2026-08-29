//! Download con resume, retry e cascata di mirror.
//!
//! Porta `Launcher/Services/NetworkService.cs` mantenendone la semantica:
//! 3 tentativi per URL, backoff `450ms × tentativo`, `Range` per il resume,
//! `416` con `Content-Range` coerente trattato come download già completo,
//! e passaggio al mirror successivo quando tutti i tentativi falliscono.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{HeaderValue, CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use tokio::io::AsyncWriteExt;

use crate::error::{CoreError, CoreResult};
use crate::progress::{CancelToken, ProgressSink, ProgressUpdate};

/// Stessa dimensione del buffer del launcher legacy.
const DOWNLOAD_BUFFER_SIZE: usize = 256 * 1024;
const DEFAULT_RETRIES: u32 = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// Esito di un download completato.
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub source_url: String,
    pub bytes_received: u64,
    pub total_bytes: u64,
    pub elapsed: Duration,
    pub attempts: Vec<AttemptInfo>,
}

impl DownloadOutcome {
    pub fn retry_count(&self) -> usize {
        self.attempts.len().saturating_sub(1)
    }

    /// Riga di log equivalente a `FormatDownloadResult` del legacy, già redatta.
    pub fn summary(&self, label: &str) -> String {
        let seconds = self.elapsed.as_secs_f64().max(0.001);
        format!(
            "{label}: {} in {:.2}s ({}/s), attempts={}, retries={}, source={}",
            crate::progress::format_bytes(self.bytes_received),
            self.elapsed.as_secs_f64(),
            crate::progress::format_bytes((self.bytes_received as f64 / seconds) as u64),
            self.attempts.len(),
            self.retry_count(),
            crate::redact::redact_url(&self.source_url),
        )
    }
}

#[derive(Debug, Clone)]
pub struct AttemptInfo {
    pub url: String,
    pub attempt: u32,
    pub success: bool,
    pub status: Option<u16>,
    pub existing_bytes: u64,
    pub bytes_received: u64,
    pub elapsed: Duration,
    pub error: String,
}

/// Client HTTP condiviso.
#[derive(Debug, Clone)]
pub struct Downloader {
    client: reqwest::Client,
    retries: u32,
    /// Consente `http://` verso indirizzi di loopback. Serve ai test di
    /// integrazione e a eventuali mirror locali; è `false` di default.
    allow_loopback_http: bool,
}

impl Downloader {
    pub fn new(user_agent: &str) -> CoreResult<Self> {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(8)
            .pool_idle_timeout(Duration::from_secs(900))
            // La decompressione automatica falsa il conteggio dei byte e rompe
            // il resume: il legacy la disattivava esplicitamente.
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .build()
            .map_err(CoreError::from)?;

        Ok(Self {
            client,
            retries: DEFAULT_RETRIES,
            allow_loopback_http: false,
        })
    }

    /// Costruisce un downloader attorno a un client già configurato.
    pub fn from_client(client: reqwest::Client) -> Self {
        Self {
            client,
            retries: DEFAULT_RETRIES,
            allow_loopback_http: false,
        }
    }

    /// Abilita `http://` verso loopback. Non usare in produzione.
    pub fn with_loopback_http(mut self, allow: bool) -> Self {
        self.allow_loopback_http = allow;
        self
    }

    /// Valida una sorgente di download secondo la policy del downloader.
    pub fn is_allowed_source(&self, url: &str) -> bool {
        if crate::endpoints::is_safe_endpoint(url) {
            return true;
        }
        if !self.allow_loopback_http {
            return false;
        }
        url::Url::parse(url.trim()).is_ok_and(|parsed| {
            parsed.scheme() == "http"
                && parsed.host_str().is_some_and(|host| {
                    host == "localhost" || host == "127.0.0.1" || host == "[::1]"
                })
        })
    }

    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries.max(1);
        self
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// GET che restituisce il corpo come stringa.
    pub async fn get_string(&self, url: &str) -> CoreResult<String> {
        self.require_allowed_source(url)?;
        let response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                status: status.as_u16(),
                url: crate::redact::redact_url(url),
            });
        }
        Ok(response.text().await?)
    }

    /// GET che restituisce il corpo grezzo.
    ///
    /// Per risposte piccole e non riprendibili — un'immagine renderizzata,
    /// un'icona — dove aprire un file temporaneo costerebbe più del download.
    pub async fn get_bytes(&self, url: &str) -> CoreResult<Vec<u8>> {
        self.require_allowed_source(url)?;
        let response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                status: status.as_u16(),
                url: crate::redact::redact_url(url),
            });
        }
        Ok(response.bytes().await?.to_vec())
    }

    /// POST JSON che restituisce il corpo come stringa.
    pub async fn post_json(&self, url: &str, body: &serde_json::Value) -> CoreResult<String> {
        self.require_allowed_source(url)?;
        let response = self.client.post(url).json(body).send().await?;
        Ok(response.text().await?)
    }

    /// Scarica su file con resume e retry, da una singola sorgente.
    pub async fn download_with_resume(
        &self,
        url: &str,
        destination: &Path,
        progress: &ProgressSink,
        cancel: &CancelToken,
    ) -> CoreResult<DownloadOutcome> {
        self.require_allowed_source(url)?;

        let overall = Instant::now();
        let mut attempts: Vec<AttemptInfo> = Vec::new();
        let mut last_error: Option<CoreError> = None;

        for attempt in 1..=self.retries {
            cancel.check()?;

            let existing_bytes = file_len(destination).await;
            let attempt_start = Instant::now();

            match self
                .download_once(url, destination, progress, cancel, existing_bytes)
                .await
            {
                Ok(transfer) => {
                    attempts.push(AttemptInfo {
                        url: url.to_string(),
                        attempt,
                        success: true,
                        status: Some(transfer.status),
                        existing_bytes: transfer.existing_bytes,
                        bytes_received: transfer.bytes_received,
                        elapsed: attempt_start.elapsed(),
                        error: String::new(),
                    });

                    return Ok(DownloadOutcome {
                        source_url: url.to_string(),
                        bytes_received: transfer.bytes_received,
                        total_bytes: transfer.total_bytes,
                        elapsed: overall.elapsed(),
                        attempts,
                    });
                }
                Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                Err(error) => {
                    let current_len = file_len(destination).await;
                    attempts.push(AttemptInfo {
                        url: url.to_string(),
                        attempt,
                        success: false,
                        status: match &error {
                            CoreError::HttpStatus { status, .. } => Some(*status),
                            _ => None,
                        },
                        existing_bytes,
                        bytes_received: current_len.saturating_sub(existing_bytes),
                        elapsed: attempt_start.elapsed(),
                        error: crate::redact::redact(&error.to_string()),
                    });

                    let retryable = error.is_retryable();
                    last_error = Some(error);

                    if attempt < self.retries && retryable {
                        tokio::time::sleep(Duration::from_millis(450 * u64::from(attempt))).await;
                    } else {
                        break;
                    }
                }
            }
        }

        Err(CoreError::AllMirrorsFailed(format!(
            "{} tentativo/i falliti su {}: {}",
            attempts.len(),
            crate::redact::redact_url(url),
            last_error
                .map(|error| crate::redact::redact(&error.to_string()))
                .unwrap_or_default()
        )))
    }

    /// Prova le sorgenti nell'ordine, con dedup case-insensitive, fermandosi
    /// alla prima che riesce. Equivale all'overload con `IEnumerable<string>`
    /// del `NetworkService` legacy.
    pub async fn download_with_mirrors(
        &self,
        urls: &[String],
        destination: &Path,
        progress: &ProgressSink,
        cancel: &CancelToken,
    ) -> CoreResult<DownloadOutcome> {
        let candidates = dedupe_urls(urls);
        if candidates.is_empty() {
            return Err(CoreError::InvalidUrl(
                "at least one download URL is needed".into(),
            ));
        }

        let mut all_attempts: Vec<AttemptInfo> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for url in &candidates {
            cancel.check()?;

            if self.require_allowed_source(url).is_err() {
                errors.push(format!("{} -> unsafe URL", crate::redact::redact_url(url)));
                continue;
            }

            match self
                .download_with_resume(url, destination, progress, cancel)
                .await
            {
                Ok(mut outcome) => {
                    all_attempts.append(&mut outcome.attempts);
                    outcome.attempts = all_attempts;
                    return Ok(outcome);
                }
                Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                Err(error) => errors.push(format!(
                    "{} -> {}",
                    crate::redact::redact_url(url),
                    crate::redact::redact(&error.to_string())
                )),
            }
        }

        Err(CoreError::AllMirrorsFailed(errors.join("\n")))
    }

    fn require_allowed_source(&self, url: &str) -> CoreResult<()> {
        if self.is_allowed_source(url) {
            Ok(())
        } else {
            Err(CoreError::InvalidUrl(crate::redact::redact_url(url)))
        }
    }

    async fn download_once(
        &self,
        url: &str,
        destination: &Path,
        progress: &ProgressSink,
        cancel: &CancelToken,
        existing_bytes: u64,
    ) -> CoreResult<Transfer> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CoreError::io(parent, e))?;
        }

        let mut existing = existing_bytes;
        let mut request = self.client.get(url);
        if existing > 0 {
            let value = HeaderValue::from_str(&format!("bytes={existing}-"))
                .map_err(|_| CoreError::InvalidUrl("invalid Range header".into()))?;
            request = request.header(RANGE, value);
        }

        let response = request.send().await?;
        let status = response.status();

        // Il server dichiara che il range richiesto è oltre la fine e la
        // lunghezza totale coincide con quanto già scaricato: file completo.
        if existing > 0 && status == StatusCode::RANGE_NOT_SATISFIABLE {
            if let Some(total) = content_range_total(response.headers().get(CONTENT_RANGE)) {
                if total == existing {
                    progress(
                        ProgressUpdate::new(crate::progress::Phase::Download, "Already downloaded")
                            .with_bytes(existing, existing),
                    );
                    return Ok(Transfer {
                        status: status.as_u16(),
                        existing_bytes: existing,
                        bytes_received: 0,
                        total_bytes: existing,
                    });
                }
            }
        }

        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                status: status.as_u16(),
                url: crate::redact::redact_url(url),
            });
        }

        // Il server ha ignorato il Range e rimanda l'intero corpo: si riparte
        // da zero, come nel legacy.
        if existing > 0 && status != StatusCode::PARTIAL_CONTENT {
            tokio::fs::remove_file(destination)
                .await
                .map_err(|e| CoreError::io(destination, e))?;
            existing = 0;
        }

        let total = response.content_length().unwrap_or(0) + existing;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(existing > 0)
            .truncate(existing == 0)
            .open(destination)
            .await
            .map_err(|e| CoreError::io(destination, e))?;

        let mut stream = response.bytes_stream();
        let mut current = existing;
        let mut received = 0u64;
        let mut since_last_report = 0usize;

        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                let _ = file.flush().await;
                return Err(CoreError::Cancelled);
            }

            let chunk = chunk?;
            file.write_all(&chunk)
                .await
                .map_err(|e| CoreError::io(destination, e))?;

            current += chunk.len() as u64;
            received += chunk.len() as u64;
            since_last_report += chunk.len();

            if since_last_report >= DOWNLOAD_BUFFER_SIZE {
                since_last_report = 0;
                progress(
                    ProgressUpdate::new(crate::progress::Phase::Download, "Downloading")
                        .with_bytes(current, total.max(current)),
                );
            }
        }

        file.flush()
            .await
            .map_err(|e| CoreError::io(destination, e))?;

        progress(
            ProgressUpdate::new(crate::progress::Phase::Download, "Download complete")
                .with_bytes(current, total.max(current)),
        );

        Ok(Transfer {
            status: status.as_u16(),
            existing_bytes: existing,
            bytes_received: received,
            total_bytes: total.max(current),
        })
    }
}

struct Transfer {
    status: u16,
    existing_bytes: u64,
    bytes_received: u64,
    total_bytes: u64,
}

async fn file_len(path: &Path) -> u64 {
    tokio::fs::metadata(path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0)
}

fn content_range_total(header: Option<&HeaderValue>) -> Option<u64> {
    let value = header?.to_str().ok()?;
    // Forme accettate: `bytes */1234` e `bytes 0-1233/1234`.
    value.rsplit('/').next()?.trim().parse::<u64>().ok()
}

/// Dedup case-insensitive che preserva l'ordine, come `Distinct` del legacy.
pub fn dedupe_urls(urls: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(urls.len());
    for url in urls {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|item| item.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

/// Destinazione temporanea per un download, accanto al file finale.
pub fn staging_path_for(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    name.push_str(".part");
    destination.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_preserving_order() {
        let urls = vec![
            "https://a.example/x".to_string(),
            "  ".to_string(),
            "HTTPS://A.EXAMPLE/X".to_string(),
            "https://b.example/x".to_string(),
        ];
        assert_eq!(
            dedupe_urls(&urls),
            vec!["https://a.example/x", "https://b.example/x"]
        );
    }

    #[test]
    fn parses_content_range_totals() {
        let header = HeaderValue::from_static("bytes 0-1233/1234");
        assert_eq!(content_range_total(Some(&header)), Some(1234));

        let star = HeaderValue::from_static("bytes */99");
        assert_eq!(content_range_total(Some(&star)), Some(99));

        let unknown = HeaderValue::from_static("bytes 0-9/*");
        assert_eq!(content_range_total(Some(&unknown)), None);
        assert_eq!(content_range_total(None), None);
    }

    #[test]
    fn staging_path_is_a_sibling() {
        let path = staging_path_for(Path::new("/tmp/a/VanzaKart.zip"));
        assert_eq!(path.file_name().unwrap(), "VanzaKart.zip.part");
        assert_eq!(path.parent(), Some(Path::new("/tmp/a")));
    }

    #[tokio::test]
    async fn refuses_an_empty_mirror_list() {
        let downloader = Downloader::new("test").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let error = downloader
            .download_with_mirrors(
                &[],
                &dir.path().join("x.bin"),
                &crate::progress::noop_sink(),
                &CancelToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, CoreError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn refuses_insecure_mirrors_without_touching_the_disk() {
        let downloader = Downloader::new("test").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("x.bin");

        let error = downloader
            .download_with_mirrors(
                &["http://insecure.example/a".to_string()],
                &destination,
                &crate::progress::noop_sink(),
                &CancelToken::new(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, CoreError::AllMirrorsFailed(_)));
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn honours_cancellation_before_any_request() {
        let downloader = Downloader::new("test").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        cancel.cancel();

        let error = downloader
            .download_with_mirrors(
                &["https://a.example/x".to_string()],
                &dir.path().join("x.bin"),
                &crate::progress::noop_sink(),
                &cancel,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, CoreError::Cancelled));
    }

    #[test]
    fn retry_classification_matches_the_legacy_rules() {
        for status in [408u16, 425, 429, 500, 502, 503, 504] {
            assert!(CoreError::HttpStatus {
                status,
                url: String::new()
            }
            .is_retryable());
        }
        for status in [400u16, 401, 403, 404, 410] {
            assert!(!CoreError::HttpStatus {
                status,
                url: String::new()
            }
            .is_retryable());
        }
        assert!(CoreError::Network("timeout".into()).is_retryable());
        assert!(!CoreError::InvalidUrl("x".into()).is_retryable());
    }
}
