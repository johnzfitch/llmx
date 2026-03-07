use crate::model::{Chunk, ChunkKind};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::collections::HashMap;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    chars.div_ceil(4)
}

pub fn detect_kind(path: &str) -> ChunkKind {
    use std::path::Path;

    // Use Path::extension() for robust detection (avoids false positives like "readme.md.txt")
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("md" | "markdown") => ChunkKind::Markdown,
        Some("json") => ChunkKind::Json,
        Some("js" | "ts" | "tsx") => ChunkKind::JavaScript,
        Some("html" | "htm") => ChunkKind::Html,
        Some("xml") => ChunkKind::Text, // XML needs tags preserved, not stripped like HTML
        Some("txt" | "log" | "jsonl" | "csv" | "ini" | "cfg" | "conf") => ChunkKind::Text,
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp") => ChunkKind::Image,
        _ => ChunkKind::Unknown,
    }
}

fn is_all_digits(token: &str) -> bool {
    token.as_bytes().iter().all(|b| b.is_ascii_digit())
}

fn is_all_hex(token: &str) -> bool {
    if token.len() < 16 {
        return false;
    }
    token
        .as_bytes()
        .iter()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn has_vowel(token: &str) -> bool {
    token
        .as_bytes()
        .iter()
        .any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u'))
}

fn is_noise_token(token: &str) -> bool {
    if token.is_empty() {
        return true;
    }

    match token {
        "prev" | "next" | "show" | "more" => return true,
        _ => {}
    }

    if token.len() == 1 && token != "c" && token != "r" {
        return true;
    }

    if token.len() > 64 {
        return true;
    }

    if is_all_hex(token) {
        return true;
    }

    if is_all_digits(token) {
        return token.len() >= 3;
    }

    let digit_count = token.as_bytes().iter().filter(|b| b.is_ascii_digit()).count();
    if digit_count > 0 && token.len() >= 8 && digit_count * 3 >= token.len() * 2 {
        return true;
    }

    if token.len() >= 24 && !has_vowel(token) {
        return true;
    }

    false
}

/// Split a token on CamelCase boundaries.
///
/// Examples:
/// - "HTMLParser" -> ["html", "parser"]
/// - "getUserById" -> ["get", "user", "by", "id"]
/// - "XMLHttpRequest" -> ["xml", "http", "request"]
/// - "simpleword" -> ["simpleword"]
/// - "URL" -> ["url"]
fn split_camel_case(token: &str) -> Vec<String> {
    if token.len() <= 2 {
        return vec![token.to_ascii_lowercase()];
    }

    let bytes = token.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;

    for i in 1..bytes.len() {
        let prev = bytes[i - 1];
        let curr = bytes[i];
        let next = bytes.get(i + 1).copied();

        // Split on:
        // 1. lowercase -> uppercase: "getUser" at U
        // 2. uppercase -> uppercase followed by lowercase: "XMLHttp" at H (to keep "XML" and "Http")
        let should_split = (prev.is_ascii_lowercase() && curr.is_ascii_uppercase())
            || (prev.is_ascii_uppercase()
                && curr.is_ascii_uppercase()
                && next.is_some_and(|n| n.is_ascii_lowercase()));

        if should_split {
            let part = &token[start..i];
            if !part.is_empty() {
                parts.push(part.to_ascii_lowercase());
            }
            start = i;
        }
    }

    // Push remaining
    if start < token.len() {
        let part = &token[start..];
        if !part.is_empty() {
            parts.push(part.to_ascii_lowercase());
        }
    }

    // If we didn't split anything, return the whole token
    if parts.is_empty() {
        vec![token.to_ascii_lowercase()]
    } else {
        parts
    }
}

pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut buf_too_long = false;
    const MAX_TOKEN_LEN: usize = 96;

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if buf.len() < MAX_TOKEN_LEN {
                buf.push(ch); // Keep original case for CamelCase detection
            } else {
                buf_too_long = true;
            }
        } else if !buf.is_empty() {
            let raw_token = std::mem::take(&mut buf);
            if !buf_too_long {
                // Split on CamelCase and add each part
                for part in split_camel_case(&raw_token) {
                    if !is_noise_token(&part) {
                        tokens.push(part);
                    }
                }
            }
            buf_too_long = false;
        }
    }

    if !buf.is_empty() && !buf_too_long {
        for part in split_camel_case(&buf) {
            if !is_noise_token(&part) {
                tokens.push(part);
            }
        }
    }

    tokens
}

pub(crate) fn tokenize_counts(text: &str, counts: &mut HashMap<String, usize>) -> usize {
    const MAX_TOKEN_LEN: usize = 96;
    let mut buf = [0u8; MAX_TOKEN_LEN];
    let mut len = 0usize;
    let mut buf_too_long = false;
    let mut doc_len = 0usize;

    // Flush a raw token buffer, splitting on CamelCase and updating counts
    let flush = |buf: &[u8],
                 len: usize,
                 buf_too_long: bool,
                 counts: &mut HashMap<String, usize>,
                 doc_len: &mut usize| {
        if len == 0 || buf_too_long {
            return;
        }
        // SAFETY: buf only contains ASCII alphanumeric bytes
        let raw_token = unsafe { std::str::from_utf8_unchecked(&buf[..len]) };

        // Split on CamelCase and count each part
        for part in split_camel_case(raw_token) {
            if is_noise_token(&part) {
                continue;
            }
            *doc_len += 1;
            if let Some(value) = counts.get_mut(&part) {
                *value += 1;
            } else {
                counts.insert(part, 1);
            }
        }
    };

    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() {
            if len < MAX_TOKEN_LEN {
                buf[len] = byte; // Keep original case for CamelCase detection
                len += 1;
            } else {
                buf_too_long = true;
            }
        } else if len > 0 {
            flush(&buf, len, buf_too_long, counts, &mut doc_len);
            len = 0;
            buf_too_long = false;
        }
    }

    if len > 0 {
        flush(&buf, len, buf_too_long, counts, &mut doc_len);
    }

    doc_len
}

pub fn snippet(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let mut out = text.chars().take(max_len).collect::<String>();
    out.push_str("...");
    out
}

#[allow(dead_code)]
pub fn redact_secrets(input: &str) -> String {
    let patterns = [
        r"AKIA[0-9A-Z]{16}",
        r"(?i)bearer\s+[a-z0-9._-]+",
        r"(?i)authorization:\s*[^\s]+",
        r"(?i)password\s*[:=]\s*[^\s]+",
    ];
    let mut out = input.to_string();
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            out = re.replace_all(&out, "[REDACTED]").to_string();
        }
    }
    out
}

pub fn short_id(full: &str, len: usize) -> String {
    full.chars().take(len).collect()
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "chunk".to_string()
    } else {
        trimmed
    }
}

pub fn build_chunk_refs(chunks: &[Chunk]) -> BTreeMap<String, String> {
    fn base36(mut value: usize) -> String {
        const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        if value == 0 {
            return "0".to_string();
        }
        let mut out = Vec::new();
        while value > 0 {
            let digit = value % 36;
            out.push(DIGITS[digit]);
            value /= 36;
        }
        out.reverse();
        String::from_utf8(out).unwrap_or_else(|_| "0".to_string())
    }

    // Use short, deterministic refs (`c0001`, base36) to minimize token overhead in
    // `manifest.llm.tsv` and chunk filenames. Ordering is deterministic by path + start_line.
    let mut sorted: Vec<&Chunk> = chunks.iter().collect();
    sorted.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => match a.start_line.cmp(&b.start_line) {
            std::cmp::Ordering::Equal => match a.end_line.cmp(&b.end_line) {
                std::cmp::Ordering::Equal => a.id.cmp(&b.id),
                other => other,
            },
            other => other,
        },
        other => other,
    });

    let width = base36(sorted.len().max(1)).len().max(4);
    let mut refs = BTreeMap::new();
    let mut seen = BTreeSet::new();

    for (idx, chunk) in sorted.into_iter().enumerate() {
        let raw = base36(idx + 1);
        let mut ref_str = format!("c{:0>width$}", raw, width = width);
        if seen.contains(&ref_str) {
            ref_str = format!("c{:0>width$}-{}", raw, idx + 1, width = width);
        }
        seen.insert(ref_str.clone());
        refs.insert(chunk.id.clone(), ref_str);
    }

    refs
}

#[cfg(test)]
mod tests {
    use super::{split_camel_case, tokenize, tokenize_counts};
    use std::collections::HashMap;

    #[test]
    fn test_camelcase_splitting() {
        // Basic CamelCase
        assert_eq!(split_camel_case("getUserById"), vec!["get", "user", "by", "id"]);
        assert_eq!(split_camel_case("HTMLParser"), vec!["html", "parser"]);
        assert_eq!(split_camel_case("XMLHttpRequest"), vec!["xml", "http", "request"]);

        // Single word
        assert_eq!(split_camel_case("simple"), vec!["simple"]);
        assert_eq!(split_camel_case("ALLCAPS"), vec!["allcaps"]);

        // Short tokens
        assert_eq!(split_camel_case("ID"), vec!["id"]);
        assert_eq!(split_camel_case("a"), vec!["a"]);

        // Numbers
        assert_eq!(split_camel_case("getUser123"), vec!["get", "user123"]);

        // Edge cases
        assert_eq!(split_camel_case("URLParser"), vec!["url", "parser"]);
        assert_eq!(split_camel_case("parseURL"), vec!["parse", "url"]);
    }

    #[test]
    fn test_tokenize_with_camelcase() {
        let tokens = tokenize("function getUserById() { return null; }");
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"user".to_string()));
        assert!(tokens.contains(&"by".to_string()));

        let tokens = tokenize("const xhr = new XMLHttpRequest();");
        assert!(tokens.contains(&"xml".to_string()));
        assert!(tokens.contains(&"http".to_string()));
        assert!(tokens.contains(&"request".to_string()));

        let tokens = tokenize("class HTMLParser extends BaseParser {}");
        assert!(tokens.contains(&"html".to_string()));
        assert!(tokens.contains(&"parser".to_string()));
        assert!(tokens.contains(&"base".to_string()));
    }

    #[test]
    fn tokenize_counts_matches_tokenize_doc_len() {
        let inputs = [
            "",
            "Hello world",
            "Prev next show more",
            "abc DEF 123 123 123",
            "sha256: a80e2e953bcd6a2cfe102043d84adfead9f21b4c2f89fa70527eebf4c2cf0821",
            "Mix-of_things-and123symbols",
            "getUserById HTMLParser XMLHttpRequest",
        ];

        for input in inputs {
            let expected = tokenize(input);
            let mut counts = HashMap::new();
            let doc_len = tokenize_counts(input, &mut counts);

            assert_eq!(
                doc_len,
                expected.len(),
                "doc_len mismatch for input: {input}"
            );

            for token in &expected {
                assert!(counts.contains_key(token), "missing token {token} for input: {input}");
            }
        }
    }
}
