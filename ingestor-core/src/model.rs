use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInput {
    pub path: String,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    #[serde(default)]
    pub mtime_ms: Option<u64>,
    #[serde(default)]
    pub fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestOptions {
    pub chunk_target_chars: usize,
    pub chunk_max_chars: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_chunks_per_file: usize,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            chunk_target_chars: 4_000,
            chunk_max_chars: 8_000,
            max_file_bytes: 10 * 1024 * 1024,
            max_total_bytes: 50 * 1024 * 1024,
            max_chunks_per_file: 2_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    Markdown,
    Json,
    JavaScript,
    Html,
    Text,
    Image,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub short_id: String,
    pub slug: String,
    pub path: String,
    pub kind: ChunkKind,
    pub chunk_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub content_hash: String,
    pub token_estimate: usize,
    pub heading_path: Vec<String>,
    pub symbol: Option<String>,
    pub address: Option<String>,
    #[serde(default)]
    pub asset_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    pub kind: ChunkKind,
    pub bytes: usize,
    pub sha256: String,
    pub line_count: usize,
    #[serde(default)]
    pub mtime_ms: Option<u64>,
    #[serde(default)]
    pub fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    pub chunk_id: String,
    pub tf: usize,
    pub doc_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermEntry {
    pub df: usize,
    pub postings: Vec<Posting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_chunks: usize,
    pub avg_chunk_chars: usize,
    pub avg_chunk_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestWarning {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFile {
    pub version: u32,
    pub index_id: String,
    pub files: Vec<FileMeta>,
    pub chunks: Vec<Chunk>,
    #[serde(default)]
    pub chunk_refs: BTreeMap<String, String>,
    pub inverted_index: BTreeMap<String, TermEntry>,
    pub stats: IndexStats,
    pub warnings: Vec<IngestWarning>,
    /// Phase 5: Embeddings for semantic search (one per chunk)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<Vec<Vec<f32>>>,
    /// Embedding model identifier for cache invalidation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    pub path_exact: Option<String>,
    pub path_prefix: Option<String>,
    pub kind: Option<ChunkKind>,
    pub heading_prefix: Option<String>,
    pub symbol_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub chunk_ref: String,
    pub score: f32,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub heading_path: Vec<String>,
}
