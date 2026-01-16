use crate::model::{Chunk, ChunkKind};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    (chars + 3) / 4
}

pub fn detect_kind(path: &str) -> ChunkKind {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        ChunkKind::Markdown
    } else if lower.ends_with(".json") {
        ChunkKind::Json
    } else if lower.ends_with(".js") || lower.ends_with(".ts") || lower.ends_with(".tsx") {
        ChunkKind::JavaScript
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        ChunkKind::Html
    } else if lower.ends_with(".txt") {
        ChunkKind::Text
    } else if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".bmp")
    {
        ChunkKind::Image
    } else {
        ChunkKind::Unknown
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

pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut buf_too_long = false;
    const MAX_TOKEN_LEN: usize = 96;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if buf.len() < MAX_TOKEN_LEN {
                buf.push(ch.to_ascii_lowercase());
            } else {
                buf_too_long = true;
            }
        } else if !buf.is_empty() {
            let token = std::mem::take(&mut buf);
            if !buf_too_long && !is_noise_token(&token) {
                tokens.push(token);
            }
            buf_too_long = false;
        }
    }
    if !buf.is_empty() {
        if !buf_too_long && !is_noise_token(&buf) {
            tokens.push(buf);
        }
    }
    tokens
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
    let mut refs = BTreeMap::new();
    let mut seen = BTreeSet::new();

    for chunk in chunks {
        let mut prefix_len = 12usize.min(chunk.id.len());
        let mut ref_str = chunk.id[..prefix_len].to_string();
        while seen.contains(&ref_str) && prefix_len < chunk.id.len() {
            prefix_len = (prefix_len + 4).min(chunk.id.len());
            ref_str = chunk.id[..prefix_len].to_string();
        }
        if seen.contains(&ref_str) {
            ref_str = format!("{}-{}", chunk.id, chunk.chunk_index);
        }
        seen.insert(ref_str.clone());
        refs.insert(chunk.id.clone(), ref_str);
    }

    refs
}
