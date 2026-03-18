#![recursion_limit = "512"]

mod chunk;
mod embedding_store;
mod export;
pub mod handlers;
mod index;
mod model;
pub mod pathnorm;
pub mod util;
pub mod walk;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "embeddings")]
mod bert;

#[cfg(feature = "embeddings")]
pub mod embeddings;

// Phase 6: Reciprocal Rank Fusion for hybrid search
pub mod rrf;

// Phase 7: Query intelligence, symbol search, and code graph
pub mod query;
pub mod symbol_search;
pub mod graph;

pub use crate::export::{
    export_catalog_llm_md, export_chunks, export_chunks_compact, export_llm, export_llm_pointer,
    export_manifest_json, export_manifest_llm_tsv, export_manifest_min_json, export_zip, export_zip_compact,
};
pub use crate::index::{build_inverted_index, compute_stats, list_outline, list_symbols, search_index};
#[cfg(feature = "embeddings")]
pub use crate::index::{hybrid_search, hybrid_search_with_strategy, vector_search};
pub use crate::model::*;
use crate::query::{classify_intent, expand_synonyms, explain_match, weights_for_intent};
use crate::rrf::{to_ranked_results, weighted_rrf_fusion, RrfConfig};
use crate::symbol_search::symbol_search;
use crate::pathnorm::{infer_root_path, normalize_root_path};
use crate::util::{build_chunk_refs, detect_kind, detect_language, sha256_hex};
use crate::graph::build_structural_indexes;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Return the default storage directory for llmx indexes.
///
/// Follows the platform data directory convention: XDG Base Directory Specification on
/// Linux (`$XDG_DATA_HOME/llmx/indexes`), `~/Library/Application Support/llmx/indexes`
/// on macOS, and the local app data directory on Windows.
///
/// Falls back to `~/.local/share/llmx/indexes` (or `~/.llmx/indexes` as last resort)
/// if the platform data directory cannot be determined.
///
/// On first run, migrates indexes from all legacy paths (`~/.llmx_mcp/indexes`,
/// `~/.llmx/indexes`) by merging their registries into the new location.
pub fn default_storage_dir() -> PathBuf {
    let new_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| {
            // Absolute last resort when neither data_dir nor home_dir resolve
            dirs::home_dir()
                .map(|h| h.join(".llmx"))
                .unwrap_or_else(|| PathBuf::from(".llmx"))
        })
        .join("llmx")
        .join("indexes");

    // Ensure the target directory structure exists
    if let Some(parent) = new_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Collect legacy paths that contain a registry
    let legacy_paths: Vec<PathBuf> = dirs::home_dir()
        .into_iter()
        .flat_map(|h| {
            vec![
                h.join(".llmx_mcp").join("indexes"),
                h.join(".llmx").join("indexes"),
            ]
        })
        .filter(|p| p.join("registry.json").exists() && *p != new_dir)
        .collect();

    if legacy_paths.is_empty() {
        return new_dir;
    }

    // Migrate all legacy stores into new_dir
    for legacy in &legacy_paths {
        migrate_legacy_store(legacy, &new_dir);
    }

    new_dir
}

/// Migrate a single legacy index store into `dest`, merging registries and copying
/// index files. Uses rename when possible, falling back to copy+delete for cross-fs.
fn migrate_legacy_store(src: &Path, dest: &Path) {
    let src_registry_path = src.join("registry.json");
    let dest_registry_path = dest.join("registry.json");

    // Load source registry
    let src_registry: serde_json::Value = match std::fs::read_to_string(&src_registry_path) {
        Ok(data) => match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("llmx: skipping {}: invalid registry: {e}", src.display());
                return;
            }
        },
        Err(e) => {
            eprintln!("llmx: skipping {}: cannot read registry: {e}", src.display());
            return;
        }
    };

    // Load or create dest registry
    let mut dest_registry: serde_json::Value = std::fs::read_to_string(&dest_registry_path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_else(|| serde_json::json!({"indexes": {}}));

    // Merge: copy each index file from src to dest, then merge registry entries
    if let Some(src_indexes) = src_registry.get("indexes").and_then(|v| v.as_object()) {
        let dest_indexes = dest_registry
            .as_object_mut()
            .and_then(|o| o.get_mut("indexes"))
            .and_then(|v| v.as_object_mut());

        let dest_indexes = match dest_indexes {
            Some(m) => m,
            None => {
                eprintln!("llmx: skipping {}: cannot parse dest registry", src.display());
                return;
            }
        };

        for (key, meta) in src_indexes {
            // Get the index id to find the corresponding .json file
            let index_id = meta.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if index_id.is_empty() {
                continue;
            }

            let src_file = src.join(format!("{index_id}.json"));
            let dest_file = dest.join(format!("{index_id}.json"));

            if src_file.exists() && !dest_file.exists() {
                // Try rename first (fast, same filesystem)
                if std::fs::rename(&src_file, &dest_file).is_err() {
                    // Cross-filesystem: copy then delete
                    match std::fs::copy(&src_file, &dest_file) {
                        Ok(_) => { let _ = std::fs::remove_file(&src_file); }
                        Err(e) => {
                            eprintln!(
                                "llmx: failed to migrate index {index_id} from {}: {e}",
                                src.display(),
                            );
                            continue;
                        }
                    }
                }

                // Also migrate embeddings file if present
                let src_emb = src.join(format!("{index_id}.embeddings"));
                let dest_emb = dest.join(format!("{index_id}.embeddings"));
                if src_emb.exists() && !dest_emb.exists() {
                    if std::fs::rename(&src_emb, &dest_emb).is_err() {
                        match std::fs::copy(&src_emb, &dest_emb) {
                            Ok(_) => { let _ = std::fs::remove_file(&src_emb); }
                            Err(e) => {
                                eprintln!("llmx: failed to migrate embeddings for {index_id}: {e}");
                            }
                        }
                    }
                }
            }

            // Merge registry entry (don't overwrite existing entries in dest)
            if !dest_indexes.contains_key(key) {
                dest_indexes.insert(key.clone(), meta.clone());
            }
        }
    }

    // Write merged registry
    match serde_json::to_string_pretty(&dest_registry) {
        Ok(data) => {
            // Atomic write: write to temp then rename
            let tmp_path = dest.join(".registry.json.tmp");
            if std::fs::write(&tmp_path, &data).is_ok() {
                if let Err(e) = std::fs::rename(&tmp_path, &dest_registry_path) {
                    eprintln!("llmx: failed to write merged registry: {e}");
                    let _ = std::fs::remove_file(&tmp_path);
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("llmx: failed to serialize merged registry: {e}");
            return;
        }
    }

    // Clean up source: remove registry and try to remove empty directory
    let _ = std::fs::remove_file(&src_registry_path);
    // Only remove the directory if it's now empty
    if is_dir_empty(src) {
        let _ = std::fs::remove_dir(src);
        // Try to remove parent too (e.g. ~/.llmx_mcp/) if empty
        if let Some(parent) = src.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    eprintln!(
        "llmx: migrated indexes from {} to {}",
        src.display(),
        dest.display(),
    );
}

fn is_dir_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

fn prepare_root_path(files: &[FileInput], root_hint: Option<&Path>) -> String {
    if let Some(root) = root_hint {
        return normalize_root_path(root);
    }

    let absolute_paths: Vec<PathBuf> = files
        .iter()
        .map(|file| PathBuf::from(&file.path))
        .filter(|path| path.is_absolute())
        .collect();

    infer_root_path(&absolute_paths)
        .map(|root| normalize_root_path(&root))
        .unwrap_or_default()
}

fn stamp_chunk_metadata(
    chunks: &mut [Chunk],
    root_path: &str,
    path: &str,
    language: Option<LanguageId>,
    resolution_tier: ResolutionTier,
) {
    for chunk in chunks {
        chunk.path = path.to_string();
        chunk.root_path = root_path.to_string();
        chunk.relative_path = path.to_string();
        chunk.language = language.clone();
        chunk.resolution_tier = resolution_tier;
    }
}

fn ingest_one_file(
    file: FileInput,
    options: &IngestOptions,
    root_path: &str,
    warnings: &mut Vec<IngestWarning>,
) -> Option<(FileMeta, Vec<Chunk>, usize)> {
    let path = file.path;
    let data = file.data;
    let mtime_ms = file.mtime_ms;
    let fingerprint_sha256 = file.fingerprint_sha256;

    if data.len() > options.max_file_bytes {
        warnings.push(IngestWarning {
            path: path.clone(),
            code: "max_file_bytes".to_string(),
            message: "File size limit exceeded; file skipped.".to_string(),
        });
        return None;
    }

    let kind = detect_kind(&path);
    let language = detect_language(&path);
    let resolution_tier = if language.is_some() {
        ResolutionTier::GenericTreeSitter
    } else {
        ResolutionTier::TextOnly
    };
    let file_hash = sha256_hex(&data);
    let bytes_len = data.len();

    let (line_count, mut file_chunks) = if kind == ChunkKind::Image {
        let mut chunks = chunk::chunk_file(&path, "", kind, options);
        for chunk in &mut chunks {
            chunk.asset_path = Some(format!("images/{}", sanitize_zip_path(&path)));
        }
        (1usize, chunks)
    } else {
        let text = match String::from_utf8(data) {
            Ok(text) => text,
            Err(_) => {
                warnings.push(IngestWarning {
                    path: path.clone(),
                    code: "utf8".to_string(),
                    message: "File is not valid UTF-8; file skipped.".to_string(),
                });
                return None;
            }
        };
        let line_count = text.lines().count().max(1);
        (line_count, chunk::chunk_file(&path, &text, kind, options))
    };

    if file_chunks.len() > options.max_chunks_per_file {
        warnings.push(IngestWarning {
            path: path.clone(),
            code: "max_chunks_per_file".to_string(),
            message: "Chunk limit exceeded; file truncated.".to_string(),
        });
        file_chunks.truncate(options.max_chunks_per_file);
    }

    stamp_chunk_metadata(&mut file_chunks, root_path, &path, language.clone(), resolution_tier);

    Some((
        FileMeta {
            path: path.clone(),
            root_path: root_path.to_string(),
            relative_path: path,
            kind,
            language,
            bytes: bytes_len,
            sha256: file_hash,
            line_count,
            is_generated: false,
            resolution_tier,
            mtime_ms,
            fingerprint_sha256,
        },
        file_chunks,
        bytes_len,
    ))
}

pub fn ingest_files(files: Vec<FileInput>, options: IngestOptions) -> IndexFile {
    ingest_files_with_root(files, options, None)
}

pub fn ingest_files_with_root(
    mut files: Vec<FileInput>,
    options: IngestOptions,
    root_hint: Option<&Path>,
) -> IndexFile {
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let root_path = prepare_root_path(&files, root_hint);
    let mut warnings = Vec::new();
    let mut total_bytes = 0usize;
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();

    for file in files {
        if total_bytes + file.data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: file.path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }

        let Some((file_meta, file_chunks, bytes_len)) =
            ingest_one_file(file, &options, &root_path, &mut warnings)
        else {
            continue;
        };

        total_bytes += bytes_len;
        chunks.extend(file_chunks);
        file_metas.push(file_meta);
    }

    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    build_index(file_metas, chunks, warnings, None, None)
}

pub fn update_index(prev: IndexFile, files: Vec<FileInput>, options: IngestOptions) -> IndexFile {
    let mut prev_map: BTreeMap<String, (FileMeta, Vec<Chunk>)> = BTreeMap::new();
    let mut chunk_map: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for chunk in prev.chunks {
        chunk_map.entry(chunk.path.clone()).or_default().push(chunk);
    }
    for file in prev.files {
        if let Some(chunks) = chunk_map.remove(&file.path) {
            prev_map.insert(file.path.clone(), (file, chunks));
        }
    }

    let mut new_files = files;
    new_files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut warnings = Vec::new();
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();
    let root_path = prepare_root_path(&new_files, None);
    let mut total_bytes = 0usize;

    for file in new_files {
        if total_bytes + file.data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: file.path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        let path = file.path.clone();
        let file_hash = sha256_hex(&file.data);
        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            if meta.sha256 == file_hash {
                file_metas.push(meta.clone());
                chunks.extend(existing_chunks.clone());
                continue;
            }
        }
        let Some((file_meta, file_chunks, bytes_len)) =
            ingest_one_file(file, &options, &root_path, &mut warnings)
        else {
            continue;
        };

        total_bytes += bytes_len;
        chunks.extend(file_chunks);
        file_metas.push(file_meta);
    }

    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    build_index(file_metas, chunks, warnings, None, None)
}

pub fn update_index_selective(
    prev: IndexFile,
    files: Vec<FileInput>,
    keep_paths: Vec<String>,
    options: IngestOptions,
) -> IndexFile {
    let mut prev_map: BTreeMap<String, (FileMeta, Vec<Chunk>)> = BTreeMap::new();
    let mut chunk_map: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for chunk in prev.chunks {
        chunk_map.entry(chunk.path.clone()).or_default().push(chunk);
    }
    for file in prev.files {
        if let Some(chunks) = chunk_map.remove(&file.path) {
            prev_map.insert(file.path.clone(), (file, chunks));
        }
    }

    let mut warnings = Vec::new();
    let mut file_metas = Vec::new();
    let mut chunks = Vec::new();
    let mut new_files = files;
    new_files.sort_by(|a, b| a.path.cmp(&b.path));
    let root_path = prepare_root_path(&new_files, None);

    let mut keep_paths_sorted = keep_paths;
    keep_paths_sorted.sort();
    keep_paths_sorted.dedup();

    for path in keep_paths_sorted {
        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            file_metas.push(meta.clone());
            chunks.extend(existing_chunks.clone());
        }
    }

    let mut total_bytes = 0usize;
    for file in new_files {
        if total_bytes + file.data.len() > options.max_total_bytes {
            warnings.push(IngestWarning {
                path: file.path.clone(),
                code: "max_total_bytes".to_string(),
                message: "Total size limit exceeded; file skipped.".to_string(),
            });
            continue;
        }
        let path = file.path.clone();
        let file_hash = sha256_hex(&file.data);

        if let Some((meta, existing_chunks)) = prev_map.get(&path) {
            if meta.sha256 == file_hash {
                file_metas.push(meta.clone());
                chunks.extend(existing_chunks.clone());
                continue;
            }
        }
        let Some((file_meta, file_chunks, bytes_len)) =
            ingest_one_file(file, &options, &root_path, &mut warnings)
        else {
            continue;
        };

        total_bytes += bytes_len;
        chunks.extend(file_chunks);
        file_metas.push(file_meta);
    }

    file_metas.sort_by(|a, b| a.path.cmp(&b.path));
    chunks.sort_by(|a, b| match a.path.cmp(&b.path) {
        std::cmp::Ordering::Equal => a.start_line.cmp(&b.start_line),
        other => other,
    });

    build_index(file_metas, chunks, warnings, None, None)
}

pub fn get_chunk(index: &IndexFile, chunk_id: &str) -> Option<Chunk> {
    index.chunks.iter().find(|c| c.id == chunk_id).cloned()
}

pub fn search(index: &IndexFile, query: &str, filters: SearchFilters, limit: usize) -> Vec<SearchResult> {
    search_index(
        &index.chunks,
        &index.inverted_index,
        &index.chunk_refs,
        query,
        &filters,
        limit,
    )
}

pub fn search_advanced(
    index: &IndexFile,
    query: &str,
    filters: SearchFilters,
    limit: usize,
    _use_semantic: bool,
    intent: QueryIntent,
    explain: bool,
) -> anyhow::Result<Vec<SearchResult>> {
    let resolved_intent = match intent {
        QueryIntent::Auto => classify_intent(query),
        other => other,
    };
    let weights = weights_for_intent(resolved_intent);

    let expanded_query = {
        let expansions = expand_synonyms(query);
        if expansions.is_empty() {
            query.to_string()
        } else {
            format!("{query} {}", expansions.join(" "))
        }
    };

    let bm25_results = search_index(
        &index.chunks,
        &index.inverted_index,
        &index.chunk_refs,
        &expanded_query,
        &filters,
        limit * 2,
    );
    let symbol_results = symbol_search(&index.chunks, &index.chunk_refs, query, &filters, limit * 2);

    #[cfg(feature = "embeddings")]
    let dense_results = if _use_semantic && resolved_intent != QueryIntent::Symbol {
        let embeddings = embeddings::validate_index_embeddings(index)?;
        let query_embedding = embeddings::generate_embedding(query)?;
        vector_search(
            &index.chunks,
            &index.chunk_refs,
            embeddings,
            &query_embedding,
            &filters,
            limit * 2,
        )
    } else {
        Vec::new()
    };

    #[cfg(not(feature = "embeddings"))]
    if _use_semantic && resolved_intent != QueryIntent::Symbol {
        anyhow::bail!("No embedding backend compiled; embeddings support is not compiled into this build");
    }
    #[cfg(not(feature = "embeddings"))]
    let dense_results: Vec<SearchResult> = Vec::new();

    let bm25_scored: Vec<(&str, f32)> = bm25_results
        .iter()
        .map(|result| (result.chunk_id.as_str(), result.score))
        .collect();
    let symbol_scored: Vec<(&str, f32)> = symbol_results
        .iter()
        .map(|result| (result.chunk_id.as_str(), result.score))
        .collect();
    let dense_scored: Vec<(&str, f32)> = dense_results
        .iter()
        .map(|result| (result.chunk_id.as_str(), result.score))
        .collect();

    let mut engine_results = vec![
        ("bm25", weights.bm25, to_ranked_results(&bm25_scored)),
        ("symbol", weights.symbol, to_ranked_results(&symbol_scored)),
    ];
    if !dense_scored.is_empty() {
        engine_results.push(("dense", weights.dense, to_ranked_results(&dense_scored)));
    }

    let fused = weighted_rrf_fusion(engine_results, RrfConfig::default(), limit);
    if fused.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_map: HashMap<&str, &Chunk> = index.chunks.iter().map(|chunk| (chunk.id.as_str(), chunk)).collect();

    Ok(fused
        .into_iter()
        .filter_map(|result| {
            let chunk = chunk_map.get(result.id.as_str())?;
            let chunk_ref = index
                .chunk_refs
                .get(result.id.as_str())
                .cloned()
                .unwrap_or_else(|| chunk.short_id.clone());

            let mut engines = result.engines;
            engines.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let matched_engines: Vec<String> = engines.iter().map(|(name, _)| name.clone()).collect();
            let engine_scores: Vec<(&str, f32)> = engines
                .iter()
                .map(|(name, score)| (name.as_str(), *score))
                .collect();

            Some(SearchResult {
                chunk_id: result.id,
                chunk_ref,
                score: result.score,
                path: chunk.path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                snippet: util::snippet(&chunk.content, 200),
                heading_path: chunk.heading_path.clone(),
                match_reason: explain.then(|| {
                    explain_match(
                        &engine_scores,
                        chunk.qualified_name.as_deref().or(chunk.symbol.as_deref()),
                        query,
                    )
                }),
                matched_engines,
            })
        })
        .collect())
}

fn compute_index_id(files: &[FileMeta]) -> String {
    let mut seed = String::new();
    for file in files {
        seed.push_str(&file.path);
        seed.push('\n');
        seed.push_str(&file.sha256);
        seed.push('\n');
    }
    sha256_hex(seed.as_bytes())
}

fn build_index(
    file_metas: Vec<FileMeta>,
    chunks: Vec<Chunk>,
    warnings: Vec<IngestWarning>,
    embeddings: Option<Vec<Vec<f32>>>,
    embedding_model: Option<String>,
) -> IndexFile {
    let chunk_refs = build_chunk_refs(&chunks);
    let inverted_index = build_inverted_index(&chunks);
    let stats = compute_stats(&file_metas, &chunks);
    let index_id = compute_index_id(&file_metas);
    let (symbols, edges) = build_structural_indexes(&chunks);

    IndexFile {
        version: INDEX_VERSION,
        index_id,
        files: file_metas,
        chunks,
        chunk_refs,
        inverted_index,
        stats,
        warnings,
        embeddings,
        embedding_model,
        symbols,
        edges,
    }
}

fn sanitize_zip_path(input: &str) -> String {
    let replaced = input.replace('\\', "/");
    let mut parts = Vec::new();
    for part in replaced.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            continue;
        }
        parts.push(part);
    }
    parts.join("/")
}
