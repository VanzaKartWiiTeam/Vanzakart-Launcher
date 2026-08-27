//! Sanitizzazione dei messaggi di log.
//!
//! Il launcher legacy scriveva nei log path utente completi e URL con query
//! string (che possono contenere token). Qui ogni messaggio passa da
//! [`redact`] prima di essere emesso.

use std::sync::OnceLock;

static HOME: OnceLock<Option<String>> = OnceLock::new();

fn home_dir() -> Option<&'static str> {
    HOME.get_or_init(|| {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok()
            .filter(|value| value.len() > 3)
    })
    .as_deref()
}

/// Sostituisce la home directory con `~` e rimuove le query string dagli URL.
pub fn redact(message: &str) -> String {
    let mut out = redact_urls_in_text(message);

    if let Some(home) = home_dir() {
        out = replace_ignore_case(&out, home, "~");
        // Anche la forma con separatori invertiti, che compare nei path
        // normalizzati in stile URL.
        let alt: String = home.replace('\\', "/");
        if alt != home {
            out = replace_ignore_case(&out, &alt, "~");
        }
    }

    out
}

/// Rimuove query string e credenziali da un singolo URL.
pub fn redact_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let base = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

    // Rimuove `user:pass@` se presente.
    match base.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('@') {
            Some((_credentials, host_and_path)) if !rest.starts_with('/') => {
                format!("{scheme}://***@{host_and_path}")
            }
            _ => base.to_string(),
        },
        None => base.to_string(),
    }
}

/// Maschera un token, lasciando visibili solo i primi 2 caratteri.
pub fn redact_token(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let visible: String = trimmed.chars().take(2).collect();
    format!("{visible}***")
}

fn redact_urls_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(index) = find_scheme(rest) {
        out.push_str(&rest[..index]);
        let tail = &rest[index..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ',')
            .unwrap_or(tail.len());
        out.push_str(&redact_url(&tail[..end]));
        rest = &tail[end..];
    }

    out.push_str(rest);
    out
}

fn find_scheme(text: &str) -> Option<usize> {
    let http = text.find("http://");
    let https = text.find("https://");
    match (http, https) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn replace_ignore_case(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }

    let lower_haystack = haystack.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0usize;

    while let Some(found) = lower_haystack[cursor..].find(&lower_needle) {
        let start = cursor + found;
        out.push_str(&haystack[cursor..start]);
        out.push_str(replacement);
        cursor = start + needle.len();
    }

    out.push_str(&haystack[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_query_string() {
        assert_eq!(
            redact_url("https://example.org/a/b.zip?t=1700000000&token=secret"),
            "https://example.org/a/b.zip"
        );
    }

    #[test]
    fn removes_credentials() {
        assert_eq!(
            redact_url("https://user:pass@example.org/file"),
            "https://***@example.org/file"
        );
    }

    #[test]
    fn redacts_urls_inside_a_sentence() {
        let input = "download da https://a.example/b.zip?token=abc fallito";
        assert_eq!(redact(input), "download da https://a.example/b.zip fallito");
    }

    #[test]
    fn masks_tokens() {
        assert_eq!(redact_token("abcdef123456"), "ab***");
        assert_eq!(redact_token("   "), "");
    }

    #[test]
    fn replace_ignore_case_handles_repetitions() {
        assert_eq!(replace_ignore_case("AaAa", "a", "-"), "----");
        assert_eq!(replace_ignore_case("xyz", "q", "-"), "xyz");
    }
}
