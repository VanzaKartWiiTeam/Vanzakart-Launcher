//! Server HTTP minimale per i test dell'installer.
//!
//! Più piccolo di quello di `vk-core`: qui non servono `Range` né i mirror che
//! falliscono — quelli sono già coperti dai test del downloader. Serve solo un
//! posto vero da cui scaricare un manifest e un pacchetto.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Server in ascolto su loopback, con un contenuto per percorso.
pub struct TestServer {
    port: u16,
    routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl TestServer {
    pub async fn start(routes: Vec<(&str, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let routes: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(
            routes
                .into_iter()
                .map(|(path, body)| (path.to_string(), body))
                .collect(),
        ));

        let served = Arc::clone(&routes);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let served = Arc::clone(&served);
                tokio::spawn(async move {
                    let _ = handle(stream, served).await;
                });
            }
        });

        Self { port, routes }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    /// Sostituisce il contenuto di una risorsa già pubblicata.
    pub fn replace(&self, path: &str, body: Vec<u8>) {
        self.routes
            .lock()
            .expect("routes")
            .insert(path.to_string(), body);
    }
}

async fn handle(
    mut stream: TcpStream,
    routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; 8192];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();

    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();

    let body = routes.lock().expect("routes").get(&path).cloned();

    match body {
        Some(body) => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).await?;
            stream.write_all(&body).await?;
        }
        None => {
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
        }
    }

    stream.flush().await
}
