use crate::model::{Chunk, FileMeta, IndexStats, SearchFilters, SearchResult, TermEntry};
use crate::util::{snippet, tokenize};
use std::collections::{BTreeMap, HashMap};

#[cfg(feature = "embeddings")]
use crate::embeddings::cosine_similarity;
#[cfg(feature = "embeddings")]
use std::collections::HashSet;

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

/// Vector search using cosine similarity.
///
/// Returns chunks sorted by similarity to query embedding (highest first).
#[cfg(feature = "embeddings")]
pub fn vector_search(
    chunks: &[Chunk],
    chunk_refs: &BTreeMap<String, String>,
    embeddings: &[Vec<f32>],
    query_embedding: &[f32],
    filters: &SearchFilters,
    limit: usize,
) -> Vec<SearchResult> {
    if embeddings.len() != chunks.len() {
        return Vec::new();
    }

    let mut results: Vec<(usize, f32)> = Vec::new();

    for (idx, chunk) in chunks.iter().enumerate() {
        if !passes_filters(chunk, filters) {
            continue;
        }

        let similarity = cosine_similarity(&embeddings[idx], query_embedding);
        results.push((idx, similarity));
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
            }
        })
        .collect()
}

/// Phase 6: Hybrid search combining BM25 and semantic similarity.
///
/// Supports two strategies:
/// - RRF (Reciprocal Rank Fusion): More robust, doesn't require normalization (default)
/// - Linear: Weighted combination of normalized scores (Phase 5 compatibility)
#[cfg(feature = "embeddings")]
pub fn hybrid_search(
    chunks: &[Chunk],
    inverted: &BTreeMap<String, TermEntry>,
    chunk_refs: &BTreeMap<String, String>,
    embeddings: &[Vec<f32>],
    query: &str,
    query_embedding: &[f32],
    filters: &SearchFilters,
    limit: usize,
) -> Vec<SearchResult> {
    hybrid_search_with_strategy(
        chunks,
        inverted,
        chunk_refs,
        embeddings,
        query,
        query_embedding,
        filters,
        limit,
        crate::model::HybridStrategy::Rrf,
    )
}

/// Phase 6: Hybrid search with configurable strategy
#[cfg(feature = "embeddings")]
pub fn hybrid_search_with_strategy(
    chunks: &[Chunk],
    inverted: &BTreeMap<String, TermEntry>,
    chunk_refs: &BTreeMap<String, String>,
    embeddings: &[Vec<f32>],
    query: &str,
    query_embedding: &[f32],
    filters: &SearchFilters,
    limit: usize,
    strategy: crate::model::HybridStrategy,
) -> Vec<SearchResult> {
    use crate::model::HybridStrategy;

    // Get results from both search methods
    let bm25_results = search_index(chunks, inverted, chunk_refs, query, filters, limit * 2);
    let semantic_results = vector_search(chunks, chunk_refs, embeddings, query_embedding, filters, limit * 2);

    match strategy {
        HybridStrategy::Rrf => {
            use crate::rrf::{rrf_fusion, to_ranked_results, RrfConfig};

            // Convert scored results to ranked results for RRF
            let bm25_scored: Vec<(&str, f32)> = bm25_results
                .iter()
                .map(|r| (r.chunk_id.as_str(), r.score))
                .collect();
            let semantic_scored: Vec<(&str, f32)> = semantic_results
                .iter()
                .map(|r| (r.chunk_id.as_str(), r.score))
                .collect();

            let bm25_ranked = to_ranked_results(&bm25_scored);
            let semantic_ranked = to_ranked_results(&semantic_scored);

            let merged = rrf_fusion(
                vec![bm25_ranked, semantic_ranked],
                RrfConfig::default(),
                limit,
            );

            // Convert RRF results back to SearchResult format
            let mut hybrid_results: Vec<SearchResult> = Vec::new();
            for (chunk_id, rrf_score) in merged {
                if let Some(chunk) = chunks.iter().find(|c| c.id == chunk_id) {
                    let chunk_ref = chunk_refs
                        .get(chunk_id.as_str())
                        .cloned()
                        .unwrap_or_else(|| chunk.short_id.clone());
                    hybrid_results.push(SearchResult {
                        chunk_id: chunk_id.clone(),
                        chunk_ref,
                        score: rrf_score,
                        path: chunk.path.clone(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        snippet: snippet(&chunk.content, 200),
                        heading_path: chunk.heading_path.clone(),
                    });
                }
            }
            hybrid_results
        }
        HybridStrategy::Linear => {
            // Phase 5 linear combination (backward compatibility)
            let mut bm25_map: HashMap<String, f32> = HashMap::new();
            let mut max_bm25 = 0.0f32;
            for result in &bm25_results {
                max_bm25 = max_bm25.max(result.score);
                bm25_map.insert(result.chunk_id.clone(), result.score);
            }

            let mut semantic_map: HashMap<String, f32> = HashMap::new();
            for result in &semantic_results {
                semantic_map.insert(result.chunk_id.clone(), result.score);
            }

            let mut all_chunk_ids: HashSet<String> = HashSet::new();
            for result in &bm25_results {
                all_chunk_ids.insert(result.chunk_id.clone());
            }
            for result in &semantic_results {
                all_chunk_ids.insert(result.chunk_id.clone());
            }

            let mut hybrid_results: Vec<SearchResult> = Vec::new();
            for chunk_id in all_chunk_ids {
                let bm25_score = bm25_map.get(&chunk_id).copied().unwrap_or(0.0);
                let semantic_score = semantic_map.get(&chunk_id).copied().unwrap_or(0.0);

                let normalized_bm25 = if max_bm25 > 0.0 {
                    bm25_score / max_bm25
                } else {
                    0.0
                };

                let final_score = 0.5 * normalized_bm25 + 0.5 * semantic_score;

                if let Some(chunk) = chunks.iter().find(|c| c.id == chunk_id) {
                    let chunk_ref = chunk_refs
                        .get(chunk_id.as_str())
                        .cloned()
                        .unwrap_or_else(|| chunk.short_id.clone());
                    hybrid_results.push(SearchResult {
                        chunk_id: chunk_id.clone(),
                        chunk_ref,
                        score: final_score,
                        path: chunk.path.clone(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        snippet: snippet(&chunk.content, 200),
                        heading_path: chunk.heading_path.clone(),
                    });
                }
            }

            hybrid_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            hybrid_results.truncate(limit);
            hybrid_results
        }
    }
}
