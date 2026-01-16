use crate::mcp::storage::{IndexStore, IndexMetadata};
use crate::{ingest_files, search, FileInput, IngestOptions, SearchFilters};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "mcp")]
use schemars::JsonSchema;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_TOKENS: usize = 16000;

// Input/Output types for MCP tools

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct IndexInput {
    #[cfg_attr(feature = "mcp", schemars(description = "File or directory paths to index"))]
    pub paths: Vec<String>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Optional ingest configuration"))]
    pub options: Option<IngestOptionsInput>,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct IngestOptionsInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Target chunk size in characters"))]
    pub chunk_target_chars: Option<usize>,
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum file size in bytes"))]
    pub max_file_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct IndexOutput {
    pub index_id: String,
    pub created: bool,
    pub stats: IndexStatsOutput,
    pub warnings: Vec<WarningOutput>,
}

#[derive(Debug, Serialize)]
pub struct IndexStatsOutput {
    pub total_files: usize,
    pub total_chunks: usize,
    pub avg_chunk_tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct WarningOutput {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct SearchInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to search"))]
    pub index_id: String,
    #[cfg_attr(feature = "mcp", schemars(description = "Search query"))]
    pub query: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Optional search filters"))]
    pub filters: Option<SearchFiltersInput>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum number of results (default 10)"))]
    pub limit: Option<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Token budget for inline content (default 16000)"))]
    pub max_tokens: Option<usize>,
    /// Phase 5: Enable semantic (hybrid BM25 + embeddings) search
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Enable semantic search with embeddings (default false)"))]
    pub use_semantic: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct SearchFiltersInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by file path prefix"))]
    pub path_prefix: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by chunk kind (markdown, javascript, json, html, text, image)"))]
    pub kind: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by symbol prefix"))]
    pub symbol_prefix: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by heading prefix"))]
    pub heading_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct ExploreInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to explore"))]
    pub index_id: String,
    #[cfg_attr(feature = "mcp", schemars(description = "What to list: 'files', 'outline', or 'symbols'"))]
    pub mode: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Optional path prefix filter"))]
    pub path_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct ManageInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Action: 'list' or 'delete'"))]
    pub action: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID (required for delete)"))]
    pub index_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResultOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_ids: Option<Vec<String>>,
    pub total_matches: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResultOutput {
    pub chunk_id: String,
    pub score: f32,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub heading_path: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExploreOutput {
    pub items: Vec<String>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ManageOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<Vec<IndexMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// Tool handler implementations

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
/// 2. Filters by extension whitelist (.rs, .js, .ts, .tsx, .md, .json, .html, .css, .txt)
/// 3. Checks for existing index by root path
/// 4. Creates new index or updates existing one
/// 5. Saves to disk and returns metadata
///
/// # Returns
///
/// Returns `IndexOutput` with:
/// - `index_id`: Unique identifier for the index
/// - `created`: Whether this was a new index (vs update)
/// - `stats`: File count, chunk count, avg token count
/// - `warnings`: Any files skipped due to size/encoding issues
///
/// # Errors
///
/// Returns error if unable to read files or save index.
pub fn llmx_index_handler(store: &mut IndexStore, input: IndexInput) -> Result<IndexOutput> {
    // Collect files from paths
    let mut files = vec![];
    for path_str in &input.paths {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            walk_directory(&path, &mut files)?;
        } else if path.is_file() {
            read_file(&path, &mut files)?;
        }
    }

    // Check if index exists for these paths
    let root_path = input.paths[0].clone();
    let existing_id = store.find_by_path(Path::new(&root_path));

    let options = IngestOptions {
        chunk_target_chars: input.options.as_ref()
            .and_then(|o| o.chunk_target_chars)
            .unwrap_or(4000),
        chunk_max_chars: 8000,
        max_file_bytes: input.options.as_ref()
            .and_then(|o| o.max_file_bytes)
            .unwrap_or(10 * 1024 * 1024),
        max_total_bytes: 50 * 1024 * 1024,
        max_chunks_per_file: 2000,
    };

    let mut index = ingest_files(files, options);
    let created = existing_id.is_none();

    // Phase 5: Generate embeddings for semantic search
    #[cfg(feature = "embeddings")]
    {
        use crate::embeddings::generate_embeddings;
        let chunk_texts: Vec<&str> = index.chunks.iter()
            .map(|c| c.content.as_str())
            .collect();
        let embeddings = generate_embeddings(&chunk_texts);
        index.embeddings = Some(embeddings);
        index.embedding_model = Some("hash-based-v1".to_string());
    }

    let index_id = store.save(index.clone(), root_path)?;

    Ok(IndexOutput {
        index_id,
        created,
        stats: IndexStatsOutput {
            total_files: index.stats.total_files,
            total_chunks: index.stats.total_chunks,
            avg_chunk_tokens: index.stats.avg_chunk_tokens,
        },
        warnings: index.warnings.iter().map(|w| WarningOutput {
            path: w.path.clone(),
            code: w.code.clone(),
            message: w.message.clone(),
        }).collect(),
    })
}

/// Handler for `llmx_search` tool: Search indexed codebase with inline content.
///
/// # Arguments
///
/// * `store` - Mutable reference to IndexStore
/// * `input` - Search input with query, filters, limit, and token budget
///
/// # Token Budgeting
///
/// Results include inline chunk content up to `max_tokens` (default: 16K).
/// When budget is exceeded:
/// - Already-included chunks are kept
/// - Remaining chunks are returned in `truncated_ids` field
/// - Agent can make follow-up calls with specific chunk IDs if needed
///
/// # Performance
///
/// - Searches 2x the requested limit initially
/// - Applies token budget filter
/// - Returns top N results within budget
/// - Typical latency: <10ms for warm cache
///
/// # Returns
///
/// Returns `SearchOutput` with:
/// - `results`: Array of matching chunks with inline content
/// - `truncated_ids`: IDs of matches excluded due to token budget (optional)
/// - `total_matches`: Total number of matching chunks (before budget filter)
///
/// # Errors
///
/// Returns error if index doesn't exist or chunk data is missing.
pub fn llmx_search_handler(store: &mut IndexStore, input: SearchInput) -> Result<SearchOutput> {
    let index = store.load(&input.index_id)?;

    let filters = input.filters.as_ref().map(|f| SearchFilters {
        path_exact: None,
        path_prefix: f.path_prefix.clone(),
        kind: f.kind.as_ref().and_then(|k| parse_chunk_kind(k)),
        heading_prefix: f.heading_prefix.clone(),
        symbol_prefix: f.symbol_prefix.clone(),
    }).unwrap_or_default();

    let limit = input.limit.unwrap_or(10);
    let max_tokens = input.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

    // Phase 5: Choose search strategy based on use_semantic flag
    let search_results = if input.use_semantic.unwrap_or(false) {
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::generate_embedding;
            use crate::index::hybrid_search;

            // Check if embeddings are available
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
                // Fall back to BM25 if embeddings not available
                search(index, &input.query, filters.clone(), limit * 2)
            }
        }
        #[cfg(not(feature = "embeddings"))]
        {
            // Semantic search not available, fall back to BM25
            search(index, &input.query, filters.clone(), limit * 2)
        }
    } else {
        // Standard BM25 search
        search(index, &input.query, filters, limit * 2)
    };

    let mut results = vec![];
    let mut tokens_used = 0;
    let mut truncated = vec![];

    for result in &search_results {
        let chunk = index.chunks.iter()
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
        truncated_ids: if truncated.is_empty() { None } else { Some(truncated) },
        total_matches: search_results.len(),
    })
}

// Helper functions

fn walk_directory(path: &Path, files: &mut Vec<FileInput>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_directory(&path, files)?;
        } else if path.is_file() {
            read_file(&path, files)?;
        }
    }
    Ok(())
}

fn read_file(path: &Path, files: &mut Vec<FileInput>) -> Result<()> {
    // Check extension whitelist
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let allowed = ["rs", "js", "ts", "tsx", "md", "json", "html", "css", "txt"];
        if !allowed.contains(&ext) {
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
        mtime_ms: metadata.modified()
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

/// Handler for `llmx_explore` tool: Explore index structure.
///
/// # Arguments
///
/// * `store` - Mutable reference to IndexStore
/// * `input` - Explore input with index ID, mode, and optional path filter
///
/// # Modes
///
/// - `files`: List all indexed file paths
/// - `outline`: List all heading paths (for markdown, code comments)
/// - `symbols`: List all symbol names (functions, classes, etc.)
///
/// # Returns
///
/// Returns `ExploreOutput` with:
/// - `items`: Sorted array of strings (paths, headings, or symbols)
/// - `total`: Count of items
///
/// # Errors
///
/// Returns error if index doesn't exist or mode is invalid.
pub fn llmx_explore_handler(store: &mut IndexStore, input: ExploreInput) -> Result<ExploreOutput> {
    let index = store.load(&input.index_id)?;

    let items: Vec<String> = match input.mode.as_str() {
        "files" => {
            let mut files: Vec<_> = index.files.iter()
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
        _ => anyhow::bail!("Invalid mode: {}. Use 'files', 'outline', or 'symbols'", input.mode),
    };

    Ok(ExploreOutput {
        total: items.len(),
        items,
    })
}

/// Handler for `llmx_manage` tool: List or delete indexes.
///
/// # Arguments
///
/// * `store` - Mutable reference to IndexStore
/// * `input` - Manage input with action and optional index ID
///
/// # Actions
///
/// - `list`: Returns all indexes with metadata (id, root_path, created_at, file_count, chunk_count)
/// - `delete`: Removes index from disk, cache, and registry (requires `index_id`)
///
/// # Returns
///
/// Returns `ManageOutput` with:
/// - `success`: Whether operation succeeded
/// - `indexes`: Array of metadata (for `list` action)
/// - `message`: Success message (for `delete` action)
///
/// # Errors
///
/// Returns error if:
/// - Action is invalid (not "list" or "delete")
/// - Delete action missing `index_id`
/// - Index file cannot be removed
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
            let index_id = input.index_id
                .context("index_id is required for delete action")?;
            store.delete(&index_id)?;
            Ok(ManageOutput {
                success: true,
                indexes: None,
                message: Some(format!("Index {} deleted successfully", index_id)),
            })
        }
        _ => anyhow::bail!("Invalid action: {}. Use 'list' or 'delete'", input.action),
    }
}
