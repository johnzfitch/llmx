//! Phase 7: Symbol search engine — fast fuzzy symbol lookup.
//!
//! This engine bypasses text search entirely. It matches queries against
//! defined symbol names using Jaro-Winkler similarity with prefix bonus,
//! cross-convention matching (camelCase ↔ snake_case), and qualified name
//! awareness (matching "verify_token" against "auth::jwt::verify_token").

use crate::model::{Chunk, SearchFilters, SearchResult};
use crate::index::passes_filters;
use crate::query::symbol_variations;
use crate::util::snippet;
use std::collections::BTreeMap;

/// Search for chunks by symbol name with fuzzy matching.
///
/// Scoring:
/// - Exact match: 1.0
/// - Prefix match: 0.9
/// - Qualified name tail match: 0.85
/// - Convention variation match: 0.8
/// - Jaro-Winkler fuzzy match: scaled 0.0-0.7
pub fn symbol_search(
    chunks: &[Chunk],
    chunk_refs: &BTreeMap<String, String>,
    query: &str,
    filters: &SearchFilters,
    limit: usize,
) -> Vec<SearchResult> {
    let query_lower = query.to_ascii_lowercase();
    let variations: Vec<String> = symbol_variations(query)
        .into_iter()
        .map(|v| v.to_ascii_lowercase())
        .collect();

    let mut results: Vec<(usize, f32)> = Vec::new();

    for (idx, chunk) in chunks.iter().enumerate() {
        if !passes_filters(chunk, filters) {
            continue;
        }

        // Try all symbol-like fields
        let best_score = best_symbol_score(chunk, &query_lower, &variations);

        if best_score > 0.3 {
            results.push((idx, best_score));
        }
    }

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    results
        .into_iter()
        .map(|(idx, score)| {
            let chunk = &chunks[idx];
            let chunk_ref = chunk_refs
                .get(chunk.id.as_str())
                .cloned()
                .unwrap_or_else(|| chunk.short_id.clone());
            SearchResult {
                chunk_id: chunk.id.clone(),
                chunk_ref,
                score,
                path: chunk.path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                snippet: snippet(&chunk.content, 200),
                heading_path: chunk.heading_path.clone(),
                match_reason: Some(format!(
                    "Symbol: {}",
                    chunk.symbol.as_deref().or(chunk.qualified_name.as_deref()).unwrap_or("?")
                )),
                matched_engines: vec!["symbol".to_string()],
            }
        })
        .collect()
}

/// Compute the best symbol match score across all symbol-bearing fields of a chunk.
fn best_symbol_score(chunk: &Chunk, query_lower: &str, variations: &[String]) -> f32 {
    let mut best = 0.0f32;

    // Check primary symbol field
    if let Some(ref symbol) = chunk.symbol {
        let score = score_symbol_match(symbol, query_lower, variations);
        best = best.max(score);
    }

    // Check qualified name
    if let Some(ref qname) = chunk.qualified_name {
        let score = score_symbol_match(qname, query_lower, variations);
        best = best.max(score);

        // Also check the tail segment of qualified name
        if let Some(tail) = qname.rsplit("::").next() {
            let score = score_symbol_match(tail, query_lower, variations);
            // Slight penalty for partial path match
            best = best.max(score * 0.95);
        }
    }

    // Check exports
    for export in &chunk.exports {
        let score = score_symbol_match(export, query_lower, variations) * 0.9;
        best = best.max(score);
    }

    best
}

/// Score how well a symbol matches a query.
fn score_symbol_match(symbol: &str, query_lower: &str, variations: &[String]) -> f32 {
    let symbol_lower = symbol.to_ascii_lowercase();

    // Exact match (case-insensitive)
    if symbol_lower == query_lower {
        return 1.0;
    }

    // Prefix match
    if symbol_lower.starts_with(query_lower) {
        let ratio = query_lower.len() as f32 / symbol_lower.len() as f32;
        return 0.85 + 0.1 * ratio; // 0.85-0.95 range
    }

    // Contains match (query is a substring)
    if symbol_lower.contains(query_lower) {
        return 0.75;
    }

    // Convention variation match (camelCase ↔ snake_case)
    for var in variations {
        if symbol_lower == *var {
            return 0.8;
        }
        if symbol_lower.starts_with(var.as_str()) {
            return 0.7;
        }
        if symbol_lower.contains(var.as_str()) {
            return 0.6;
        }
    }

    // Jaro-Winkler fuzzy match
    let jw = jaro_winkler(&symbol_lower, query_lower);
    if jw > 0.85 {
        return jw * 0.7; // Scale to 0.0-0.7 range
    }

    0.0
}

/// Jaro-Winkler string similarity (0.0 to 1.0).
///
/// Optimized for symbol names: gives bonus for matching prefixes,
/// which is exactly what we want for partial symbol lookup.
fn jaro_winkler(s1: &str, s2: &str) -> f32 {
    let jaro = jaro_similarity(s1, s2);

    // Winkler prefix bonus: up to 4 chars of common prefix
    let prefix_len = s1
        .chars()
        .zip(s2.chars())
        .take(4)
        .take_while(|(a, b)| a == b)
        .count();

    let p = 0.1; // Standard Winkler scaling factor
    jaro + (prefix_len as f32 * p * (1.0 - jaro))
}

/// Jaro string similarity.
fn jaro_similarity(s1: &str, s2: &str) -> f32 {
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let len1 = s1_chars.len();
    let len2 = s2_chars.len();

    let match_distance = (len1.max(len2) / 2).saturating_sub(1);

    let mut s1_matched = vec![false; len1];
    let mut s2_matched = vec![false; len2];

    let mut matches = 0usize;
    let mut transpositions = 0usize;

    // Find matches
    for i in 0..len1 {
        let start = i.saturating_sub(match_distance);
        let end = (i + match_distance + 1).min(len2);

        for j in start..end {
            if s2_matched[j] || s1_chars[i] != s2_chars[j] {
                continue;
            }
            s1_matched[i] = true;
            s2_matched[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    // Count transpositions
    let mut k = 0;
    for i in 0..len1 {
        if !s1_matched[i] {
            continue;
        }
        while !s2_matched[k] {
            k += 1;
        }
        if s1_chars[i] != s2_chars[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let m = matches as f32;
    (m / len1 as f32 + m / len2 as f32 + (m - transpositions as f32 / 2.0) / m) / 3.0
}

/// Build a symbol definition index for fast lookup.
///
/// Returns a map from lowercase symbol name → list of chunk indices that define it.
pub fn build_symbol_index(chunks: &[Chunk]) -> BTreeMap<String, Vec<usize>> {
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if let Some(ref symbol) = chunk.symbol {
            index
                .entry(symbol.to_ascii_lowercase())
                .or_default()
                .push(i);
        }
        if let Some(ref qname) = chunk.qualified_name {
            index
                .entry(qname.to_ascii_lowercase())
                .or_default()
                .push(i);
            // Also index the tail segment
            if let Some(tail) = qname.rsplit("::").next() {
                let tail_lower = tail.to_ascii_lowercase();
                if tail_lower != qname.to_ascii_lowercase() {
                    index.entry(tail_lower).or_default().push(i);
                }
            }
        }
        for export in &chunk.exports {
            index
                .entry(export.to_ascii_lowercase())
                .or_default()
                .push(i);
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jaro_winkler_exact() {
        assert!((jaro_winkler("hello", "hello") - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_jaro_winkler_similar() {
        let score = jaro_winkler("verify_token", "verify_tok");
        assert!(score > 0.9, "Expected > 0.9, got {}", score);
    }

    #[test]
    fn test_jaro_winkler_different() {
        let score = jaro_winkler("hello", "world");
        assert!(score < 0.5, "Expected < 0.5, got {}", score);
    }

    #[test]
    fn test_score_symbol_exact() {
        let score = score_symbol_match("verify_token", "verify_token", &[]);
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_score_symbol_prefix() {
        let score = score_symbol_match("verifyToken", "verify", &[]);
        assert!(score > 0.85, "Expected > 0.85, got {}", score);
    }

    #[test]
    fn test_score_symbol_convention_variation() {
        let variations = vec!["verify_token".to_string()];
        let score = score_symbol_match("verify_token", "verifytoken", &variations);
        assert!(score >= 0.8, "Expected >= 0.8, got {}", score);
    }

    #[test]
    fn test_symbol_search_basic() {
        let chunks = vec![
            make_chunk("0", "auth/jwt.rs", Some("verify_token"), "fn verify_token() {}"),
            make_chunk("1", "auth/jwt.rs", Some("Claims"), "struct Claims {}"),
            make_chunk("2", "db/pool.rs", Some("get_connection"), "fn get_connection() {}"),
        ];
        let refs = BTreeMap::new();
        let filters = SearchFilters::default();

        let results = symbol_search(&chunks, &refs, "verify_token", &filters, 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_id, "0");
    }

    fn make_chunk(id: &str, path: &str, symbol: Option<&str>, content: &str) -> Chunk {
        Chunk {
            id: id.to_string(),
            short_id: id.to_string(),
            slug: id.to_string(),
            path: path.to_string(),
            root_path: String::new(),
            relative_path: path.to_string(),
            kind: crate::model::ChunkKind::JavaScript,
            language: Some(crate::model::LanguageId::JavaScript),
            chunk_index: 0,
            start_line: 1,
            end_line: 1,
            content: content.to_string(),
            content_hash: String::new(),
            token_estimate: 10,
            heading_path: Vec::new(),
            symbol: symbol.map(|s| s.to_string()),
            address: None,
            asset_path: None,
            is_generated: false,
            quality_score: None,
            resolution_tier: crate::model::ResolutionTier::GenericTreeSitter,
            ast_kind: None,
            qualified_name: None,
            symbol_id: None,
            symbol_tail: None,
            signature: None,
            module_path: None,
            parent_symbol: None,
            visibility: None,
            imports: Vec::new(),
            exports: Vec::new(),
            calls: Vec::new(),
            type_refs: Vec::new(),
            doc_summary: None,
        }
    }
}
