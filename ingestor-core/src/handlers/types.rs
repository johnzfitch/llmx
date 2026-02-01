//! Input/Output types for llmx handlers.

use crate::handlers::storage::IndexMetadata;
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Deserialize, Default)]
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
