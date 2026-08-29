//! Sfoglia e installa le mod di GameBanana.
//!
//! Porta `Launcher/Services/GameBananaService.cs` e il flusso di
//! `GameBananaFilePickerDialog` + `AddonDownloadDialog`.
//!
//! Due vincoli guidano la forma di questo modulo:
//!
//! - **L'URL di download non attraversa mai l'IPC.** Il frontend passa
//!   l'identificativo della mod e del file; è il backend a rileggere l'URL
//!   dall'API e a validarlo. Un frontend compromesso non può far scaricare al
//!   launcher un file da un host qualsiasi (`docs/decisions.md` §D-005).
//! - **Allowlist di host stretta**: `gamebanana.com` e `files.gamebanana.com`,
//!   niente sottodomini a caso. È l'unico punto del launcher che scarica da un
//!   host di terze parti sulla base di dati forniti da terze parti.

use std::collections::BTreeSet;
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};

use serde::Serialize;
use serde_json::Value;
use vk_core::progress::{CancelToken, Phase, ProgressSink, ProgressUpdate};

use crate::domain::AddonView;
use crate::error::{AppError, AppResult};
use crate::services::addons::{self, ImportRequest};
use crate::state::AppState;

/// Identificativo di Mario Kart Wii su GameBanana.
pub const GAME_ID: i64 = 5896;

const API_ROOT: &str = "https://gamebanana.com/apiv11";
const PER_PAGE: usize = 30;
const CATALOG_PER_PAGE: usize = 50;

/// Tetto alle pagine di catalogo scaricate in una ricerca.
///
/// Il launcher legacy ne scaricava quante ne dichiarava l'API, senza limite:
/// con un `_nRecordCount` sballato sarebbero migliaia di richieste. 200 pagine
/// da 50 sono 10 000 mod, ben oltre il catalogo reale di Mario Kart Wii.
const MAX_CATALOG_PAGES: usize = 200;

/// Richieste in volo verso GameBanana.
///
/// Sei sono un compromesso: abbastanza da rendere la prima ricerca sopportabile,
/// abbastanza poche da non sembrare un attacco a un'API che non e' nostra.
const CATALOG_CONCURRENCY: usize = 6;
const DETAIL_CONCURRENCY: usize = 6;

/// Gli unici host da cui questo modulo accetta di scaricare.
const ALLOWED_HOSTS: [&str; 2] = ["gamebanana.com", "files.gamebanana.com"];

/// Host da cui GameBanana serve le anteprime. Vedi [`is_allowed_image_url`].
const IMAGE_HOST: &str = "images.gamebanana.com";

/// Ordinamenti accettati, gli stessi della UI legacy.
const SORTS: [&str; 5] = [
    "Generic_Newest",
    "Generic_MostLiked",
    "Generic_MostViewed",
    "Generic_MostDownloaded",
    "Generic_Alphabetically",
];

// ---------------------------------------------------------------------------
// Tipi verso il frontend
// ---------------------------------------------------------------------------

/// Un file scaricabile di una mod.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameBananaFile {
    pub file_id: i64,
    pub file_name: String,
    pub description: String,
    pub size_bytes: i64,
    pub download_count: i64,
    pub date_added_utc: String,
    /// L'URL non viene serializzato: resta nel backend.
    #[serde(skip)]
    pub download_url: String,
}

/// Una mod di GameBanana.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameBananaMod {
    pub id: i64,
    pub name: String,
    pub author: String,
    pub description: String,
    pub profile_url: String,
    pub views: i64,
    pub likes: i64,
    pub downloads: i64,
    /// Miniatura mostrata nell'elenco; vuota quando la mod non ne ha.
    pub preview_url: String,
    pub files: Vec<GameBananaFile>,
}

/// Una pagina di risultati.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameBananaSearchResult {
    pub mods: Vec<GameBananaMod>,
    pub total_available: usize,
    pub has_more: bool,
    /// Vero quando il catalogo è stato troncato al limite di pagine.
    pub catalog_truncated: bool,
}

/// Una voce del catalogo: solo id e nome, per la ricerca fuzzy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: i64,
    pub name: String,
}

/// Il catalogo scaricato una volta per sessione.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

/// `true` se l'URL è `https` verso uno degli host consentiti.
///
/// Il confronto è sull'host **esatto**: `gamebanana.com.example.org` non passa,
/// e nemmeno un sottodominio non elencato.
pub fn is_allowed_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };

    parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed
            .host_str()
            .is_some_and(|host| ALLOWED_HOSTS.contains(&host.to_ascii_lowercase().as_str()))
}

/// `true` se l'URL è un'immagine `https` servita dall'host delle anteprime.
///
/// Host a parte rispetto a [`ALLOWED_HOSTS`], e per un motivo: da qui il
/// launcher non scarica niente: ci punta soltanto un `<img>` della webview.
/// Tenerlo fuori dall'allowlist dei download significa che un URL di immagine
/// non può diventare per sbaglio la sorgente di un archivio.
pub fn is_allowed_image_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };

    parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(IMAGE_HOST))
}

fn require_allowed_url(url: &str) -> AppResult<()> {
    if is_allowed_url(url) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "GameBanana returned a URL outside the allowed hosts: {}",
            vk_core::redact::redact_url(url)
        )))
    }
}

/// Ordinamento validato, con il default del legacy.
fn normalize_sort(sort: &str) -> &'static str {
    SORTS
        .into_iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(sort.trim()))
        .unwrap_or("Generic_Newest")
}

// ---------------------------------------------------------------------------
// Ricerca
// ---------------------------------------------------------------------------

/// Cerca fra le mod di Mario Kart Wii.
///
/// Senza testo di ricerca si impagina direttamente sull'API. Con un testo si
/// usa il catalogo dei nomi e un punteggio fuzzy, perché `Mod/Index` non
/// accetta un filtro per nome: è la stessa strategia del launcher legacy.
pub async fn search(
    state: &Arc<AppState>,
    query: &str,
    sort: &str,
    page: usize,
) -> AppResult<GameBananaSearchResult> {
    let page = page.max(1);
    let sort = normalize_sort(sort);
    let query = query.trim();

    let (ids, total, has_more, truncated) = if query.is_empty() {
        let url = format!(
            "{API_ROOT}/Mod/Index?_nPage={page}&_nPerpage={PER_PAGE}&_aFilters%5BGeneric_Game%5D={GAME_ID}&_sSort={sort}"
        );
        let raw = fetch(state, &url).await?;
        let index = parse_index_page(&raw, page, PER_PAGE);
        (
            index.ids.into_iter().take(PER_PAGE).collect::<Vec<i64>>(),
            index.total,
            index.has_more,
            false,
        )
    } else {
        let catalog = catalog(state).await?;
        let ranked = rank_catalog(&catalog.entries, query);
        let start = (page - 1) * PER_PAGE;

        (
            ranked
                .iter()
                .skip(start)
                .take(PER_PAGE)
                .copied()
                .collect::<Vec<i64>>(),
            ranked.len(),
            start + PER_PAGE < ranked.len(),
            catalog.truncated,
        )
    };

    // `buffered` conserva l'ordine della classifica: i risultati piu' pertinenti
    // restano in cima anche se il server risponde in ordine sparso.
    let mods: Vec<GameBananaMod> = stream::iter(ids)
        .map(|id| async move {
            match fetch_mod(state, id).await {
                Ok(found) => found,
                // Una mod che non si carica non deve svuotare la pagina:
                // succede per le mod ritirate, che l'indice elenca ancora.
                Err(error) => {
                    tracing::warn!(
                        id,
                        error = %vk_core::redact::redact(&error.to_string()),
                        "mod di GameBanana non leggibile: saltata"
                    );
                    None
                }
            }
        })
        .buffered(DETAIL_CONCURRENCY)
        .filter_map(|item| async move { item })
        .collect()
        .await;

    let mut mods = mods;

    if !query.is_empty() {
        sort_mods(&mut mods, sort);
    }

    Ok(GameBananaSearchResult {
        total_available: total,
        has_more,
        catalog_truncated: truncated,
        mods,
    })
}

fn sort_mods(mods: &mut [GameBananaMod], sort: &str) {
    match sort {
        // Chiave negata: `sort_by_key` ordina in senso crescente e qui serve
        // il contrario, dal più apprezzato al meno.
        "Generic_MostLiked" => mods.sort_by_key(|item| -item.likes),
        "Generic_MostViewed" => mods.sort_by_key(|item| -item.views),
        "Generic_MostDownloaded" => mods.sort_by_key(|item| -item.downloads),
        "Generic_Alphabetically" => mods.sort_by_key(|item| item.name.to_lowercase()),
        _ => {}
    }
}

/// Catalogo dei nomi, scaricato una volta e tenuto per la sessione.
async fn catalog(state: &Arc<AppState>) -> AppResult<Catalog> {
    if let Some(cached) = state.gamebanana_catalog.read().await.clone() {
        return Ok(cached);
    }

    let mut guard = state.gamebanana_catalog.write().await;
    if let Some(cached) = guard.clone() {
        return Ok(cached);
    }

    let first = fetch_catalog_page(state, 1).await?;
    let declared_pages = first.total.div_ceil(CATALOG_PER_PAGE).max(1);
    let pages = declared_pages.min(MAX_CATALOG_PAGES);

    // Il catalogo di Mario Kart Wii sono oltre quaranta pagine: una alla volta
    // sono venti secondi buoni prima che la prima ricerca risponda, e chi
    // guarda conclude che non funziona. Poche richieste in parallelo bastano a
    // farlo scendere a pochi secondi senza martellare un'API di terzi.
    let rest: Vec<Vec<CatalogEntry>> = stream::iter(2..=pages)
        .map(|page| async move {
            match fetch_catalog_page(state, page).await {
                Ok(next) => next.entries,
                Err(error) => {
                    // Una pagina persa restringe la ricerca, non la fa fallire.
                    tracing::warn!(
                        page,
                        error = %vk_core::redact::redact(&error.to_string()),
                        "pagina di catalogo GameBanana non scaricata"
                    );
                    Vec::new()
                }
            }
        })
        .buffered(CATALOG_CONCURRENCY)
        .collect()
        .await;

    let mut entries = first.entries;
    for page in rest {
        entries.extend(page);
    }

    let mut seen = BTreeSet::new();
    entries.retain(|entry| entry.id > 0 && !entry.name.trim().is_empty() && seen.insert(entry.id));

    let catalog = Catalog {
        entries,
        truncated: declared_pages > pages,
    };
    tracing::info!(
        mods = catalog.entries.len(),
        truncated = catalog.truncated,
        "catalogo GameBanana caricato"
    );

    *guard = Some(catalog.clone());
    Ok(catalog)
}

async fn fetch_catalog_page(state: &Arc<AppState>, page: usize) -> AppResult<IndexPage> {
    let url = format!(
        "{API_ROOT}/Mod/Index?_nPage={page}&_nPerpage={CATALOG_PER_PAGE}&_aFilters%5BGeneric_Game%5D={GAME_ID}&_sSort=Generic_Alphabetically"
    );
    let raw = fetch(state, &url).await?;
    Ok(parse_catalog_page(&raw))
}

async fn fetch_mod(state: &Arc<AppState>, id: i64) -> AppResult<Option<GameBananaMod>> {
    const PROPERTIES: &str = "_idRow,_sName,_sProfileUrl,_aSubmitter,_aFiles,_sText,_nViewCount,_nLikeCount,_aGame,_aPreviewMedia";

    let url = format!("{API_ROOT}/Mod/{id}?_csvProperties={PROPERTIES}");
    let raw = fetch(state, &url).await?;
    Ok(parse_mod(&raw, id))
}

async fn fetch(state: &Arc<AppState>, url: &str) -> AppResult<String> {
    require_allowed_url(url)?;
    state
        .downloader
        .get_string(url)
        .await
        .map_err(|error| match error {
            vk_core::CoreError::Cancelled => AppError::Cancelled,
            other => AppError::Core(other),
        })
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct IndexPage {
    ids: Vec<i64>,
    entries: Vec<CatalogEntry>,
    total: usize,
    has_more: bool,
}

/// Legge una pagina di `Mod/Index`.
///
/// Solo i record con `_bHasFiles`: gli altri non hanno niente da scaricare e
/// occuperebbero la pagina.
fn parse_index_page(raw: &str, page: usize, per_page: usize) -> IndexPage {
    let Ok(root) = serde_json::from_str::<Value>(vk_core::json::strip_leading_noise(raw)) else {
        return IndexPage::default();
    };

    let records: Vec<&Value> = root
        .get("_aRecords")
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .filter(|record| flag(record, "_bHasFiles"))
                .collect()
        })
        .unwrap_or_default();

    let ids: Vec<i64> = records
        .iter()
        .map(|record| number(record, "_idRow"))
        .filter(|id| *id > 0)
        .collect();

    let metadata = root.get("_aMetadata");
    let total = metadata.map_or(0, |value| number(value, "_nRecordCount").max(0) as usize);
    let declared_per_page = metadata
        .map_or(0, |value| number(value, "_nPerpage").max(0) as usize)
        .max(1);
    let complete = metadata.is_some_and(|value| flag(value, "_bIsComplete"));
    let effective_per_page = if metadata.is_some() {
        declared_per_page
    } else {
        per_page
    };

    IndexPage {
        entries: records
            .iter()
            .map(|record| CatalogEntry {
                id: number(record, "_idRow"),
                name: text(record, "_sName"),
            })
            .collect(),
        has_more: !complete && page * effective_per_page < total,
        ids,
        total,
    }
}

fn parse_catalog_page(raw: &str) -> IndexPage {
    parse_index_page(raw, 1, CATALOG_PER_PAGE)
}

/// Legge il dettaglio di una mod.
///
/// `None` quando la mod non appartiene a Mario Kart Wii o non ha file
/// scaricabili: l'indice elenca anche mod ritirate.
fn parse_mod(raw: &str, id: i64) -> Option<GameBananaMod> {
    let root: Value = serde_json::from_str(vk_core::json::strip_leading_noise(raw)).ok()?;

    let game = root.get("_aGame")?;
    if number(game, "_idRow") != GAME_ID {
        return None;
    }

    let files: Vec<GameBananaFile> = root
        .get("_aFiles")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let download_url = text(item, "_sDownloadUrl");
            (!download_url.trim().is_empty()).then(|| GameBananaFile {
                file_id: number(item, "_idRow"),
                file_name: text(item, "_sFile"),
                description: strip_html(&text(item, "_sDescription")),
                size_bytes: number(item, "_nFilesize"),
                download_count: number(item, "_nDownloadCount"),
                date_added_utc: iso_from_unix(number(item, "_tsDateAdded")),
                download_url,
            })
        })
        .collect();

    if files.is_empty() {
        return None;
    }

    Some(GameBananaMod {
        id: if id > 0 { id } else { number(&root, "_idRow") },
        name: text(&root, "_sName"),
        author: root
            .get("_aSubmitter")
            .map(|submitter| text(submitter, "_sName"))
            .unwrap_or_default(),
        description: strip_html(&text(&root, "_sText")),
        profile_url: text(&root, "_sProfileUrl"),
        views: number(&root, "_nViewCount"),
        likes: number(&root, "_nLikeCount"),
        downloads: files.iter().map(|file| file.download_count).sum(),
        preview_url: preview_url(&root),
        files,
    })
}

/// Miniatura della mod, o stringa vuota se non ce n'è una utilizzabile.
///
/// GameBanana pubblica la stessa immagine in più tagli (`_sFile220`,
/// `_sFile530`, …) ma non li ha tutti per ogni voce: si parte dai 220 px, che
/// è la misura della riga nell'elenco, e si scende finché qualcosa c'è.
fn preview_url(root: &Value) -> String {
    const SIZES: [&str; 4] = ["_sFile220", "_sFile530", "_sFile100", "_sFile"];

    let Some(images) = root
        .get("_aPreviewMedia")
        .and_then(|media| media.get("_aImages"))
        .and_then(Value::as_array)
    else {
        return String::new();
    };

    for image in images {
        let base = text(image, "_sBaseUrl");
        let file = SIZES
            .iter()
            .map(|key| text(image, key))
            .find(|value| !value.is_empty());

        let (Some(file), false) = (file, base.is_empty()) else {
            continue;
        };

        let candidate = format!("{}/{file}", base.trim_end_matches('/'));
        if is_allowed_image_url(&candidate) {
            return candidate;
        }
    }

    String::new()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Timestamp unix in ISO-8601, vuoto se assente.
fn iso_from_unix(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }

    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .and_then(|stamp| {
            stamp
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_default()
}

/// Toglie i tag HTML da una descrizione.
///
/// Le descrizioni di GameBanana sono HTML e finiscono in un `<p>` del
/// frontend, che non usa `innerHTML`: senza questa ripulitura l'utente
/// leggerebbe i tag.
fn strip_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                out.push(' ');
            }
            _ if !inside_tag => out.push(character),
            _ => {}
        }
    }

    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    decoded.split_whitespace().collect::<Vec<&str>>().join(" ")
}

// ---------------------------------------------------------------------------
// Ricerca fuzzy
// ---------------------------------------------------------------------------

/// Id del catalogo ordinati per pertinenza rispetto alla query.
fn rank_catalog(entries: &[CatalogEntry], query: &str) -> Vec<i64> {
    let normalized = normalize_for_search(query);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(u32, String, i64)> = entries
        .iter()
        .filter_map(|entry| {
            let name = normalize_for_search(&entry.name);
            fuzzy_score(&normalized, &name)
                .map(|score| (score, entry.name.to_lowercase(), entry.id))
        })
        .collect();

    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, id)| id).collect()
}

/// Minuscolo, senza accenti, con la punteggiatura ridotta a spazi.
fn normalize_for_search(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for character in value.to_lowercase().chars() {
        let folded = fold_accent(character);
        if folded.is_alphanumeric() {
            out.push(folded);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }

    out.trim().to_string()
}

/// Riduce le lettere accentate più comuni alla loro base.
///
/// Non è una normalizzazione Unicode completa: serve solo a far combaciare i
/// nomi delle mod, che sono quasi sempre ASCII.
fn fold_accent(character: char) -> char {
    match character {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Punteggio di pertinenza: più basso è meglio. `None` se non pertinente.
///
/// Stessa scala del legacy: uguale 0, prefisso 2, sottostringa 5 + posizione,
/// altrimenti 20 + distanza di edit fra i token.
fn fuzzy_score(query: &str, name: &str) -> Option<u32> {
    if query.is_empty() || name.is_empty() {
        return None;
    }
    if name == query {
        return Some(0);
    }
    if name.starts_with(query) {
        return Some(2);
    }
    if let Some(position) = name.find(query) {
        return Some(5 + u32::try_from(position).unwrap_or(u32::MAX - 5));
    }

    let name_tokens: Vec<&str> = name.split_whitespace().collect();
    if name_tokens.is_empty() {
        return None;
    }

    let mut total = 0u32;
    for token in query.split_whitespace() {
        if name_tokens.iter().any(|other| other.starts_with(token)) {
            continue;
        }

        let best = name_tokens
            .iter()
            .map(|other| levenshtein(token, other))
            .min()
            .unwrap_or(u32::MAX);
        let tolerance = ((token.chars().count() as f64) * 0.4).ceil().max(1.0) as u32;

        if best > tolerance {
            return None;
        }
        total += best;
    }

    Some(20 + total)
}

/// Distanza di edit fra due stringhe, sui caratteri.
fn levenshtein(left: &str, right: &str) -> u32 {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<u32> = (0..=right.len() as u32).collect();

    for (row, left_char) in left.chars().enumerate() {
        let mut current = vec![row as u32 + 1];
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + u32::from(left_char != *right_char);
            current.push(
                (current[column] + 1)
                    .min(previous[column + 1] + 1)
                    .min(substitution),
            );
        }
        previous = current;
    }

    previous[right.len()]
}

// ---------------------------------------------------------------------------
// Installazione
// ---------------------------------------------------------------------------

/// Identificativo dell'addon, nella stessa forma del launcher legacy.
pub fn addon_id(mod_id: i64, file_id: i64) -> String {
    format!("gamebanana-{mod_id}-{file_id}")
}

/// Scarica un file di una mod e lo installa come addon.
///
/// L'URL viene riletto dall'API adesso, non ricevuto dal frontend, e validato
/// contro l'allowlist prima di qualunque richiesta.
pub async fn install(
    state: &Arc<AppState>,
    mod_id: i64,
    file_id: i64,
    progress: ProgressSink,
) -> AppResult<AddonView> {
    let guard = state.mod_operation.try_lock().map_err(|_| AppError::Busy)?;
    let cancel = state.renew_cancel_token().await;

    let result = install_inner(state, mod_id, file_id, &progress, &cancel).await;
    drop(guard);

    if let Err(error) = &result {
        progress(ProgressUpdate::new(
            Phase::Error,
            vk_core::redact::redact(&error.to_string()),
        ));
    }
    result
}

async fn install_inner(
    state: &Arc<AppState>,
    mod_id: i64,
    file_id: i64,
    progress: &ProgressSink,
    cancel: &CancelToken,
) -> AppResult<AddonView> {
    let layout = state.layout(state.channel().await).await;
    if !layout.is_installed() {
        return Err(AppError::Configuration(
            "Install the modpack first: addons go inside its My Stuff folder.".into(),
        ));
    }

    progress(ProgressUpdate::new(
        Phase::Connecting,
        "Reading the mod from GameBanana...",
    ));

    let item = fetch_mod(state, mod_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Mod not available on GameBanana.".into()))?;

    let file = item
        .files
        .iter()
        .find(|file| file.file_id == file_id)
        .ok_or_else(|| AppError::BadRequest("File not found in this mod.".into()))?;

    require_allowed_url(&file.download_url)?;
    if !addons::is_supported_archive(&file.file_name) {
        return Err(AppError::BadRequest(format!(
            "'{}' is not an archive the launcher can open.",
            file.file_name
        )));
    }

    let archive = state
        .paths
        .downloads_dir()
        .join(format!("gamebanana-{mod_id}-{file_id}.zip"));
    tokio::fs::create_dir_all(state.paths.downloads_dir())
        .await
        .map_err(|error| AppError::io(state.paths.downloads_dir(), error))?;

    progress(ProgressUpdate::new(
        Phase::Download,
        format!("Download di {}...", file.file_name),
    ));

    let download = state
        .downloader
        .download_with_mirrors(
            std::slice::from_ref(&file.download_url),
            &archive,
            progress,
            cancel,
        )
        .await;

    let result = async {
        download.map_err(|error| match error {
            vk_core::CoreError::Cancelled => AppError::Cancelled,
            other => AppError::Core(other),
        })?;

        progress(ProgressUpdate::new(
            Phase::Installing,
            "Extracting the addon...",
        ));

        addons::import_archive_as(
            &layout,
            &archive,
            ImportRequest {
                id: addon_id(mod_id, file_id),
                name: item.name.clone(),
                author: item.author.clone(),
                source: "GameBanana".into(),
                source_url: item.profile_url.clone(),
                preview_url: item.preview_url.clone(),
                replace_existing: true,
            },
        )
        .await
    }
    .await;

    let _ = tokio::fs::remove_file(&archive).await;
    let addon = result?;

    progress(ProgressUpdate::new(
        Phase::Completed,
        format!("{} installed: {} files.", addon.name, addon.file_count),
    ));
    tracing::info!(
        mod_id,
        file_id,
        files = addon.file_count,
        "addon GameBanana installato"
    );

    Ok(addon)
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

    /// Ricerca reale contro l'API di GameBanana.
    ///
    /// Gli altri test girano su risposte finte: dimostrano che il parsing e'
    /// coerente con se stesso. Questo dimostra che la ricerca trova qualcosa
    /// nel catalogo vero, che e' l'unica cosa che l'utente vede.
    ///
    /// ```bash
    /// cargo test -p vanzakart-launcher --all-features gamebanana::tests::live -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "richiede la rete"]
    async fn live_search_finds_something() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let started = std::time::Instant::now();
        let catalog = catalog(&state).await.expect("catalogo");
        println!(
            "catalogo: {} voci in {:.1}s (troncato: {})",
            catalog.entries.len(),
            started.elapsed().as_secs_f64(),
            catalog.truncated
        );
        assert!(!catalog.entries.is_empty(), "catalogo vuoto");

        for query in ["mario", "luigi", "rainbow", "texture"] {
            let ranked = rank_catalog(&catalog.entries, query);
            println!("  '{query}': {} nomi in classifica", ranked.len());
            assert!(!ranked.is_empty(), "nessun nome per '{query}'");
        }

        let started = std::time::Instant::now();
        let result = search(&state, "mario", "Generic_Newest", 1)
            .await
            .expect("ricerca");
        println!(
            "ricerca 'mario': {} mod su {} in {:.1}s",
            result.mods.len(),
            result.total_available,
            started.elapsed().as_secs_f64()
        );
        assert!(!result.mods.is_empty(), "la ricerca non ha restituito mod");
    }

    /// Pagina di indice nella forma che restituisce `apiv11`.
    const INDEX_PAGE: &str = r#"{
        "_aMetadata": { "_nRecordCount": 120, "_nPerpage": 30, "_bIsComplete": false },
        "_aRecords": [
            { "_idRow": 11, "_sName": "Rainbow Road HD", "_bHasFiles": true },
            { "_idRow": 12, "_sName": "Senza file", "_bHasFiles": false },
            { "_idRow": 13, "_sName": "Luigi Circuit Remix", "_bHasFiles": true }
        ]
    }"#;

    const MOD_DETAIL: &str = r#"{
        "_idRow": 11,
        "_sName": "Rainbow Road HD",
        "_sProfileUrl": "https://gamebanana.com/mods/11",
        "_sText": "<p>Una <b>texture</b> in alta risoluzione &amp; altro</p>",
        "_nViewCount": 4200,
        "_nLikeCount": 130,
        "_aGame": { "_idRow": 5896, "_sName": "Mario Kart Wii" },
        "_aSubmitter": { "_sName": "autrice" },
        "_aFiles": [
            {
                "_idRow": 901,
                "_sFile": "rainbow.zip",
                "_sDescription": "<i>versione 2</i>",
                "_sDownloadUrl": "https://files.gamebanana.com/mods/rainbow.zip",
                "_nFilesize": 1048576,
                "_nDownloadCount": 900,
                "_tsDateAdded": 1700000000
            },
            {
                "_idRow": 902,
                "_sFile": "senza-url.zip",
                "_sDownloadUrl": ""
            }
        ]
    }"#;

    // --- allowlist --------------------------------------------------------

    #[test]
    fn only_the_two_gamebanana_hosts_are_accepted() {
        assert!(is_allowed_url("https://gamebanana.com/apiv11/Mod/Index"));
        assert!(is_allowed_url("https://files.gamebanana.com/mods/a.zip"));
        assert!(is_allowed_url("https://FILES.GameBanana.com/mods/a.zip"));
    }

    #[test]
    fn lookalike_and_insecure_hosts_are_rejected() {
        for hostile in [
            "http://gamebanana.com/a.zip",
            "https://gamebanana.com.evil.example/a.zip",
            "https://evil.example/gamebanana.com/a.zip",
            "https://images.gamebanana.com/a.png",
            "https://user:pass@files.gamebanana.com/a.zip",
            "file:///C:/Windows/system32/cmd.exe",
            "ftp://files.gamebanana.com/a.zip",
            "",
            "non-un-url",
        ] {
            assert!(!is_allowed_url(hostile), "URL accettato: {hostile}");
        }
    }

    #[test]
    fn a_rejected_url_never_reaches_the_downloader() {
        let error = require_allowed_url("https://evil.example/a.zip").unwrap_err();
        assert_eq!(error.code(), "bad-request");
    }

    // --- parsing ----------------------------------------------------------

    #[test]
    fn an_index_page_keeps_only_records_with_files() {
        let page = parse_index_page(INDEX_PAGE, 1, PER_PAGE);

        assert_eq!(page.ids, vec![11, 13]);
        assert_eq!(page.total, 120);
        assert!(page.has_more, "120 record su pagine da 30");
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.entries[0].name, "Rainbow Road HD");
    }

    #[test]
    fn the_last_page_does_not_announce_more() {
        let page = parse_index_page(INDEX_PAGE, 4, PER_PAGE);
        assert!(!page.has_more);
    }

    #[test]
    fn malformed_json_yields_an_empty_page() {
        assert_eq!(
            parse_index_page("non json", 1, PER_PAGE),
            IndexPage::default()
        );
        assert_eq!(parse_index_page("{}", 1, PER_PAGE), IndexPage::default());
    }

    #[test]
    fn a_mod_detail_is_read_with_its_downloadable_files() {
        let item = parse_mod(MOD_DETAIL, 11).expect("mod leggibile");

        assert_eq!(item.id, 11);
        assert_eq!(item.name, "Rainbow Road HD");
        assert_eq!(item.author, "autrice");
        assert_eq!(item.views, 4200);
        assert_eq!(item.likes, 130);
        assert_eq!(item.downloads, 900);

        assert_eq!(item.files.len(), 1, "il file senza URL viene scartato");
        let file = &item.files[0];
        assert_eq!(file.file_id, 901);
        assert_eq!(file.file_name, "rainbow.zip");
        assert_eq!(file.description, "versione 2");
        assert_eq!(file.size_bytes, 1_048_576);
        assert!(file.date_added_utc.starts_with("2023-11-"));
    }

    /// Payload delle anteprime nella forma in cui lo manda l'API.
    fn with_preview(images: &str) -> String {
        format!(
            r#"{{"_aGame":{{"_idRow":{GAME_ID}}},
                 "_aFiles":[{{"_idRow":1,"_sFile":"a.zip",
                              "_sDownloadUrl":"https://files.gamebanana.com/a.zip"}}],
                 "_aPreviewMedia":{{"_aImages":{images}}}}}"#
        )
    }

    #[test]
    fn the_preview_takes_the_220px_cut() {
        let item = parse_mod(
            &with_preview(
                r#"[{"_sBaseUrl":"https://images.gamebanana.com/img/ss/mods",
                     "_sFile":"pieno.jpg","_sFile530":"530_pieno.jpg","_sFile220":"220_pieno.jpg"}]"#,
            ),
            11,
        )
        .unwrap();

        assert_eq!(
            item.preview_url,
            "https://images.gamebanana.com/img/ss/mods/220_pieno.jpg"
        );
    }

    #[test]
    fn the_preview_falls_back_when_a_cut_is_missing() {
        let item = parse_mod(
            &with_preview(
                r#"[{"_sBaseUrl":"https://images.gamebanana.com/img/ss/mods","_sFile":"solo.jpg"}]"#,
            ),
            11,
        )
        .unwrap();

        assert_eq!(
            item.preview_url,
            "https://images.gamebanana.com/img/ss/mods/solo.jpg"
        );
    }

    #[test]
    fn a_preview_on_another_host_is_dropped() {
        let item = parse_mod(
            &with_preview(
                r#"[{"_sBaseUrl":"https://cattivo.example/img","_sFile220":"x.jpg"},
                    {"_sBaseUrl":"http://images.gamebanana.com/img","_sFile220":"y.jpg"}]"#,
            ),
            11,
        )
        .unwrap();

        assert!(item.preview_url.is_empty(), "{}", item.preview_url);
    }

    #[test]
    fn a_mod_without_preview_has_an_empty_url() {
        let item = parse_mod(MOD_DETAIL, 11).unwrap();
        assert!(item.preview_url.is_empty());
    }

    #[test]
    fn the_image_host_is_not_a_download_host() {
        // L'host delle anteprime resta fuori dall'allowlist dei download: da
        // lì il launcher non deve poter scaricare un archivio.
        assert!(is_allowed_image_url(
            "https://images.gamebanana.com/img/ss/mods/220_x.jpg"
        ));
        assert!(!is_allowed_url(
            "https://images.gamebanana.com/img/ss/mods/220_x.jpg"
        ));
        assert!(!is_allowed_image_url("https://files.gamebanana.com/a.zip"));
        assert!(!is_allowed_image_url(
            "https://images.gamebanana.com.example.org/x.jpg"
        ));
    }

    #[test]
    fn the_download_url_never_leaves_the_backend() {
        let item = parse_mod(MOD_DETAIL, 11).unwrap();
        assert!(!item.files[0].download_url.is_empty());

        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("downloadUrl"), "{json}");
        assert!(!json.contains("files.gamebanana.com"), "{json}");
    }

    #[test]
    fn a_mod_from_another_game_is_refused() {
        let other = MOD_DETAIL.replace("\"_idRow\": 5896", "\"_idRow\": 1234");
        assert!(parse_mod(&other, 11).is_none());
    }

    #[test]
    fn a_mod_without_downloadable_files_is_refused() {
        let empty = MOD_DETAIL.replace("https://files.gamebanana.com/mods/rainbow.zip", "");
        assert!(parse_mod(&empty, 11).is_none());
        assert!(parse_mod("{}", 11).is_none());
    }

    #[test]
    fn html_descriptions_become_plain_text() {
        assert_eq!(
            strip_html("<p>Una <b>texture</b> in alta risoluzione &amp; altro</p>"),
            "Una texture in alta risoluzione & altro"
        );
        assert_eq!(strip_html("  spazi   multipli  "), "spazi multipli");
        assert_eq!(strip_html("<br/>"), "");
        assert_eq!(strip_html("&lt;script&gt;"), "<script>");
    }

    // --- ricerca ----------------------------------------------------------

    #[test]
    fn search_normalisation_folds_case_accents_and_punctuation() {
        assert_eq!(normalize_for_search("Rainbow-Road HD!"), "rainbow road hd");
        assert_eq!(normalize_for_search("Città  Perduta"), "citta perduta");
        assert_eq!(normalize_for_search("   "), "");
    }

    #[test]
    fn the_fuzzy_score_prefers_exact_then_prefix_then_substring() {
        assert_eq!(fuzzy_score("rainbow", "rainbow"), Some(0));
        assert_eq!(fuzzy_score("rainbow", "rainbow road"), Some(2));

        let substring = fuzzy_score("road", "rainbow road").unwrap();
        assert!(substring > 2, "una sottostringa vale meno di un prefisso");

        // Un refuso resta pertinente, una parola diversa no.
        assert!(fuzzy_score("rainbov road", "rainbow road").is_some());
        assert!(fuzzy_score("luigi", "rainbow road").is_none());
        assert!(fuzzy_score("", "rainbow").is_none());
    }

    #[test]
    fn the_catalog_is_ranked_by_relevance() {
        let entries = vec![
            CatalogEntry {
                id: 1,
                name: "Luigi Circuit Remix".into(),
            },
            CatalogEntry {
                id: 2,
                name: "Rainbow Road HD".into(),
            },
            CatalogEntry {
                id: 3,
                name: "Neo Rainbow Road".into(),
            },
            CatalogEntry {
                id: 4,
                name: "Coconut Mall".into(),
            },
        ];

        let ranked = rank_catalog(&entries, "rainbow road");

        assert_eq!(ranked.first(), Some(&2), "il prefisso viene prima");
        assert!(ranked.contains(&3));
        assert!(!ranked.contains(&4), "un nome non pertinente resta fuori");
        assert!(rank_catalog(&entries, "   ").is_empty());
    }

    #[test]
    fn levenshtein_measures_edits() {
        assert_eq!(levenshtein("kart", "kart"), 0);
        assert_eq!(levenshtein("kart", "cart"), 1);
        assert_eq!(levenshtein("kart", ""), 4);
        assert_eq!(levenshtein("", "kart"), 4);
    }

    #[test]
    fn unknown_sorts_fall_back_to_the_default() {
        assert_eq!(normalize_sort("Generic_MostLiked"), "Generic_MostLiked");
        assert_eq!(normalize_sort("generic_mostliked"), "Generic_MostLiked");
        assert_eq!(normalize_sort("'; DROP TABLE"), "Generic_Newest");
        assert_eq!(normalize_sort(""), "Generic_Newest");
    }

    #[test]
    fn mods_are_sorted_by_the_requested_criterion() {
        let mut mods = vec![
            GameBananaMod {
                id: 1,
                name: "Beta".into(),
                likes: 5,
                views: 100,
                downloads: 9,
                ..Default::default()
            },
            GameBananaMod {
                id: 2,
                name: "Alfa".into(),
                likes: 9,
                views: 10,
                downloads: 1,
                ..Default::default()
            },
        ];

        sort_mods(&mut mods, "Generic_MostLiked");
        assert_eq!(mods[0].id, 2);

        sort_mods(&mut mods, "Generic_MostViewed");
        assert_eq!(mods[0].id, 1);

        sort_mods(&mut mods, "Generic_Alphabetically");
        assert_eq!(mods[0].name, "Alfa");
    }

    // --- installazione ----------------------------------------------------

    #[test]
    fn the_addon_identifier_follows_the_legacy_shape() {
        assert_eq!(addon_id(11, 901), "gamebanana-11-901");
        // Deve restare un identificatore accettabile come nome di cartella.
        assert_eq!(addons::slug(&addon_id(11, 901)), addon_id(11, 901));
    }

    #[tokio::test]
    async fn installing_without_the_modpack_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        let error = install(&state, 11, 901, vk_core::progress::noop_sink())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "configuration");
    }

    #[tokio::test]
    async fn the_catalog_is_only_fetched_once_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        // Precaricando la cache si verifica che `catalog` non tocchi la rete:
        // senza il controllo, il test fallirebbe per timeout di connessione.
        *state.gamebanana_catalog.write().await = Some(Catalog {
            entries: vec![CatalogEntry {
                id: 7,
                name: "Cached".into(),
            }],
            truncated: false,
        });

        let catalog = catalog(&state).await.unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].id, 7);
    }

    #[tokio::test]
    async fn searching_a_cached_catalog_needs_no_index_request() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_at(dir.path()).await;

        *state.gamebanana_catalog.write().await = Some(Catalog {
            entries: vec![CatalogEntry {
                id: 7,
                name: "Coconut Mall".into(),
            }],
            truncated: true,
        });

        // Il dettaglio della mod non è raggiungibile in test: la ricerca lo
        // salta e restituisce comunque i conteggi.
        let result = search(&state, "coconut", "Generic_Newest", 1)
            .await
            .unwrap();
        assert_eq!(result.total_available, 1);
        assert!(result.catalog_truncated);
        assert!(result.mods.is_empty());
    }
}
