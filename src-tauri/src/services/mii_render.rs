//! Render degli avatar Mii e runtime di rendering del gioco.
//!
//! Porta `Launcher/Services/{MiiAvatarRenderService,MiiRuntimeSetupService}.cs`.
//! Sono due cose distinte che il launcher legacy teneva vicine:
//!
//! - il **runtime** è `FFLResHigh.dat`, estratto da un archivio Miitomo
//!   conservato su web.archive.org. Serve a **Dolphin**, non al launcher:
//!   senza, il gioco non disegna i Mii sincronizzati;
//! - l'**avatar** è l'immagine mostrata dal launcher accanto a un profilo. La
//!   produce il servizio immagini di Mii Studio, a cui si manda la stringa
//!   "studio data" del Mii.
//!
//! Il runtime pesa decine di megabyte e resta **opt-in** (§D-011). Gli avatar
//! invece si renderizzano da soli, come nel launcher legacy: sono la faccia del
//! Mii, e un editor che non la mostra non è un editor. Quando il render non
//! riesce resta la silhouette con l'iniziale, che è già il fallback del legacy
//! (§D-031).

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Voce dell'archivio Miitomo che contiene la risorsa di rendering.
const ARCHIVE_ENTRY: &str = "asset/model/character/mii/AFLResHigh_2_3.dat";
/// Nome con cui la Wii si aspetta la risorsa.
const RESOURCE_FILE: &str = "FFLResHigh.dat";
/// Sotto questa dimensione la risorsa è troncata e Dolphin non la accetta.
const MIN_RESOURCE_BYTES: u64 = 1024 * 1024;

/// Servizio immagini di Mii Studio e suo specchio.
const STUDIO_ENDPOINT: &str = "https://studio.mii.nintendo.com/miis/image.png";
const STUDIO_FALLBACK: &str = "https://mii-unsecure.ariankordi.net/miis/image.png";

/// Larghezza richiesta al renderer, la stessa del legacy.
const RENDER_WIDTH: u32 = 512;

/// Tentativi di render, come `MiiAvatarRenderService.MaxAttempts`.
const MAX_ATTEMPTS: usize = 3;

/// Inquadrature che il servizio di Mii Studio sa produrre.
///
/// Sono i due valori che il legacy passa nel parametro `type`: il ritratto e
/// la figura intera.
pub const RENDER_KINDS: [&str; 2] = ["face", "all_body"];

/// Firma di un file PNG.
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
/// Sotto questa dimensione la risposta non è un'immagine utile.
const MIN_PNG_BYTES: usize = 512;

/// Stato del rendering, per la pagina Mii & Licenses.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiiRendererStatus {
    /// `FFLResHigh.dat` presente e di dimensione plausibile.
    pub runtime_installed: bool,
    pub runtime_size_bytes: u64,
    pub cached_avatars: usize,
    /// Host che verrebbero contattati, per dirlo prima di contattarli.
    pub runtime_host: String,
    pub render_host: String,
    pub message: String,
}

fn runtime_dir(state: &Arc<AppState>) -> PathBuf {
    state.paths.mii_runtime_dir()
}

/// Percorso di `FFLResHigh.dat` dentro i dati del launcher.
pub fn resource_path(state: &Arc<AppState>) -> PathBuf {
    runtime_dir(state).join(RESOURCE_FILE)
}

/// `true` se il runtime è installato e di dimensione plausibile.
pub fn runtime_installed(state: &Arc<AppState>) -> bool {
    std::fs::metadata(resource_path(state)).is_ok_and(|meta| meta.len() >= MIN_RESOURCE_BYTES)
}

/// Host di un URL, per mostrarlo all'utente senza esporre l'indirizzo intero.
fn host_of(url: &str) -> String {
    url::Url::parse(url.trim())
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Stato corrente, senza toccare la rete.
pub async fn status(state: &Arc<AppState>) -> MiiRendererStatus {
    let installed = runtime_installed(state);
    let size = std::fs::metadata(resource_path(state)).map_or(0, |meta| meta.len());
    let archive_url = state
        .endpoints
        .read()
        .await
        .mii_rendering_archive_url
        .clone();

    let cached = std::fs::read_dir(state.paths.mii_avatars_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
                .count()
        })
        .unwrap_or(0);

    MiiRendererStatus {
        message: if installed {
            "The runtime is installed: synced Miis are drawn by the game.".into()
        } else {
            "Without the runtime, Dolphin shows synced Miis as empty silhouettes.".into()
        },
        runtime_installed: installed,
        runtime_size_bytes: size,
        cached_avatars: cached,
        runtime_host: host_of(&archive_url),
        render_host: host_of(STUDIO_ENDPOINT),
    }
}

// ---------------------------------------------------------------------------
// Runtime di rendering
// ---------------------------------------------------------------------------

/// Scarica e installa `FFLResHigh.dat`.
///
/// **Solo su richiesta esplicita dell'utente** (§D-011): l'archivio sta su
/// web.archive.org, è un asset di terze parti e pesa decine di megabyte.
pub async fn install_runtime(
    state: &Arc<AppState>,
    progress: ProgressSink,
) -> AppResult<MiiRendererStatus> {
    let guard = state.mod_operation.try_lock().map_err(|_| AppError::Busy)?;
    let cancel = state.renew_cancel_token().await;

    let result = install_runtime_inner(state, &progress, &cancel).await;
    drop(guard);

    if let Err(error) = &result {
        progress(ProgressUpdate::new(
            Phase::Error,
            vk_core::redact::redact(&error.to_string()),
        ));
    }
    result
}

async fn install_runtime_inner(
    state: &Arc<AppState>,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<MiiRendererStatus> {
    let url = state
        .endpoints
        .read()
        .await
        .mii_rendering_archive_url
        .trim()
        .to_string();
    if url.is_empty() {
        return Err(AppError::Configuration(
            "No URL configured for the rendering runtime.".into(),
        ));
    }

    let directory = runtime_dir(state);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    let archive = directory.join("mii-render-assets.zip.partial");

    progress(ProgressUpdate::new(
        Phase::Download,
        format!(
            "Downloading the rendering runtime from {}...",
            host_of(&url)
        ),
    ));

    let download = state
        .downloader
        .download_with_mirrors(std::slice::from_ref(&url), &archive, progress, cancel)
        .await;

    let result = async {
        download.map_err(|error| match error {
            vk_core::CoreError::Cancelled => AppError::Cancelled,
            other => AppError::Core(other),
        })?;

        progress(ProgressUpdate::new(
            Phase::Installing,
            "Extracting the rendering resource...",
        ));

        // Una sola voce serve: estrarre l'intero archivio Miitomo occuperebbe
        // centinaia di megabyte per poi buttarli.
        let source = archive.clone();
        let bytes =
            tokio::task::spawn_blocking(move || vk_core::zipx::read_entry(&source, ARCHIVE_ENTRY))
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?
                .map_err(AppError::Core)?;

        if (bytes.len() as u64) < MIN_RESOURCE_BYTES {
            return Err(AppError::BadRequest(format!(
                "The downloaded rendering resource is incomplete ({} bytes).",
                bytes.len()
            )));
        }

        vk_core::fsx::write_atomic(&resource_path(state), &bytes).await?;
        Ok(bytes.len())
    }
    .await;

    let _ = tokio::fs::remove_file(&archive).await;
    let size = result?;

    progress(ProgressUpdate::new(
        Phase::Completed,
        "Rendering runtime installed.",
    ));
    tracing::info!(size, "runtime di rendering Mii installato");

    Ok(status(state).await)
}

/// Rimuove il runtime scaricato.
pub async fn remove_runtime(state: &Arc<AppState>) -> AppResult<MiiRendererStatus> {
    let path = resource_path(state);
    if path.exists() {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|error| AppError::io(&path, error))?;
        tracing::info!("runtime di rendering Mii rimosso");
    }

    Ok(status(state).await)
}

/// Copia il runtime dentro la cartella `FaceLib` di Dolphin.
///
/// Il gioco cerca la risorsa con due nomi diversi a seconda della qualità
/// richiesta; il legacy ne scrive entrambi con lo stesso contenuto.
pub async fn install_runtime_into_dolphin(state: &Arc<AppState>) -> AppResult<usize> {
    if !runtime_installed(state) {
        return Ok(0);
    }

    let user_folder = state.settings.read().await.user_folder();
    if user_folder.as_os_str().is_empty() || !user_folder.is_dir() {
        return Ok(0);
    }

    let face_lib = user_folder
        .join("Wii")
        .join("shared2")
        .join("menu")
        .join("FaceLib");
    tokio::fs::create_dir_all(&face_lib)
        .await
        .map_err(|error| AppError::io(&face_lib, error))?;

    let source = resource_path(state);
    let expected = tokio::fs::metadata(&source)
        .await
        .map_err(|error| AppError::io(&source, error))?
        .len();

    let mut copied = 0;
    for name in [RESOURCE_FILE, "FFLRes.dat"] {
        let destination = face_lib.join(name);
        let same = tokio::fs::metadata(&destination)
            .await
            .is_ok_and(|meta| meta.len() == expected);
        if same {
            continue;
        }

        vk_core::fsx::copy_file(&source, &destination).await?;
        copied += 1;
    }

    if copied > 0 {
        tracing::info!(copied, "runtime di rendering copiato in FaceLib");
    }
    Ok(copied)
}

// ---------------------------------------------------------------------------
// Avatar
// ---------------------------------------------------------------------------

/// Chiave di cache di un render: SHA-256 di studio data, tipo e rotazione.
pub fn cache_key(studio_data: &str, kind: &str, rotation: i32) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(format!("{studio_data}_{kind}_{rotation}").as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cache_path(state: &Arc<AppState>, key: &str) -> PathBuf {
    state.paths.mii_avatars_dir().join(format!("{key}.png"))
}

/// URL del render, con i parametri del launcher legacy.
fn render_url(endpoint: &str, studio_data: &str, kind: &str, rotation: i32) -> String {
    let query = [
        ("data", studio_data.to_string()),
        ("type", kind.to_string()),
        ("expression", "normal".to_string()),
        ("width", RENDER_WIDTH.to_string()),
        ("bgColor", "FFFFFF00".to_string()),
        ("clothesColor", "default".to_string()),
        ("cameraXRotate", "0".to_string()),
        ("cameraYRotate", "0".to_string()),
        ("cameraZRotate", "0".to_string()),
        ("characterXRotate", "0".to_string()),
        ("characterYRotate", rotation.to_string()),
        ("characterZRotate", "0".to_string()),
        ("lightXDirection", "0".to_string()),
        ("lightYDirection", "0".to_string()),
        ("lightZDirection", "1".to_string()),
        ("instanceCount", "1".to_string()),
    ];

    let encoded: Vec<String> = query
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect();

    format!("{endpoint}?{}", encoded.join("&"))
}

/// Percent-encoding di un componente di query.
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// `Some(motivo)` quando i byte non sono un PNG utilizzabile.
fn png_problem(bytes: &[u8]) -> Option<String> {
    if bytes.len() < MIN_PNG_BYTES {
        return Some(format!(
            "the renderer returned an image that is too small ({} bytes)",
            bytes.len()
        ));
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Some("the renderer did not return a PNG image".into());
    }
    None
}

/// Normalizza l'inquadratura richiesta: tutto ciò che non conosciamo è `face`.
fn normalize_kind(kind: &str) -> &'static str {
    if kind == "all_body" {
        "all_body"
    } else {
        "face"
    }
}

/// Render di una "studio data" come `data:` URI, oppure `None`.
///
/// Porta `MiiAvatarRenderService.EnsureAvatarRenderAsync`: prima la cache su
/// disco, poi tre tentativi — Nintendo, lo specchio, di nuovo Nintendo —
/// esattamente come il legacy. Un fallimento non è un errore: la UI mostra la
/// silhouette con l'iniziale, che è lo stesso fallback del legacy.
pub async fn render_studio(
    state: &Arc<AppState>,
    studio_data: &str,
    kind: &str,
    rotation: i32,
) -> AppResult<Option<String>> {
    let studio_data = studio_data.trim();
    if studio_data.is_empty() {
        return Ok(None);
    }

    let kind = normalize_kind(kind);
    let key = cache_key(studio_data, kind, rotation);
    let path = cache_path(state, &key);

    if let Ok(bytes) = tokio::fs::read(&path).await {
        if png_problem(&bytes).is_none() {
            return Ok(Some(data_uri(&bytes)));
        }
        // Una cache corrotta si butta invece di restare lì per sempre.
        let _ = tokio::fs::remove_file(&path).await;
    }

    if !state.avatar_render_online {
        return Ok(None);
    }

    let directory = state.paths.mii_avatars_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    for attempt in 1..=MAX_ATTEMPTS {
        // Ordine del legacy: l'originale, lo specchio, di nuovo l'originale.
        let endpoint = if attempt == 2 {
            STUDIO_FALLBACK
        } else {
            STUDIO_ENDPOINT
        };

        let url = render_url(endpoint, studio_data, kind, rotation);
        match state.downloader.get_bytes(&url).await {
            Ok(bytes) => match png_problem(&bytes) {
                None => {
                    vk_core::fsx::write_atomic(&path, &bytes).await?;
                    tracing::info!(
                        host = %host_of(endpoint),
                        attempt,
                        kind,
                        "avatar Mii renderizzato"
                    );
                    return Ok(Some(data_uri(&bytes)));
                }
                Some(problem) => tracing::warn!(
                    host = %host_of(endpoint),
                    attempt,
                    %problem,
                    "render dell'avatar non utilizzabile"
                ),
            },
            Err(error) => tracing::warn!(
                host = %host_of(endpoint),
                attempt,
                error = %vk_core::redact::redact(&error.to_string()),
                "render dell'avatar non riuscito"
            ),
        }

        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64)).await;
        }
    }

    Ok(None)
}

/// Render di uno stato dell'editor, senza salvarlo da nessuna parte.
///
/// È l'anteprima dal vivo dell'editor e la miniatura di ogni opzione: il
/// legacy costruisce il blocco da 74 byte con `CreateMii(state)` e ne
/// renderizza la "studio data", che è esattamente quel che succede qui.
pub async fn render_editor_state(
    state: &Arc<AppState>,
    editor: &vk_save::mii::MiiEditorState,
    kind: &str,
    rotation: i32,
) -> AppResult<Option<String>> {
    let block = vk_save::mii::write_editor_state(editor);
    let studio_data = vk_save::mii::studio_data(&block);
    render_studio(state, &studio_data, kind, rotation).await
}

/// PNG come `data:` URI.
///
/// La webview non può caricare un percorso del filesystem — la CSP consente
/// `data:` ma non `file:` — e passare percorsi al frontend violerebbe §D-017.
fn data_uri(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        vk_save::mii::base64_encode(bytes)
    )
}

/// Svuota la cache degli avatar, restituendo quanti file ha rimosso.
pub async fn clear_cache(state: &Arc<AppState>) -> AppResult<usize> {
    let directory = state.paths.mii_avatars_dir();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(0);
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "png")
            && tokio::fs::remove_file(&path).await.is_ok()
        {
            removed += 1;
        }
    }

    tracing::info!(removed, "cache degli avatar Mii svuotata");
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::AppPaths;
    use std::path::Path;

    async fn state_at(dir: &Path) -> Arc<AppState> {
        AppState::bootstrap_isolated(AppPaths::at(dir.join("VanzaKart")))
            .await
            .unwrap()
    }

    /// I Mii vivono nel database di Dolphin: creane uno richiede la cartella.
    async fn seed_user_folder(dir: &Path, state: &Arc<AppState>) {
        let user = dir.join("Dolphin Emulator");
        std::fs::create_dir_all(&user).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();
    }

    fn fake_png(size: usize) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.resize(size, 0x42);
        bytes
    }

    #[tokio::test]
    async fn a_fresh_installation_has_no_runtime_and_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let status = status(&state).await;
        assert!(!status.runtime_installed);
        assert_eq!(status.cached_avatars, 0);
        assert!(status.message.contains("silhouettes"));
    }

    #[tokio::test]
    async fn the_status_names_the_hosts_it_would_contact() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let status = status(&state).await;
        assert_eq!(status.render_host, "studio.mii.nintendo.com");
        assert_eq!(status.runtime_host, "web.archive.org");
    }

    #[tokio::test]
    async fn a_truncated_runtime_does_not_count_as_installed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        std::fs::create_dir_all(runtime_dir(&state)).unwrap();
        std::fs::write(resource_path(&state), vec![0u8; 1024]).unwrap();

        assert!(!runtime_installed(&state));
        let status = status(&state).await;
        assert!(!status.runtime_installed);
        assert_eq!(status.runtime_size_bytes, 1024);
    }

    #[tokio::test]
    async fn the_runtime_is_copied_into_dolphins_face_lib() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(&user).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        std::fs::create_dir_all(runtime_dir(&state)).unwrap();
        std::fs::write(
            resource_path(&state),
            vec![7u8; MIN_RESOURCE_BYTES as usize],
        )
        .unwrap();

        assert_eq!(install_runtime_into_dolphin(&state).await.unwrap(), 2);

        let face_lib = user.join("Wii/shared2/menu/FaceLib");
        assert!(face_lib.join("FFLResHigh.dat").is_file());
        assert!(face_lib.join("FFLRes.dat").is_file());

        // Una seconda chiamata non riscrive quello che è già a posto.
        assert_eq!(install_runtime_into_dolphin(&state).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn without_the_runtime_nothing_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let user = dir.path().join("Dolphin Emulator");
        std::fs::create_dir_all(&user).unwrap();
        state.settings.write().await.user_folder_path = user.to_string_lossy().to_string();

        assert_eq!(install_runtime_into_dolphin(&state).await.unwrap(), 0);
        assert!(!user.join("Wii/shared2/menu/FaceLib").exists());
    }

    #[tokio::test]
    async fn removing_the_runtime_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        std::fs::create_dir_all(runtime_dir(&state)).unwrap();
        std::fs::write(
            resource_path(&state),
            vec![7u8; MIN_RESOURCE_BYTES as usize],
        )
        .unwrap();

        assert!(!remove_runtime(&state).await.unwrap().runtime_installed);
        assert!(!remove_runtime(&state).await.unwrap().runtime_installed);
    }

    #[tokio::test]
    async fn a_cached_avatar_is_served_without_touching_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let created = crate::services::mii::create(&state, "Vanza", 0, false)
            .await
            .unwrap();
        let mii = crate::services::mii::load(&state, &created.id)
            .await
            .unwrap();

        let key = cache_key(&mii.studio_data, "face", 0);
        std::fs::create_dir_all(state.paths.mii_avatars_dir()).unwrap();
        std::fs::write(cache_path(&state, &key), fake_png(2048)).unwrap();

        let avatar = render_studio(&state, &mii.studio_data, "face", 0)
            .await
            .unwrap()
            .expect("avatar dalla cache");
        assert!(avatar.starts_with("data:image/png;base64,"));
    }

    /// Lo stesso Mii renderizzato dall'editor e dalla lista condivide una sola
    /// voce di cache: riaprire l'editor non richiede un secondo render.
    #[tokio::test]
    async fn the_editor_and_the_mii_share_one_cache_entry() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let created = crate::services::mii::create(&state, "Vanza", 0, false)
            .await
            .unwrap();
        let mii = crate::services::mii::load(&state, &created.id)
            .await
            .unwrap();
        let editor = crate::services::mii::editor_state(&state, &created.id)
            .await
            .unwrap();

        let key = cache_key(&mii.studio_data, "face", 0);
        std::fs::create_dir_all(state.paths.mii_avatars_dir()).unwrap();
        std::fs::write(cache_path(&state, &key), fake_png(2048)).unwrap();

        assert!(render_editor_state(&state, &editor, "face", 0)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn a_corrupted_cache_entry_is_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;
        seed_user_folder(dir.path(), &state).await;

        let created = crate::services::mii::create(&state, "Vanza", 0, false)
            .await
            .unwrap();
        let mii = crate::services::mii::load(&state, &created.id)
            .await
            .unwrap();

        let key = cache_key(&mii.studio_data, "face", 0);
        std::fs::create_dir_all(state.paths.mii_avatars_dir()).unwrap();
        let path = cache_path(&state, &key);
        std::fs::write(&path, b"non un png").unwrap();

        // Lo stato dei test non contatta Mii Studio, ma il file rotto se ne va.
        assert!(render_studio(&state, &mii.studio_data, "face", 0)
            .await
            .unwrap()
            .is_none());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_mii_without_studio_data_asks_for_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        assert!(render_studio(&state, "   ", "face", 0)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn clearing_the_cache_counts_what_it_removed() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let directory = state.paths.mii_avatars_dir();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("a.png"), fake_png(1024)).unwrap();
        std::fs::write(directory.join("b.png"), fake_png(1024)).unwrap();
        std::fs::write(directory.join("note.txt"), b"non toccare").unwrap();

        assert_eq!(clear_cache(&state).await.unwrap(), 2);
        assert!(directory.join("note.txt").is_file());
        assert_eq!(clear_cache(&state).await.unwrap(), 0);
    }

    #[test]
    fn the_cache_key_depends_on_every_input() {
        let base = cache_key("abcdef", "face", 0);

        assert_eq!(base.len(), 64);
        assert_eq!(base, cache_key("abcdef", "face", 0));
        assert_ne!(base, cache_key("abcdee", "face", 0));
        assert_ne!(base, cache_key("abcdef", "body", 0));
        assert_ne!(base, cache_key("abcdef", "face", 90));
    }

    #[test]
    fn the_render_url_carries_the_studio_data_encoded() {
        let url = render_url(STUDIO_ENDPOINT, "00aabb", "face", 45);

        assert!(url.starts_with(STUDIO_ENDPOINT));
        assert!(url.contains("data=00aabb"));
        assert!(url.contains("type=face"));
        assert!(url.contains("characterYRotate=45"));
        assert!(url.contains(&format!("width={RENDER_WIDTH}")));
        // Il carattere `#` in un parametro troncherebbe la query.
        assert!(render_url(STUDIO_ENDPOINT, "a#b", "face", 0).contains("data=a%23b"));
    }

    #[test]
    fn the_body_shot_is_a_different_url_and_a_different_cache_entry() {
        assert!(render_url(STUDIO_ENDPOINT, "00aabb", "all_body", 0).contains("type=all_body"));
        assert_ne!(
            cache_key("00aabb", "face", 0),
            cache_key("00aabb", "all_body", 0)
        );
    }

    #[test]
    fn an_unknown_shot_falls_back_to_the_portrait() {
        assert_eq!(normalize_kind("face"), "face");
        assert_eq!(normalize_kind("all_body"), "all_body");
        assert_eq!(normalize_kind("qualsiasi-cosa"), "face");
    }

    #[test]
    fn only_real_png_payloads_are_accepted() {
        assert!(png_problem(&fake_png(1024)).is_none());
        assert!(png_problem(&fake_png(100)).is_some(), "troppo piccola");
        assert!(png_problem(&vec![0x42; 2048]).is_some(), "firma sbagliata");
        assert!(png_problem(&[]).is_some());
    }

    #[test]
    fn a_data_uri_round_trips_the_bytes() {
        let bytes = fake_png(600);
        let uri = data_uri(&bytes);

        let encoded = uri
            .strip_prefix("data:image/png;base64,")
            .expect("prefisso");
        assert_eq!(vk_save::mii::base64_decode(encoded).unwrap(), bytes);
    }

    #[test]
    fn hosts_are_extracted_without_the_rest_of_the_url() {
        assert_eq!(host_of("https://a.example/b/c?d=e"), "a.example");
        assert_eq!(host_of("non-un-url"), "");
        assert_eq!(host_of(""), "");
    }
}
