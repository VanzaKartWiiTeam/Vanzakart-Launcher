//! Addon locali dentro `My Stuff`.
//!
//! Porta `Launcher/Services/{AddonManagerService,ModConflictService}.cs`.
//!
//! Il layout su disco è **condiviso con il launcher legacy** e va rispettato
//! alla lettera, altrimenti i due launcher non si vedono gli addon a vicenda:
//!
//! ```text
//! <mod_folder>/<Mod>_UserData/Addons/
//!   <id>/addon.json     metadati dell'addon
//!   <id>/payload/…      i file, nella stessa struttura di My Stuff
//!   <id>/displaced/…    i file di My Stuff sovrascritti all'attivazione
//! ```
//!
//! `displaced` è ciò che rende reversibile l'attivazione: disattivare un
//! addon rimette al loro posto i file che aveva coperto.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::{AddonView, ConflictView};
use crate::error::{AppError, AppResult};

/// Estensioni considerate nella ricerca dei conflitti.
const ADDON_EXTENSIONS: &[&str] = &[".szs", ".brres", ".tpl", ".png", ".json", ".xml", ".ini"];

/// Nome del manifest di un addon, uno per cartella.
pub const MANIFEST_NAME: &str = "addon.json";

/// Identificatore dell'addon ufficiale del music pack.
pub const OFFICIAL_MUSIC_PACK_ID: &str = "official-vanzakart-music-pack";

/// `<mod_folder>/<Mod>_UserData/Addons`.
pub fn library_dir(layout: &vk_core::ModLayout) -> PathBuf {
    layout.user_data_root().join("Addons")
}

/// Cartella di un singolo addon.
pub fn addon_dir(layout: &vk_core::ModLayout, id: &str) -> PathBuf {
    library_dir(layout).join(id)
}

/// Manifest di un singolo addon.
pub fn manifest_path(layout: &vk_core::ModLayout, id: &str) -> PathBuf {
    addon_dir(layout, id).join(MANIFEST_NAME)
}

/// Payload gestito di un addon: la copia autorevole dei suoi file.
pub fn payload_dir(layout: &vk_core::ModLayout, id: &str) -> PathBuf {
    addon_dir(layout, id).join("payload")
}

/// File di `My Stuff` messi da parte quando l'addon è stato attivato.
pub fn displaced_dir(layout: &vk_core::ModLayout, id: &str) -> PathBuf {
    addon_dir(layout, id).join("displaced")
}

/// Metadati di un addon, nello stesso formato JSON del launcher legacy.
///
/// I nomi dei campi sono in PascalCase perché è così che li serializza
/// `System.Text.Json` con le impostazioni predefinite del legacy.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AddonManifest {
    #[serde(rename = "Id", alias = "id")]
    pub id: String,
    #[serde(rename = "Name", alias = "name")]
    pub name: String,
    #[serde(rename = "Author", alias = "author")]
    pub author: String,
    #[serde(rename = "Source", alias = "source")]
    pub source: String,
    #[serde(rename = "SourceUrl", alias = "sourceUrl")]
    pub source_url: String,
    #[serde(rename = "PreviewUrl", alias = "previewUrl")]
    pub preview_url: String,
    #[serde(rename = "InstalledUtc", alias = "installedUtc")]
    pub installed_utc: String,
    #[serde(rename = "Files", alias = "files")]
    pub files: Vec<String>,
    #[serde(rename = "DisplacedFiles", alias = "displacedFiles")]
    pub displaced_files: Vec<String>,
    #[serde(rename = "IsManaged", alias = "isManaged")]
    pub is_managed: bool,
    #[serde(rename = "IsEnabled", alias = "isEnabled")]
    pub is_enabled: bool,
}

impl AddonManifest {
    fn to_view(&self) -> AddonView {
        AddonView {
            id: self.id.clone(),
            name: if self.name.trim().is_empty() {
                self.id.clone()
            } else {
                self.name.clone()
            },
            author: self.author.clone(),
            source: if self.source.trim().is_empty() {
                "Local".into()
            } else {
                self.source.clone()
            },
            source_url: self.source_url.clone(),
            preview_url: self.preview_url.clone(),
            installed_utc: self.installed_utc.clone(),
            file_count: self.files.len(),
            enabled: self.is_enabled,
            managed: self.is_managed,
        }
    }
}

/// Elenca gli addon installati leggendo un `addon.json` per cartella.
///
/// Un manifest illeggibile viene saltato: un addon corrotto non deve
/// impedire di vedere gli altri.
pub async fn list(layout: &vk_core::ModLayout) -> Vec<AddonView> {
    let library = library_dir(layout);
    let Ok(entries) = std::fs::read_dir(&library) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }

        let manifest = entry.path().join(MANIFEST_NAME);
        match vk_core::fsx::read_text_opt(&manifest).await {
            Ok(Some(raw)) => match serde_json::from_str::<AddonManifest>(&raw) {
                Ok(addon) => out.push(addon.to_view()),
                Err(error) => tracing::warn!(
                    addon = %entry.file_name().to_string_lossy(),
                    %error,
                    "manifest dell'addon illeggibile: saltato"
                ),
            },
            _ => continue,
        }
    }

    out.sort_by_key(|addon| addon.name.to_lowercase());
    out
}

/// Legge il manifest di un addon.
pub async fn read_manifest(layout: &vk_core::ModLayout, id: &str) -> AppResult<AddonManifest> {
    let path = manifest_path(layout, id);
    let raw = vk_core::fsx::read_text_opt(&path)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("unknown addon: {id}")))?;

    serde_json::from_str(&raw)
        .map_err(|error| AppError::Storage(format!("invalid manifest for {id}: {error}")))
}

/// Scrive il manifest di un addon in modo atomico.
pub async fn write_manifest(
    layout: &vk_core::ModLayout,
    manifest: &AddonManifest,
) -> AppResult<()> {
    vk_core::fsx::write_json_atomic(&manifest_path(layout, &manifest.id), manifest).await?;
    Ok(())
}

/// Estensioni di archivio che il launcher sa aprire.
///
/// GameBanana pubblica anche `.rar` e `.7z`, che il crate `zip` non legge: un
/// file così va rifiutato prima del download, non dopo.
pub const SUPPORTED_ARCHIVES: &[&str] = &[".zip"];

/// `true` se il nome di file ha un'estensione di archivio supportata.
pub fn is_supported_archive(file_name: &str) -> bool {
    let lowercase = file_name.trim().to_lowercase();
    SUPPORTED_ARCHIVES
        .iter()
        .any(|extension| lowercase.ends_with(extension))
}

/// Provenienza di un addon importato, oltre all'archivio stesso.
#[derive(Debug, Clone, Default)]
pub struct ImportRequest {
    /// Identificatore sul disco. Deve essere già sicuro come nome di cartella.
    pub id: String,
    pub name: String,
    pub author: String,
    pub source: String,
    pub source_url: String,
    pub preview_url: String,
    /// Sostituisce un addon con lo stesso identificatore invece di rifiutare.
    pub replace_existing: bool,
}

/// Importa un archivio scelto dall'utente come addon locale.
pub async fn import_archive(
    layout: &vk_core::ModLayout,
    archive: &Path,
    name: &str,
) -> AppResult<AddonView> {
    let id = slug(name);
    if id.is_empty() {
        return Err(AppError::BadRequest("invalid addon name".into()));
    }

    import_archive_as(
        layout,
        archive,
        ImportRequest {
            id,
            name: name.trim().to_string(),
            source: "Local".into(),
            ..Default::default()
        },
    )
    .await
}

/// Importa un archivio come addon gestito, con provenienza esplicita.
///
/// L'archivio viene estratto nel **payload**, non direttamente in `My Stuff`:
/// così l'addon resta disinstallabile e i suoi file restano tracciati.
/// L'estrazione passa da `vk_core::zipx::extract_safe`, quindi un archivio
/// malevolo non può uscire dalla cartella di destinazione.
pub async fn import_archive_as(
    layout: &vk_core::ModLayout,
    archive: &Path,
    request: ImportRequest,
) -> AppResult<AddonView> {
    if !archive.is_file() {
        return Err(AppError::BadRequest("the file does not exist".into()));
    }

    let id = request.id.trim().to_string();
    if id.is_empty() || id != slug(&id) {
        return Err(AppError::BadRequest(format!(
            "invalid addon identifier: '{id}'"
        )));
    }

    if manifest_path(layout, &id).exists() {
        if !request.replace_existing {
            return Err(AppError::BadRequest(format!(
                "an addon with identifier '{id}' already exists"
            )));
        }
        remove(layout, &id).await?;
    }

    // L'estrazione va su una directory temporanea: solo dopo aver individuato
    // la radice del payload si sa quali file vanno tenuti e a che percorso.
    let staging = addon_dir(layout, &id).join(".import");
    let _ = tokio::fs::remove_dir_all(&staging).await;
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|error| AppError::io(&staging, error))?;

    let source_archive = archive.to_path_buf();
    let target = staging.clone();
    let extraction = tokio::task::spawn_blocking(move || {
        vk_core::zipx::extract_safe(
            &source_archive,
            &target,
            &vk_core::zipx::ExtractOptions {
                skip_identical: false,
                ..Default::default()
            },
            &vk_core::progress::noop_sink(),
            &vk_core::progress::CancelToken::new(),
        )
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()));

    let extraction = match extraction {
        Ok(Ok(report)) => report,
        Ok(Err(error)) => {
            let _ = tokio::fs::remove_dir_all(addon_dir(layout, &id)).await;
            return Err(AppError::Core(error));
        }
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(addon_dir(layout, &id)).await;
            return Err(error);
        }
    };

    let payload = payload_dir(layout, &id);
    let files = match collect_payload(&staging, &payload).await {
        Ok(files) if !files.is_empty() => files,
        result => {
            let _ = tokio::fs::remove_dir_all(addon_dir(layout, &id)).await;
            result?;
            return Err(AppError::BadRequest(
                "the archive holds no usable file".into(),
            ));
        }
    };

    let _ = tokio::fs::remove_dir_all(&staging).await;
    tracing::debug!(
        entries = extraction.entry_paths.len(),
        kept = files.len(),
        "payload dell'addon estratto"
    );

    let manifest = AddonManifest {
        id: id.clone(),
        name: if request.name.trim().is_empty() {
            id.clone()
        } else {
            request.name.trim().to_string()
        },
        author: request.author,
        source: if request.source.trim().is_empty() {
            "Local".into()
        } else {
            request.source
        },
        source_url: request.source_url,
        preview_url: request.preview_url,
        installed_utc: crate::state::now_iso(),
        files,
        is_managed: true,
        is_enabled: false,
        ..Default::default()
    };
    write_manifest(layout, &manifest).await?;

    tracing::info!(addon = %id, files = manifest.files.len(), "addon importato");
    Ok(manifest.to_view())
}

/// Sposta i file estratti nel payload, restituendo i percorsi relativi.
///
/// Due normalizzazioni, entrambe portate dal legacy:
///
/// - se l'archivio contiene una cartella `My Stuff`, è **quella** la radice:
///   senza questo passaggio i file finirebbero in `My Stuff/My Stuff/...` e il
///   gioco non li vedrebbe;
/// - `__MACOSX` e `.DS_Store` vengono scartati: sono residui degli archivi
///   creati su macOS, non file dell'addon.
async fn collect_payload(staging: &Path, payload: &Path) -> AppResult<Vec<String>> {
    let root = find_payload_root(staging);
    let mut files = Vec::new();

    for source in vk_core::fsx::list_files_recursive(&root) {
        let relative = vk_core::fsx::relative_slash(&root, &source);
        if relative.is_empty() || is_archive_noise(&relative) {
            continue;
        }

        let destination = safe_join(payload, &relative)?;
        vk_core::fsx::copy_file(&source, &destination).await?;
        files.push(relative);
    }

    files.sort();
    Ok(files)
}

/// Radice del payload dentro l'archivio estratto.
///
/// È la cartella `My Stuff` meno profonda, se c'è; altrimenti la radice
/// dell'estrazione. Porta `FindPayloadRoot`.
fn find_payload_root(extracted: &Path) -> PathBuf {
    let mut best: Option<(usize, PathBuf)> = None;
    let mut queue = vec![(0usize, extracted.to_path_buf())];

    while let Some((depth, directory)) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }

            let path = entry.path();
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("My Stuff")
                && best
                    .as_ref()
                    .is_none_or(|(best_depth, _)| depth < *best_depth)
            {
                best = Some((depth, path.clone()));
            }
            queue.push((depth + 1, path));
        }
    }

    best.map_or_else(|| extracted.to_path_buf(), |(_, path)| path)
}

/// `true` per i residui degli archivi creati su macOS.
fn is_archive_noise(relative: &str) -> bool {
    relative
        .split('/')
        .any(|part| part.eq_ignore_ascii_case("__MACOSX"))
        || relative
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(".DS_Store"))
}

/// Attiva o disattiva un addon.
///
/// All'attivazione i file di `My Stuff` che verrebbero coperti finiscono in
/// `displaced/`; alla disattivazione tornano al loro posto. È ciò che rende
/// l'operazione reversibile senza perdere nulla.
pub async fn set_enabled(
    layout: &vk_core::ModLayout,
    id: &str,
    enabled: bool,
) -> AppResult<AddonView> {
    let mut manifest = read_manifest(layout, id).await?;

    if !manifest.is_managed {
        return Err(AppError::BadRequest(
            "this addon is not managed by the launcher: remove it by hand from My Stuff".into(),
        ));
    }
    if manifest.is_enabled == enabled {
        return Ok(manifest.to_view());
    }

    let my_stuff = layout.my_stuff();
    let payload = payload_dir(layout, id);
    let displaced = displaced_dir(layout, id);

    if enabled {
        // Un file già fornito da un altro addon attivo è un conflitto vero:
        // meglio fermarsi che sovrascrivere silenziosamente.
        if let Some(collision) = first_collision(layout, id, &manifest.files).await {
            return Err(AppError::BadRequest(format!(
                "'{collision}' is already provided by another active addon. Turn that one off first."
            )));
        }

        let mut displaced_files = Vec::new();
        for relative in &manifest.files {
            let source = safe_join(&payload, relative)?;
            let destination = safe_join(&my_stuff, relative)?;

            if destination.is_file() {
                let aside = safe_join(&displaced, relative)?;
                vk_core::fsx::copy_file(&destination, &aside).await?;
                displaced_files.push(relative.clone());
            }
            vk_core::fsx::copy_file(&source, &destination).await?;
        }

        manifest.displaced_files = displaced_files;
        manifest.is_enabled = true;
    } else {
        for relative in &manifest.files {
            let target = safe_join(&my_stuff, relative)?;
            let _ = tokio::fs::remove_file(&target).await;
        }

        for relative in &manifest.displaced_files {
            let aside = safe_join(&displaced, relative)?;
            if aside.is_file() {
                vk_core::fsx::copy_file(&aside, &safe_join(&my_stuff, relative)?).await?;
                let _ = tokio::fs::remove_file(&aside).await;
            }
        }

        manifest.displaced_files.clear();
        manifest.is_enabled = false;
    }

    write_manifest(layout, &manifest).await?;
    tracing::info!(addon = %id, enabled, "stato dell'addon aggiornato");
    Ok(manifest.to_view())
}

/// Rimuove un addon.
///
/// Se era attivo viene prima disattivato, così i file che aveva coperto
/// tornano al loro posto invece di sparire con lui.
pub async fn remove(layout: &vk_core::ModLayout, id: &str) -> AppResult<()> {
    let manifest = read_manifest(layout, id).await?;
    if manifest.is_enabled {
        set_enabled(layout, id, false).await?;
    }

    let directory = addon_dir(layout, id);
    tokio::fs::remove_dir_all(&directory)
        .await
        .map_err(|error| AppError::io(&directory, error))?;

    tracing::info!(addon = %id, "addon rimosso");
    Ok(())
}

/// Primo file già fornito da un altro addon attivo, se esiste.
async fn first_collision(
    layout: &vk_core::ModLayout,
    id: &str,
    files: &[String],
) -> Option<String> {
    for other in list(layout).await {
        if other.id.eq_ignore_ascii_case(id) || !other.enabled || !other.managed {
            continue;
        }
        let Ok(manifest) = read_manifest(layout, &other.id).await else {
            continue;
        };
        if let Some(collision) = files.iter().find(|file| {
            manifest
                .files
                .iter()
                .any(|owned| owned.eq_ignore_ascii_case(file))
        }) {
            return Some(collision.clone());
        }
    }
    None
}

/// Unisce un percorso relativo a una radice, rifiutando ogni fuga.
pub(crate) fn safe_join(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let sanitized = vk_core::zipx::sanitize_entry_path(relative)?;
    let joined = root.join(sanitized);
    vk_core::zipx::ensure_within(root, &joined)?;
    Ok(joined)
}

/// File con lo stesso nome in più punti dell'albero degli addon.
///
/// Riivolution applica l'ultimo file trovato: due addon che forniscono lo
/// stesso nome si sovrascrivono a vicenda.
pub fn scan_conflicts(addon_folder: &Path) -> Vec<ConflictView> {
    if !addon_folder.is_dir() {
        return Vec::new();
    }

    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for path in vk_core::fsx::list_files_recursive(addon_folder) {
        let Some(file_name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };

        let has_relevant_extension = ADDON_EXTENSIONS.iter().any(|extension| {
            file_name
                .to_ascii_lowercase()
                .ends_with(&extension.to_ascii_lowercase())
        });
        if !has_relevant_extension {
            continue;
        }

        by_name
            .entry(file_name.to_lowercase())
            .or_default()
            .push(vk_core::fsx::relative_slash(addon_folder, &path));
    }

    let mut conflicts: Vec<ConflictView> = by_name
        .into_iter()
        .filter(|(_, locations)| locations.len() > 1)
        .map(|(file_name, locations)| ConflictView {
            file_name,
            count: locations.len(),
            locations,
        })
        .collect();

    conflicts.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    conflicts
}

/// Identificatore stabile e sicuro come nome di file.
pub fn slug(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    cleaned
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use vk_core::Channel;

    fn write(path: &Path, body: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn layout(root: &Path) -> vk_core::ModLayout {
        vk_core::ModLayout::new(root.join("riiv"), Channel::Stable)
    }

    /// Archivio ZIP con i file indicati.
    fn build_archive(path: &Path, files: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, body) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn the_layout_matches_the_legacy_one() {
        let layout = vk_core::ModLayout::new("/riiv", Channel::Stable);

        assert_eq!(
            library_dir(&layout),
            PathBuf::from("/riiv/VanzaKart_UserData/Addons")
        );
        assert_eq!(
            manifest_path(&layout, "test"),
            PathBuf::from("/riiv/VanzaKart_UserData/Addons/test/addon.json")
        );
        assert_eq!(
            payload_dir(&layout, "test"),
            PathBuf::from("/riiv/VanzaKart_UserData/Addons/test/payload")
        );
    }

    #[tokio::test]
    async fn a_manifest_written_by_the_legacy_launcher_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        // PascalCase, come lo serializza System.Text.Json.
        write(
            &manifest_path(&layout, "kart"),
            br#"{"Id":"kart","Name":"Kart custom","Author":"tizio","Source":"GameBanana",
                 "Files":["a.szs","b.szs"],"DisplacedFiles":[],"IsManaged":true,"IsEnabled":true}"#,
        );

        let addons = list(&layout).await;
        assert_eq!(addons.len(), 1);
        assert_eq!(addons[0].name, "Kart custom");
        assert_eq!(addons[0].author, "tizio");
        assert_eq!(addons[0].file_count, 2);
        assert!(addons[0].enabled);
        assert!(addons[0].managed);
    }

    #[tokio::test]
    async fn a_corrupt_manifest_does_not_hide_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        write(&manifest_path(&layout, "rotto"), b"{ non json");
        write(
            &manifest_path(&layout, "buono"),
            br#"{"Id":"buono","Name":"Buono","IsManaged":true}"#,
        );

        let addons = list(&layout).await;
        assert_eq!(addons.len(), 1);
        assert_eq!(addons[0].id, "buono");
    }

    #[tokio::test]
    async fn importing_writes_the_payload_and_leaves_the_addon_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("kart/custom.szs", b"contenuto")]);

        let addon = import_archive(&layout, &archive, "Il Mio Kart")
            .await
            .unwrap();

        assert_eq!(addon.id, "il-mio-kart");
        assert_eq!(addon.file_count, 1);
        assert!(!addon.enabled, "l'import non attiva l'addon");

        assert!(payload_dir(&layout, "il-mio-kart")
            .join("kart/custom.szs")
            .is_file());
        // My Stuff non è stata toccata.
        assert!(!layout.my_stuff().join("kart/custom.szs").exists());
    }

    #[tokio::test]
    async fn enabling_and_disabling_restores_the_displaced_file() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        // Un file personale già presente in My Stuff.
        write(
            &layout.my_stuff().join("kart/custom.szs"),
            b"file-personale",
        );

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("kart/custom.szs", b"file-addon")]);
        import_archive(&layout, &archive, "Kart").await.unwrap();

        // Attivazione: il file personale viene messo da parte, non perso.
        set_enabled(&layout, "kart", true).await.unwrap();
        assert_eq!(
            std::fs::read(layout.my_stuff().join("kart/custom.szs")).unwrap(),
            b"file-addon"
        );
        assert!(displaced_dir(&layout, "kart")
            .join("kart/custom.szs")
            .is_file());

        // Disattivazione: torna esattamente com'era.
        set_enabled(&layout, "kart", false).await.unwrap();
        assert_eq!(
            std::fs::read(layout.my_stuff().join("kart/custom.szs")).unwrap(),
            b"file-personale"
        );
    }

    #[tokio::test]
    async fn disabling_removes_files_that_displaced_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("kart/nuovo.szs", b"x")]);
        import_archive(&layout, &archive, "Kart").await.unwrap();

        set_enabled(&layout, "kart", true).await.unwrap();
        assert!(layout.my_stuff().join("kart/nuovo.szs").is_file());

        set_enabled(&layout, "kart", false).await.unwrap();
        assert!(!layout.my_stuff().join("kart/nuovo.szs").exists());
    }

    #[tokio::test]
    async fn two_addons_cannot_own_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        for name in ["Primo", "Secondo"] {
            let archive = dir.path().join(format!("{name}.zip"));
            build_archive(&archive, &[("kart/comune.szs", b"x")]);
            import_archive(&layout, &archive, name).await.unwrap();
        }

        set_enabled(&layout, "primo", true).await.unwrap();

        let error = set_enabled(&layout, "secondo", true).await.unwrap_err();
        assert_eq!(error.code(), "bad-request");
        assert!(error.to_string().contains("comune.szs"));
    }

    #[tokio::test]
    async fn removing_an_enabled_addon_restores_what_it_covered() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        write(
            &layout.my_stuff().join("kart/custom.szs"),
            b"file-personale",
        );

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("kart/custom.szs", b"file-addon")]);
        import_archive(&layout, &archive, "Kart").await.unwrap();
        set_enabled(&layout, "kart", true).await.unwrap();

        remove(&layout, "kart").await.unwrap();

        assert_eq!(
            std::fs::read(layout.my_stuff().join("kart/custom.szs")).unwrap(),
            b"file-personale"
        );
        assert!(!addon_dir(&layout, "kart").exists());
        assert!(list(&layout).await.is_empty());
    }

    #[tokio::test]
    async fn an_archive_wrapping_my_stuff_is_unwrapped() {
        // Gli archivi pubblicati contengono quasi sempre la cartella
        // `My Stuff` al loro interno. Senza questo passaggio i file
        // finirebbero in `My Stuff/My Stuff/...` e il gioco non li vedrebbe.
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("pack.zip");
        build_archive(
            &archive,
            &[
                ("VanzaKart Pack/My Stuff/Music/song.brstm", b"nota"),
                ("VanzaKart Pack/leggimi.txt", b"ignorato"),
            ],
        );

        let addon = import_archive(&layout, &archive, "Pack").await.unwrap();

        assert_eq!(addon.file_count, 1);
        assert!(payload_dir(&layout, "pack")
            .join("Music/song.brstm")
            .is_file());
        assert!(
            !payload_dir(&layout, "pack").join("leggimi.txt").exists(),
            "fuori da My Stuff non fa parte del payload"
        );
    }

    #[tokio::test]
    async fn macos_archive_noise_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("mac.zip");
        build_archive(
            &archive,
            &[
                ("kart/custom.szs", b"contenuto"),
                ("__MACOSX/kart/._custom.szs", b"spazzatura"),
                ("kart/.DS_Store", b"spazzatura"),
            ],
        );

        let addon = import_archive(&layout, &archive, "Mac").await.unwrap();

        assert_eq!(addon.file_count, 1);
        assert!(!payload_dir(&layout, "mac").join("__MACOSX").exists());
        assert!(!payload_dir(&layout, "mac").join("kart/.DS_Store").exists());
    }

    #[tokio::test]
    async fn the_import_staging_directory_is_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("a.szs", b"x")]);
        import_archive(&layout, &archive, "Kart").await.unwrap();

        assert!(!addon_dir(&layout, "kart").join(".import").exists());
    }

    #[tokio::test]
    async fn an_explicit_identifier_can_replace_an_existing_addon() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let first = dir.path().join("uno.zip");
        build_archive(&first, &[("a.szs", b"vecchio")]);
        let second = dir.path().join("due.zip");
        build_archive(&second, &[("b.szs", b"nuovo")]);

        let request = |replace: bool| ImportRequest {
            id: "pacchetto-ufficiale".into(),
            name: "Pacchetto ufficiale".into(),
            source: "Official".into(),
            replace_existing: replace,
            ..Default::default()
        };

        import_archive_as(&layout, &first, request(false))
            .await
            .unwrap();

        // Senza il permesso esplicito, un secondo import è un errore.
        assert!(import_archive_as(&layout, &first, request(false))
            .await
            .is_err());

        let replaced = import_archive_as(&layout, &second, request(true))
            .await
            .unwrap();

        assert_eq!(replaced.id, "pacchetto-ufficiale");
        assert_eq!(replaced.source, "Official");
        assert!(payload_dir(&layout, "pacchetto-ufficiale")
            .join("b.szs")
            .is_file());
        assert!(
            !payload_dir(&layout, "pacchetto-ufficiale")
                .join("a.szs")
                .exists(),
            "il payload precedente non è stato rimosso"
        );
        assert_eq!(list(&layout).await.len(), 1);
    }

    #[tokio::test]
    async fn an_unsafe_identifier_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("a.szs", b"x")]);

        for hostile in ["../fuga", "a/b", "", "   ", "Maiuscole"] {
            let request = ImportRequest {
                id: hostile.into(),
                name: "x".into(),
                ..Default::default()
            };
            assert!(
                import_archive_as(&layout, &archive, request).await.is_err(),
                "id accettato: {hostile}"
            );
        }
    }

    #[test]
    fn the_payload_root_is_the_shallowest_my_stuff() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Nessuna `My Stuff`: la radice resta quella dell'estrazione.
        std::fs::create_dir_all(root.join("kart")).unwrap();
        assert_eq!(find_payload_root(root), root);

        std::fs::create_dir_all(root.join("pack/altro/My Stuff")).unwrap();
        std::fs::create_dir_all(root.join("pack/My Stuff")).unwrap();
        assert_eq!(find_payload_root(root), root.join("pack/My Stuff"));
    }

    #[test]
    fn only_zip_archives_are_accepted() {
        assert!(is_supported_archive("mod.zip"));
        assert!(is_supported_archive("MOD.ZIP"));
        assert!(is_supported_archive("  spazi.zip  "));
        assert!(!is_supported_archive("mod.rar"));
        assert!(!is_supported_archive("mod.7z"));
        assert!(!is_supported_archive("mod"));
    }

    #[test]
    fn archive_noise_is_recognised_anywhere_in_the_path() {
        assert!(is_archive_noise("__MACOSX/a/b.szs"));
        assert!(is_archive_noise("a/__macosx/b.szs"));
        assert!(is_archive_noise("a/b/.DS_Store"));
        assert!(!is_archive_noise("a/b.szs"));
        assert!(!is_archive_noise("MACOSX.szs"));
    }

    #[tokio::test]
    async fn importing_twice_with_the_same_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("addon.zip");
        build_archive(&archive, &[("a.szs", b"x")]);

        import_archive(&layout, &archive, "Kart").await.unwrap();
        let error = import_archive(&layout, &archive, "Kart").await.unwrap_err();
        assert_eq!(error.code(), "bad-request");
    }

    #[tokio::test]
    async fn an_empty_archive_is_refused_without_leaving_traces() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let archive = dir.path().join("vuoto.zip");
        build_archive(&archive, &[]);

        assert!(import_archive(&layout, &archive, "Vuoto").await.is_err());
        assert!(!addon_dir(&layout, "vuoto").exists());
    }

    #[tokio::test]
    async fn importing_a_missing_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());

        let error = import_archive(&layout, &dir.path().join("assente.zip"), "x")
            .await
            .unwrap_err();
        assert_eq!(error.code(), "bad-request");
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let root = Path::new("/riiv/My Stuff");
        assert!(safe_join(root, "a/b.szs").is_ok());
        assert!(safe_join(root, "../fuga.szs").is_err());
        assert!(safe_join(root, "/etc/passwd").is_err());
    }

    #[test]
    fn conflicts_are_detected_across_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        write(&root.join("addonA/kart.szs"), b"a");
        write(&root.join("addonB/kart.szs"), b"b");
        write(&root.join("addonC/kart.szs"), b"c");
        write(&root.join("addonA/unico.brres"), b"x");
        write(&root.join("note.txt"), b"non rilevante");

        let conflicts = scan_conflicts(root);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].file_name, "kart.szs");
        assert_eq!(conflicts[0].count, 3);
    }

    #[test]
    fn conflicts_ignore_irrelevant_extensions() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a/appunti.txt"), b"x");
        write(&dir.path().join("b/appunti.txt"), b"y");

        assert!(scan_conflicts(dir.path()).is_empty());
    }

    #[test]
    fn a_missing_folder_has_no_conflicts() {
        assert!(scan_conflicts(Path::new("/percorso/inesistente")).is_empty());
    }

    #[test]
    fn slugs_are_filesystem_safe() {
        assert_eq!(slug("Il Mio Addon!"), "il-mio-addon");
        assert_eq!(slug("  ../fuga  "), "fuga");
        assert_eq!(slug("a/b\\c"), "a-b-c");
        assert!(slug("   ").is_empty());
        assert!(!slug("Addon").contains(std::path::MAIN_SEPARATOR));
    }
}
