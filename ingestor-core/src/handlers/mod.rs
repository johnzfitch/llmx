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
pub use crate::walk::{ALLOWED_DOTFILES, ALLOWED_EXTENSIONS};

use crate::{
    graph::{ast_kind_label, canonical_symbol_key, normalize_symbol_key, raw_symbol_key, CodeGraph},
    ingest_files_with_root, search, search_advanced, Edge, EdgeKind, IndexFile, IngestOptions,
    QueryIntent, SearchFilters, SymbolIndexEntry, DEFAULT_MAX_FILE_BYTES,
};
use crate::query::classify_intent;
use crate::walk::{collect_input_files, WalkConfig};
#[cfg(feature = "embeddings")]
use crate::vector_search;
#[cfg(feature = "embeddings")]
use crate::HybridStrategy;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const DEFAULT_MAX_TOKENS: usize = 8000;

/// Maximum results per search to prevent huge allocations from unbounded `limit`.
pub const MAX_SEARCH_LIMIT: usize = 200;

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
    let walk_config = WalkConfig {
        max_depth: 50,
        max_files: 200_000,
        max_total_bytes: usize::MAX,
        timeout_secs: 300,
        respect_gitignore: true,
    };
    let (files, root_path, _) = collect_input_files(&input.paths, &walk_config)?;
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
            .unwrap_or(DEFAULT_MAX_FILE_BYTES),
        max_total_bytes: input
            .options
            .as_ref()
            .and_then(|o| o.max_total_bytes)
            .unwrap_or(usize::MAX),
        max_chunks_per_file: 2000,
    };

    let index = {
        #[cfg_attr(not(feature = "embeddings"), allow(unused_mut))]
        let mut index = ingest_files_with_root(files, options, Some(Path::new(&root_path)));
        #[cfg(feature = "embeddings")]
        {
            if index.chunks.iter().any(|chunk| !chunk.content.trim().is_empty()) {
                use crate::embeddings::{generate_embeddings, runtime_model_id};
                let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
                let embeddings = generate_embeddings(&chunk_texts)?;
                index.embeddings = Some(embeddings);
                index.embedding_model = Some(runtime_model_id()?.to_string());
            }
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
/// Results include inline chunk content up to `max_tokens` (default: 8K).
/// When budget is exceeded, remaining chunks are returned in `truncated_ids`.
pub fn llmx_search_handler(store: &mut IndexStore, input: SearchInput) -> Result<SearchOutput> {
    let index = store.load(&input.index_id)?;
    let (results, truncated_ids, total_matches, notices) = perform_search(
        index,
        &input.query,
        &input.filters,
        input.limit,
        input.max_tokens,
        input.use_semantic,
        input.hybrid_strategy.as_deref(),
        input.intent.as_deref(),
        input.explain,
        input.strategy.as_deref(),
    )?;

    Ok(SearchOutput {
        results,
        truncated_ids,
        total_matches,
        notices,
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
        "callers" | "callees" | "importers" => {
            let symbol = input.path_filter.as_deref()
                .ok_or_else(|| anyhow::anyhow!(
                    "mode '{}' requires path_filter to be set to the target symbol name", input.mode
                ))?;
            let graph = CodeGraph::build(&index.chunks);
            let chunk_ids: Vec<&str> = match input.mode.as_str() {
                "callers" => graph.get_callers(symbol).iter().map(|s| s.as_str()).collect(),
                "callees" => {
                    let def_ids = graph.get_definitions(symbol);
                    def_ids.iter().flat_map(|id| graph.get_callees(id.as_str())).map(|s| s.as_str()).collect()
                }
                "importers" => graph.get_importers(symbol).iter().map(|s| s.as_str()).collect(),
                _ => unreachable!(),
            };
            let chunk_map: std::collections::HashMap<&str, &crate::Chunk> =
                index.chunks.iter().map(|c| (c.id.as_str(), c)).collect();
            let mut result: Vec<String> = chunk_ids.iter()
                .filter_map(|id| chunk_map.get(*id))
                .map(|chunk| {
                    let sym_or_path = chunk.qualified_name.as_deref()
                        .or(chunk.symbol.as_deref()).unwrap_or("(anonymous)");
                    format!("{}:{}-{} {}", chunk.path, chunk.start_line, chunk.end_line, sym_or_path)
                })
                .collect();
            result.sort();
            result.dedup();
            result
        }
        _ => anyhow::bail!(
            "Invalid mode: {}. Use 'files', 'outline', 'symbols', 'callers', 'callees', or 'importers'",
            input.mode
        ),
    };

    Ok(ExploreOutput {
        total: items.len(),
        items,
    })
}

/// Handler for `llmx_symbols` tool: Precise symbol table lookup.
///
/// Zero search cost — scans structural metadata, not inverted index.
pub fn llmx_symbols_handler(store: &mut IndexStore, input: SymbolsInput) -> Result<SymbolsOutput> {
    let index = store.load(&input.index_id)?;
    let limit = input.limit.unwrap_or(50).min(500);

    let kind_filter = input.ast_kind.as_deref().map(parse_ast_kind_filter);

    let mut entries: Vec<SymbolEntry> = index
        .chunks
        .iter()
        .filter(|chunk| chunk.ast_kind.is_some())
        .filter(|chunk| {
            if let Some(ref prefix) = input.path_prefix {
                if !chunk.path.starts_with(prefix.as_str()) {
                    return false;
                }
            }
            true
        })
        .filter(|chunk| {
            if let Some(ref filter_kind) = kind_filter {
                return chunk.ast_kind.as_ref() == Some(filter_kind);
            }
            true
        })
        .filter(|chunk| {
            let name = chunk.qualified_name.as_deref()
                .or(chunk.symbol.as_deref())
                .unwrap_or("");
            match_symbol_pattern(name, input.pattern.as_deref())
        })
        .map(|chunk| {
            let qname = chunk.qualified_name.clone()
                .or_else(|| chunk.symbol.clone())
                .unwrap_or_else(|| chunk.short_id.clone());
            let ast_kind_str = chunk.ast_kind.as_ref()
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

    entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    let total = entries.len();
    entries.truncate(limit);
    Ok(SymbolsOutput { symbols: entries, total })
}

pub fn llmx_lookup_handler(store: &mut IndexStore, input: LookupInput) -> Result<LookupOutput> {
    let index = store.load(&input.index_id)?;
    let limit = input.limit.unwrap_or(20).min(200);
    let kind_filter = input.kind.as_deref().map(|kind| kind.to_ascii_lowercase());
    let chunk_map: HashMap<&str, &crate::Chunk> = index
        .chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();

    let normalized_symbol = normalize_symbol_key(&input.symbol);
    let prefix = input.symbol.strip_suffix('*').map(normalize_symbol_key);

    let mut entries = Vec::new();
    let mut seen = HashSet::new();

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
    })
}

pub fn llmx_refs_handler(store: &mut IndexStore, input: RefsInput) -> Result<RefsOutput> {
    let index = store.load(&input.index_id)?;
    let limit = input.limit.unwrap_or(20).min(200);
    let depth = input.depth.unwrap_or(1).clamp(1, 8);
    let direction = input.direction.to_ascii_lowercase();
    let chunk_map: HashMap<&str, &crate::Chunk> = index
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
    })
}

fn match_symbol_pattern(name: &str, pattern: Option<&str>) -> bool {
    let Some(pat) = pattern else { return true };
    if pat.is_empty() { return true; }
    let name_lower = name.to_ascii_lowercase();
    let pat_lower = pat.to_ascii_lowercase();
    if pat_lower.starts_with('*') && pat_lower.ends_with('*') {
        name_lower.contains(pat_lower.trim_matches('*'))
    } else if pat_lower.ends_with('*') {
        name_lower.starts_with(pat_lower.trim_end_matches('*'))
    } else if pat_lower.starts_with('*') {
        name_lower.ends_with(pat_lower.trim_start_matches('*'))
    } else {
        name_lower == pat_lower
    }
}

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

/// Handler for `llmx_manage` tool: List or delete indexes.
pub fn llmx_manage_handler(store: &mut IndexStore, input: ManageInput) -> Result<ManageOutput> {
    match input.action.as_str() {
        "list" => {
            let indexes = store.list()?;
            Ok(ManageOutput {
                success: true,
                indexes: Some(indexes),
                message: None,
                stats: None,
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
                stats: None,
            })
        }
        "stats" => {
            let index_id = input
                .index_id
                .context("index_id is required for stats action")?;
            let index = store.load(&index_id)?;
            Ok(ManageOutput {
                success: true,
                indexes: None,
                message: None,
                stats: Some(build_manage_stats(index)),
            })
        }
        _ => anyhow::bail!(
            "Invalid action: {}. Use 'list', 'delete', or 'stats'",
            input.action
        ),
    }
}

fn build_manage_stats(index: &crate::IndexFile) -> ManageStatsOutput {
    let mut file_kind_breakdown = std::collections::BTreeMap::new();
    let mut extension_breakdown = std::collections::BTreeMap::new();
    let mut ast_kind_breakdown = std::collections::BTreeMap::new();
    let mut edge_kind_breakdown = std::collections::BTreeMap::new();
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
                .entry(edge_kind_label(edge.edge_kind).to_string())
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

fn edge_kind_label(kind: EdgeKind) -> &'static str {
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

fn collect_reverse_refs(
    index: &crate::IndexFile,
    direction: &str,
    symbol: &str,
    depth: usize,
    limit: usize,
    chunk_map: &HashMap<&str, &crate::Chunk>,
) -> Result<Vec<RefResult>> {
    let edge_kind = match direction {
        "callers" => EdgeKind::Calls,
        "importers" => EdgeKind::Imports,
        "type_users" => EdgeKind::TypeRef,
        _ => anyhow::bail!("Unsupported reverse direction: {direction}"),
    };

    let mut frontier = resolve_reverse_keys(index, symbol);
    let mut visited_symbols = HashSet::new();
    let mut seen_refs = HashSet::new();
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
    chunk_map: &HashMap<&str, &crate::Chunk>,
) -> Result<Vec<RefResult>> {
    let edge_kind = match direction {
        "callees" => EdgeKind::Calls,
        "imports" => EdgeKind::Imports,
        _ => anyhow::bail!("Unsupported forward direction: {direction}"),
    };

    let mut frontier = lookup_symbol_chunk_ids(index, symbol);
    let mut visited_chunks = HashSet::new();
    let mut seen_refs = HashSet::new();
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
    let mut seen = HashSet::new();
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

    let mut keys = HashSet::new();
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
    chunk_map: &HashMap<&str, &crate::Chunk>,
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
    let target_symbol = edge
        .target_chunk_id
        .as_ref()
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
        ast_kind: context_chunk
            .ast_kind
            .map(|kind| ast_kind_label(kind).to_string()),
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

    // Step 1: Determine the root path (canonicalize to match what llmx_index stores)
    let root = if let Some(ref path) = input.path {
        path.canonicalize().unwrap_or_else(|_| path.clone())
    } else {
        cwd.canonicalize().unwrap_or_else(|_| cwd.clone())
    };

    // Step 2: Check for persistent index first (unless force_dynamic)
    if !input.force_dynamic {
        // Try exact match first, then check if any parent index contains this path
        let persistent_match: Option<(String, Option<String>)> =
            if let Some(metadata) = store.find_metadata_by_path(&root) {
                Some((metadata.id.clone(), None))
            } else if let Some((metadata, relative)) = store.find_metadata_containing_path(&root) {
                Some((metadata.id.clone(), Some(relative)))
            } else {
                None
            };

        if let Some((index_id, sub_path)) = persistent_match {
            let index = store.load(&index_id)?;

            // If searching from a subdirectory, scope results via path_prefix filter.
            // Ensure trailing '/' so "src/lib" doesn't match "src/library/...".
            let scoped_filters = if let Some(ref relative) = sub_path {
                let mut filters = input.filters.clone().unwrap_or_default();
                let mut prefix = relative.clone();
                if !prefix.is_empty() && !prefix.ends_with('/') {
                    prefix.push('/');
                }
                filters.path_prefix = Some(match filters.path_prefix {
                    Some(existing) => format!("{}{}", prefix, existing),
                    None => prefix,
                });
                Some(filters)
            } else {
                input.filters.clone()
            };

            let search_start = Instant::now();
            let (results, truncated_ids, total_matches, notices) = perform_search(
                index,
                &input.query,
                &scoped_filters,
                input.limit,
                input.max_tokens,
                input.use_semantic,
                input.hybrid_strategy.as_deref(),
                input.intent.as_deref(),
                input.explain,
                input.strategy.as_deref(),
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
                notices,
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
            let (results, truncated_ids, total_matches, notices) = perform_search(
                index,
                &input.query,
                &input.filters,
                input.limit,
                input.max_tokens,
                input.use_semantic,
                input.hybrid_strategy.as_deref(),
                input.intent.as_deref(),
                input.explain,
                input.strategy.as_deref(),
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
                notices,
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
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        max_total_bytes: usize::MAX,
        max_chunks_per_file: 2000,
    };

    let index = {
        #[cfg_attr(not(feature = "embeddings"), allow(unused_mut))]
        let mut index = ingest_files_with_root(files, options, Some(root.as_path()));
        #[cfg(feature = "embeddings")]
        {
            if input.use_semantic.unwrap_or(false)
                && index.chunks.iter().any(|chunk| !chunk.content.trim().is_empty())
            {
                use crate::embeddings::{generate_embeddings, runtime_model_id};
                let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
                let embeddings = generate_embeddings(&chunk_texts)?;
                index.embeddings = Some(embeddings);
                index.embedding_model = Some(runtime_model_id()?.to_string());
            }
        }
        index
    };

    let index_time_ms = index_start.elapsed().as_millis() as u64;

    // Step 7: Perform search
    let search_start = Instant::now();
    let (results, truncated_ids, total_matches, notices) = perform_search(
        &index,
        &input.query,
        &input.filters,
        input.limit,
        input.max_tokens,
        input.use_semantic,
        input.hybrid_strategy.as_deref(),
        input.intent.as_deref(),
        input.explain,
        input.strategy.as_deref(),
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
        notices,
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
    hybrid_strategy: Option<&str>,
    intent: Option<&str>,
    explain: Option<bool>,
    strategy: Option<&str>,
) -> Result<(
    Vec<SearchResultOutput>,
    Option<Vec<String>>,
    usize,
    Vec<SearchNoticeOutput>,
)> {
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

    let limit = limit.unwrap_or(10).min(MAX_SEARCH_LIMIT);
    let max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let chunk_map: HashMap<&str, &crate::Chunk> = index
        .chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();
    let mut notices = Vec::new();
    let explain = explain.unwrap_or(false);
    let intent = parse_query_intent(intent)?;
    let parsed_strategy = parse_search_strategy(strategy)?;
    let effective_strategy = match parsed_strategy {
        Some(strategy) => Some(strategy),
        None if use_semantic.is_none() => Some(SearchStrategy::Auto),
        None => None,
    };
    let use_advanced = explain || intent != QueryIntent::Auto;

    let search_results = if let Some(strategy) = effective_strategy {
        match plan_search_strategy(strategy, query, intent) {
            SearchStrategyPlan::Bm25 => search(index, query, filters.clone(), limit * 2),
            SearchStrategyPlan::Advanced { use_semantic, intent } => {
                match search_advanced(index, query, filters.clone(), limit * 2, use_semantic, intent, explain) {
                    Ok(results) => results,
                    Err(err) if strategy == SearchStrategy::Auto && use_semantic => {
                        if let Some(reason) = embedding_downgrade_reason(&err) {
                            notices.push(SearchNoticeOutput {
                                code: "semantic_downgrade".to_string(),
                                message: format!(
                                    "Auto search downgraded to BM25 + symbol routing because embeddings are unavailable for this index ({reason}). To enable semantic search, rebuild the index with embeddings so it stores vectors and an embedding_model matching the current runtime."
                                ),
                            });
                            search_advanced(index, query, filters.clone(), limit * 2, false, intent, explain)?
                        } else {
                            return Err(err);
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            SearchStrategyPlan::SemanticOnly => semantic_only_search(index, query, &filters, limit * 2, explain)?,
        }
    } else if use_advanced {
        search_advanced(index, query, filters.clone(), limit * 2, use_semantic.unwrap_or(false), intent, explain)?
    } else if use_semantic.unwrap_or(false) {
        #[cfg(feature = "embeddings")]
        {
            use crate::embeddings::{generate_embedding, validate_index_embeddings};
            use crate::index::hybrid_search_with_strategy;

            let embeddings = validate_index_embeddings(index)?;
            let query_embedding = generate_embedding(query)?;
            let strategy = parse_hybrid_strategy(hybrid_strategy)?;
            hybrid_search_with_strategy(
                &index.chunks,
                &index.inverted_index,
                &index.chunk_refs,
                embeddings,
                query,
                &query_embedding,
                &filters,
                limit * 2,
                strategy,
            )
        }
        #[cfg(not(feature = "embeddings"))]
        {
            let _ = hybrid_strategy;
            anyhow::bail!("Semantic search requested, but embeddings support is not compiled into this build")
        }
    } else {
        search(index, query, filters, limit * 2)
    };

    let mut results = vec![];
    let mut tokens_used = 0;
    let mut truncated = vec![];

    for result in &search_results {
        let chunk = chunk_map
            .get(result.chunk_id.as_str())
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
                match_reason: result.match_reason.clone(),
                matched_engines: result.matched_engines.clone(),
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
        notices,
    ))
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
            "Invalid intent: {}. Use 'auto', 'symbol', 'semantic', or 'keyword'.",
            other
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
        #[cfg(feature = "embeddings")]
        for result in &mut results {
            if result.match_reason.is_none() {
                result.match_reason = Some(crate::query::explain_match(&[("dense", result.score)], None, query));
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
