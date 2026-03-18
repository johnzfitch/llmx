# llmx v3: Complete Implementation Specification

This is the single source of truth for building llmx v3. It contains every
architectural decision, every MCP tool schema, every work item, and every acceptance
criterion. An implementing agent should read this document top-to-bottom and have
everything needed to build the system.

Updated: 2026-03-14
Consolidates: STRUCTURAL_INDEXING_EPIC_PLAN_v2.md + LLMX_MCP_TOOL_REDESIGN.md

---

## 1. What llmx Is

llmx is an MCP (Model Context Protocol) server that provides structural code
intelligence to AI agents. It runs as a long-lived stdio process alongside Claude
Code, VS Code, or any MCP host. Agents use its tools to search code, resolve
symbols, traverse call graphs, and understand codebases.

**Current state**: llmx is a JS-biased text retriever. Rust files fall through to
`ChunkKind::Unknown -> chunk_text()`. Paths are absolute (break across machines).
BM25 returns logs and generated artifacts above source code. Symbol lookup is
unreliable.

**Target state**: llmx is a multi-language structural code intelligence engine that
is useful within milliseconds of startup, converges to full-quality with background
indexing, and exposes graph traversal as a first-class retrieval path for agentic
multi-hop reasoning. Every tool response includes a readiness tier so agents can
adapt their retrieval strategy.

---

## 2. Core Architecture

### 2.1 Tiered Readiness Model

llmx is useful before indexing completes. The server operates at four tiers,
progressing from instant availability to full intelligence:

```
Tier 0 — File Manifest (instant, <100ms)
  ignore::WalkParallel builds .gitignore-aware file tree.
  Available: llmx_status, llmx_explore (tree mode), llmx_search (filename only)

Tier 1 — On-Demand Parse (<500ms per file)
  Tree-sitter parses files on first access. Results cached in LRU.
  Available: llmx_lookup (in parsed files), llmx_refs (intra-file),
             llmx_explore (symbols mode)

Tier 2 — Full Structural Index (seconds to minutes, background)
  SCIP-aligned symbol IDs in fst index. Stack-graph cross-file resolution.
  Full graph with typed edges.
  Available: llmx_lookup (cross-file), llmx_graph_walk (multi-hop),
             llmx_search (structural ranking, symbol boosts)

Tier 3 — Vector Embeddings (minutes, background)
  Code-specific embedding model over full corpus.
  Available: llmx_search (semantic vector retrieval, hybrid fusion)
```

**Critical rule**: no tool ever blocks on a higher tier. Every tool returns the best
result available at the current tier, with `readiness_tier` in the structured output
so the agent knows what quality level it's getting.

When a tier completes, the server emits `notifications/tools/list_changed` so clients
know tool capabilities have expanded.

### 2.2 Three-Tier Parser Resolution

Parsing quality varies by language. The system uses three parser tiers with
automatic fallback:

**Parser Tier 1: Stack Graphs** (cross-file name resolution)
- Languages: Rust, Python, TypeScript/JavaScript, Java
- Uses `tree-sitter-stack-graphs` with `.tsg` stanza files
- Produces precise cross-file edges, no heuristics
- Incremental: re-resolves only changed files and their dependents

**Parser Tier 2: Custom Query Packs** (per-language `.scm` queries)
- Languages: Go, C, C++, C#
- Hand-tuned tree-sitter queries for definitions, imports, calls, types
- Intra-file structural extraction; cross-file edges are heuristic (name-matching)

**Parser Tier 3: Generic Tree-Sitter Adapter** (universal fallback)
- Languages: all 200+ with tree-sitter grammars
- Extracts definitions from built-in node types: `function_definition`,
  `class_declaration`, `method_definition`, `interface_declaration`,
  `type_alias_declaration`, `enum_declaration`, `struct_definition`,
  `module_definition`
- No import/call/type edges — definitions and doc comments only
- Still vastly better than `ChunkKind::Unknown -> chunk_text()`

Dispatch: try Parser Tier 1 → Parser Tier 2 → Parser Tier 3 → text chunking.

**Do not confuse parser tiers with readiness tiers.** Parser tiers are about
language analysis quality. Readiness tiers are about how much of the codebase has
been analyzed. A file can be Parser Tier 1 (stack-graphs for Rust) at Readiness
Tier 1 (on-demand parsed, not yet in the full index).

### 2.3 SCIP-Aligned Symbol IDs

All symbol identifiers follow the SCIP string format:

```
rust-analyzer cargo llmx-core 0.1.0 src/exec.rs/codex_exec().
scip-typescript npm @llmx/core 1.0.0 src/index.ts/defaultStorage().
scip-python python llmx 0.1.0 core/exec.py/CodexRunner#run().
```

For languages without a SCIP indexer, generate SCIP-format strings from tree-sitter
output: `generic <lang> <file_path>/<symbol_name>.`

Benefits: interop with Sourcegraph, rust-analyzer SCIP output, and scip-typescript.
8x smaller than LSIF, 3x faster to process.

### 2.4 Filesystem Watching

The server is long-running. It watches all project roots for changes:

- `notify` crate (cross-platform: inotify on Linux, FSEvents on macOS,
  ReadDirectoryChangesW on Windows)
- On file change: invalidate Tier 1 cache entry, mark Tier 2 symbols stale,
  queue background re-parse and re-embed
- Emit `notifications/resources/updated` for changed file URIs
- If symbol table changes, emit `notifications/tools/list_changed`

Cost: ~100 bytes per watch descriptor. Scales to 100k+ directories.

### 2.5 Data Model

New/changed fields on `Chunk` and `FileMeta`:

```rust
pub enum LanguageId {
    Rust, Python, TypeScript, JavaScript, Go, Java,
    C, Cpp, CSharp, // ... extensible
    Other(String),
}

pub enum ResolutionTier {
    StackGraph,       // Parser Tier 1
    QueryPack,        // Parser Tier 2
    GenericTreeSitter, // Parser Tier 3
    TextOnly,
}

pub enum Visibility { Pub, Crate, Private }

// New fields on Chunk/FileMeta:
language: Option<LanguageId>,
root_path: String,
relative_path: String,
is_generated: bool,
quality_score: Option<u16>,
symbol_id: Option<String>,    // SCIP-format
symbol_tail: Option<String>,  // last component for fuzzy matching
module_path: Option<String>,
visibility: Option<Visibility>,
resolution_tier: ResolutionTier,
```

Schema version bumps from `2` to `3`. Old v2 indexes are rejected with a clear
error message on load.

---

## 3. MCP Server Specification

### 3.1 Server Capabilities

Declared during `initialize` response:

```json
{
  "protocolVersion": "2025-11-25",
  "capabilities": {
    "tools": { "listChanged": true },
    "resources": { "subscribe": true, "listChanged": true },
    "tasks": {
      "list": {},
      "cancel": {},
      "requests": { "tools": { "call": {} } }
    },
    "logging": {}
  },
  "serverInfo": {
    "name": "llmx",
    "title": "llmx Code Intelligence",
    "version": "0.3.0",
    "description": "Structural code search, symbol resolution, and graph traversal."
  }
}
```

### 3.2 Roots Integration

Use `roots/list` (server → client request, only during tools/call processing) to
discover project directories. Do NOT require explicit `--root` arguments in tool
input schemas.

When the client emits `notifications/roots/list_changed`, the server:
1. Re-walks the file tree (Tier 0 rebuild, instant)
2. Invalidates stale cache entries
3. Queues background re-index for new/changed files

### 3.3 Resources Exposed

Source files:
```json
{ "uri": "file:///src/exec.rs", "name": "src/exec.rs", "mimeType": "text/x-rust" }
```

Index status:
```json
{ "uri": "llmx://index/status", "name": "Index Status", "mimeType": "application/json" }
```

Clients can `resources/subscribe` to get live updates.

### 3.4 Notifications Emitted

- `notifications/tools/list_changed` — when readiness tier advances
- `notifications/resources/updated` — when watched files change
- `notifications/resources/list_changed` — when file tree changes
- `notifications/progress` — during indexing (with `progressToken`)
- `notifications/message` — structured logging during indexing/queries

---

## 4. MCP Tool Definitions

Every tool follows these conventions:
- `outputSchema` present for structured programmatic consumption
- `annotations` present for agent auto-approval decisions
- `execution.taskSupport` specified for slow/fast tool classification
- `readiness_tier` in every `structuredContent` response
- `resource_link` content items for lazy content loading when `include_content: false`

### 4.1 llmx_status

```json
{
  "name": "llmx_status",
  "title": "Index Status",
  "description": "Report current index readiness tier, indexed file count, available capabilities, and background task progress. Agents should call this at session start to understand query quality.",
  "inputSchema": {
    "type": "object",
    "additionalProperties": false
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "readiness_tier": { "type": "integer", "minimum": 0, "maximum": 3 },
      "files_indexed": { "type": "integer" },
      "files_total": { "type": "integer" },
      "symbols_indexed": { "type": "integer" },
      "embeddings_ready": { "type": "boolean" },
      "languages": { "type": "array" },
      "stale_files": { "type": "integer" },
      "background_tasks": { "type": "array" }
    },
    "required": ["readiness_tier", "files_indexed", "files_total", "symbols_indexed", "embeddings_ready"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "forbidden" }
}
```

### 4.2 llmx_search

```json
{
  "name": "llmx_search",
  "title": "Code Search",
  "description": "Search code using BM25 keywords, semantic vectors, or structural symbol matching. Quality depends on readiness tier. At Tier 0: filename/path only. Tier 2+: structural ranking with symbol boosts. Tier 3: semantic vector retrieval.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "Search query" },
      "intent": {
        "type": "string",
        "enum": ["auto", "symbol", "semantic", "keyword", "filename"],
        "default": "auto"
      },
      "path_prefix": { "type": "string" },
      "language": { "type": "string" },
      "max_results": { "type": "integer", "default": 10, "minimum": 1, "maximum": 50 },
      "include_content": { "type": "boolean", "default": true }
    },
    "required": ["query"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "results": { "type": "array" },
      "strategy_used": {
        "type": "string",
        "enum": ["bm25", "symbol_exact", "symbol_fuzzy", "vector", "hybrid", "filename"]
      },
      "readiness_tier": { "type": "integer" },
      "total_matches": { "type": "integer" },
      "truncated": { "type": "boolean" }
    },
    "required": ["results", "strategy_used", "readiness_tier", "total_matches", "truncated"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "optional" }
}
```

Each result in `structuredContent.results` contains:
```json
{
  "symbol_id": "rust-analyzer cargo llmx-core 0.1.0 src/exec.rs/codex_exec().",
  "symbol_tail": "codex_exec",
  "file_path": "src/exec.rs",
  "line": 42,
  "kind": "function",
  "visibility": "pub",
  "score": 0.95,
  "resolution_tier": "StackGraph",
  "content": "pub fn codex_exec(args: &ExecArgs) -> Result<()> { ... }"
}
```

When `include_content: false`, results omit `content` and unstructured `content`
blocks use `resource_link` items instead of embedded resources:
```json
{
  "type": "resource_link",
  "uri": "file:///project/src/exec.rs",
  "name": "src/exec.rs:42 — codex_exec",
  "mimeType": "text/x-rust"
}
```

### 4.3 llmx_lookup

```json
{
  "name": "llmx_lookup",
  "title": "Symbol Lookup",
  "description": "Resolve a symbol name to its definition(s). At Tier 1+, triggers on-demand tree-sitter parse for uncached files. At Tier 2+, uses fst-backed exact index with cross-file resolution.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "symbol": { "type": "string", "description": "Symbol name, tail name, or SCIP ID" },
      "mode": {
        "type": "string",
        "enum": ["auto", "exact", "tail", "prefix", "contains", "qualified"],
        "default": "auto"
      },
      "language": { "type": "string" },
      "max_results": { "type": "integer", "default": 5, "minimum": 1, "maximum": 20 }
    },
    "required": ["symbol"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "definitions": { "type": "array" },
      "mode_used": { "type": "string", "enum": ["exact", "tail", "prefix", "contains", "qualified"] },
      "readiness_tier": { "type": "integer" }
    },
    "required": ["definitions", "mode_used", "readiness_tier"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "forbidden" }
}
```

On-demand parse behavior at Tier 1: when the full index isn't ready, the server
searches the file manifest for likely candidates (files named `exec.rs`, etc.),
tree-sitter parses those files, caches the results, and returns the match.

### 4.4 llmx_graph_walk

```json
{
  "name": "llmx_graph_walk",
  "title": "Code Graph Traversal",
  "description": "Traverse the structural code graph from a symbol to find callers, callees, imports, or dependents with inline content. Supports multi-hop. Requires Tier 2 for cross-file edges; Tier 1 returns intra-file only.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "symbol": { "type": "string", "description": "Symbol name, tail name, or SCIP ID" },
      "direction": {
        "type": "string",
        "enum": ["callers", "callees", "imports", "dependents", "type_refs"]
      },
      "depth": { "type": "integer", "minimum": 1, "maximum": 3, "default": 1 },
      "include_content": { "type": "boolean", "default": true },
      "filter": {
        "type": "object",
        "properties": {
          "file_pattern": { "type": "string" },
          "language": { "type": "string" },
          "visibility": { "type": "string", "enum": ["pub", "crate", "private", "any"], "default": "any" }
        }
      },
      "max_results": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 }
    },
    "required": ["symbol", "direction"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "matches": { "type": "array" },
      "total_count": { "type": "integer" },
      "truncated": { "type": "boolean" },
      "traversal_depth_reached": { "type": "integer" },
      "readiness_tier": { "type": "integer" },
      "edge_confidence": {
        "type": "string",
        "enum": ["precise", "heuristic", "intra_file_only"]
      }
    },
    "required": ["matches", "total_count", "truncated", "traversal_depth_reached", "readiness_tier", "edge_confidence"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "optional" }
}
```

Each match in `structuredContent.matches`:
```json
{
  "symbol_id": "rust-analyzer cargo llmx-core 0.1.0 src/cmd.rs/run_command().",
  "symbol_tail": "run_command",
  "file_path": "src/cmd.rs",
  "line": 128,
  "kind": "function",
  "depth": 1,
  "edge_type": "Calls",
  "confidence": "precise",
  "content": "pub fn run_command(...) { ... codex_exec(...) ... }"
}
```

### 4.5 llmx_explore

```json
{
  "name": "llmx_explore",
  "title": "Codebase Explorer",
  "description": "Browse file tree, list symbols in a file, or get structural overview. Available at all tiers. Tier 0: file tree. Tier 1+: symbol listings.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "default": "", "description": "Root-relative path" },
      "mode": {
        "type": "string",
        "enum": ["tree", "symbols", "overview"],
        "default": "tree"
      },
      "depth": { "type": "integer", "minimum": 1, "maximum": 5, "default": 2 },
      "include_hidden": { "type": "boolean", "default": false }
    },
    "required": []
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "entries": { "type": "array" },
      "path": { "type": "string" },
      "mode": { "type": "string" },
      "readiness_tier": { "type": "integer" }
    },
    "required": ["entries", "path", "mode", "readiness_tier"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "forbidden" }
}
```

### 4.6 llmx_refs

```json
{
  "name": "llmx_refs",
  "title": "Find References",
  "description": "Find all references to a symbol. Tier 1: references in parsed files. Tier 2+: cross-file references with edge types.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "symbol": { "type": "string" },
      "include_definition": { "type": "boolean", "default": false },
      "path_prefix": { "type": "string" },
      "max_results": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 }
    },
    "required": ["symbol"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "references": { "type": "array" },
      "total_count": { "type": "integer" },
      "truncated": { "type": "boolean" },
      "readiness_tier": { "type": "integer" }
    },
    "required": ["references", "total_count", "truncated", "readiness_tier"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "optional" }
}
```

### 4.7 llmx_index

```json
{
  "name": "llmx_index",
  "title": "Index Management",
  "description": "Trigger re-indexing, invalidate cache, or refresh stale files. Runs as background task. Server stays responsive during indexing.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "action": {
        "type": "string",
        "enum": ["reindex", "invalidate", "refresh"],
        "default": "refresh"
      },
      "paths": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Root-relative paths to invalidate"
      }
    },
    "required": []
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "action": { "type": "string" },
      "files_affected": { "type": "integer" },
      "task_id": { "type": "string" },
      "estimated_duration_ms": { "type": "integer" }
    },
    "required": ["action", "files_affected"]
  },
  "annotations": {
    "readOnlyHint": false,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  },
  "execution": { "taskSupport": "optional" }
}
```

When called with `progressToken` in `_meta`, emits:
```json
{
  "method": "notifications/progress",
  "params": {
    "progressToken": "idx-001",
    "progress": 4200,
    "total": 25000,
    "message": "Parsing src/chunk/parsers/rust.rs (Tier 2)"
  }
}
```

---

## 5. Module Layout

Target layout after refactoring:

```
ingestor-core/
  src/
    model.rs              # LanguageId, ResolutionTier, Chunk, FileMeta, schema v3
    pathnorm.rs           # Root detection, relative path computation
    walk.rs               # ignore-crate walker, artifact exclusion, fs watching
    chunk/
      mod.rs              # Re-exports
      language.rs          # LanguageAdapter trait, ParseOptions, ParseResult
      registry.rs          # Three-tier dispatch
      query_loader.rs      # Compile-time .scm embedding
      symbol_id.rs         # SCIP-format ID generation
      generic.rs           # Generic tree-sitter adapter (Parser Tier 3)
      stack_graphs.rs      # Stack-graphs integration (Parser Tier 1)
      incr_cache.rs        # Incremental parse cache (LRU)
      parsers/
        rust.rs            # Rust adapter
        javascript.rs      # JS adapter
        typescript.rs      # TS adapter
        python.rs          # Python adapter (if stack-graphs insufficient)
        go.rs              # Go adapter (Parser Tier 2)
        c.rs               # C adapter
        cpp.rs             # C++ adapter
        csharp.rs          # C# adapter
    graph.rs              # Symbol table, edge index, graph walk
    index.rs              # fst-backed symbol index, ranking priors
    retrieval.rs          # Hybrid fusion (structural + graph + semantic)
    symbol_search.rs      # Symbol search on canonical SCIP IDs
    query.rs              # Query parsing, intent detection
    mcp/
      server.rs           # MCP server lifecycle, capability declaration
      tools.rs            # Tool registration, dispatch
      resources.rs        # Resource exposure, subscriptions
      notifications.rs    # Progress, list_changed, resource updates
    handlers/
      mod.rs              # Request routing
      types.rs            # Lookup modes, search intents
      storage.rs          # Schema validation, migration
      safety.rs           # Input validation, rate limiting
  queries/
    rust/                 # defs.scm, imports.scm, calls.scm, types.scm, docs.scm, tests.scm
    go/
    c/
    cpp/
    csharp/
    typescript/
  tests/
    path_normalization_tests.rs
    walk_filter_tests.rs
    schema_migration_tests.rs
    noise_ranking_tests.rs
    parser_framework_tests.rs
    generic_adapter_tests.rs
    stack_graph_tests.rs
    lookup_modes_tests.rs
    identifier_index_tests.rs
    graph_walk_tests.rs
    graph_expansion_tests.rs
    ranking_priors_tests.rs
    hybrid_retrieval_tests.rs
    multilang_parser_tests.rs
    fixtures/
      projects/           # Multi-file test repos
      filetypes/          # Single-file per-language fixtures

ingestor-wasm/
  Cargo.toml
  src/lib.rs
  tests/structural_parity_tests.rs
```

---

## 6. Dependency Plan

### Core Runtime

| Crate | Purpose |
|-------|---------|
| `ignore` | .gitignore-aware parallel file walking (BurntSushi) |
| `fst` | Memory-mapped ordered set/map for symbol index (BurntSushi) |
| `notify` | Cross-platform filesystem watching |
| `compact_str` | Inline small strings |
| `rustc-hash` | Fast non-crypto hashing |
| `smallvec` | Stack-allocated small vecs |
| `rayon` | Parallel parsing and indexing |
| `memchr` | Fast byte scanning |

### Tree-Sitter Stack

| Crate | Purpose |
|-------|---------|
| `tree-sitter` | Core parsing (pin to latest stable) |
| `tree-sitter-stack-graphs` | Cross-file name resolution |
| `tree-sitter-stack-graphs-python` | Python .tsg stanzas |
| `tree-sitter-stack-graphs-javascript` | JS .tsg stanzas |
| `tree-sitter-stack-graphs-typescript` | TS .tsg stanzas |
| `tree-sitter-rust` | Rust grammar |
| `tree-sitter-python` | Python grammar |
| `tree-sitter-javascript` | JS grammar |
| `tree-sitter-typescript` | TS/TSX grammar |
| `tree-sitter-go` | Go grammar |
| `tree-sitter-java` | Java grammar |
| `tree-sitter-c` | C grammar |
| `tree-sitter-cpp` | C++ grammar |
| `tree-sitter-c-sharp` | C# grammar |

### MCP Runtime

| Crate | Purpose |
|-------|---------|
| `mcp-server` or hand-rolled | JSON-RPC 2.0 over stdio |
| `serde` + `serde_json` | Serialization |
| `tokio` | Async runtime for fs watching + background indexing |

### Removed

| Crate | Reason |
|-------|--------|
| `globset` | Subsumed by `ignore` |
| `ra_ap_syntax` | Stack-graphs replaces |
| `rustpython-parser` | Stack-graphs replaces |

---

## 7. Implementation Phases

### Phase 1: Foundation

**Objective**: Fix indexing correctness. Instant usefulness via Tier 0.

| ID | Work Item | Files | Acceptance |
|----|-----------|-------|------------|
| E1-01 | Schema v3: `LanguageId`, `ResolutionTier`, SCIP fields | `model.rs` | Schema represents root-relative files, SCIP IDs, resolution tiers |
| E1-02 | Root-relative path normalization | `pathnorm.rs`, `lib.rs` | `path_prefix` operates on root-relative paths |
| E1-03 | Replace walker with `ignore` crate + Tier 0 manifest | `walk.rs`, `Cargo.toml` | Parallel walk, .gitignore support, <100ms for 25k files |
| E1-04 | Noisy artifact exclusion | `walk.rs`, `util.rs`, `index.rs` | Generated outputs don't dominate BM25 |
| E1-05 | Schema compat enforcement | `handlers/storage.rs`, `mcp/storage.rs` | v2 indexes rejected with clear error |
| E1-06 | MCP server skeleton with capabilities | `mcp/server.rs`, `mcp/tools.rs`, `mcp/resources.rs`, `mcp/notifications.rs` | Server initializes with correct capabilities, tools/list works, roots/list integration |
| E1-07 | Filesystem watching | `walk.rs` | `notify` crate watches roots, cache invalidation on change |
| E1-08 | Tests + docs | `tests/`, `docs/` | Path, walker, migration, noise tests pass |

**Critical**: E1-06 (MCP server skeleton) is new and must land in Phase 1. The server
must speak proper MCP from the start — capability negotiation, tools/list,
roots/list, and notifications are not afterthoughts.

### Phase 2: Parser Framework + Stack Graphs + Generic Adapter

**Objective**: Structural indexing for all tree-sitter languages (generic adapter),
plus precise cross-file resolution for top languages (stack graphs).

| ID | Work Item | Depends On | Acceptance |
|----|-----------|------------|------------|
| E2-01 | Split chunk.rs into module tree | E1-05 | Clean module layout |
| E2-02 | Define `LanguageAdapter` trait | E2-01 | Trait compiles with parse/incremental methods |
| E2-03 | Parser registry with three-tier dispatch | E2-02 | Correct tier resolution per language |
| E2-04 | Compile-time query loader (`include_str!`) | E2-03 | .scm files embedded, single binary |
| E2-05 | Generic tree-sitter adapter (Parser Tier 3) | E2-02 | Kotlin file gets structural chunks without adapter |
| E2-06 | Grammar loading infrastructure | E2-05 | Core grammars compiled in, extended behind feature flags |
| E2-07 | Stack-graphs integration (Parser Tier 1) | E2-02 | Python cross-file import resolution works |
| E2-08 | SCIP symbol ID generation | E2-07 | Stable SCIP IDs from stack-graph nodes and tree-sitter |
| E2-09 | Rust adapter | E2-03 + (E2-05 or E2-07) | Rust files no longer `ChunkKind::Unknown` |
| E2-10 | JS/TS migration to adapter form | E2-03 + E2-07 | Existing JS tests pass, TS gets stack-graphs |
| E2-11 | Incremental parse cache | E2-02 | Re-index of 10 changed files in 10k repo: ≤20% of full |
| E2-12 | Parser framework tests | all E2-* | Dispatch, generic, stack-graph, incremental tests pass |

**Parallel tracks**:
- Track A (framework): E2-01 → E2-02 → E2-03 → E2-04
- Track B (generic): E2-02 → E2-05 → E2-06
- Track C (stack-graphs): E2-02 → E2-07 → E2-08
- Track D (languages): (E2-03 + E2-05) → E2-09; (E2-03 + E2-07) → E2-10
- Track E (incremental): E2-02 → E2-11

### Phase 3: Retrieval + Graph Intelligence + Ranking

**Objective**: fst-backed exact index, graph-walk MCP tool, hybrid retrieval, ranking overhaul.

| ID | Work Item | Depends On | Acceptance |
|----|-----------|------------|------------|
| E3-01 | fst-backed exact symbol index | E2-08 | Memory-mapped, ordered iteration for prefix |
| E3-02 | Lookup modes (exact/tail/prefix/contains/qualified/auto) | E3-01 | Each mode produces correct results |
| E3-03 | Symbol search on SCIP canonical fields | E3-02 | fst first, BM25 fallback |
| E3-04 | Lookup tests | E3-03 | Regression suite passes |
| E3-05 | Graph: symbol table keyed on SCIP IDs | E2-08 | Bidirectional adjacency list |
| E3-06 | Graph: edge index on canonical IDs | E3-05 | Edge types: Calls, CalledBy, Imports, ImportedBy, TypeRef, etc. |
| E3-07 | `llmx_graph_walk` MCP tool | E3-06 | Multi-hop traversal returns inline content |
| E3-08 | Graph-walk tests | E3-07 | Depth-2, heuristic edge labeling, filter tests |
| E3-09 | Deterministic source priors | E1-04 + E3-03 | Source > test > doc > generated > log |
| E3-10 | Symbol and identifier boosts | E3-01 + E3-03 | SCIP exact: 10x, tail: 5x, substring: 2x |
| E3-11 | Graph-neighbor expansion in ranking | E3-06 + E3-10 | 1-hop neighbors, precise > heuristic weighting |
| E3-12 | Hybrid retrieval fusion | E3-11 | Structural (3.0) + graph (2.0) + semantic (1.0) RRF |
| E3-13 | Ranking regression tests | E3-12 | codex_exec: definition first, callers second, docs third |

**Parallel tracks**:
- Track F (lookup): E3-01 → E3-02 → E3-03 → E3-04
- Track G (graph): E3-05 → E3-06 → E3-07 → E3-08
- Track H (ranking): E3-09 → E3-10 → E3-11 → E3-12 → E3-13

### Phase 4: WASM + Extended Languages + Polish

**Objective**: WASM support, language quality matrix, query pack refinement.

| ID | Work Item | Depends On | Acceptance |
|----|-----------|------------|------------|
| E4-01 | Go query pack (Parser Tier 2) | E2-04 | Definitions + heuristic cross-file edges |
| E4-02 | C/C++ query packs | E2-04 | #include tracking, macro defs |
| E4-03 | C# query pack | E2-04 | Definitions, namespace resolution |
| E4-04 | Python query pack (Tier 2 fallback) | E2-04 | For stack-graphs-is-too-slow cases |
| E4-05 | Cross-language tests | E4-01..E4-04 | Each Tier 2 ≥ Tier 3 quality on fixtures |
| E4-06 | WASM Mode A: ship core parsers | E3-01 | <5MB gzipped, generic adapter only |
| E4-07 | WASM Mode B: accept pre-indexed SCIP | E3-01 | Deserialize SCIP index, no parser needed |
| E4-08 | WASM capability metadata + parity tests | E4-06/E4-07 | Honest tier reporting |
| E4-09 | Extended language grammars (batched) | E2-05 | Batch A-D: JVM, scripting, infra, specialized |
| E4-10 | Language quality matrix + maintenance policy | E4-09 | Published `LANGUAGE_SUPPORT.md` |

---

## 8. Ranking Configuration

### Deterministic Priors (source quality)

| File type | Weight |
|-----------|--------|
| Source file | 1.0 |
| Test file | 0.6 |
| Doc/markdown | 0.4 |
| Generated/vendored | 0.2 |
| Log/artifact | 0.05 |

### Query-Time Boosts

| Match type | Multiplier |
|------------|------------|
| SCIP exact match | 10x |
| symbol_tail exact | 5x |
| Identifier substring | 2x |
| Path-prefix match | 1.5x |

### Hybrid Retrieval Fusion (RRF weights)

| Retrieval path | Weight |
|----------------|--------|
| Structural (fst exact/prefix) | 3.0 |
| Graph-expanded (1-hop neighbors) | 2.0 |
| Semantic (BM25 + vector) | 1.0 |

---

## 9. Artifact Exclusion Rules

### Hard exclusion (never walk)

`target/`, `dist/`, `build/`, `.next/`, `.turbo/`, `coverage/`, `node_modules/`,
`__pycache__/`, `.tox/`, `.mypy_cache/`, `.git/`

### Soft exclusion (walk but tag `is_generated: true`)

`export/`, `llmx-export/`, `*.log`, `*.zip`, `*.min.js`, `*.min.css`, `*.map`,
lock files (`package-lock.json`, `Cargo.lock`, `yarn.lock`, `pnpm-lock.yaml`,
`poetry.lock`, `Gemfile.lock`)

---

## 10. Key Agent Workflow Patterns

### First Contact (Tier 0)

```
agent → llmx_status()                         # readiness_tier: 0
agent → llmx_explore(path="", mode="tree")    # file tree, instant
agent → llmx_explore(path="src", mode="tree") # narrow down
agent → llmx_lookup(symbol="codex_exec")      # on-demand parse (Tier 1)
```

### Deep Investigation (Tier 2+)

```
agent → llmx_status()                         # readiness_tier: 2
agent → llmx_lookup(symbol="codex_exec")      # fst exact match
agent → llmx_graph_walk(symbol="codex_exec",
          direction="callers", depth=2)        # multi-hop
agent → llmx_graph_walk(symbol="run_command",
          direction="callees",
          filter={visibility: "pub"})          # filtered walk
agent → synthesizes call chain
```

### Adaptive Degradation

```
agent → llmx_search(query="permission checking", intent="semantic")
       # strategy_used: "bm25" (Tier 3 not ready)
agent → (adapts) llmx_search(query="check_permissions fn", intent="symbol")
       # strategy_used: "symbol_fuzzy"
```

---

## 11. Open Questions

1. **Stack-graphs Rust maturity.** Evaluate after E2-07. If <80% quality on test
   fixtures, fall back to Parser Tier 2 with custom query packs.

2. **Stack-graphs at scale.** O(n·m) worst case. For >100k files, may need to
   partition by crate/package boundary.

3. **SCIP version stability.** Pin to a specific SCIP spec version.

4. **WASM binary size.** Mode A target <5MB gzipped. If blown, default to Mode B.

5. **Embedding model.** Current: mdbr-leaf-ir. Consider Voyage-Code-3 (13-17%
   improvement on code retrieval) or CodeSage Large V2 (open-weights, 1.3B params,
   trained on The Stack V2). This is a config swap, not an architectural change.

6. **MCP transport.** Current: stdio. Consider adding streamable HTTP transport for
   remote deployment (CI/CD indexing server that multiple agents connect to).

---

## 12. Exit Criteria

This specification is fully implemented when:

1. Server initializes with correct MCP capabilities (tools, resources, tasks, logging).
2. Tier 0 is useful within 100ms of startup on any codebase.
3. Any tree-sitter language gets structural indexing (Parser Tier 3 minimum).
4. Rust, Python, TS/JS, Java have cross-file resolution (Parser Tier 1).
5. Symbol lookup uses SCIP IDs via fst-backed index.
6. `llmx_graph_walk` returns multi-hop results with inline content and edge confidence.
7. Every tool response includes `readiness_tier` in structuredContent.
8. Hybrid retrieval (structural + graph + semantic) outperforms flat BM25.
9. Filesystem watching keeps the index fresh without manual re-indexing.
10. Path filters are stable and root-relative.
11. WASM exposes documented structural core pack.
12. Language quality matrix is published.
