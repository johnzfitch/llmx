use crate::model::{Chunk, FileMeta, IndexStats, SearchFilters, SearchResult, TermEntry};
use crate::util::{snippet, tokenize};
use std::collections::{BTreeMap, HashMap};

pub fn build_inverted_index(chunks: &[Chunk]) -> BTreeMap<String, TermEntry> {
    let mut term_map: BTreeMap<String, Vec<(String, usize, usize)>> = BTreeMap::new();
    for chunk in chunks {
        let tokens = tokenize(&chunk.content);
        if tokens.is_empty() {
            continue;
        }
        let doc_len = tokens.len();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for token in tokens {
            *counts.entry(token).or_insert(0) += 1;
        }
        for (token, tf) in counts {
            term_map
                .entry(token)
                .or_default()
                .push((chunk.id.clone(), tf, doc_len));
        }
    }

    let mut inverted = BTreeMap::new();
    for (term, postings) in term_map {
        let mut postings_sorted = postings;
        postings_sorted.sort_by(|a, b| a.0.cmp(&b.0));
        inverted.insert(
            term,
            TermEntry {
                df: postings_sorted.len(),
                postings: postings_sorted
                    .into_iter()
                    .map(|(chunk_id, tf, doc_len)| crate::model::Posting { chunk_id, tf, doc_len })
                    .collect(),
            },
        );
    }

    inverted
}

pub fn compute_stats(files: &[FileMeta], chunks: &[Chunk]) -> IndexStats {
    let total_chunks = chunks.len();
    let total_files = files.len();
    let avg_chunk_chars = if total_chunks == 0 {
        0
    } else {
        chunks.iter().map(|c| c.content.len()).sum::<usize>() / total_chunks
    };
    let avg_chunk_tokens = if total_chunks == 0 {
        0
    } else {
        chunks.iter().map(|c| c.token_estimate).sum::<usize>() / total_chunks
    };
    IndexStats {
        total_files,
        total_chunks,
        avg_chunk_chars,
        avg_chunk_tokens,
    }
}

pub fn search_index(
    chunks: &[Chunk],
    inverted: &BTreeMap<String, TermEntry>,
    chunk_refs: &BTreeMap<String, String>,
    query: &str,
    filters: &SearchFilters,
    limit: usize,
) -> Vec<SearchResult> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut chunk_map: HashMap<&str, &Chunk> = HashMap::new();
    for chunk in chunks {
        chunk_map.insert(chunk.id.as_str(), chunk);
    }

    let doc_count = chunks.len().max(1) as f32;
    let avg_doc_len = if chunks.is_empty() {
        1.0
    } else {
        chunks.iter().map(|c| c.token_estimate).sum::<usize>() as f32 / chunks.len() as f32
    };

    let k1 = 1.5f32;
    let b = 0.75f32;
    let mut scores: HashMap<String, f32> = HashMap::new();

    for token in tokens {
        if let Some(entry) = inverted.get(&token) {
            let df = entry.df as f32;
            let idf = ((doc_count - df + 0.5) / (df + 0.5) + 1.0).ln();
            for posting in &entry.postings {
                if let Some(chunk) = chunk_map.get(posting.chunk_id.as_str()) {
                    if !passes_filters(chunk, filters) {
                        continue;
                    }
                    let doc_len = posting.doc_len as f32;
                    let tf = posting.tf as f32;
                    let denom = tf + k1 * (1.0 - b + b * (doc_len / avg_doc_len));
                    let score = idf * (tf * (k1 + 1.0)) / denom;
                    *scores.entry(posting.chunk_id.clone()).or_insert(0.0) += score;
                }
            }
        }
    }

    let mut results: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(chunk_id, score)| {
            let chunk = chunk_map.get(chunk_id.as_str())?;
            let chunk_ref = chunk_refs
                .get(chunk_id.as_str())
                .cloned()
                .unwrap_or_else(|| chunk.short_id.clone());
            Some(SearchResult {
                chunk_id: chunk_id.clone(),
                chunk_ref,
                score,
                path: chunk.path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                snippet: snippet(&chunk.content, 200),
                heading_path: chunk.heading_path.clone(),
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

fn passes_filters(chunk: &Chunk, filters: &SearchFilters) -> bool {
    if let Some(exact) = &filters.path_exact {
        if chunk.path != *exact {
            return false;
        }
    }
    if let Some(prefix) = &filters.path_prefix {
        if !chunk.path.starts_with(prefix) {
            return false;
        }
    }
    if let Some(kind) = filters.kind {
        if chunk.kind != kind {
            return false;
        }
    }
    if let Some(prefix) = &filters.heading_prefix {
        let heading = chunk.heading_path.join("/");
        if !heading.starts_with(prefix) {
            return false;
        }
    }
    if let Some(prefix) = &filters.symbol_prefix {
        if let Some(symbol) = &chunk.symbol {
            if !symbol.starts_with(prefix) {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

pub fn list_outline(chunks: &[Chunk], path: &str) -> Vec<String> {
    let mut seen = BTreeMap::new();
    for chunk in chunks {
        if chunk.path != path {
            continue;
        }
        if !chunk.heading_path.is_empty() {
            let key = chunk.heading_path.join("/");
            seen.insert(key, ());
        }
    }
    seen.keys().cloned().collect()
}

pub fn list_symbols(chunks: &[Chunk], path: &str) -> Vec<String> {
    let mut seen = BTreeMap::new();
    for chunk in chunks {
        if chunk.path != path {
            continue;
        }
        if let Some(symbol) = &chunk.symbol {
            seen.insert(symbol.clone(), ());
        }
    }
    seen.keys().cloned().collect()
}
