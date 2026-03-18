use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INDEX_VERSION: u32 = 3;
pub const DEFAULT_MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LanguageId {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Shell,
    Sql,
    Html,
    Css,
    Json,
    Markdown,
    Toml,
    Yaml,
    Other(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTier {
    StackGraph,
    QueryPack,
    GenericTreeSitter,
    #[default]
    TextOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Pub,
    Crate,
    Private,
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
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_total_bytes: usize::MAX,
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
    #[serde(default)]
    pub root_path: String,
    #[serde(default)]
    pub relative_path: String,
    pub kind: ChunkKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<LanguageId>,
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
    #[serde(default)]
    pub is_generated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<u16>,
    #[serde(default)]
    pub resolution_tier: ResolutionTier,

    // Phase 7: Structural metadata for code intelligence
    /// AST node kind (function, class, method, module, import, type, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast_kind: Option<AstNodeKind>,
    /// Fully qualified symbol name: "auth::jwt::verify_token"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    /// Stable symbol identity for cross-file lookup and graph traversal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// Final segment of the qualified symbol for fuzzy/tail lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_tail: Option<String>,
    /// Function/method signature: "fn verify_token(token: &str) -> Result<Claims>"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Language-specific module/namespace path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// Parent scope symbol: "auth::jwt" or enclosing class/module
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_symbol: Option<String>,
    /// Public API visibility when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    /// Symbols imported or referenced by this chunk
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    /// Symbols defined/exported by this chunk
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    /// Functions/methods called within this chunk
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<String>,
    /// Types referenced (struct names, trait bounds, type annotations)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_refs: Vec<String>,
    /// First sentence of doc comment, if present
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_summary: Option<String>,
}

/// Phase 7: AST node classification for structural search
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AstNodeKind {
    Function,
    Method,
    Class,
    Module,
    Interface,
    Type,
    Enum,
    Constant,
    Variable,
    Import,
    Export,
    Test,
    /// Non-code or unclassified
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    #[serde(default)]
    pub root_path: String,
    #[serde(default)]
    pub relative_path: String,
    pub kind: ChunkKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<LanguageId>,
    pub bytes: usize,
    pub sha256: String,
    pub line_count: usize,
    #[serde(default)]
    pub is_generated: bool,
    #[serde(default)]
    pub resolution_tier: ResolutionTier,
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
    /// Structural symbol lookup table keyed by lowercase symbol/prefix.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbols: SymbolTable,
    /// Forward and reverse relationship indexes for code graph traversal.
    #[serde(default, skip_serializing_if = "EdgeIndex::is_empty")]
    pub edges: EdgeIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolIndexEntry {
    pub name: String,
    pub qualified_name: String,
    pub ast_kind: AstNodeKind,
    pub chunk_id: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_symbol: Option<String>,
}

pub type SymbolTable = BTreeMap<String, Vec<SymbolIndexEntry>>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Imports,
    Calls,
    TypeRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Edge {
    pub source_chunk_id: String,
    pub target_symbol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_chunk_id: Option<String>,
    pub edge_kind: EdgeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EdgeIndex {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub forward: BTreeMap<String, Vec<Edge>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reverse: BTreeMap<String, Vec<Edge>>,
}

impl EdgeIndex {
    pub fn is_empty(&self) -> bool { self.forward.is_empty() && self.reverse.is_empty() }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    pub path_exact: Option<String>,
    pub path_prefix: Option<String>,
    pub kind: Option<ChunkKind>,
    pub heading_prefix: Option<String>,
    pub symbol_prefix: Option<String>,
}

/// Phase 6: Hybrid search strategy
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HybridStrategy {
    /// Linear combination (Phase 5 default, kept for compatibility)
    Linear,
    /// Reciprocal Rank Fusion (Phase 6 default, recommended)
    #[default]
    Rrf,
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
    /// Phase 7: Human-readable explanation of why this result matched
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_reason: Option<String>,
    /// Phase 7: Which engines contributed to this result
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_engines: Vec<String>,
}

/// Phase 7: Query intent classification for adaptive routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    /// Looks like a symbol name: camelCase, snake_case, ::, .method
    Symbol,
    /// Natural language question: "how does auth work"
    Semantic,
    /// Exact keyword/grep-like: "TODO", "FIXME", "unsafe"
    Keyword,
    /// Auto-detect (default)
    Auto,
}

impl Default for QueryIntent {
    fn default() -> Self { Self::Auto }
}
