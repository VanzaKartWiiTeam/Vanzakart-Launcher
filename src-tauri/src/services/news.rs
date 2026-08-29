//! News del launcher.
//!
//! L'unica sorgente è `news.json` sul server (endpoint `news_url`, con
//! `news_json_url` come alias). Il payload è quello scritto dal launcher
//! legacy — chiavi PascalCase — quindi la lettura è tollerante sul caso delle
//! chiavi invece di pretendere il camelCase (vedi `docs/decisions.md` §D-042).
//! Il seed locale resta solo come fallback offline e non contiene media di
//! terze parti (§D-021).

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::domain::NewsItem;
use crate::error::AppResult;
use crate::state::AppState;

/// Scarica le news dal server, con fallback sul seed locale.
pub async fn fetch(state: &Arc<AppState>) -> AppResult<Vec<NewsItem>> {
    let url = state.endpoints.read().await.resolved_news_url().to_string();

    if url.trim().is_empty() {
        tracing::warn!("nessun endpoint news configurato: si usa il seed locale");
        return Ok(sorted(seed()));
    }

    let no_cache = vk_core::endpoints::add_no_cache_query(&url, vk_core::now_millis());
    let items = match state.downloader.get_string(&no_cache).await {
        Ok(raw) => parse(&raw).unwrap_or_else(|| {
            tracing::warn!("news.json non interpretabile: si usa il seed locale");
            seed()
        }),
        Err(error) => {
            tracing::warn!(
                error = %vk_core::redact::redact(&error.to_string()),
                "news non raggiungibili: si usa il seed locale"
            );
            seed()
        }
    };

    Ok(sorted(items))
}

/// Interpreta il payload `news.json`. `None` se non contiene voci leggibili.
///
/// Il file pubblicato usa `Title`/`Summary`/`MediaPath`; le voci scritte a mano
/// possono usare il camelCase dei DTO. Entrambe le forme sono accettate.
pub fn parse(raw: &str) -> Option<Vec<NewsItem>> {
    let cleaned = vk_core::json::strip_leading_noise(raw);
    let entries: Vec<Value> = serde_json::from_str(cleaned).ok()?;

    let items: Vec<NewsItem> = entries.iter().filter_map(read_item).collect();
    if items.is_empty() {
        return None;
    }

    Some(items)
}

/// Una voce del payload. `None` se non è un oggetto o se è vuota: una news
/// senza titolo né testo è il sintomo di uno schema che non combacia, e
/// mostrarla come card vuota nasconde il problema.
fn read_item(entry: &Value) -> Option<NewsItem> {
    let object = entry.as_object()?;

    let title = text(object, "title");
    let summary = text(object, "summary");
    if title.is_empty() && summary.is_empty() {
        return None;
    }

    let media_path = optional_text(object, "mediaPath").map(|path| normalize_media_url(&path));
    let media_kind = media_path.as_deref().map(media_kind).map(str::to_string);

    Some(NewsItem {
        title,
        category: text(object, "category"),
        version: text(object, "version"),
        summary,
        date_label: text(object, "dateLabel"),
        is_pinned: flag(object, "isPinned"),
        media_path,
        media_kind,
    })
}

/// Cerca la chiave così com'è e, se manca, ignorando maiuscole e minuscole.
/// L'ordine conta: un `title` esplicito vince sul `Title` del legacy.
fn field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object.get(name).or_else(|| {
        object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    })
}

/// Il campo come stringa; stringa vuota se assente. Numeri e booleani sono
/// accettati perché un `"Version": 1.5` scritto a mano non deve far saltare
/// l'intera news.
fn text(object: &Map<String, Value>, name: &str) -> String {
    match field(object, name) {
        Some(Value::String(value)) => value.trim().to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

/// Come [`text`], ma `None` quando il campo manca o è vuoto.
fn optional_text(object: &Map<String, Value>, name: &str) -> Option<String> {
    let value = text(object, name);
    (!value.is_empty()).then_some(value)
}

/// Il campo come booleano, accettando anche `"true"` e `1`.
fn flag(object: &Map<String, Value>, name: &str) -> bool {
    match field(object, name) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => {
            let value = value.trim();
            value.eq_ignore_ascii_case("true") || value == "1"
        }
        Some(Value::Number(value)) => value.as_f64().is_some_and(|number| number != 0.0),
        _ => false,
    }
}

/// News locali usate quando il server non risponde.
pub fn seed() -> Vec<NewsItem> {
    vec![
        NewsItem {
            title: "The new VanzaKart launcher".into(),
            category: "UPDATE".into(),
            version: format!("Launcher v{}", crate::state::LAUNCHER_VERSION),
            date_label: "Local".into(),
            is_pinned: true,
            summary: "# The launcher, rewritten\n\
                      The launcher is now native on **Windows, macOS and Linux**.\n\n\
                      - **Differential updates**: only the changed files are downloaded.\n\
                      - **Your data is safe**: automatic backup and a verified restore before every update.\n\
                      - *The Stable channel and the Beta channel stay installed side by side.*"
                .into(),
            ..Default::default()
        },
        NewsItem {
            title: "The news will be back as soon as the server answers".into(),
            category: "INFO".into(),
            version: String::new(),
            date_label: "Offline".into(),
            is_pinned: false,
            summary: "`news.json` could not be read. \
                      The launcher keeps working as usual: the modpack, the \
                      settings and starting the game do not depend on this page."
                .into(),
            ..Default::default()
        },
    ]
}

/// Ordina mettendo in cima le voci fissate, senza toccare l'ordine del file.
fn sorted(mut items: Vec<NewsItem>) -> Vec<NewsItem> {
    items.sort_by_key(|item| !item.is_pinned);
    items
}

/// Normalizza gli URL dei media.
///
/// Il server VanzaKart pubblica i media su 8443; l'endpoint HTTPS predefinito
/// presenta un certificato che alcuni player rifiutano. Il legacy applicava lo
/// stesso rimpiazzo.
pub fn normalize_media_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url.trim()) else {
        return url.trim().to_string();
    };

    if parsed.scheme() == "https"
        && parsed.host_str() == Some("sitodaking.it")
        && parsed.port().is_none()
        && parsed.set_port(Some(8443)).is_ok()
    {
        return parsed.to_string();
    }

    url.trim().to_string()
}

/// `image`, `video` o `link`. L'estensione decide; quando manca — è il caso
/// degli URL di Unsplash — valgono le stesse euristiche del legacy.
pub fn media_kind(url: &str) -> &'static str {
    const VIDEO: &[&str] = &[".mp4", ".webm", ".wmv", ".avi", ".mov"];
    const IMAGE: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif"];

    let lowered = url.to_ascii_lowercase();
    let path = lowered.split(['?', '#']).next().unwrap_or(lowered.as_str());

    if IMAGE.iter().any(|extension| path.ends_with(extension)) {
        "image"
    } else if VIDEO.iter().any(|extension| path.ends_with(extension)) {
        "video"
    } else if lowered.contains("photo-") || lowered.contains("image") {
        "image"
    } else if lowered.contains("video") {
        "video"
    } else {
        "link"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_contains_no_remote_media() {
        for item in seed() {
            assert!(item.media_path.is_none(), "{}", item.title);
        }
    }

    #[test]
    fn parsing_reads_the_pascal_case_payload_of_the_server() {
        let items = parse(
            r##"[{"Title":"Novità","Category":"UPDATE","Version":"v1.1.3",
                  "DateLabel":"Live","IsPinned":true,"Summary":"# testo",
                  "MediaPath":"https://sitodaking.it/media/clip.mp4"}]"##,
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Novità");
        assert_eq!(items[0].category, "UPDATE");
        assert_eq!(items[0].version, "v1.1.3");
        assert_eq!(items[0].date_label, "Live");
        assert_eq!(items[0].summary, "# testo");
        assert!(items[0].is_pinned);
        assert_eq!(items[0].media_kind.as_deref(), Some("video"));
        assert_eq!(
            items[0].media_path.as_deref(),
            Some("https://sitodaking.it:8443/media/clip.mp4")
        );
    }

    #[test]
    fn parsing_also_reads_the_camel_case_form() {
        let items = parse(
            r#"[{"title":"Novità","category":"UPDATE","summary":"testo",
                 "mediaPath":"https://esempio.test/foto.png","isPinned":false}]"#,
        )
        .unwrap();

        assert_eq!(items[0].title, "Novità");
        assert_eq!(items[0].media_kind.as_deref(), Some("image"));
    }

    #[test]
    fn an_explicit_camel_case_key_wins_over_the_pascal_case_one() {
        let items = parse(r#"[{"Title":"vecchio","title":"nuovo","summary":"x"}]"#).unwrap();
        assert_eq!(items[0].title, "nuovo");
    }

    #[test]
    fn empty_entries_are_dropped_instead_of_becoming_blank_cards() {
        assert!(parse(r#"[{"Autore":"tizio"},{"nulla":1}]"#).is_none());
        assert_eq!(
            parse(r#"[{"nulla":1},{"Title":"buona"}]"#).unwrap().len(),
            1
        );
    }

    #[test]
    fn an_empty_or_broken_payload_yields_none() {
        assert!(parse("[]").is_none());
        assert!(parse("{ non json").is_none());
        assert!(parse(r#"{"non":"una lista"}"#).is_none());
    }

    #[test]
    fn the_utf8_bom_does_not_break_the_payload() {
        assert!(parse("\u{FEFF}[{\"Title\":\"con bom\"}]").is_some());
    }

    #[test]
    fn flags_accept_the_forms_written_by_hand() {
        let items =
            parse(r#"[{"Title":"a","IsPinned":"true"},{"Title":"b","IsPinned":0}]"#).unwrap();
        assert!(items[0].is_pinned);
        assert!(!items[1].is_pinned);
    }

    #[test]
    fn media_urls_on_other_hosts_are_untouched() {
        assert_eq!(
            normalize_media_url("https://altro.example/clip.mp4"),
            "https://altro.example/clip.mp4"
        );
        assert_eq!(
            normalize_media_url("https://sitodaking.it:9000/clip.mp4"),
            "https://sitodaking.it:9000/clip.mp4"
        );
        assert_eq!(normalize_media_url("non un url"), "non un url");
    }

    #[test]
    fn media_kinds_are_recognised() {
        assert_eq!(media_kind("https://a/b.MP4"), "video");
        assert_eq!(media_kind("https://a/b.png?x=1"), "image");
        assert_eq!(media_kind("https://a/pagina"), "link");
    }

    #[test]
    fn media_without_extension_falls_back_to_the_legacy_heuristics() {
        assert_eq!(
            media_kind("https://images.unsplash.com/photo-1551103782-8ab07afd45c1?w=800"),
            "image"
        );
        assert_eq!(media_kind("https://cdn.example/video/12345"), "video");
    }

    #[test]
    fn pinned_items_come_first() {
        let items = sorted(vec![
            NewsItem {
                title: "normale".into(),
                ..Default::default()
            },
            NewsItem {
                title: "fissata".into(),
                is_pinned: true,
                ..Default::default()
            },
        ]);
        assert_eq!(items[0].title, "fissata");
    }
}
