//! Server HTTP minimale per i test di integrazione.
//!
//! Supporta `Range`, risposte pilotate (500/404) e chiusure a metà risposta,
//! così da esercitare resume, retry e failover fra mirror senza rete esterna.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Comportamento richiesto per una risorsa.
#[derive(Debug, Clone)]
pub enum Behaviour {
    /// Serve il contenuto, onorando `Range`.
    Serve(Vec<u8>),
    /// Risponde con lo status indicato per le prime `times` richieste, poi serve.
    FailThenServe {
        status: u16,
        times: u32,
        body: Vec<u8>,
    },
    /// Invia solo `prefix` byte e chiude la connessione (download troncato).
    TruncateThenServe { prefix: usize, body: Vec<u8> },
    /// Risponde sempre con lo status indicato.
    Always(u16),
    /// Ignora `Range` e risponde 200 con il corpo intero.
    IgnoreRange(Vec<u8>),
}

#[derive(Default)]
struct State {
    routes: HashMap<String, Behaviour>,
    hits: HashMap<String, u32>,
}

/// Server di test in ascolto su loopback.
pub struct TestServer {
    port: u16,
    state: Arc<Mutex<State>>,
    requests: Arc<AtomicU32>,
}

impl TestServer {
    pub async fn start(routes: Vec<(&str, Behaviour)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();

        let state = Arc::new(Mutex::new(State {
            routes: routes
                .into_iter()
                .map(|(path, behaviour)| (path.to_string(), behaviour))
                .collect(),
            hits: HashMap::new(),
        }));
        let requests = Arc::new(AtomicU32::new(0));

        let task_state = Arc::clone(&state);
        let task_requests = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let state = Arc::clone(&task_state);
                let requests = Arc::clone(&task_requests);
                tokio::spawn(async move {
                    let _ = handle(stream, state, requests).await;
                });
            }
        });

        Self {
            port,
            state,
            requests,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn total_requests(&self) -> u32 {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn hits(&self, path: &str) -> u32 {
        self.state
            .lock()
            .unwrap()
            .hits
            .get(path)
            .copied()
            .unwrap_or(0)
    }
}

async fn handle(
    mut stream: TcpStream,
    state: Arc<Mutex<State>>,
    requests: Arc<AtomicU32>,
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; 8192];
    let read = stream.read(&mut buffer).await?;
    if read == 0 {
        return Ok(());
    }
    requests.fetch_add(1, Ordering::SeqCst);

    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let mut lines = request.lines();
    let start_line = lines.next().unwrap_or_default();
    let raw_path = start_line.split_whitespace().nth(1).unwrap_or("/");
    // Le query anti-cache non fanno parte della chiave di routing.
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();
    let path = percent_decode(&path);

    let range_start = lines
        .find(|line| line.to_ascii_lowercase().starts_with("range:"))
        .and_then(|line| {
            line.split('=')
                .nth(1)
                .and_then(|value| value.split('-').next())
                .and_then(|value| value.trim().parse::<usize>().ok())
        });

    let behaviour = {
        let mut guard = state.lock().unwrap();
        *guard.hits.entry(path.clone()).or_insert(0) += 1;
        let hits = guard.hits[&path];
        guard
            .routes
            .get(&path)
            .cloned()
            .map(|behaviour| (behaviour, hits))
    };

    let Some((behaviour, hits)) = behaviour else {
        return write_status(&mut stream, 404).await;
    };

    match behaviour {
        Behaviour::Always(status) => write_status(&mut stream, status).await,
        Behaviour::FailThenServe {
            status,
            times,
            body,
        } => {
            if hits <= times {
                write_status(&mut stream, status).await
            } else {
                write_body(&mut stream, &body, range_start, None).await
            }
        }
        Behaviour::TruncateThenServe { prefix, body } => {
            if hits == 1 {
                write_body(&mut stream, &body, range_start, Some(prefix)).await
            } else {
                write_body(&mut stream, &body, range_start, None).await
            }
        }
        Behaviour::IgnoreRange(body) => write_body(&mut stream, &body, None, None).await,
        Behaviour::Serve(body) => write_body(&mut stream, &body, range_start, None).await,
    }
}

async fn write_status(stream: &mut TcpStream, status: u16) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        reason(status)
    );
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await
}

async fn write_body(
    stream: &mut TcpStream,
    body: &[u8],
    range_start: Option<usize>,
    truncate_after: Option<usize>,
) -> std::io::Result<()> {
    let total = body.len();

    let (status, slice, extra) = match range_start {
        Some(start) if start >= total => {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(head.as_bytes()).await?;
            return stream.flush().await;
        }
        Some(start) => (
            206u16,
            &body[start..],
            format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
        ),
        None => (200u16, body, String::new()),
    };

    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}Connection: close\r\n\r\n",
        reason(status),
        slice.len()
    );
    stream.write_all(head.as_bytes()).await?;

    match truncate_after {
        Some(prefix) => {
            let end = prefix.min(slice.len());
            stream.write_all(&slice[..end]).await?;
            stream.flush().await?;
            // Chiusura brusca: il client vede un corpo più corto del dichiarato.
            Ok(())
        }
        None => {
            stream.write_all(slice).await?;
            stream.flush().await
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).to_string()
}
