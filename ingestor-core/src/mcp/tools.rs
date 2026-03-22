use crate::graph::{ast_kind_label, canonical_symbol_key, normalize_symbol_key, raw_symbol_key, CodeGraph};
use crate::handlers::MAX_SEARCH_LIMIT;
use crate::mcp::jobs::{JobStatus, JobStore};
use crate::mcp::storage::{IndexStore, IndexMetadata};
use crate::walk::{collect_input_files, WalkConfig};
use crate::{ingest_files_with_root, search, search_advanced, Edge, EdgeKind, IngestOptions, QueryIntent, SearchFilters, SymbolIndexEntry, DEFAULT_MAX_FILE_BYTES};
use crate::query::classify_intent;
#[cfg(feature = "embeddings")]
use crate::query::explain_match;
#[cfg(feature = "embeddings")]
use crate::vector_search;
#[cfg(feature = "embeddings")]
use crate::HybridStrategy;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "mcp")]
use schemars::JsonSchema;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_TOKENS: usize = 8000;

#[derive(Debug, Serialize)]
pub struct BackgroundTaskOutput {
    pub job_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct StatusOutput {
    pub readiness_tier: u8,
    pub files_indexed: usize,
    pub files_total: usize,
    pub symbols_indexed: usize,
    pub embeddings_ready: bool,
    pub languages: Vec<String>,
    pub stale_files: usize,
    pub background_tasks: Vec<BackgroundTaskOutput>,
}

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
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum total bytes to ingest (default 100MB)"))]
    pub max_total_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct IndexOutput {
    pub index_id: String,
    pub created: bool,
    pub stats: IndexStatsOutput,
    pub warnings: Vec<WarningOutput>,
}

#[derive(Debug, Clone, Serialize)]
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
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to search. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Search query"))]
    pub query: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Optional search filters"))]
    pub filters: Option<SearchFiltersInput>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum number of results (default 10)"))]
    pub limit: Option<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Token budget for inline content (default 8000)"))]
    pub max_tokens: Option<usize>,
    /// Phase 5: Enable semantic (hybrid BM25 + embeddings) search
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Enable semantic search with embeddings (default false)"))]
    pub use_semantic: Option<bool>,
    /// Phase 6: Hybrid search strategy (rrf or linear, default rrf)
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Hybrid search strategy: 'rrf' (Reciprocal Rank Fusion, default) or 'linear' (weighted combination)"))]
    pub hybrid_strategy: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Phase 7 intent routing: 'auto', 'symbol', 'semantic', or 'keyword'"))]
    pub intent: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Include human-readable explanations for why each result matched"))]
    pub explain: Option<bool>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Search strategy: 'auto' (default), 'bm25', 'semantic', or 'hybrid'"))]
    pub strategy: Option<String>,
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
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to explore. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "What to list: 'files', 'outline', or 'symbols'"))]
    pub mode: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Optional path prefix filter"))]
    pub path_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct ManageInput {
    #[cfg_attr(feature = "mcp", schemars(description = "Action: 'list', 'delete', 'stats', or 'job_status'"))]
    pub action: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID (required for delete or job_status; optional for stats when loc/current directory identifies an indexed project) or job ID (required for job_status)"))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against for stats. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResultOutput>,
    pub readiness_tier: u8,
    /// Number of matches excluded from results due to token budget.
    #[serde(skip_serializing_if = "is_zero")]
    pub truncated_count: usize,
    pub total_matches: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SearchNoticeOutput>,
}

fn is_zero(n: &usize) -> bool { *n == 0 }

#[derive(Debug, Serialize)]
pub struct SearchResultOutput {
    #[serde(skip)]
    pub chunk_id: String,
    #[serde(skip)]
    pub score: f32,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub heading_path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_engines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchNoticeOutput {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ExploreOutput {
    pub items: Vec<String>,
    pub total: usize,
    pub readiness_tier: u8,
}

#[derive(Debug, Serialize)]
pub struct ManageOutput {
    pub success: bool,
    pub readiness_tier: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<Vec<IndexMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ManageStatsOutput>,
}

#[derive(Debug, Serialize)]
pub struct ManageStatsOutput {
    pub total_files: usize,
    pub total_chunks: usize,
    pub avg_chunk_tokens: usize,
    pub symbol_count: usize,
    pub edge_count: usize,
    pub language_count: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub file_kind_breakdown: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extension_breakdown: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub ast_kind_breakdown: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub edge_kind_breakdown: BTreeMap<String, usize>,
}

/// Phase 7: llmx_symbols input — fast symbol table lookup by name pattern.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct SymbolsInput {
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to query. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    /// Name pattern: exact, prefix (ending with *), or substring (surrounded by *).
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Symbol name pattern: exact 'foo', prefix 'foo*', or substring '*foo*'"))]
    pub pattern: Option<String>,
    /// Filter by AST kind: function, method, class, interface, type, enum, constant, variable, test.
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by kind: function, method, class, interface, type, enum, constant, variable, test"))]
    pub ast_kind: Option<String>,
    /// Filter by file path prefix.
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by file path prefix"))]
    pub path_prefix: Option<String>,
    /// Maximum number of results (default 50).
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum results (default 50)"))]
    pub limit: Option<usize>,
}

/// A single symbol entry returned by llmx_symbols.
#[derive(Debug, Serialize)]
pub struct SymbolEntry {
    /// Fully qualified name: "AuthService.login" or "verifyToken"
    pub qualified_name: String,
    /// AST node kind: "function", "class", "method", etc.
    pub ast_kind: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    /// Function signature: "login(email: string, password: string): Promise<Token>"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// First sentence of doc comment if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_summary: Option<String>,
    /// True if this symbol is exported from its module.
    #[serde(skip_serializing_if = "is_false")]
    pub exported: bool,
    /// Chunk ID for follow-up `get_chunk` calls.
    pub chunk_id: String,
}

fn is_false(b: &bool) -> bool { !b }

#[derive(Debug, Serialize)]
pub struct SymbolsOutput {
    pub symbols: Vec<SymbolEntry>,
    pub total: usize,
    pub readiness_tier: u8,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct LookupInput {
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to query. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Exact symbol or prefix pattern, for example 'parseConfig' or 'parse*'"))]
    pub symbol: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by kind: function, method, class, interface, type, enum, constant, variable, test"))]
    pub kind: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Filter by file path prefix"))]
    pub path_prefix: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum results (default 20)"))]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LookupOutput {
    pub matches: Vec<SymbolEntry>,
    pub total: usize,
    pub readiness_tier: u8,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct RefsInput {
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to query. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Symbol to trace references for"))]
    pub symbol: String,
    #[cfg_attr(feature = "mcp", schemars(description = "Direction: callers, callees, importers, imports, or type_users"))]
    pub direction: String,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Traversal depth in hops (default 1)"))]
    pub depth: Option<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Maximum results (default 20)"))]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RefResult {
    pub source_symbol: String,
    pub target_symbol: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub context: String,
    pub chunk_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_chunk_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefsOutput {
    pub refs: Vec<RefResult>,
    pub total: usize,
    pub readiness_tier: u8,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct GetChunkInput {
    #[serde(default)]
    #[cfg_attr(feature = "mcp", schemars(description = "Index ID to query. Optional when loc or the current directory identifies an indexed project."))]
    pub index_id: Option<String>,
    #[serde(default, alias = "path")]
    #[cfg_attr(feature = "mcp", schemars(description = "Filesystem location to resolve against. Defaults to the current directory when index_id is omitted."))]
    pub loc: Option<String>,
    #[cfg_attr(feature = "mcp", schemars(description = "Chunk ID, chunk ref, or chunk ID prefix"))]
    pub chunk_id: String,
}

#[derive(Debug, Serialize)]
pub struct GetChunkOutput {
    pub chunk_id: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub heading_path: Vec<String>,
    pub token_estimate: usize,
    pub readiness_tier: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchStrategy {
    Auto,
    Bm25,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchStrategyPlan {
    Bm25,
    SemanticOnly,
    Advanced {
        use_semantic: bool,
        intent: QueryIntent,
    },
}

fn resolve_index_id(store: &IndexStore, index_id: Option<&str>, loc: Option<&str>) -> Result<String> {
    if let Some(index_id) = index_id {
        return Ok(index_id.to_string());
    }

    let requested = loc
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().context("Could not get current directory")?);
    let requested = requested.canonicalize().unwrap_or(requested);
    let root = if requested.is_file() {
        requested.parent().unwrap_or(requested.as_path()).to_path_buf()
    } else {
        requested.clone()
    };

    if loc.is_some() {
        let mut cursor = root.clone();
        loop {
            if let Some(index_id) = store.find_by_path(&cursor) {
                return Ok(index_id);
            }
            if !cursor.pop() {
                break;
            }
        }
    } else if let Some(index_id) = store.find_by_path(&root) {
        return Ok(index_id);
    }

    if let Some(loc) = loc {
        anyhow::bail!(
            "No index found for location {}. Pass index_id explicitly or create an index for that project first.",
            loc
        );
    }

    anyhow::bail!(
        "No index found for current directory {}. Pass loc or index_id, or create an index for this project first.",
        root.display()
    )
}

fn readiness_tier_for_index(index: &crate::IndexFile) -> u8 {
    let has_files = !index.files.is_empty();
    let has_symbols = !index.symbols.is_empty()
        || index
            .chunks
            .iter()
            .any(|chunk| chunk.symbol.is_some() || chunk.qualified_name.is_some() || chunk.ast_kind.is_some());
    let embeddings_ready = index
        .embeddings
        .as_ref()
        .map(|embeddings| !embeddings.is_empty())
        .unwrap_or(false);

    match (has_files, has_symbols, embeddings_ready) {
        (false, _, _) => 0,
        (true, false, _) => 1,
        (true, true, false) => 2,
        (true, true, true) => 3,
    }
}

fn symbol_count(index: &crate::IndexFile) -> usize {
    index.symbols.values().map(Vec::len).sum()
}

fn language_labels(index: &crate::IndexFile) -> Vec<String> {
    let mut labels: Vec<String> = index
        .files
        .iter()
        .filter_map(|file| file.language.as_ref())
        .map(|language| match language {
            crate::LanguageId::Rust => "rust".to_string(),
            crate::LanguageId::Python => "python".to_string(),
            crate::LanguageId::TypeScript => "typescript".to_string(),
            crate::LanguageId::JavaScript => "javascript".to_string(),
            crate::LanguageId::Go => "go".to_string(),
            crate::LanguageId::Java => "java".to_string(),
            crate::LanguageId::C => "c".to_string(),
            crate::LanguageId::Cpp => "cpp".to_string(),
            crate::LanguageId::CSharp => "csharp".to_string(),
            crate::LanguageId::Ruby => "ruby".to_string(),
            crate::LanguageId::Php => "php".to_string(),
            crate::LanguageId::Swift => "swift".to_string(),
            crate::LanguageId::Shell => "shell".to_string(),
            crate::LanguageId::Sql => "sql".to_string(),
            crate::LanguageId::Html => "html".to_string(),
            crate::LanguageId::Css => "css".to_string(),
            crate::LanguageId::Json => "json".to_string(),
            crate::LanguageId::Markdown => "markdown".to_string(),
            crate::LanguageId::Toml => "toml".to_string(),
            crate::LanguageId::Yaml => "yaml".to_string(),
            crate::LanguageId::Other(other) => other.clone(),
        })
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn readiness_tier_for_store(store: &mut IndexStore) -> Result<u8> {
    let indexes = store.list()?;
    let mut readiness_tier = 0u8;
    for metadata in indexes {
        let index = store.load(&metadata.index_id)?;
        readiness_tier = readiness_tier.max(readiness_tier_for_index(index));
    }
    Ok(readiness_tier)
}

pub fn llmx_status_handler(store: &mut IndexStore, jobs: &JobStore) -> Result<StatusOutput> {
    let indexes = store.list()?;
    let mut files_indexed = 0usize;
    let mut files_total = 0usize;
    let mut symbols_indexed = 0usize;
    let mut embeddings_ready = !indexes.is_empty();
    let mut readiness_tier = 0u8;
    let mut languages = Vec::new();

    for metadata in &indexes {
        let index = store.load(&metadata.index_id)?;
        files_indexed += index.files.len();
        files_total += index.files.len();
        symbols_indexed += symbol_count(index);
        embeddings_ready &= index
            .embeddings
            .as_ref()
            .map(|embeddings| !embeddings.is_empty())
            .unwrap_or(false);
        readiness_tier = readiness_tier.max(readiness_tier_for_index(index));
        languages.extend(language_labels(index));
    }

    languages.sort();
    languages.dedup();

    let mut background_tasks: Vec<BackgroundTaskOutput> = jobs
        .lock()
        .map(|guard| {
            guard
                .iter()
                .filter_map(|(job_id, state)| {
                    let status = match &state.status {
                        JobStatus::Queued => Some("queued"),
                        JobStatus::Running => Some("running"),
                        JobStatus::Complete { .. } => None,
                        JobStatus::Error { .. } => Some("error"),
                    }?;
                    Some(BackgroundTaskOutput {
                        job_id: job_id.clone(),
                        status: status.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    background_tasks.sort_by(|a, b| a.job_id.cmp(&b.job_id));

    Ok(StatusOutput {
        readiness_tier,
        files_indexed,
        files_total,
        symbols_indexed,
        embeddings_ready,
        languages,
        stale_files: 0,
        background_tasks,
    })
}

// Tool handler implementations

/// CPU/IO heavy part of indexing -- runs in spawn_blocking, no store lock held.
pub fn run_index_work(input: &IndexInput) -> Result<(crate::IndexFile, String, IngestOptions)> {
    let walk_config = WalkConfig {
        max_depth: 50,
        max_files: 200_000,
        max_total_bytes: usize::MAX,
        timeout_secs: 300,
        respect_gitignore: true,
    };
    let (files, root_path, _) = collect_input_files(&input.paths, &walk_config)?;

    let options = IngestOptions {
        chunk_target_chars: input.options.as_ref()
            .and_then(|o| o.chunk_target_chars)
            .unwrap_or(4000),
        chunk_max_chars: 8000,
        max_file_bytes: input.options.as_ref()
            .and_then(|o| o.max_file_bytes)
            .unwrap_or(DEFAULT_MAX_FILE_BYTES),
        max_total_bytes: input.options.as_ref()
            .and_then(|o| o.max_total_bytes)
            .unwrap_or(usize::MAX),
        max_chunks_per_file: 2000,
    };

    let index = {
        #[cfg_attr(not(feature = "embeddings"), allow(unused_mut))]
        let mut index = ingest_files_with_root(files, options.clone(), Some(Path::new(&root_path)));
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::{generate_embeddings, runtime_model_id};
            let chunk_texts: Vec<&str> = index.chunks.iter()
                .map(|c| c.content.as_str())
                .collect();
            let embeddings = generate_embeddings(&chunk_texts)?;
            index.embeddings = Some(embeddings);
            index.embedding_model = Some(runtime_model_id()?.to_string());
        }
        index
    };

    Ok((index, root_path, options))
}

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
/// 2. Filters by extension whitelist (see `handlers::ALLOWED_EXTENSIONS`)
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
    let (index, root_path, _options) = run_index_work(&input)?;
    let existing_id = store.find_by_path(Path::new(&root_path));
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
/// Results include inline chunk content up to `max_tokens` (default: 8K).
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
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);

    // Build chunk lookup map once - O(n) instead of O(n*m) lookups
    let chunk_map: std::collections::HashMap<&str, &crate::Chunk> = index.chunks
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    let filters = input.filters.as_ref().map(|f| SearchFilters {
        path_exact: None,
        path_prefix: f.path_prefix.clone(),
        kind: f.kind.as_ref().and_then(|k| parse_chunk_kind(k)),
        heading_prefix: f.heading_prefix.clone(),
        symbol_prefix: f.symbol_prefix.clone(),
    }).unwrap_or_default();

    let limit = input.limit.unwrap_or(10).min(MAX_SEARCH_LIMIT);
    let max_tokens = input.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let intent = parse_query_intent(input.intent.as_deref())?;
    let explain = input.explain.unwrap_or(false);
    let strategy = parse_search_strategy(input.strategy.as_deref())?;
    let use_advanced = explain || input.intent.is_some();
    let mut notices = Vec::new();

    let effective_strategy = match strategy {
        Some(strategy) => Some(strategy),
        None if input.use_semantic.is_none() => Some(SearchStrategy::Auto),
        None => None,
    };

    let search_results = if let Some(strategy) = effective_strategy {
        match plan_search_strategy(strategy, &input.query, intent) {
            SearchStrategyPlan::Bm25 => search(index, &input.query, filters.clone(), limit * 2),
            SearchStrategyPlan::Advanced { use_semantic, intent } => {
                match search_advanced(
                    index,
                    &input.query,
                    filters.clone(),
                    limit * 2,
                    use_semantic,
                    intent,
                    explain,
                ) {
                    Ok(results) => results,
                    Err(err) if strategy == SearchStrategy::Auto && use_semantic => {
                        if let Some(reason) = embedding_downgrade_reason(&err) {
                            notices.push(SearchNoticeOutput {
                                code: "semantic_downgrade".to_string(),
                                message: format!(
                                    "Auto search downgraded to BM25 + symbol routing because embeddings are unavailable for this index ({reason}). To enable semantic search, rebuild the index with embeddings so it stores vectors and an embedding_model matching the current runtime."
                                ),
                            });
                            search_advanced(
                                index,
                                &input.query,
                                filters.clone(),
                                limit * 2,
                                false,
                                intent,
                                explain,
                            )?
                        } else {
                            return Err(err);
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            SearchStrategyPlan::SemanticOnly => semantic_only_search(
                index,
                &input.query,
                &filters,
                limit * 2,
                explain,
            )?,
        }
    } else if use_advanced {
        search_advanced(
            index,
            &input.query,
            filters.clone(),
            limit * 2,
            input.use_semantic.unwrap_or(false),
            intent,
            explain,
        )?
    } else if input.use_semantic.unwrap_or(false) {
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::{generate_embedding, validate_index_embeddings};
            use crate::index::hybrid_search_with_strategy;

            let embeddings = validate_index_embeddings(index)?;
            let query_embedding = generate_embedding(&input.query)?;
            let strategy = parse_hybrid_strategy(input.hybrid_strategy.as_deref())?;
            hybrid_search_with_strategy(
                &index.chunks,
                &index.inverted_index,
                &index.chunk_refs,
                embeddings,
                &input.query,
                &query_embedding,
                &filters,
                limit * 2,
                strategy,
            )
        }
        #[cfg(not(feature = "embeddings"))]
        {
            anyhow::bail!("Semantic search requested, but embeddings support is not compiled into this build")
        }
    } else {
        // Standard BM25 search
        search(index, &input.query, filters, limit * 2)
    };

    let mut results = vec![];
    let mut tokens_used = 0;
    let mut truncated_count = 0;

    for result in &search_results {
        let chunk = chunk_map.get(result.chunk_id.as_str())
            .context("Chunk not found in index")?;

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
                match_reason: result.match_reason.clone(),
                matched_engines: result.matched_engines.clone(),
            });
            tokens_used += chunk.token_estimate;
        } else {
            truncated_count += 1;
        }

        if results.len() >= limit {
            break;
        }
    }

    Ok(SearchOutput {
        results,
        readiness_tier,
        truncated_count,
        total_matches: search_results.len(),
        notices,
    })
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

fn parse_query_intent(value: Option<&str>) -> Result<QueryIntent> {
    Ok(match value {
        None => QueryIntent::Auto,
        Some("auto") => QueryIntent::Auto,
        Some("symbol") => QueryIntent::Symbol,
        Some("semantic") => QueryIntent::Semantic,
        Some("keyword") => QueryIntent::Keyword,
        Some(other) => anyhow::bail!(
            "Invalid intent: {other}. Use 'auto', 'symbol', 'semantic', or 'keyword'."
        ),
    })
}

fn parse_search_strategy(value: Option<&str>) -> Result<Option<SearchStrategy>> {
    Ok(match value {
        None => None,
        Some("auto") => Some(SearchStrategy::Auto),
        Some("bm25") => Some(SearchStrategy::Bm25),
        Some("semantic") => Some(SearchStrategy::Semantic),
        Some("hybrid") => Some(SearchStrategy::Hybrid),
        Some(other) => anyhow::bail!(
            "Invalid strategy: {other}. Use 'auto', 'bm25', 'semantic', or 'hybrid'."
        ),
    })
}

fn embedding_downgrade_reason(err: &anyhow::Error) -> Option<&'static str> {
    let message = err.to_string();
    if message.contains("Semantic search requires indexed embeddings, but this index has none") {
        Some("index has no embeddings")
    } else if message.contains("Re-index before using semantic search.") {
        Some("index embeddings do not match the current runtime model")
    } else if message.contains("No embedding backend compiled")
        || message.contains("embeddings support is not compiled into this build")
    {
        Some("this build does not include semantic embedding support")
    } else {
        None
    }
}

fn plan_search_strategy(
    strategy: SearchStrategy,
    query: &str,
    intent: QueryIntent,
) -> SearchStrategyPlan {
    match strategy {
        SearchStrategy::Bm25 => SearchStrategyPlan::Bm25,
        SearchStrategy::Semantic => SearchStrategyPlan::SemanticOnly,
        SearchStrategy::Hybrid => SearchStrategyPlan::Advanced {
            use_semantic: true,
            intent,
        },
        SearchStrategy::Auto => {
            let resolved_intent = match intent {
                QueryIntent::Auto => classify_intent(query),
                other => other,
            };
            SearchStrategyPlan::Advanced {
                use_semantic: resolved_intent != QueryIntent::Symbol,
                intent: resolved_intent,
            }
        }
    }
}

#[cfg(feature = "embeddings")]
fn semantic_only_search(
    index: &crate::IndexFile,
    query: &str,
    filters: &SearchFilters,
    limit: usize,
    explain: bool,
) -> anyhow::Result<Vec<crate::SearchResult>> {
    let embeddings = crate::embeddings::validate_index_embeddings(index)?;
    let query_embedding = crate::embeddings::generate_embedding(query)?;
    let mut results = vector_search(
        &index.chunks,
        &index.chunk_refs,
        embeddings,
        &query_embedding,
        filters,
        limit,
    );

    if explain {
        for result in &mut results {
            if result.match_reason.is_none() {
                result.match_reason = Some(explain_match(&[("dense", result.score)], None, query));
            }
        }
    }

    Ok(results)
}

#[cfg(not(feature = "embeddings"))]
fn semantic_only_search(
    index: &crate::IndexFile,
    query: &str,
    filters: &SearchFilters,
    limit: usize,
    explain: bool,
) -> anyhow::Result<Vec<crate::SearchResult>> {
    search_advanced(
        index,
        query,
        filters.clone(),
        limit,
        false,
        QueryIntent::Semantic,
        explain,
    )
}

#[cfg(feature = "embeddings")]
fn parse_hybrid_strategy(value: Option<&str>) -> Result<HybridStrategy> {
    let normalized = value.map(|v| v.to_ascii_lowercase());
    match normalized.as_deref() {
        None => Ok(HybridStrategy::default()),
        Some("rrf") => Ok(HybridStrategy::Rrf),
        Some("linear") => Ok(HybridStrategy::Linear),
        Some(other) => anyhow::bail!("Invalid hybrid_strategy: {other}. Use 'rrf' or 'linear'."),
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
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);

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
        // Graph modes: callers/callees/importers use the code graph
        "callers" | "callees" | "importers" => {
            let symbol = input.path_filter.as_deref()
                .ok_or_else(|| anyhow::anyhow!(
                    "mode '{}' requires path_filter to be set to the target symbol name", input.mode
                ))?;

            let graph = CodeGraph::build(&index.chunks);

            let chunk_ids: Vec<&str> = match input.mode.as_str() {
                "callers" => graph.get_callers(symbol).iter().map(|s| s.as_str()).collect(),
                "callees" => {
                    // For callees, find the chunk(s) that define this symbol, then get their callees
                    let def_ids = graph.get_definitions(symbol);
                    def_ids.iter()
                        .flat_map(|id| graph.get_callees(id.as_str()))
                        .map(|s| s.as_str())
                        .collect()
                }
                "importers" => graph.get_importers(symbol).iter().map(|s| s.as_str()).collect(),
                _ => unreachable!(),
            };

            // Resolve chunk IDs to human-readable path:line references
            let chunk_map: std::collections::HashMap<&str, &crate::Chunk> = index.chunks
                .iter()
                .map(|c| (c.id.as_str(), c))
                .collect();

            let mut result: Vec<String> = chunk_ids
                .iter()
                .filter_map(|id| chunk_map.get(*id))
                .map(|chunk| {
                    let sym_or_path = chunk.qualified_name.as_deref()
                        .or(chunk.symbol.as_deref())
                        .unwrap_or("(anonymous)");
                    format!("{}:{}-{} {}", chunk.path, chunk.start_line, chunk.end_line, sym_or_path)
                })
                .collect();
            result.sort();
            result.dedup();
            result
        }
        _ => anyhow::bail!("Invalid mode: {}. Use 'files', 'outline', 'symbols', 'callers', 'callees', or 'importers'", input.mode),
    };

    Ok(ExploreOutput {
        total: items.len(),
        items,
        readiness_tier,
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
/// - `stats`: Returns detailed counts for a single index (requires `index_id`)
///
/// # Returns
///
/// Returns `ManageOutput` with:
/// - `success`: Whether operation succeeded
/// - `indexes`: Array of metadata (for `list` action)
/// - `message`: Success message (for `delete` action)
/// - `stats`: Detailed counts (for `stats` action)
///
/// # Errors
///
/// Returns error if:
/// - Action is invalid (not "list", "delete", or "stats")
/// - Delete/stats action missing `index_id`
/// - Index file cannot be removed
pub fn llmx_manage_handler(store: &mut IndexStore, input: ManageInput) -> Result<ManageOutput> {
    match input.action.as_str() {
        "list" => {
            let indexes = store.list()?;
            let readiness_tier = readiness_tier_for_store(store)?;
            Ok(ManageOutput {
                success: true,
                readiness_tier,
                indexes: Some(indexes),
                message: None,
                stats: None,
            })
        }
        "delete" => {
            let index_id = input.index_id
                .context("index_id is required for delete action")?;
            store.delete(&index_id)?;
            let readiness_tier = readiness_tier_for_store(store)?;
            Ok(ManageOutput {
                success: true,
                readiness_tier,
                indexes: None,
                message: Some(format!("Index {} deleted successfully", index_id)),
                stats: None,
            })
        }
        "stats" => {
            let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
            let index = store.load(&index_id)?;
            let readiness_tier = readiness_tier_for_index(index);
            Ok(ManageOutput {
                success: true,
                readiness_tier,
                indexes: None,
                message: None,
                stats: Some(build_manage_stats(index)),
            })
        }
        _ => anyhow::bail!("Invalid action: {}. Use 'list', 'delete', or 'stats'", input.action),
    }
}

fn build_manage_stats(index: &crate::IndexFile) -> ManageStatsOutput {
    let mut file_kind_breakdown = BTreeMap::new();
    let mut extension_breakdown = BTreeMap::new();
    let mut ast_kind_breakdown = BTreeMap::new();
    let mut edge_kind_breakdown = BTreeMap::new();
    let mut unique_languages = std::collections::BTreeSet::new();

    for file in &index.files {
        let kind = chunk_kind_label(file.kind).to_string();
        *file_kind_breakdown.entry(kind.clone()).or_insert(0) += 1;
        unique_languages.insert(kind);

        let extension = file_extension_label(&file.path);
        *extension_breakdown.entry(extension).or_insert(0) += 1;
    }

    for chunk in &index.chunks {
        if let Some(ast_kind) = chunk.ast_kind {
            *ast_kind_breakdown
                .entry(ast_kind_label_for_stats(ast_kind).to_string())
                .or_insert(0) += 1;
        }
    }

    for edges in index.edges.forward.values() {
        for edge in edges {
            *edge_kind_breakdown
                .entry(edge_kind_label_for_stats(edge.edge_kind).to_string())
                .or_insert(0) += 1;
        }
    }

    ManageStatsOutput {
        total_files: index.stats.total_files,
        total_chunks: index.stats.total_chunks,
        avg_chunk_tokens: index.stats.avg_chunk_tokens,
        symbol_count: index.symbols.values().map(Vec::len).sum(),
        edge_count: index.edges.forward.values().map(Vec::len).sum(),
        language_count: unique_languages.len(),
        file_kind_breakdown,
        extension_breakdown,
        ast_kind_breakdown,
        edge_kind_breakdown,
    }
}

fn chunk_kind_label(kind: crate::ChunkKind) -> &'static str {
    match kind {
        crate::ChunkKind::Markdown => "markdown",
        crate::ChunkKind::Json => "json",
        crate::ChunkKind::JavaScript => "javascript",
        crate::ChunkKind::Html => "html",
        crate::ChunkKind::Text => "text",
        crate::ChunkKind::Image => "image",
        crate::ChunkKind::Unknown => "unknown",
    }
}

fn ast_kind_label_for_stats(kind: crate::model::AstNodeKind) -> &'static str {
    match kind {
        crate::model::AstNodeKind::Function => "function",
        crate::model::AstNodeKind::Method => "method",
        crate::model::AstNodeKind::Class => "class",
        crate::model::AstNodeKind::Module => "module",
        crate::model::AstNodeKind::Interface => "interface",
        crate::model::AstNodeKind::Type => "type",
        crate::model::AstNodeKind::Enum => "enum",
        crate::model::AstNodeKind::Constant => "constant",
        crate::model::AstNodeKind::Variable => "variable",
        crate::model::AstNodeKind::Import => "import",
        crate::model::AstNodeKind::Export => "export",
        crate::model::AstNodeKind::Test => "test",
        crate::model::AstNodeKind::Other => "other",
    }
}

fn edge_kind_label_for_stats(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Imports => "imports",
        EdgeKind::Calls => "calls",
        EdgeKind::TypeRef => "type_ref",
    }
}

fn file_extension_label(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "(none)".to_string())
}

/// Handler for `llmx_symbols` tool: Zero-cost symbol table lookup.
///
/// Unlike `llmx_search`, this doesn't invoke BM25 or fuzzy matching — it scans
/// the structural metadata populated by tree-sitter enrichment, making it
/// essentially free (<1ms) and 100% precise for exact/prefix/glob lookups.
///
/// # Pattern syntax
///
/// - `"verifyToken"` — exact match on qualified_name or symbol
/// - `"verify*"` — prefix match (anything starting with "verify")
/// - `"*Token"` — suffix match
/// - `"*token*"` — case-insensitive substring match
/// - `None` — return all symbols (subject to `limit`)
///
/// # Returns
///
/// `SymbolsOutput` with:
/// - `symbols`: Sorted by qualified_name; each entry has path, lines, signature, doc_summary, chunk_id
/// - `total`: Count before limit
pub fn llmx_symbols_handler(store: &mut IndexStore, input: SymbolsInput) -> Result<SymbolsOutput> {
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);
    let limit = input.limit.unwrap_or(50).min(500);

    let kind_filter = input.ast_kind.as_deref().map(parse_ast_kind_filter);

    let mut entries: Vec<SymbolEntry> = index
        .chunks
        .iter()
        .filter(|chunk| {
            // Must have structural metadata
            chunk.ast_kind.is_some()
        })
        .filter(|chunk| {
            // Path prefix filter
            if let Some(ref prefix) = input.path_prefix {
                if !chunk.path.starts_with(prefix.as_str()) {
                    return false;
                }
            }
            true
        })
        .filter(|chunk| {
            // AST kind filter
            if let Some(ref filter_kind) = kind_filter {
                return chunk.ast_kind.as_ref() == Some(filter_kind);
            }
            true
        })
        .filter(|chunk| {
            // Pattern match against qualified_name or symbol
            let name = chunk.qualified_name.as_deref()
                .or(chunk.symbol.as_deref())
                .unwrap_or("");
            match_pattern(name, input.pattern.as_deref())
        })
        .map(|chunk| {
            let qname = chunk.qualified_name.clone()
                .or_else(|| chunk.symbol.clone())
                .unwrap_or_else(|| chunk.short_id.clone());
            let ast_kind_str = chunk.ast_kind
                .as_ref()
                .map(|k| format!("{:?}", k).to_ascii_lowercase())
                .unwrap_or_else(|| "other".to_string());
            let exported = !chunk.exports.is_empty();
            SymbolEntry {
                qualified_name: qname,
                ast_kind: ast_kind_str,
                path: chunk.path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                signature: chunk.signature.clone(),
                doc_summary: chunk.doc_summary.clone(),
                exported,
                chunk_id: chunk.id.clone(),
            }
        })
        .collect();

    // Sort by qualified_name for deterministic output
    entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    let total = entries.len();
    entries.truncate(limit);

    Ok(SymbolsOutput {
        symbols: entries,
        total,
        readiness_tier,
    })
}

pub fn llmx_lookup_handler(store: &mut IndexStore, input: LookupInput) -> Result<LookupOutput> {
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);
    let limit = input.limit.unwrap_or(20).min(200);
    let kind_filter = input.kind.as_deref().map(|kind| kind.to_ascii_lowercase());
    let chunk_map: std::collections::HashMap<&str, &crate::Chunk> = index
        .chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();

    let normalized_symbol = normalize_symbol_key(&input.symbol);
    let prefix = input.symbol.strip_suffix('*').map(normalize_symbol_key);

    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_entry = |entry: &SymbolIndexEntry| {
        if !seen.insert(entry.chunk_id.clone()) {
            return;
        }
        if let Some(prefix) = input.path_prefix.as_deref() {
            if !entry.path.starts_with(prefix) {
                return;
            }
        }
        if let Some(kind_filter) = kind_filter.as_deref() {
            if ast_kind_label(entry.ast_kind) != kind_filter {
                return;
            }
        }

        let exported = chunk_map
            .get(entry.chunk_id.as_str())
            .map(|chunk| !chunk.exports.is_empty())
            .unwrap_or(false);

        entries.push(SymbolEntry {
            qualified_name: entry.qualified_name.clone(),
            ast_kind: ast_kind_label(entry.ast_kind).to_string(),
            path: entry.path.clone(),
            start_line: entry.start_line,
            end_line: entry.end_line,
            signature: entry.signature.clone(),
            doc_summary: entry.doc_summary.clone(),
            exported,
            chunk_id: entry.chunk_id.clone(),
        });
    };

    if let Some(prefix) = prefix.as_deref() {
        for (key, matches) in index.symbols.range(prefix.to_string()..) {
            if !key.starts_with(prefix) {
                break;
            }
            for entry in matches {
                if entry.name.to_ascii_lowercase().starts_with(prefix)
                    || entry.qualified_name.to_ascii_lowercase().starts_with(prefix)
                {
                    push_entry(entry);
                }
            }
        }
    } else if let Some(matches) = index.symbols.get(&normalized_symbol) {
        for entry in matches {
            push_entry(entry);
        }
    }

    entries.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    let total = entries.len();
    entries.truncate(limit);

    Ok(LookupOutput {
        matches: entries,
        total,
        readiness_tier,
    })
}

pub fn llmx_refs_handler(store: &mut IndexStore, input: RefsInput) -> Result<RefsOutput> {
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);
    let limit = input.limit.unwrap_or(20).min(200);
    let depth = input.depth.unwrap_or(1).clamp(1, 8);
    let direction = input.direction.to_ascii_lowercase();
    let chunk_map: std::collections::HashMap<&str, &crate::Chunk> = index
        .chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();

    let refs = match direction.as_str() {
        "callers" | "importers" | "type_users" => {
            collect_reverse_refs(index, &direction, &input.symbol, depth, limit, &chunk_map)?
        }
        "callees" | "imports" => {
            collect_forward_refs(index, &direction, &input.symbol, depth, limit, &chunk_map)?
        }
        _ => anyhow::bail!(
            "Invalid direction: {}. Use 'callers', 'callees', 'importers', 'imports', or 'type_users'.",
            input.direction
        ),
    };

    Ok(RefsOutput {
        total: refs.len(),
        refs,
        readiness_tier,
    })
}

pub fn llmx_get_chunk_handler(store: &mut IndexStore, input: GetChunkInput) -> Result<Option<GetChunkOutput>> {
    let index_id = resolve_index_id(store, input.index_id.as_deref(), input.loc.as_deref())?;
    let index = store.load(&index_id)?;
    let readiness_tier = readiness_tier_for_index(index);

    let chunk = index.chunks.iter().find(|chunk| chunk.id == input.chunk_id)
        .or_else(|| {
            let id_from_ref = index.chunk_refs.iter()
                .find(|(_, reference)| reference.as_str() == input.chunk_id)
                .map(|(chunk_id, _)| chunk_id.as_str());
            id_from_ref.and_then(|chunk_id| index.chunks.iter().find(|chunk| chunk.id == chunk_id))
        })
        .or_else(|| index.chunks.iter().find(|chunk| chunk.id.starts_with(&input.chunk_id)));

    Ok(chunk.map(|chunk| GetChunkOutput {
        chunk_id: chunk.id.clone(),
        path: chunk.path.clone(),
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        content: chunk.content.clone(),
        symbol: chunk.symbol.clone(),
        heading_path: chunk.heading_path.clone(),
        token_estimate: chunk.token_estimate,
        readiness_tier,
    }))
}

fn collect_reverse_refs(
    index: &crate::IndexFile,
    direction: &str,
    symbol: &str,
    depth: usize,
    limit: usize,
    chunk_map: &std::collections::HashMap<&str, &crate::Chunk>,
) -> Result<Vec<RefResult>> {
    let edge_kind = match direction {
        "callers" => EdgeKind::Calls,
        "importers" => EdgeKind::Imports,
        "type_users" => EdgeKind::TypeRef,
        _ => anyhow::bail!("Unsupported reverse direction: {direction}"),
    };

    let mut frontier = resolve_reverse_keys(index, symbol);
    let mut visited_symbols = std::collections::HashSet::new();
    let mut seen_refs = std::collections::HashSet::new();
    let mut results = Vec::new();

    for _ in 0..depth {
        let mut next_frontier = Vec::new();
        for key in frontier {
            if !visited_symbols.insert(key.clone()) {
                continue;
            }
            let Some(edges) = index.edges.reverse.get(&key) else { continue };
            for edge in edges {
                if edge.edge_kind != edge_kind {
                    continue;
                }
                if !seen_refs.insert((edge.source_chunk_id.clone(), edge.target_symbol.clone(), edge.edge_kind)) {
                    continue;
                }
                if let Some(ref_result) = build_ref_result(edge, chunk_map, false) {
                    if results.len() < limit {
                        results.push(ref_result);
                    }

                    if let Some(source_chunk) = chunk_map.get(edge.source_chunk_id.as_str()) {
                        if let Some(symbol) = source_chunk
                            .qualified_name
                            .as_deref()
                            .or(source_chunk.symbol.as_deref())
                        {
                            next_frontier.push(canonical_symbol_key(symbol));
                        }
                    }
                }
            }
        }
        if next_frontier.is_empty() || results.len() >= limit {
            break;
        }
        frontier = next_frontier;
    }

    sort_ref_results(&mut results);
    Ok(results)
}

fn collect_forward_refs(
    index: &crate::IndexFile,
    direction: &str,
    symbol: &str,
    depth: usize,
    limit: usize,
    chunk_map: &std::collections::HashMap<&str, &crate::Chunk>,
) -> Result<Vec<RefResult>> {
    let edge_kind = match direction {
        "callees" => EdgeKind::Calls,
        "imports" => EdgeKind::Imports,
        _ => anyhow::bail!("Unsupported forward direction: {direction}"),
    };

    let mut frontier = lookup_symbol_chunk_ids(index, symbol);
    let mut visited_chunks = std::collections::HashSet::new();
    let mut seen_refs = std::collections::HashSet::new();
    let mut results = Vec::new();

    for _ in 0..depth {
        let mut next_frontier = Vec::new();
        for chunk_id in frontier {
            if !visited_chunks.insert(chunk_id.clone()) {
                continue;
            }
            let Some(edges) = index.edges.forward.get(&chunk_id) else { continue };
            for edge in edges {
                if edge.edge_kind != edge_kind {
                    continue;
                }
                if !seen_refs.insert((edge.source_chunk_id.clone(), edge.target_symbol.clone(), edge.edge_kind)) {
                    continue;
                }
                if let Some(ref_result) = build_ref_result(edge, chunk_map, true) {
                    if results.len() < limit {
                        results.push(ref_result);
                    }

                    if let Some(target_chunk_id) = edge.target_chunk_id.as_ref() {
                        next_frontier.push(target_chunk_id.clone());
                    } else {
                        next_frontier.extend(lookup_symbol_chunk_ids(index, &edge.target_symbol));
                    }
                }
            }
        }
        if next_frontier.is_empty() || results.len() >= limit {
            break;
        }
        frontier = next_frontier;
    }

    sort_ref_results(&mut results);
    Ok(results)
}

fn lookup_symbol_chunk_ids(index: &crate::IndexFile, symbol: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut chunk_ids = Vec::new();

    for key in resolve_symbol_lookup_keys(index, symbol) {
        if let Some(entries) = index.symbols.get(&key) {
            for entry in entries {
                if seen.insert(entry.chunk_id.clone()) {
                    chunk_ids.push(entry.chunk_id.clone());
                }
            }
        }
    }

    chunk_ids
}

fn resolve_reverse_keys(index: &crate::IndexFile, symbol: &str) -> Vec<String> {
    let keys = resolve_symbol_lookup_keys(index, symbol);
    if keys.is_empty() {
        vec![raw_symbol_key(symbol)]
    } else {
        keys
    }
}

fn resolve_symbol_lookup_keys(index: &crate::IndexFile, symbol: &str) -> Vec<String> {
    let normalized = normalize_symbol_key(symbol);
    let Some(entries) = index.symbols.get(&normalized) else {
        return Vec::new();
    };

    if looks_qualified_symbol(symbol) {
        return vec![normalized];
    }

    let mut keys = std::collections::HashSet::new();
    let mut ordered = Vec::new();
    for entry in entries {
        let key = canonical_symbol_key(&entry.qualified_name);
        if keys.insert(key.clone()) {
            ordered.push(key);
        }
    }
    ordered
}

fn looks_qualified_symbol(symbol: &str) -> bool {
    symbol.contains("::") || symbol.contains('.')
}

fn build_ref_result(
    edge: &Edge,
    chunk_map: &std::collections::HashMap<&str, &crate::Chunk>,
    use_target_context: bool,
) -> Option<RefResult> {
    let source_chunk = chunk_map.get(edge.source_chunk_id.as_str())?;
    let target_chunk = edge
        .target_chunk_id
        .as_ref()
        .and_then(|chunk_id| chunk_map.get(chunk_id.as_str()));
    let context_chunk = if use_target_context {
        target_chunk.unwrap_or(source_chunk)
    } else {
        source_chunk
    };
    let target_symbol = edge.target_chunk_id.as_ref()
        .and_then(|chunk_id| chunk_map.get(chunk_id.as_str()))
        .and_then(|target| target.qualified_name.clone().or_else(|| target.symbol.clone()))
        .unwrap_or_else(|| edge.target_symbol.clone());
    Some(RefResult {
        source_symbol: source_chunk
            .qualified_name
            .clone()
            .or_else(|| source_chunk.symbol.clone())
            .unwrap_or_else(|| source_chunk.short_id.clone()),
        target_symbol,
        path: context_chunk.path.clone(),
        start_line: context_chunk.start_line,
        end_line: context_chunk.end_line,
        ast_kind: context_chunk.ast_kind.map(|kind| ast_kind_label(kind).to_string()),
        signature: context_chunk.signature.clone(),
        context: crate::util::snippet(&context_chunk.content, 200),
        chunk_id: context_chunk.id.clone(),
        target_chunk_id: edge.target_chunk_id.clone(),
    })
}

fn sort_ref_results(results: &mut [RefResult]) {
    results.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.source_symbol.cmp(&b.source_symbol))
            .then_with(|| a.target_symbol.cmp(&b.target_symbol))
    });
}

/// Pattern match a name against a user-supplied glob-like pattern.
///
/// - `None` → always matches
/// - `"*foo*"` → case-insensitive contains
/// - `"foo*"` → case-insensitive prefix
/// - `"*foo"` → case-insensitive suffix
/// - `"foo"` → case-insensitive exact
fn match_pattern(name: &str, pattern: Option<&str>) -> bool {
    let Some(pat) = pattern else { return true };
    if pat.is_empty() { return true; }
    let name_lower = name.to_ascii_lowercase();
    let pat_lower = pat.to_ascii_lowercase();
    if pat_lower.starts_with('*') && pat_lower.ends_with('*') {
        let inner = pat_lower.trim_matches('*');
        name_lower.contains(inner)
    } else if pat_lower.ends_with('*') {
        let prefix = pat_lower.trim_end_matches('*');
        name_lower.starts_with(prefix)
    } else if pat_lower.starts_with('*') {
        let suffix = pat_lower.trim_start_matches('*');
        name_lower.ends_with(suffix)
    } else {
        name_lower == pat_lower
    }
}

/// Parse an ast_kind string filter into the enum variant.
fn parse_ast_kind_filter(s: &str) -> crate::model::AstNodeKind {
    use crate::model::AstNodeKind;
    match s.to_ascii_lowercase().as_str() {
        "function" => AstNodeKind::Function,
        "method" => AstNodeKind::Method,
        "class" => AstNodeKind::Class,
        "module" => AstNodeKind::Module,
        "interface" => AstNodeKind::Interface,
        "type" => AstNodeKind::Type,
        "enum" => AstNodeKind::Enum,
        "constant" => AstNodeKind::Constant,
        "variable" => AstNodeKind::Variable,
        "import" => AstNodeKind::Import,
        "export" => AstNodeKind::Export,
        "test" => AstNodeKind::Test,
        _ => AstNodeKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Chunk, ChunkKind, FileMeta, IndexFile, IndexStats, ResolutionTier, LanguageId, EdgeIndex, INDEX_VERSION};
    use tempfile::tempdir;

    #[test]
    fn test_plan_search_strategy_auto_skips_semantic_for_symbol_queries() {
        let plan = plan_search_strategy(SearchStrategy::Auto, "verifyToken", QueryIntent::Auto);
        assert_eq!(
            plan,
            SearchStrategyPlan::Advanced {
                use_semantic: false,
                intent: QueryIntent::Symbol,
            }
        );
    }

    #[test]
    fn test_plan_search_strategy_semantic_and_hybrid_are_distinct() {
        let semantic = plan_search_strategy(SearchStrategy::Semantic, "auth flow", QueryIntent::Auto);
        let hybrid = plan_search_strategy(SearchStrategy::Hybrid, "auth flow", QueryIntent::Auto);

        assert_eq!(semantic, SearchStrategyPlan::SemanticOnly);
        assert_eq!(
            hybrid,
            SearchStrategyPlan::Advanced {
                use_semantic: true,
                intent: QueryIntent::Auto,
            }
        );
    }

    #[test]
    fn test_embedding_downgrade_reason_detects_missing_embeddings() {
        let err = anyhow::anyhow!("Semantic search requires indexed embeddings, but this index has none");
        assert_eq!(
            embedding_downgrade_reason(&err),
            Some("index has no embeddings")
        );
    }

    #[test]
    fn test_embedding_downgrade_reason_detects_model_mismatch() {
        let err = anyhow::anyhow!(
            "Index embeddings were built with model 'a', but the runtime model is 'b'. Re-index before using semantic search."
        );
        assert_eq!(
            embedding_downgrade_reason(&err),
            Some("index embeddings do not match the current runtime model")
        );
    }

    fn make_index(with_symbols: bool, with_embeddings: bool) -> IndexFile {
        IndexFile {
            version: INDEX_VERSION,
            index_id: "status-test".to_string(),
            files: vec![FileMeta {
                path: "core/src/exec.rs".to_string(),
                root_path: "/tmp/project".to_string(),
                relative_path: "core/src/exec.rs".to_string(),
                kind: ChunkKind::Unknown,
                language: Some(LanguageId::Rust),
                bytes: 16,
                sha256: "abc".to_string(),
                line_count: 1,
                is_generated: false,
                resolution_tier: ResolutionTier::GenericTreeSitter,
                mtime_ms: None,
                fingerprint_sha256: None,
            }],
            chunks: vec![Chunk {
                id: "chunk-1".to_string(),
                short_id: "chunk-1".to_string(),
                slug: "exec".to_string(),
                path: "core/src/exec.rs".to_string(),
                root_path: "/tmp/project".to_string(),
                relative_path: "core/src/exec.rs".to_string(),
                kind: ChunkKind::Unknown,
                language: Some(LanguageId::Rust),
                chunk_index: 0,
                start_line: 1,
                end_line: 1,
                content: "fn codex_exec() {}".to_string(),
                content_hash: "def".to_string(),
                token_estimate: 4,
                heading_path: Vec::new(),
                symbol: with_symbols.then(|| "codex_exec".to_string()),
                address: None,
                asset_path: None,
                is_generated: false,
                quality_score: None,
                resolution_tier: ResolutionTier::GenericTreeSitter,
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
            }],
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
            stats: IndexStats {
                total_files: 1,
                total_chunks: 1,
                avg_chunk_chars: 16,
                avg_chunk_tokens: 4,
            },
            warnings: vec![],
            embeddings: with_embeddings.then(|| vec![vec![0.1, 0.2]]),
            embedding_model: with_embeddings.then(|| "test-model".to_string()),
            symbols: if with_symbols {
                let mut symbols = BTreeMap::new();
                symbols.insert(
                    "codex_exec".to_string(),
                    vec![crate::SymbolIndexEntry {
                        chunk_id: "chunk-1".to_string(),
                        name: "codex_exec".to_string(),
                        qualified_name: "codex_exec".to_string(),
                        ast_kind: crate::AstNodeKind::Function,
                        path: "core/src/exec.rs".to_string(),
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        doc_summary: None,
                        parent_symbol: None,
                    }],
                );
                symbols
            } else {
                BTreeMap::new()
            },
            edges: EdgeIndex::default(),
        }
    }

    #[test]
    fn test_llmx_status_handler_reports_readiness_and_languages() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        store.save(make_index(true, true), "/tmp/project".to_string())?;
        let jobs = crate::mcp::new_job_store();

        let status = llmx_status_handler(&mut store, &jobs)?;
        assert_eq!(status.readiness_tier, 3);
        assert_eq!(status.files_indexed, 1);
        assert_eq!(status.files_total, 1);
        assert_eq!(status.symbols_indexed, 1);
        assert!(status.embeddings_ready);
        assert_eq!(status.languages, vec!["rust".to_string()]);
        assert!(status.background_tasks.is_empty());
        Ok(())
    }

    #[test]
    fn test_llmx_status_handler_tracks_background_jobs() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        store.save(make_index(false, false), "/tmp/project".to_string())?;
        let jobs = crate::mcp::new_job_store();
        jobs.lock().unwrap().insert("job-1".to_string(), crate::mcp::JobState::queued());

        let status = llmx_status_handler(&mut store, &jobs)?;
        assert_eq!(status.readiness_tier, 1);
        assert_eq!(status.background_tasks.len(), 1);
        assert_eq!(status.background_tasks[0].job_id, "job-1");
        assert_eq!(status.background_tasks[0].status, "queued");
        Ok(())
    }
}
