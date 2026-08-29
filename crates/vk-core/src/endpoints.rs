//! Contratto `endpoints.json` e costruzione delle liste di mirror.
//!
//! Replica `Launcher/Models/LauncherEndpointsInfo.cs`,
//! `Launcher/Services/LauncherConfig.cs` e i builder di URL in
//! `MainWindow.xaml.cs` (`BuildModMirrorList`, `BuildModFileMirrorList`,
//! `BuildDifferentialFileUrlCandidates`, `EscapeRelativeUrlPath`,
//! `ResolveParentBaseUrl`, `AddNoCacheQuery`).

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::json::{string_or_array, strip_leading_noise};
use crate::versions::Channel;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointsInfo {
    #[serde(default, rename = "endpoints_url")]
    pub endpoints_url: String,
    #[serde(default, rename = "endpoints_json_url")]
    pub endpoints_json_url: String,
    #[serde(default, rename = "versions_json_url")]
    pub versions_json_url: String,

    #[serde(default, rename = "mod_url")]
    pub mod_url: String,
    #[serde(default, rename = "mod_mirrors", deserialize_with = "string_or_array")]
    pub mod_mirrors: Vec<String>,
    #[serde(default, rename = "mod_manifest_url")]
    pub mod_manifest_url: String,
    #[serde(default, rename = "mod_files_url")]
    pub mod_files_url: String,
    #[serde(
        default,
        rename = "mod_files_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub mod_files_mirrors: Vec<String>,
    #[serde(default, rename = "mod_hash_files_url")]
    pub mod_hash_files_url: String,
    #[serde(
        default,
        rename = "mod_hash_files_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub mod_hash_files_mirrors: Vec<String>,

    #[serde(default, rename = "beta_mod_url")]
    pub beta_mod_url: String,
    #[serde(
        default,
        rename = "beta_mod_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub beta_mod_mirrors: Vec<String>,
    #[serde(default, rename = "beta_mod_manifest_url")]
    pub beta_mod_manifest_url: String,
    #[serde(default, rename = "beta_mod_files_url")]
    pub beta_mod_files_url: String,
    #[serde(
        default,
        rename = "beta_mod_files_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub beta_mod_files_mirrors: Vec<String>,
    #[serde(default, rename = "beta_mod_hash_files_url")]
    pub beta_mod_hash_files_url: String,
    #[serde(
        default,
        rename = "beta_mod_hash_files_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub beta_mod_hash_files_mirrors: Vec<String>,

    #[serde(default, rename = "music_pack_url")]
    pub music_pack_url: String,
    #[serde(
        default,
        rename = "music_pack_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub music_pack_mirrors: Vec<String>,
    #[serde(default, rename = "music_pack_manifest_url")]
    pub music_pack_manifest_url: String,
    #[serde(default, rename = "music_pack_files_url")]
    pub music_pack_files_url: String,
    #[serde(
        default,
        rename = "music_pack_files_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub music_pack_files_mirrors: Vec<String>,

    #[serde(default, rename = "launcher_url")]
    pub launcher_url: String,
    #[serde(
        default,
        rename = "launcher_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub launcher_mirrors: Vec<String>,

    /// Manifest dei pacchetti d'installazione, letto dall'installer
    /// (`install.json`, vedi `vk_install::release`). Non lo usa il launcher:
    /// sta qui perché sia pubblicabile senza rifare l'installer.
    #[serde(default, rename = "launcher_install_url")]
    pub launcher_install_url: String,
    #[serde(
        default,
        rename = "launcher_install_mirrors",
        deserialize_with = "string_or_array"
    )]
    pub launcher_install_mirrors: Vec<String>,

    #[serde(default, rename = "news_url")]
    pub news_url: String,
    #[serde(default, rename = "news_json_url")]
    pub news_json_url: String,
    #[serde(default, rename = "leaderboard_api_url")]
    pub leaderboard_api_url: String,
    #[serde(default, rename = "leaderboard_details_api_url")]
    pub leaderboard_details_api_url: String,
    #[serde(default, rename = "rooms_api_url")]
    pub rooms_api_url: String,
    #[serde(default, rename = "beta_token_verify_api_url")]
    pub beta_token_verify_api_url: String,
    #[serde(default, rename = "download_page_url")]
    pub download_page_url: String,
    #[serde(default, rename = "mii_rendering_archive_url")]
    pub mii_rendering_archive_url: String,
    #[serde(default, rename = "server_base_url")]
    pub server_base_url: String,
    #[serde(default, rename = "rank_images_base_url")]
    pub rank_images_base_url: String,
}

impl EndpointsInfo {
    pub fn parse(raw: &str) -> CoreResult<Self> {
        Ok(serde_json::from_str(strip_leading_noise(raw))?)
    }

    /// URL delle news, con la stessa precedenza del legacy
    /// (`news_url` prima, `news_json_url` come alias).
    pub fn resolved_news_url(&self) -> &str {
        first_non_empty(&[&self.news_url, &self.news_json_url])
    }

    /// URL di `endpoints.json`, con la stessa precedenza del legacy.
    pub fn resolved_endpoints_url(&self) -> &str {
        first_non_empty(&[&self.endpoints_url, &self.endpoints_json_url])
    }

    pub fn mod_url_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_url,
            Channel::Beta => &self.beta_mod_url,
        }
    }

    pub fn mod_mirrors_for(&self, channel: Channel) -> &[String] {
        match channel {
            Channel::Stable => &self.mod_mirrors,
            Channel::Beta => &self.beta_mod_mirrors,
        }
    }

    pub fn manifest_url_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_manifest_url,
            Channel::Beta => &self.beta_mod_manifest_url,
        }
    }

    pub fn files_url_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_files_url,
            Channel::Beta => &self.beta_mod_files_url,
        }
    }

    pub fn files_mirrors_for(&self, channel: Channel) -> &[String] {
        match channel {
            Channel::Stable => &self.mod_files_mirrors,
            Channel::Beta => &self.beta_mod_files_mirrors,
        }
    }

    pub fn hash_files_url_for(&self, channel: Channel) -> &str {
        match channel {
            Channel::Stable => &self.mod_hash_files_url,
            Channel::Beta => &self.beta_mod_hash_files_url,
        }
    }

    pub fn hash_files_mirrors_for(&self, channel: Channel) -> &[String] {
        match channel {
            Channel::Stable => &self.mod_hash_files_mirrors,
            Channel::Beta => &self.beta_mod_hash_files_mirrors,
        }
    }

    /// Fonde `remote` dentro `self` scartando i campi vuoti **e** gli URL non
    /// sicuri, campo per campo. Restituisce i nomi dei campi scartati.
    ///
    /// Rispetto al legacy (`LauncherConfig.ApplyEndpoints`) un endpoint non
    /// valido non contamina la configurazione: vedi `docs/decisions.md` §D-004.
    pub fn merge_remote(&mut self, remote: &EndpointsInfo) -> Vec<String> {
        let mut rejected = Vec::new();

        macro_rules! merge_url {
            ($field:ident) => {
                if !remote.$field.trim().is_empty() {
                    if is_safe_endpoint(&remote.$field) {
                        self.$field = remote.$field.trim().to_string();
                    } else {
                        rejected.push(stringify!($field).to_string());
                    }
                }
            };
        }

        macro_rules! merge_mirrors {
            ($field:ident) => {{
                let (safe, unsafe_count) = partition_safe(&remote.$field);
                if unsafe_count > 0 {
                    rejected.push(format!("{} ({unsafe_count})", stringify!($field)));
                }
                if !safe.is_empty() || !remote.$field.is_empty() {
                    self.$field = safe;
                }
            }};
        }

        merge_url!(endpoints_url);
        merge_url!(endpoints_json_url);
        merge_url!(versions_json_url);
        merge_url!(mod_url);
        merge_url!(mod_manifest_url);
        merge_url!(mod_files_url);
        merge_url!(mod_hash_files_url);
        merge_url!(beta_mod_url);
        merge_url!(beta_mod_manifest_url);
        merge_url!(beta_mod_files_url);
        merge_url!(beta_mod_hash_files_url);
        merge_url!(music_pack_url);
        merge_url!(music_pack_manifest_url);
        merge_url!(music_pack_files_url);
        merge_url!(launcher_url);
        merge_url!(launcher_install_url);
        merge_url!(news_url);
        merge_url!(news_json_url);
        merge_url!(leaderboard_api_url);
        merge_url!(leaderboard_details_api_url);
        merge_url!(rooms_api_url);
        merge_url!(beta_token_verify_api_url);
        merge_url!(download_page_url);
        merge_url!(mii_rendering_archive_url);
        merge_url!(server_base_url);
        merge_url!(rank_images_base_url);

        merge_mirrors!(mod_mirrors);
        merge_mirrors!(mod_files_mirrors);
        merge_mirrors!(mod_hash_files_mirrors);
        merge_mirrors!(beta_mod_mirrors);
        merge_mirrors!(beta_mod_files_mirrors);
        merge_mirrors!(beta_mod_hash_files_mirrors);
        merge_mirrors!(music_pack_mirrors);
        merge_mirrors!(music_pack_files_mirrors);
        merge_mirrors!(launcher_mirrors);
        merge_mirrors!(launcher_install_mirrors);

        rejected
    }
}

fn first_non_empty<'a>(candidates: &[&'a String]) -> &'a str {
    candidates
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

fn partition_safe(values: &[String]) -> (Vec<String>, usize) {
    let mut safe = Vec::new();
    let mut rejected = 0usize;
    for value in values {
        if value.trim().is_empty() {
            continue;
        }
        if is_safe_endpoint(value) {
            safe.push(value.trim().to_string());
        } else {
            rejected += 1;
        }
    }
    (safe, rejected)
}

/// Un endpoint remoto è accettato solo se è HTTPS, con host, senza credenziali.
pub fn is_safe_endpoint(candidate: &str) -> bool {
    let Ok(parsed) = url::Url::parse(candidate.trim()) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|host| !host.is_empty())
        && parsed.username().is_empty()
        && parsed.password().is_none()
}

/// Come [`is_safe_endpoint`], ma restituisce un errore descrittivo.
pub fn require_safe_endpoint(candidate: &str) -> CoreResult<url::Url> {
    if !is_safe_endpoint(candidate) {
        return Err(CoreError::InvalidUrl(crate::redact::redact_url(candidate)));
    }
    url::Url::parse(candidate.trim()).map_err(|_| CoreError::InvalidUrl("invalid URL".into()))
}

// ---------------------------------------------------------------------------
// Costruzione delle liste di mirror
// ---------------------------------------------------------------------------

/// Percent-encoding equivalente a `Uri.EscapeDataString` di .NET:
/// preserva solo i caratteri non riservati `A-Z a-z 0-9 - _ . ~`.
pub fn escape_data_string(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Equivalente di `EscapeRelativeUrlPath`: normalizza i separatori, elimina i
/// segmenti vuoti e percent-encoda ogni segmento.
pub fn escape_relative_url_path(relative: &str) -> String {
    relative
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(escape_data_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// Equivalente di `AddNoCacheQuery`: aggiunge `t=<millis>`.
pub fn add_no_cache_query(url: &str, now_millis: u128) -> String {
    if url.trim().is_empty() {
        return url.to_string();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}t={now_millis}")
}

/// Equivalente di `ResolveParentBaseUrl`: da `.../files` risale a `...`.
pub fn resolve_parent_base_url(files_url: &str) -> String {
    let trimmed = files_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.len() >= 6 && trimmed[trimmed.len() - 6..].eq_ignore_ascii_case("/files") {
        return trimmed[..trimmed.len() - 6].to_string();
    }

    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    let segments: Vec<String> = parsed
        .path()
        .trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    if segments.len() > 1 {
        let parent = format!("/{}", segments[..segments.len() - 1].join("/"));
        parsed.set_path(&parent);
        parsed.set_query(None);
        return parsed.as_str().trim_end_matches('/').to_string();
    }

    trimmed.to_string()
}

/// Percorso hash-addressed usato dal server: `_by_sha256/<hash minuscolo>`.
pub fn hash_addressed_relative_path(sha256: &str) -> String {
    format!("_by_sha256/{}", sha256.trim().to_lowercase())
}

/// Equivalente di `BuildDifferentialFileUrlCandidates`: per ogni base URL
/// produce fino a 4 candidati (escaped/raw × con e senza no-cache).
pub fn differential_file_url_candidates(
    base_url: &str,
    escaped_path: &str,
    raw_path: &str,
    now_millis: u128,
) -> Vec<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Vec::new();
    }

    let escaped_url = format!("{base}/{escaped_path}");
    let raw_url = format!("{base}/{raw_path}");
    let differ = !raw_url.eq_ignore_ascii_case(&escaped_url);

    let mut out = vec![add_no_cache_query(&escaped_url, now_millis)];
    if differ {
        out.push(add_no_cache_query(&raw_url, now_millis));
    }
    out.push(escaped_url);
    if differ {
        out.push(raw_url);
    }
    out
}

/// Descrive da dove scaricare i payload di un canale.
#[derive(Debug, Clone, Default)]
pub struct MirrorPlan {
    pub archive_url: String,
    pub archive_mirrors: Vec<String>,
    pub archive_default: String,
    pub files_url: String,
    pub files_mirrors: Vec<String>,
    pub files_default: String,
    pub hash_files_url: String,
    pub hash_files_mirrors: Vec<String>,
    pub hash_files_default: String,
}

impl MirrorPlan {
    /// Piano per la modpack di un canale, a partire dagli endpoint risolti e
    /// dai default compilati.
    pub fn for_channel(
        resolved: &EndpointsInfo,
        defaults: &EndpointsInfo,
        channel: Channel,
    ) -> Self {
        Self {
            archive_url: resolved.mod_url_for(channel).to_string(),
            archive_mirrors: resolved.mod_mirrors_for(channel).to_vec(),
            archive_default: defaults.mod_url_for(channel).to_string(),
            files_url: resolved.files_url_for(channel).to_string(),
            files_mirrors: resolved.files_mirrors_for(channel).to_vec(),
            files_default: defaults.files_url_for(channel).to_string(),
            hash_files_url: resolved.hash_files_url_for(channel).to_string(),
            hash_files_mirrors: resolved.hash_files_mirrors_for(channel).to_vec(),
            hash_files_default: defaults.hash_files_url_for(channel).to_string(),
        }
    }

    /// Piano per il music pack (che non ha directory `_by_sha256`).
    pub fn for_music_pack(resolved: &EndpointsInfo, defaults: &EndpointsInfo) -> Self {
        Self {
            archive_url: resolved.music_pack_url.clone(),
            archive_mirrors: resolved.music_pack_mirrors.clone(),
            archive_default: defaults.music_pack_url.clone(),
            files_url: resolved.music_pack_files_url.clone(),
            files_mirrors: resolved.music_pack_files_mirrors.clone(),
            files_default: defaults.music_pack_files_url.clone(),
            ..Default::default()
        }
    }

    /// Equivalente di `BuildModMirrorList`.
    pub fn archive_candidates(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_unique(&mut out, &self.archive_url);
        for mirror in &self.archive_mirrors {
            push_unique(&mut out, mirror);
        }
        push_unique(&mut out, &self.archive_default);
        out
    }

    /// Equivalente di `BuildModFileMirrorList`: percorsi diretti in `files/`
    /// seguiti dal fallback hash-addressed in `_by_sha256/`.
    pub fn file_candidates(
        &self,
        relative_path: &str,
        sha256: &str,
        now_millis: u128,
    ) -> Vec<String> {
        let escaped = escape_relative_url_path(relative_path);
        let raw = relative_path.replace('\\', "/");
        let hash_name = sha256.trim().to_lowercase();
        let hash_path = hash_addressed_relative_path(sha256);

        let mut out: Vec<String> = Vec::new();
        let mut push_candidates = |base: &str, escaped_part: &str, raw_part: &str| {
            for candidate in
                differential_file_url_candidates(base, escaped_part, raw_part, now_millis)
            {
                push_unique(&mut out, &candidate);
            }
        };

        // 1. Percorsi diretti dentro files/.
        push_candidates(&self.files_url, &escaped, &raw);
        for mirror in &self.files_mirrors {
            push_candidates(mirror, &escaped, &raw);
        }
        if !self.files_default.is_empty() && self.files_default != self.files_url {
            push_candidates(&self.files_default, &escaped, &raw);
        }

        // 2. Fallback hash-addressed in _by_sha256/.
        if !self.hash_files_url.trim().is_empty() {
            push_candidates(&self.hash_files_url, &hash_name, &hash_name);
        } else if !self.files_url.trim().is_empty() {
            let parent = resolve_parent_base_url(&self.files_url);
            push_candidates(&parent, &hash_path, &hash_path);
        }

        if !self.hash_files_mirrors.is_empty() {
            for mirror in &self.hash_files_mirrors {
                push_candidates(mirror, &hash_name, &hash_name);
            }
        } else {
            for mirror in &self.files_mirrors {
                let parent = resolve_parent_base_url(mirror);
                push_candidates(&parent, &hash_path, &hash_path);
            }
        }

        if !self.hash_files_default.is_empty() && self.hash_files_default != self.hash_files_url {
            push_candidates(&self.hash_files_default, &hash_name, &hash_name);
        }

        out
    }
}

fn push_unique(out: &mut Vec<String>, candidate: &str) {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return;
    }
    if out.iter().any(|item| item.eq_ignore_ascii_case(trimmed)) {
        return;
    }
    out.push(trimmed.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u128 = 1_700_000_000_000;

    fn defaults() -> EndpointsInfo {
        EndpointsInfo {
            mod_url: "https://s.example/Modpack/VanzaKart.zip".into(),
            mod_files_url: "https://s.example/Modpack/files/".into(),
            mod_hash_files_url: "https://s.example/Modpack/_by_sha256/".into(),
            beta_mod_url: "https://s.example/VanzakartBeta/VKBeta.zip".into(),
            beta_mod_files_url: "https://s.example/VanzakartBeta/files/".into(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_the_shipped_endpoints_file() {
        let raw = include_str!("../fixtures/endpoints.sample.json");
        let endpoints = EndpointsInfo::parse(raw).unwrap();
        assert_eq!(
            endpoints.mod_url,
            "https://sitodaking.it:8443/Modpack/VanzaKart.zip"
        );
        assert_eq!(
            endpoints.resolved_news_url(),
            "https://sitodaking.it:8443/Launcher/news.json"
        );
        assert!(endpoints.mod_mirrors.is_empty());
        assert_eq!(
            endpoints.hash_files_url_for(Channel::Beta),
            "https://sitodaking.it:8443/VanzakartBeta/_by_sha256/"
        );
    }

    #[test]
    fn escapes_url_paths_like_dotnet() {
        assert_eq!(
            escape_relative_url_path("Riivolution/My Stuff/a+b.szs"),
            "Riivolution/My%20Stuff/a%2Bb.szs"
        );
        assert_eq!(escape_relative_url_path("a\\\\b//c"), "a/b/c");
        assert_eq!(escape_data_string("aA0-_.~"), "aA0-_.~");
        assert_eq!(escape_data_string("è"), "%C3%A8");
    }

    #[test]
    fn adds_a_no_cache_query() {
        assert_eq!(
            add_no_cache_query("https://a/b", NOW),
            format!("https://a/b?t={NOW}")
        );
        assert_eq!(
            add_no_cache_query("https://a/b?x=1", NOW),
            format!("https://a/b?x=1&t={NOW}")
        );
    }

    #[test]
    fn resolves_the_parent_of_a_files_url() {
        assert_eq!(
            resolve_parent_base_url("https://s.example/Modpack/files/"),
            "https://s.example/Modpack"
        );
        assert_eq!(
            resolve_parent_base_url("https://s.example/Modpack/payload/"),
            "https://s.example/Modpack"
        );
        assert_eq!(
            resolve_parent_base_url("https://s.example/root"),
            "https://s.example/root"
        );
        assert_eq!(resolve_parent_base_url("  "), "");
    }

    #[test]
    fn builds_four_candidates_when_escaping_changes_the_path() {
        let candidates = differential_file_url_candidates(
            "https://s.example/files/",
            "My%20Stuff",
            "My Stuff",
            NOW,
        );
        assert_eq!(
            candidates,
            vec![
                format!("https://s.example/files/My%20Stuff?t={NOW}"),
                format!("https://s.example/files/My Stuff?t={NOW}"),
                "https://s.example/files/My%20Stuff".to_string(),
                "https://s.example/files/My Stuff".to_string(),
            ]
        );
    }

    #[test]
    fn builds_two_candidates_when_escaping_is_a_no_op() {
        let candidates =
            differential_file_url_candidates("https://s.example/files", "a.szs", "a.szs", NOW);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[1], "https://s.example/files/a.szs");
    }

    #[test]
    fn archive_candidates_follow_the_legacy_order() {
        let mut resolved = defaults();
        resolved.mod_url = "https://mirror-a.example/VanzaKart.zip".into();
        resolved.mod_mirrors = vec!["https://mirror-b.example/VanzaKart.zip".into()];

        let plan = MirrorPlan::for_channel(&resolved, &defaults(), Channel::Stable);
        assert_eq!(
            plan.archive_candidates(),
            vec![
                "https://mirror-a.example/VanzaKart.zip",
                "https://mirror-b.example/VanzaKart.zip",
                "https://s.example/Modpack/VanzaKart.zip",
            ]
        );
    }

    #[test]
    fn archive_candidates_deduplicate() {
        let plan = MirrorPlan::for_channel(&defaults(), &defaults(), Channel::Stable);
        assert_eq!(plan.archive_candidates().len(), 1);
    }

    #[test]
    fn file_candidates_end_with_the_hash_addressed_fallback() {
        let plan = MirrorPlan::for_channel(&defaults(), &defaults(), Channel::Stable);
        let sha = "a".repeat(64);
        let candidates = plan.file_candidates("Riivolution/VanzaKart.xml", &sha, NOW);

        assert!(
            candidates[0].starts_with("https://s.example/Modpack/files/Riivolution/VanzaKart.xml")
        );
        assert!(
            candidates
                .iter()
                .any(|c| c == &format!("https://s.example/Modpack/_by_sha256/{sha}")),
            "manca il fallback hash-addressed: {candidates:?}"
        );
    }

    #[test]
    fn file_candidates_derive_the_hash_directory_when_missing() {
        let mut resolved = defaults();
        resolved.beta_mod_hash_files_url.clear();
        let plan = MirrorPlan::for_channel(&resolved, &EndpointsInfo::default(), Channel::Beta);
        let sha = "b".repeat(64);
        let candidates = plan.file_candidates("a.szs", &sha, NOW);

        assert!(
            candidates
                .iter()
                .any(|c| c == &format!("https://s.example/VanzakartBeta/_by_sha256/{sha}")),
            "il parent di files/ non è stato risolto: {candidates:?}"
        );
    }

    #[test]
    fn rejects_unsafe_endpoints() {
        assert!(is_safe_endpoint("https://a.example/x"));
        assert!(!is_safe_endpoint("http://a.example/x"));
        assert!(!is_safe_endpoint("file:///C:/evil"));
        assert!(!is_safe_endpoint("https://user:pw@a.example/x"));
        assert!(!is_safe_endpoint("not a url"));
        assert!(!is_safe_endpoint(""));
    }

    #[test]
    fn merge_keeps_defaults_for_rejected_fields() {
        let mut current = defaults();
        let remote = EndpointsInfo {
            mod_url: "http://insecure.example/VanzaKart.zip".into(),
            mod_files_url: "https://new.example/files/".into(),
            mod_mirrors: vec![
                "https://ok.example/a.zip".into(),
                "ftp://bad.example/a.zip".into(),
            ],
            ..Default::default()
        };

        let rejected = current.merge_remote(&remote);

        assert_eq!(current.mod_url, "https://s.example/Modpack/VanzaKart.zip");
        assert_eq!(current.mod_files_url, "https://new.example/files/");
        assert_eq!(current.mod_mirrors, vec!["https://ok.example/a.zip"]);
        assert!(rejected.iter().any(|item| item == "mod_url"));
        assert!(rejected.iter().any(|item| item.starts_with("mod_mirrors")));
    }

    #[test]
    fn merge_ignores_empty_remote_fields() {
        let mut current = defaults();
        current.merge_remote(&EndpointsInfo::default());
        assert_eq!(current.mod_url, "https://s.example/Modpack/VanzaKart.zip");
    }
}
