//! Input/Output types for llmx handlers.

use super::IndexMetadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// Input types

#[derive(Debug, Deserialize)]
pub struct IndexInput {
    pub paths: Vec<String>,
    #[serde(default)]
    pub options: Option<IngestOptionsInput>,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestOptionsInput {
    pub chunk_target_chars: Option<usize>,
    pub max_file_bytes: Option<usize>,
    pub max_total_bytes: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct SearchInput {
    pub index_id: String,
    pub query: String,
    #[serde(default)]
    pub filters: Option<SearchFiltersInput>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub use_semantic: Option<bool>,
    #[serde(default)]
    pub hybrid_strategy: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub explain: Option<bool>,
    #[serde(default)]
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchFiltersInput {
    pub path_prefix: Option<String>,
    pub kind: Option<String>,
    pub symbol_prefix: Option<String>,
    pub heading_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExploreInput {
    pub index_id: String,
    pub mode: String,
    #[serde(default)]
    pub path_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ManageInput {
    pub action: String,
    #[serde(default)]
    pub index_id: Option<String>,
}

// Output types

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

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResultOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_ids: Option<Vec<String>>,
    pub total_matches: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SearchNoticeOutput>,
}

#[derive(Debug, Serialize)]
pub struct SearchNoticeOutput {
    pub code: String,
    pub message: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_engines: Vec<String>,
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

#[derive(Debug, Serialize)]
pub struct ChunkOutput {
    pub chunk_id: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub heading_path: Vec<String>,
    pub token_estimate: usize,
}

// Dynamic search types

/// Input for dynamic search (no persistent index required).
#[derive(Debug, Deserialize)]
pub struct DynamicSearchInput {
    /// The search query
    pub query: String,
    /// Explicit search path (default: auto-detect project root)
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Force dynamic mode (ignore persistent index)
    #[serde(default)]
    pub force_dynamic: bool,
    /// Skip cache (force fresh index build)
    #[serde(default)]
    pub no_cache: bool,
    /// Allow dangerous paths (/, /home, etc.)
    #[serde(default)]
    pub force_dangerous: bool,
    /// Search filters
    #[serde(default)]
    pub filters: Option<SearchFiltersInput>,
    /// Maximum number of results (default: 10)
    #[serde(default)]
    pub limit: Option<usize>,
    /// Token budget for inline content (default: 8000)
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Use hybrid BM25+embeddings search
    #[serde(default)]
    pub use_semantic: Option<bool>,
    /// Hybrid fusion strategy (`rrf` or `linear`)
    #[serde(default)]
    pub hybrid_strategy: Option<String>,
    /// Phase 7: Query intent routing (`auto`, `symbol`, `semantic`, `keyword`)
    #[serde(default)]
    pub intent: Option<String>,
    /// Phase 7: Include human-readable result explanations
    #[serde(default)]
    pub explain: Option<bool>,
    /// Search strategy (`auto`, `bm25`, `semantic`, or `hybrid`)
    #[serde(default)]
    pub strategy: Option<String>,
}

/// Output from dynamic search.
#[derive(Debug, Serialize)]
pub struct DynamicSearchOutput {
    /// Search mode used: "dynamic", "cached", or "persistent"
    pub mode: String,
    /// Search results with inline content
    pub results: Vec<SearchResultOutput>,
    /// Statistics about the search operation
    pub stats: DynamicSearchStats,
    /// IDs of results that exceeded token budget
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_ids: Option<Vec<String>>,
    /// Total number of matches found
    pub total_matches: usize,
    /// Search notices such as semantic downgrade messages
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SearchNoticeOutput>,
}

/// Phase 7: Input for llmx_symbols — precise symbol table lookup.
#[derive(Debug, Deserialize)]
pub struct SymbolsInput {
    pub index_id: String,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub ast_kind: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// A single symbol entry returned by llmx_symbols.
#[derive(Debug, Serialize)]
pub struct SymbolEntry {
    pub qualified_name: String,
    pub ast_kind: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_summary: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub exported: bool,
    pub chunk_id: String,
}

fn is_false(b: &bool) -> bool { !b }

#[derive(Debug, Serialize)]
pub struct SymbolsOutput {
    pub symbols: Vec<SymbolEntry>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct LookupInput {
    pub index_id: String,
    pub symbol: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LookupOutput {
    pub matches: Vec<SymbolEntry>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct RefsInput {
    pub index_id: String,
    pub symbol: String,
    pub direction: String,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
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
}

/// Statistics for dynamic search operations.
#[derive(Debug, Serialize, Clone, Default)]
pub struct DynamicSearchStats {
    /// Number of files processed
    pub file_count: usize,
    /// Total bytes processed
    pub total_bytes: usize,
    /// Number of chunks in index
    pub chunk_count: usize,
    /// Indexing time in milliseconds
    pub index_time_ms: u64,
    /// Search time in milliseconds
    pub search_time_ms: u64,
    /// Whether the walk was truncated due to limits
    pub truncated: bool,
    /// Root path that was searched
    pub root_path: String,
}
