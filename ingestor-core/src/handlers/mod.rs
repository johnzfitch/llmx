//! Core handlers for llmx operations.
//!
//! This module contains the business logic for indexing, searching, and managing
//! codebase indexes. It's designed to be used by both the CLI and MCP server.

mod cache;
mod safety;
mod storage;
mod types;

pub use cache::{CacheStats, DynamicCache};
pub use safety::{
    dynamic_walk, find_project_root, has_project_marker, is_dangerous_path, SafetyLimits,
    WalkStats, PROJECT_MARKERS,
};
pub use storage::{IndexMetadata, IndexStore, Registry};
pub use types::*;

use crate::{ingest_files, search, FileInput, IndexFile, IngestOptions, SearchFilters};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_TOKENS: usize = 16000;

/// File extensions to include when indexing.
pub const ALLOWED_EXTENSIONS: &[&str] = &[
    // Rust
    "rs",
    // JavaScript/TypeScript
    "js", "ts", "tsx", "jsx", "mjs", "cjs",
    // Web
    "html", "css", "scss", "sass", "less",
    // Data
    "json", "yaml", "yml", "toml",
    // Documentation
    "md", "txt",
    // Python
    "py",
    // Go
    "go",
    // C/C++
    "c", "cpp", "cc", "cxx", "h", "hpp", "hxx",
    // Java
    "java",
    // Ruby
    "rb",
    // PHP
    "php",
    // Swift
    "swift",
    // Shell
    "sh", "bash", "zsh",
    // SQL
    "sql",
    // Data/Logs (note: .env excluded - commonly contains secrets)
    "log", "jsonl", "csv", "xml", "ini", "cfg", "conf",
];

/// Handler for `llmx_index` tool: Create or update codebase indexes.
///
/// # Arguments
///
/// * `store` - Mutable reference to IndexStore
/// * `input` - Index input containing paths and options
///
/// # Behavior
///
/// 1. Recursively walks directories and reads files
/// 2. Filters by extension whitelist
/// 3. Checks for existing index by root path
/// 4. Creates new index or updates existing one
/// 5. Saves to disk and returns metadata
pub fn llmx_index_handler(store: &mut IndexStore, input: IndexInput) -> Result<IndexOutput> {
    let mut files = vec![];
    for path_str in &input.paths {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            walk_directory(&path, &mut files)?;
        } else if path.is_file() {
            read_file(&path, &mut files)?;
        }
    }

    let root_path = input.paths[0].clone();
    let existing_id = store.find_by_path(Path::new(&root_path));

    let options = IngestOptions {
        chunk_target_chars: input
            .options
            .as_ref()
            .and_then(|o| o.chunk_target_chars)
            .unwrap_or(4000),
        chunk_max_chars: 8000,
        max_file_bytes: input
            .options
            .as_ref()
            .and_then(|o| o.max_file_bytes)
            .unwrap_or(10 * 1024 * 1024),
        max_total_bytes: 50 * 1024 * 1024,
        max_chunks_per_file: 2000,
    };

    let index = {
        #[cfg_attr(not(feature = "embeddings"), allow(unused_mut))]
        let mut index = ingest_files(files, options);
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::{generate_embeddings, MODEL_ID};
            let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
            let embeddings = generate_embeddings(&chunk_texts);
            index.embeddings = Some(embeddings);
            index.embedding_model = Some(MODEL_ID.to_string());
        }
        index
    };
    let created = existing_id.is_none();

    let index_id = store.save(index.clone(), root_path)?;

    Ok(IndexOutput {
        index_id,
        created,
        stats: IndexStatsOutput {
            total_files: index.stats.total_files,
            total_chunks: index.stats.total_chunks,
            avg_chunk_tokens: index.stats.avg_chunk_tokens,
        },
        warnings: index
            .warnings
            .iter()
            .map(|w| WarningOutput {
                path: w.path.clone(),
                code: w.code.clone(),
                message: w.message.clone(),
            })
            .collect(),
    })
}

/// Handler for `llmx_search` tool: Search indexed codebase with inline content.
///
/// Results include inline chunk content up to `max_tokens` (default: 16K).
/// When budget is exceeded, remaining chunks are returned in `truncated_ids`.
pub fn llmx_search_handler(store: &mut IndexStore, input: SearchInput) -> Result<SearchOutput> {
    let index = store.load(&input.index_id)?;

    let filters = input
        .filters
        .as_ref()
        .map(|f| SearchFilters {
            path_exact: None,
            path_prefix: f.path_prefix.clone(),
            kind: f.kind.as_ref().and_then(|k| parse_chunk_kind(k)),
            heading_prefix: f.heading_prefix.clone(),
            symbol_prefix: f.symbol_prefix.clone(),
        })
        .unwrap_or_default();

    let limit = input.limit.unwrap_or(10);
    let max_tokens = input.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

    let search_results = if input.use_semantic.unwrap_or(false) {
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::generate_embedding;
            use crate::index::hybrid_search;

            if let Some(embeddings) = &index.embeddings {
                let query_embedding = generate_embedding(&input.query);
                hybrid_search(
                    &index.chunks,
                    &index.inverted_index,
                    &index.chunk_refs,
                    embeddings,
                    &input.query,
                    &query_embedding,
                    &filters,
                    limit * 2,
                )
            } else {
                search(index, &input.query, filters.clone(), limit * 2)
            }
        }
        #[cfg(not(feature = "embeddings"))]
        {
            search(index, &input.query, filters.clone(), limit * 2)
        }
    } else {
        search(index, &input.query, filters, limit * 2)
    };

    let mut results = vec![];
    let mut tokens_used = 0;
    let mut truncated = vec![];

    for result in &search_results {
        let chunk = index
            .chunks
            .iter()
            .find(|c| c.id == result.chunk_id)
            .context("Chunk not found")?;

        if tokens_used + chunk.token_estimate <= max_tokens {
            results.push(SearchResultOutput {
                chunk_id: result.chunk_id.clone(),
                score: result.score,
                path: result.path.clone(),
                start_line: result.start_line,
                end_line: result.end_line,
                content: chunk.content.clone(),
                symbol: chunk.symbol.clone(),
                heading_path: result.heading_path.clone(),
            });
            tokens_used += chunk.token_estimate;
        } else {
            truncated.push(result.chunk_id.clone());
        }

        if results.len() >= limit {
            break;
        }
    }

    Ok(SearchOutput {
        results,
        truncated_ids: if truncated.is_empty() {
            None
        } else {
            Some(truncated)
        },
        total_matches: search_results.len(),
    })
}

/// Handler for `llmx_explore` tool: Explore index structure.
///
/// Modes:
/// - `files`: List all indexed file paths
/// - `outline`: List all heading paths
/// - `symbols`: List all symbol names
pub fn llmx_explore_handler(store: &mut IndexStore, input: ExploreInput) -> Result<ExploreOutput> {
    let index = store.load(&input.index_id)?;

    let items: Vec<String> = match input.mode.as_str() {
        "files" => {
            let mut files: Vec<_> = index
                .files
                .iter()
                .filter(|f| {
                    if let Some(ref prefix) = input.path_filter {
                        f.path.starts_with(prefix)
                    } else {
                        true
                    }
                })
                .map(|f| f.path.clone())
                .collect();
            files.sort();
            files
        }
        "outline" => {
            let mut headings = HashSet::new();
            for chunk in &index.chunks {
                if let Some(ref prefix) = input.path_filter {
                    if !chunk.path.starts_with(prefix) {
                        continue;
                    }
                }
                for heading in &chunk.heading_path {
                    headings.insert(heading.clone());
                }
            }
            let mut result: Vec<_> = headings.into_iter().collect();
            result.sort();
            result
        }
        "symbols" => {
            let mut symbols = HashSet::new();
            for chunk in &index.chunks {
                if let Some(ref prefix) = input.path_filter {
                    if !chunk.path.starts_with(prefix) {
                        continue;
                    }
                }
                if let Some(ref symbol) = chunk.symbol {
                    symbols.insert(symbol.clone());
                }
            }
            let mut result: Vec<_> = symbols.into_iter().collect();
            result.sort();
            result
        }
        _ => anyhow::bail!(
            "Invalid mode: {}. Use 'files', 'outline', or 'symbols'",
            input.mode
        ),
    };

    Ok(ExploreOutput {
        total: items.len(),
        items,
    })
}

/// Handler for `llmx_manage` tool: List or delete indexes.
pub fn llmx_manage_handler(store: &mut IndexStore, input: ManageInput) -> Result<ManageOutput> {
    match input.action.as_str() {
        "list" => {
            let indexes = store.list()?;
            Ok(ManageOutput {
                success: true,
                indexes: Some(indexes),
                message: None,
            })
        }
        "delete" => {
            let index_id = input
                .index_id
                .context("index_id is required for delete action")?;
            store.delete(&index_id)?;
            Ok(ManageOutput {
                success: true,
                indexes: None,
                message: Some(format!("Index {} deleted successfully", index_id)),
            })
        }
        _ => anyhow::bail!(
            "Invalid action: {}. Use 'list' or 'delete'",
            input.action
        ),
    }
}

/// Handler for getting a single chunk by ID or ref.
///
/// Searches by:
/// 1. Exact chunk ID match
/// 2. Chunk ref match (from chunk_refs)
/// 3. ID prefix match (for short refs)
pub fn llmx_get_chunk_handler(
    store: &mut IndexStore,
    index_id: &str,
    chunk_id: &str,
) -> Result<Option<ChunkOutput>> {
    let index = store.load(index_id)?;

    // Try exact ID match
    let chunk = index.chunks.iter().find(|c| c.id == chunk_id);

    // Try ref match
    let chunk = chunk.or_else(|| {
        // Find chunk ID by ref
        let id_from_ref = index
            .chunk_refs
            .iter()
            .find(|(_, r)| r.as_str() == chunk_id)
            .map(|(id, _)| id.as_str());

        if let Some(id) = id_from_ref {
            index.chunks.iter().find(|c| c.id == id)
        } else {
            None
        }
    });

    // Try ID prefix match (for short refs like first 12 chars)
    let chunk = chunk.or_else(|| index.chunks.iter().find(|c| c.id.starts_with(chunk_id)));

    Ok(chunk.map(|c| ChunkOutput {
        chunk_id: c.id.clone(),
        path: c.path.clone(),
        start_line: c.start_line,
        end_line: c.end_line,
        content: c.content.clone(),
        symbol: c.symbol.clone(),
        heading_path: c.heading_path.clone(),
        token_estimate: c.token_estimate,
    }))
}

/// Handler for dynamic search (no persistent index required).
///
/// # Behavior
///
/// 1. Find project root (if path not specified)
/// 2. Check for dangerous paths (reject unless force_dangerous)
/// 3. Check cache (if not no_cache)
/// 4. If cache miss: walk files -> chunk -> build index
/// 5. Search with BM25 (+ embeddings if use_semantic)
/// 6. Cache result for next search
/// 7. Return results with mode indicator
pub fn llmx_search_dynamic_handler(
    store: &mut IndexStore,
    cache: &mut DynamicCache,
    input: DynamicSearchInput,
) -> Result<DynamicSearchOutput> {
    use std::collections::HashMap;
    use std::time::Instant;

    let _start = Instant::now();
    let cwd = std::env::current_dir().context("Could not get current directory")?;

    // Step 1: Determine the root path
    let root = if let Some(ref path) = input.path {
        path.canonicalize().unwrap_or_else(|_| path.clone())
    } else {
        find_project_root(&cwd).unwrap_or(cwd.clone())
    };

    // Step 2: Check for persistent index first (unless force_dynamic)
    if !input.force_dynamic {
        if let Some(metadata) = store.find_metadata_by_path(&root) {
            // Use persistent index
            let index_id = metadata.id.clone();
            let index = store.load(&index_id)?;
            let search_start = Instant::now();
            let (results, truncated_ids, total_matches) = perform_search(
                index,
                &input.query,
                &input.filters,
                input.limit,
                input.max_tokens,
                input.use_semantic,
            )?;

            return Ok(DynamicSearchOutput {
                mode: "persistent".to_string(),
                results,
                stats: DynamicSearchStats {
                    file_count: index.files.len(),
                    total_bytes: index.files.iter().map(|f| f.bytes).sum(),
                    chunk_count: index.chunks.len(),
                    index_time_ms: 0,
                    search_time_ms: search_start.elapsed().as_millis() as u64,
                    truncated: false,
                    root_path: root.to_string_lossy().to_string(),
                },
                truncated_ids,
                total_matches,
            });
        }
    }

    // Step 3: Safety check for dangerous paths
    if is_dangerous_path(&root) && !input.force_dangerous {
        anyhow::bail!(
            "Refusing to search dangerous path: {}\n\
             Use --force to override this safety check.",
            root.display()
        );
    }

    // Step 4: Warn if no project markers found
    if !has_project_marker(&root) {
        eprintln!(
            "Warning: No project markers found in {}. Results may include unexpected files.",
            root.display()
        );
    }

    // Step 5: Check cache (unless no_cache)
    if !input.no_cache {
        if let Some(index) = cache.get(&root) {
            let search_start = Instant::now();
            let (results, truncated_ids, total_matches) = perform_search(
                index,
                &input.query,
                &input.filters,
                input.limit,
                input.max_tokens,
                input.use_semantic,
            )?;

            return Ok(DynamicSearchOutput {
                mode: "cached".to_string(),
                results,
                stats: DynamicSearchStats {
                    file_count: index.files.len(),
                    total_bytes: index.files.iter().map(|f| f.bytes).sum(),
                    chunk_count: index.chunks.len(),
                    index_time_ms: 0,
                    search_time_ms: search_start.elapsed().as_millis() as u64,
                    truncated: false,
                    root_path: root.to_string_lossy().to_string(),
                },
                truncated_ids,
                total_matches,
            });
        }
    }

    // Step 6: Build dynamic index
    let index_start = Instant::now();
    let limits = SafetyLimits::default();
    let (files, walk_stats) = dynamic_walk(&root, &limits)?;

    // Collect mtimes for cache invalidation
    let file_mtimes: HashMap<String, u64> = files
        .iter()
        .filter_map(|f| f.mtime_ms.map(|m| (f.path.clone(), m)))
        .collect();

    let options = IngestOptions {
        chunk_target_chars: 4000,
        chunk_max_chars: 8000,
        max_file_bytes: 10 * 1024 * 1024,
        max_total_bytes: 100 * 1024 * 1024,
        max_chunks_per_file: 2000,
    };

    let index = {
        #[cfg_attr(not(feature = "embeddings"), allow(unused_mut))]
        let mut index = ingest_files(files, options);
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::{generate_embeddings, MODEL_ID};
            let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
            let embeddings = generate_embeddings(&chunk_texts);
            index.embeddings = Some(embeddings);
            index.embedding_model = Some(MODEL_ID.to_string());
        }
        index
    };

    let index_time_ms = index_start.elapsed().as_millis() as u64;

    // Step 7: Perform search
    let search_start = Instant::now();
    let (results, truncated_ids, total_matches) = perform_search(
        &index,
        &input.query,
        &input.filters,
        input.limit,
        input.max_tokens,
        input.use_semantic,
    )?;
    let search_time_ms = search_start.elapsed().as_millis() as u64;

    // Step 8: Cache the index (unless no_cache)
    if !input.no_cache {
        cache.insert(&root, index.clone(), file_mtimes);
    }

    Ok(DynamicSearchOutput {
        mode: "dynamic".to_string(),
        results,
        stats: DynamicSearchStats {
            file_count: walk_stats.file_count,
            total_bytes: walk_stats.total_bytes,
            chunk_count: index.chunks.len(),
            index_time_ms,
            search_time_ms,
            truncated: walk_stats.truncated,
            root_path: root.to_string_lossy().to_string(),
        },
        truncated_ids,
        total_matches,
    })
}

/// Perform search on an index and return formatted results.
fn perform_search(
    index: &IndexFile,
    query: &str,
    filters: &Option<SearchFiltersInput>,
    limit: Option<usize>,
    max_tokens: Option<usize>,
    use_semantic: Option<bool>,
) -> Result<(Vec<SearchResultOutput>, Option<Vec<String>>, usize)> {
    let filters = filters
        .as_ref()
        .map(|f| SearchFilters {
            path_exact: None,
            path_prefix: f.path_prefix.clone(),
            kind: f.kind.as_ref().and_then(|k| parse_chunk_kind(k)),
            heading_prefix: f.heading_prefix.clone(),
            symbol_prefix: f.symbol_prefix.clone(),
        })
        .unwrap_or_default();

    let limit = limit.unwrap_or(10);
    let max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

    let search_results = if use_semantic.unwrap_or(false) {
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::generate_embedding;
            use crate::index::hybrid_search;

            if let Some(embeddings) = &index.embeddings {
                let query_embedding = generate_embedding(query);
                hybrid_search(
                    &index.chunks,
                    &index.inverted_index,
                    &index.chunk_refs,
                    embeddings,
                    query,
                    &query_embedding,
                    &filters,
                    limit * 2,
                )
            } else {
                search(index, query, filters.clone(), limit * 2)
            }
        }
        #[cfg(not(feature = "embeddings"))]
        {
            search(index, query, filters.clone(), limit * 2)
        }
    } else {
        search(index, query, filters, limit * 2)
    };

    let mut results = vec![];
    let mut tokens_used = 0;
    let mut truncated = vec![];

    for result in &search_results {
        let chunk = index
            .chunks
            .iter()
            .find(|c| c.id == result.chunk_id)
            .context("Chunk not found")?;

        if tokens_used + chunk.token_estimate <= max_tokens {
            results.push(SearchResultOutput {
                chunk_id: result.chunk_id.clone(),
                score: result.score,
                path: result.path.clone(),
                start_line: result.start_line,
                end_line: result.end_line,
                content: chunk.content.clone(),
                symbol: chunk.symbol.clone(),
                heading_path: result.heading_path.clone(),
            });
            tokens_used += chunk.token_estimate;
        } else {
            truncated.push(result.chunk_id.clone());
        }

        if results.len() >= limit {
            break;
        }
    }

    Ok((
        results,
        if truncated.is_empty() {
            None
        } else {
            Some(truncated)
        },
        search_results.len(),
    ))
}

// Helper functions

fn walk_directory(path: &Path, files: &mut Vec<FileInput>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        // Skip hidden directories and common non-code directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
            {
                continue;
            }
        }

        if path.is_dir() {
            walk_directory(&path, files)?;
        } else if path.is_file() {
            read_file(&path, files)?;
        }
    }
    Ok(())
}

fn read_file(path: &Path, files: &mut Vec<FileInput>) -> Result<()> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if !ALLOWED_EXTENSIONS.contains(&ext) {
            return Ok(());
        }
    } else {
        return Ok(());
    }

    let data = fs::read(path)?;
    let metadata = fs::metadata(path)?;

    files.push(FileInput {
        path: path.to_string_lossy().to_string(),
        data,
        mtime_ms: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64),
        fingerprint_sha256: None,
    });

    Ok(())
}

fn parse_chunk_kind(s: &str) -> Option<crate::ChunkKind> {
    match s {
        "markdown" => Some(crate::ChunkKind::Markdown),
        "json" => Some(crate::ChunkKind::Json),
        "javascript" => Some(crate::ChunkKind::JavaScript),
        "html" => Some(crate::ChunkKind::Html),
        "text" => Some(crate::ChunkKind::Text),
        "image" => Some(crate::ChunkKind::Image),
        _ => None,
    }
}
