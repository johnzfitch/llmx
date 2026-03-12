/// Phase 7: Code graph — bidirectional call/import/export relationships.
///
/// Built on-demand from the `calls`, `imports`, `exports`, and `qualified_name`
/// fields populated by the tree-sitter enrichment pass in `chunk.rs`.
///
/// No serialization overhead: recomputed from the index in <1ms for typical
/// codebases (10k chunks), so there is no reason to persist it.
use crate::model::{AstNodeKind, Chunk, Edge, EdgeIndex, EdgeKind, SymbolIndexEntry, SymbolTable};
use std::collections::{BTreeMap, HashMap};

/// Adjacency structure for code relationships.
///
/// All maps use symbol names as keys/values because qualified_name is the
/// canonical identity: "AuthService.login" not the opaque chunk_id.
/// Chunk IDs are still surfaced so callers can look up full chunk content.
#[derive(Debug, Default)]
pub struct CodeGraph {
    /// symbol_name → chunk_ids that contain a `call` to this symbol.
    /// "What code calls `verifyToken`?"
    pub callers: HashMap<String, Vec<String>>,

    /// chunk_id → symbol names called from that chunk.
    /// "What does `AuthService.login` call?"
    pub callees: HashMap<String, Vec<String>>,

    /// symbol_name → chunk_ids that define/export this symbol.
    /// "Where is `Router` defined?"
    pub definitions: HashMap<String, Vec<String>>,

    /// symbol_name → chunk_ids that import this symbol.
    /// "What files use `jwt`?"
    pub importers: HashMap<String, Vec<String>>,
}

pub fn build_symbol_table(chunks: &[Chunk]) -> SymbolTable {
    let mut table: SymbolTable = BTreeMap::new();

    for chunk in chunks {
        let Some(ast_kind) = chunk.ast_kind else { continue };
        let qualified_name = chunk
            .qualified_name
            .clone()
            .or_else(|| chunk.symbol.clone())
            .unwrap_or_else(|| chunk.short_id.clone());
        let name = chunk
            .symbol
            .clone()
            .unwrap_or_else(|| qualified_name.rsplit("::").next().unwrap_or(&qualified_name).to_string());

        let entry = SymbolIndexEntry {
            name: name.clone(),
            qualified_name: qualified_name.clone(),
            ast_kind,
            chunk_id: chunk.id.clone(),
            path: chunk.path.clone(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            signature: chunk.signature.clone(),
            doc_summary: chunk.doc_summary.clone(),
            parent_symbol: chunk.parent_symbol.clone(),
        };

        add_symbol_entry(&mut table, &name, entry.clone());
        if qualified_name != name {
            add_symbol_entry(&mut table, &qualified_name, entry.clone());
        }
        if let Some(tail) = qualified_name.rsplit("::").next() {
            if tail != name && tail != qualified_name {
                add_symbol_entry(&mut table, tail, entry);
            }
        }
    }

    table
}

pub fn build_edge_index(chunks: &[Chunk], symbols: &SymbolTable) -> EdgeIndex {
    let mut edges = EdgeIndex::default();

    for chunk in chunks {
        push_edges(
            &mut edges,
            chunk,
            &chunk.imports,
            EdgeKind::Imports,
            symbols,
        );
        push_edges(
            &mut edges,
            chunk,
            &chunk.calls,
            EdgeKind::Calls,
            symbols,
        );
        push_edges(
            &mut edges,
            chunk,
            &chunk.type_refs,
            EdgeKind::TypeRef,
            symbols,
        );
    }

    for edge_list in edges.forward.values_mut() {
        edge_list.sort_by(|a, b| {
            a.target_symbol
                .cmp(&b.target_symbol)
                .then_with(|| a.source_chunk_id.cmp(&b.source_chunk_id))
                .then_with(|| a.edge_kind.cmp(&b.edge_kind))
        });
        edge_list.dedup();
    }

    for edge_list in edges.reverse.values_mut() {
        edge_list.sort_by(|a, b| {
            a.source_chunk_id
                .cmp(&b.source_chunk_id)
                .then_with(|| a.target_symbol.cmp(&b.target_symbol))
                .then_with(|| a.edge_kind.cmp(&b.edge_kind))
        });
        edge_list.dedup();
    }

    edges
}

pub fn build_structural_indexes(chunks: &[Chunk]) -> (SymbolTable, EdgeIndex) {
    let symbols = build_symbol_table(chunks);
    let edges = build_edge_index(chunks, &symbols);
    (symbols, edges)
}

impl CodeGraph {
    /// Build a `CodeGraph` from all chunks in an index.
    ///
    /// O(n * avg_calls) where n = number of chunks. Typically <1ms for 10k chunks.
    pub fn build(chunks: &[Chunk]) -> Self {
        let mut graph = CodeGraph::default();

        for chunk in chunks {
            let chunk_id = &chunk.id;

            // Populate definitions: both symbol and qualified_name point here
            if let Some(sym) = &chunk.symbol {
                graph.definitions.entry(sym.clone()).or_default().push(chunk_id.clone());
            }
            if let Some(qname) = &chunk.qualified_name {
                // Only add if different from symbol (avoids duplicates)
                if chunk.symbol.as_deref() != Some(qname.as_str()) {
                    graph.definitions.entry(qname.clone()).or_default().push(chunk_id.clone());
                }
            }

            // Populate exports (same as definitions for exported symbols)
            for export in &chunk.exports {
                graph.definitions
                    .entry(export.clone())
                    .or_default()
                    .push(chunk_id.clone());
            }

            // Populate callees (what this chunk calls) and callers (reverse index)
            if !chunk.calls.is_empty() {
                let calls_clone: Vec<String> = chunk.calls.clone();
                graph.callees.insert(chunk_id.clone(), calls_clone.clone());

                for callee_name in &calls_clone {
                    graph.callers
                        .entry(callee_name.clone())
                        .or_default()
                        .push(chunk_id.clone());
                }
            }

            // Populate importers (what symbols this chunk imports)
            for import_name in &chunk.imports {
                graph.importers
                    .entry(import_name.clone())
                    .or_default()
                    .push(chunk_id.clone());
            }
        }

        graph
    }

    /// Return all chunk IDs that call `symbol_name` (direct callers).
    pub fn get_callers(&self, symbol_name: &str) -> &[String] {
        self.callers
            .get(symbol_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Return all symbol names called by the chunk with `chunk_id`.
    pub fn get_callees(&self, chunk_id: &str) -> &[String] {
        self.callees
            .get(chunk_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Return all chunk IDs that define or export `symbol_name`.
    pub fn get_definitions(&self, symbol_name: &str) -> &[String] {
        self.definitions
            .get(symbol_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Return all chunk IDs that import `symbol_name`.
    pub fn get_importers(&self, symbol_name: &str) -> &[String] {
        self.importers
            .get(symbol_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Walk N hops from a starting symbol, accumulating reachable chunk IDs.
    ///
    /// `direction` controls traversal:
    /// - `"callers"`: follow caller edges (who calls this?)
    /// - `"callees"`: follow callee edges (what does this call?)
    ///
    /// Returns deduplicated chunk IDs in BFS order.
    pub fn walk(&self, start_symbol: &str, direction: &str, max_hops: usize) -> Vec<String> {
        let mut visited_chunks: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut visited_symbols: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut frontier_chunks: Vec<String> = Vec::new();
        let mut result: Vec<String> = Vec::new();

        // Seed: chunks that define the start symbol
        for chunk_id in self.get_definitions(start_symbol) {
            if visited_chunks.insert(chunk_id.clone()) {
                frontier_chunks.push(chunk_id.clone());
                result.push(chunk_id.clone());
            }
        }

        visited_symbols.insert(start_symbol.to_string());

        for _hop in 0..max_hops {
            let mut next_chunks = Vec::new();
            for chunk_id in &frontier_chunks {
                match direction {
                    "callers" => {
                        // Find what calls any symbol defined in this chunk
                        // We need the symbol names → then look up callers
                        // For simplicity, check callers indexed by symbol names this chunk has
                        // This is done by looking at what symbols we know this chunk defines
                        for (sym, defs) in &self.definitions {
                            if defs.contains(chunk_id) && !visited_symbols.contains(sym) {
                                visited_symbols.insert(sym.clone());
                                for caller_id in self.get_callers(sym) {
                                    if visited_chunks.insert(caller_id.clone()) {
                                        next_chunks.push(caller_id.clone());
                                        result.push(caller_id.clone());
                                    }
                                }
                            }
                        }
                    }
                    "callees" => {
                        // Follow callees from this chunk
                        for callee_name in self.get_callees(chunk_id) {
                            if !visited_symbols.contains(callee_name) {
                                visited_symbols.insert(callee_name.clone());
                                for def_id in self.get_definitions(callee_name) {
                                    if visited_chunks.insert(def_id.clone()) {
                                        next_chunks.push(def_id.clone());
                                        result.push(def_id.clone());
                                    }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
            if next_chunks.is_empty() {
                break;
            }
            frontier_chunks = next_chunks;
        }

        result
    }

    /// Summarize the graph as stats for debugging.
    pub fn stats(&self) -> GraphStats {
        let total_caller_edges: usize = self.callers.values().map(|v| v.len()).sum();
        let total_importer_edges: usize = self.importers.values().map(|v| v.len()).sum();
        GraphStats {
            unique_symbols: self.definitions.len(),
            unique_call_targets: self.callers.len(),
            total_caller_edges,
            total_importer_edges,
        }
    }
}

fn add_symbol_entry(table: &mut SymbolTable, key: &str, entry: SymbolIndexEntry) {
    let normalized = normalize_symbol_key(key);
    if normalized.is_empty() {
        return;
    }

    let entries = table.entry(normalized).or_default();
    if !entries.iter().any(|existing| {
        existing.chunk_id == entry.chunk_id && existing.qualified_name == entry.qualified_name
    }) {
        entries.push(entry);
    }
}

fn push_edges(
    edges: &mut EdgeIndex,
    chunk: &Chunk,
    targets: &[String],
    edge_kind: EdgeKind,
    symbols: &SymbolTable,
) {
    for target_symbol in targets {
        if target_symbol.trim().is_empty() {
            continue;
        }

        let resolved_target = resolve_target_symbol(chunk, target_symbol, symbols);
        let reverse_key = resolved_target
            .map(|entry| canonical_symbol_key(&entry.qualified_name))
            .unwrap_or_else(|| raw_symbol_key(target_symbol));

        let edge = Edge {
            source_chunk_id: chunk.id.clone(),
            target_symbol: target_symbol.clone(),
            target_chunk_id: resolved_target.map(|entry| entry.chunk_id.clone()),
            edge_kind,
        };

        edges
            .forward
            .entry(chunk.id.clone())
            .or_default()
            .push(edge.clone());
        edges
            .reverse
            .entry(reverse_key)
            .or_default()
            .push(edge);
    }
}

fn resolve_target_symbol<'a>(
    chunk: &Chunk,
    target_symbol: &str,
    symbols: &'a SymbolTable,
) -> Option<&'a SymbolIndexEntry> {
    let candidates = symbols.get(&normalize_symbol_key(target_symbol))?;
    unique_symbol_entry(candidates.iter().filter(|candidate| candidate.path == chunk.path))
        .or_else(|| unique_symbol_entry(candidates.iter()))
}

fn unique_symbol_entry<'a>(
    mut candidates: impl Iterator<Item = &'a SymbolIndexEntry>,
) -> Option<&'a SymbolIndexEntry> {
    let first = candidates.next()?;
    if candidates.any(|candidate| {
        candidate.chunk_id != first.chunk_id || candidate.qualified_name != first.qualified_name
    }) {
        None
    } else {
        Some(first)
    }
}

pub fn normalize_symbol_key(symbol: &str) -> String {
    symbol.trim().to_ascii_lowercase()
}

pub fn canonical_symbol_key(symbol: &str) -> String {
    normalize_symbol_key(symbol)
}

pub fn raw_symbol_key(symbol: &str) -> String {
    format!("raw::{}", normalize_symbol_key(symbol))
}

pub fn ast_kind_label(ast_kind: AstNodeKind) -> &'static str {
    match ast_kind {
        AstNodeKind::Function => "function",
        AstNodeKind::Method => "method",
        AstNodeKind::Class => "class",
        AstNodeKind::Module => "module",
        AstNodeKind::Interface => "interface",
        AstNodeKind::Type => "type",
        AstNodeKind::Enum => "enum",
        AstNodeKind::Constant => "constant",
        AstNodeKind::Variable => "variable",
        AstNodeKind::Import => "import",
        AstNodeKind::Export => "export",
        AstNodeKind::Test => "test",
        AstNodeKind::Other => "other",
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphStats {
    pub unique_symbols: usize,
    pub unique_call_targets: usize,
    pub total_caller_edges: usize,
    pub total_importer_edges: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AstNodeKind, Chunk, ChunkKind};

    fn make_chunk(id: &str, symbol: &str, calls: Vec<&str>, imports: Vec<&str>, exports: Vec<&str>) -> Chunk {
        Chunk {
            id: id.to_string(),
            short_id: id.to_string(),
            slug: id.to_string(),
            path: "test.ts".to_string(),
            kind: ChunkKind::JavaScript,
            chunk_index: 0,
            start_line: 1, end_line: 10,
            content: String::new(),
            content_hash: String::new(),
            token_estimate: 10,
            heading_path: Vec::new(),
            symbol: Some(symbol.to_string()),
            address: None,
            asset_path: None,
            ast_kind: Some(AstNodeKind::Function),
            qualified_name: Some(symbol.to_string()),
            signature: None,
            parent_symbol: None,
            imports: imports.into_iter().map(|s| s.to_string()).collect(),
            exports: exports.into_iter().map(|s| s.to_string()).collect(),
            calls: calls.into_iter().map(|s| s.to_string()).collect(),
            type_refs: Vec::new(),
            doc_summary: None,
        }
    }

    #[test]
    fn test_build_graph_callers() {
        let chunks = vec![
            make_chunk("c1", "verifyToken", vec![], vec![], vec!["verifyToken"]),
            make_chunk("c2", "login", vec!["verifyToken", "hashPassword"], vec![], vec![]),
            make_chunk("c3", "register", vec!["hashPassword"], vec![], vec![]),
        ];
        let graph = CodeGraph::build(&chunks);

        // verifyToken is called by login (c2)
        let callers = graph.get_callers("verifyToken");
        assert_eq!(callers, &["c2"], "verifyToken should have 1 caller");

        // hashPassword is called by login and register
        let callers = graph.get_callers("hashPassword");
        assert_eq!(callers.len(), 2, "hashPassword should have 2 callers");
        assert!(callers.contains(&"c2".to_string()));
        assert!(callers.contains(&"c3".to_string()));
    }

    #[test]
    fn test_build_graph_callees() {
        let chunks = vec![
            make_chunk("c1", "login", vec!["verifyToken", "createSession"], vec![], vec![]),
        ];
        let graph = CodeGraph::build(&chunks);

        let callees = graph.get_callees("c1");
        assert_eq!(callees.len(), 2);
        assert!(callees.contains(&"verifyToken".to_string()));
        assert!(callees.contains(&"createSession".to_string()));
    }

    #[test]
    fn test_build_graph_definitions() {
        let chunks = vec![
            make_chunk("c1", "Router", vec![], vec![], vec!["Router"]),
        ];
        let graph = CodeGraph::build(&chunks);

        // Both symbol and export entry should point to c1
        let defs = graph.get_definitions("Router");
        assert!(defs.contains(&"c1".to_string()), "Router should have definition in c1");
    }

    #[test]
    fn test_build_graph_importers() {
        let chunks = vec![
            make_chunk("c1", "login", vec![], vec!["jwt", "bcrypt"], vec![]),
            make_chunk("c2", "refresh", vec![], vec!["jwt"], vec![]),
        ];
        let graph = CodeGraph::build(&chunks);

        let importers = graph.get_importers("jwt");
        assert_eq!(importers.len(), 2);

        let importers = graph.get_importers("bcrypt");
        assert_eq!(importers.len(), 1);
        assert_eq!(importers[0], "c1");
    }

    #[test]
    fn test_walk_callees_two_hops() {
        let chunks = vec![
            make_chunk("c1", "main", vec!["login"], vec![], vec![]),
            make_chunk("c2", "login", vec!["verifyToken"], vec![], vec!["login"]),
            make_chunk("c3", "verifyToken", vec![], vec![], vec!["verifyToken"]),
        ];
        let graph = CodeGraph::build(&chunks);

        // Walk callees from "main": main→login→verifyToken
        let reachable = graph.walk("main", "callees", 3);
        // Should include c2 (login) and c3 (verifyToken)
        assert!(reachable.contains(&"c2".to_string()), "Should reach login: {:?}", reachable);
        assert!(reachable.contains(&"c3".to_string()), "Should reach verifyToken: {:?}", reachable);
    }

    #[test]
    fn test_graph_stats() {
        let chunks = vec![
            make_chunk("c1", "a", vec!["b", "c"], vec![], vec![]),
            make_chunk("c2", "b", vec!["c"], vec![], vec!["b"]),
            make_chunk("c3", "c", vec![], vec![], vec!["c"]),
        ];
        let graph = CodeGraph::build(&chunks);
        let stats = graph.stats();
        assert!(stats.unique_symbols > 0);
        assert!(stats.total_caller_edges > 0);
    }

    #[test]
    fn test_build_edge_index_resolves_unique_cross_file_symbol() {
        let mut verify = make_chunk("c1", "verifyToken", vec![], vec![], vec!["verifyToken"]);
        verify.path = "auth.ts".to_string();

        let mut parse = make_chunk("c2", "parseConfig", vec!["verifyToken"], vec!["verifyToken"], vec!["parseConfig"]);
        parse.path = "config.ts".to_string();

        let chunks = vec![verify, parse];
        let symbols = build_symbol_table(&chunks);
        let edges = build_edge_index(&chunks, &symbols);

        let parse_edges = edges
            .forward
            .get("c2")
            .expect("parseConfig should have outgoing edges");
        let call_edge = parse_edges
            .iter()
            .find(|edge| edge.edge_kind == EdgeKind::Calls && edge.target_symbol == "verifyToken")
            .expect("verifyToken call edge should exist");

        assert_eq!(call_edge.target_chunk_id.as_deref(), Some("c1"));
    }
}
