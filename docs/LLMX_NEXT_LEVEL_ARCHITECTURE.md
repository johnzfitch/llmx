# llmx MCP Server: Next-Level Architecture Plan

**Date:** 2026-03-10
**Status:** Proposal
**Scope:** Revolutionary redesign of llmx search architecture for production MCP deployment

---

## The Core Thesis

The current llmx stack (BM25 + arctic-embed-s + RRF) is architecturally sound but fundamentally **text-centric**. It treats code as strings. Every improvement so far has been about better ways to match strings — better embeddings, better fusion, better tokenization.

The revolutionary move is to stop thinking of llmx as a **text search engine that happens to index code** and start building it as a **code intelligence engine that exposes search as one of several retrieval primitives**.

This means three paradigm shifts:

1. **Index structure, not just text.** The unit of indexing becomes the AST node, not the text chunk. Relationships between nodes (calls, imports, types) become first-class searchable signals.

2. **The MCP server becomes an agent's working memory.** Not a search box — a persistent, queryable understanding of a codebase that agents interact with through multiple specialized tools, not one generic `llmx_search`.

3. **The index learns from how agents use it.** Every `get_chunk` call after a `search` is implicit relevance feedback. The server should get smarter over time without any ML training loop.

---

## Architecture: Five Layers

```
┌─────────────────────────────────────────────────────┐
│  Layer 5: Agent Interface (MCP Tools)               │
│  Specialized tools, not one generic search           │
├─────────────────────────────────────────────────────┤
│  Layer 4: Query Intelligence                         │
│  Intent classification, adaptive routing, reranking  │
├─────────────────────────────────────────────────────┤
│  Layer 3: Multi-Signal Fusion                        │
│  RRF++ with graph proximity, recency, quality        │
├─────────────────────────────────────────────────────┤
│  Layer 2: Retrieval Engines (parallel)               │
│  BM25 │ Dense │ Graph │ Symbol │ Type               │
├─────────────────────────────────────────────────────┤
│  Layer 1: Structural Index                           │
│  AST chunks │ Code graph │ Embeddings │ Inverted idx │
└─────────────────────────────────────────────────────┘
```

---

## Layer 1: Structural Index

### 1.1 AST-Native Chunking (replaces line/char-based chunking)

**Current state:** `chunk.rs` does heading-based chunking for markdown and tree-sitter boundaries for JS/TS. Other languages fall back to fixed-size character windows.

**Revolutionary change:** Every chunk becomes an **AST node with metadata**, not a text blob with line numbers.

```rust
struct StructuralChunk {
    // Current fields preserved
    id: String,
    content: String,
    path: String,
    start_line: usize,
    end_line: usize,

    // NEW: Structural metadata
    ast_kind: AstNodeKind,        // Function, Class, Method, Module, Block, Import, Type
    symbol: Option<String>,        // Fully qualified name: "auth::jwt::verify_token"
    signature: Option<String>,     // "fn verify_token(token: &str, key: &[u8]) -> Result<Claims>"
    parent_symbol: Option<String>, // "auth::jwt" (enclosing scope)
    imports: Vec<String>,          // What this chunk imports/uses
    exports: Vec<String>,          // What this chunk exports/defines
    calls: Vec<String>,            // Functions called within this chunk
    type_refs: Vec<String>,        // Types referenced (struct names, trait bounds)
    complexity: u16,               // Cyclomatic complexity estimate
    doc_summary: Option<String>,   // Extracted doc comment, first sentence
}
```

**Why this matters:** When an agent searches for "authentication", the current system matches text containing that word. The structural system also surfaces `verify_token` in `auth/jwt.rs` because it's in the `auth` module, returns a `Claims` type, and is called by `login_handler` — even if the word "authentication" never appears in the function body.

**Implementation path:** tree-sitter already handles JS/TS. Add grammars for Rust (`tree-sitter-rust`), Python (`tree-sitter-python`), Go (`tree-sitter-go`), and the top 10 languages. tree-sitter has Rust bindings for all of them. The grammar files are ~50-200KB each. Total binary size increase for 10 languages: ~1.5MB.

**Effort estimate:** Medium. tree-sitter infrastructure already exists in `chunk.rs`. The work is extracting richer metadata from the parse tree and populating the new fields.

### 1.2 Code Graph Index

**New data structure:** A directed graph where nodes are chunks and edges are relationships.

```rust
struct CodeGraph {
    // Adjacency list representation
    edges: Vec<GraphEdge>,
    // Inverted index: symbol name → chunk IDs that define it
    symbol_defs: BTreeMap<String, Vec<String>>,
    // Inverted index: symbol name → chunk IDs that reference it
    symbol_refs: BTreeMap<String, Vec<String>>,
    // Module hierarchy: "auth::jwt" → ["auth::jwt::verify_token", "auth::jwt::Claims"]
    module_tree: BTreeMap<String, Vec<String>>,
}

struct GraphEdge {
    from_chunk: String,
    to_chunk: String,
    kind: EdgeKind,
    weight: f32,  // 1.0 for direct call, 0.5 for type reference, etc.
}

enum EdgeKind {
    Calls,        // Function A calls function B
    Imports,      // File A imports from file B
    Implements,   // Struct A implements trait B
    Extends,      // Class A extends class B
    TypeRef,      // Function A uses type defined in B
    CoModified,   // A and B change together in git history (requires git log)
}
```

**Why this matters:** Enables **graph-walk retrieval**. When a search hits `verify_token`, the graph can surface `Claims` (the return type, defined elsewhere), `login_handler` (the caller), and `jwt_secret` (the config it depends on) — all without those chunks matching the query textually or semantically. This is **multi-hop retrieval** and it's what makes GitHub Copilot's semantic indexing feel magical.

**Implementation path:** Build the graph at index time by cross-referencing the `calls`, `imports`, `type_refs`, and `exports` fields from structural chunks. No ML required — pure symbol resolution. For cross-file resolution, use a two-pass approach: first pass extracts all symbols, second pass resolves references.

**Effort estimate:** Medium-high. The symbol extraction is straightforward with tree-sitter. Cross-file resolution is the hard part — you need a lightweight symbol table. Start with exact-match resolution (same name = same symbol) and iterate.

### 1.3 Index-Time Enrichment

Two enrichments that pay for themselves at search time:

**A. Synthetic query generation.** For each chunk, generate 3-5 natural language questions it would answer. Store these in the inverted index alongside the actual content.

```rust
struct EnrichedChunk {
    // ... all StructuralChunk fields ...
    synthetic_queries: Vec<String>,  // "How does JWT verification work?"
                                      // "What validates authentication tokens?"
                                      // "Where is the token signature checked?"
}
```

**Generation approach (no LLM required for v1):** Template-based from structural metadata:
- Function `verify_token` in module `auth::jwt` → "How does [jwt] [verification] work?", "What [verifies] [tokens]?"
- Extract noun phrases from doc comments
- Use the symbol name, parent module, parameter names, and return type as seed terms

This is the generation-augmented retrieval pattern but using **algorithmic templates** instead of an LLM. Add LLM generation as an optional enrichment pass later.

**B. Quality scoring.** Assign each chunk a quality signal at index time:
- Has doc comments → +0.2
- Has tests (or is referenced by test files) → +0.2
- Cyclomatic complexity < 15 → +0.1
- Recently modified (git blame recency) → +0.1
- High fan-in (many callers in the code graph) → +0.2

This quality score becomes a signal in the fusion layer. When two chunks are equally relevant, surface the well-documented, well-tested, recently-maintained one.

---

## Layer 2: Retrieval Engines

Five parallel retrieval engines, each returning a ranked list. All run concurrently via `tokio::join!`.

### 2.1 BM25 (existing, enhanced)

**Enhancement:** Index synthetic queries alongside content. When a user searches "authentication", BM25 now also matches chunks whose synthetic queries contain "authentication" even if the code itself doesn't.

**Enhancement:** Add a lightweight synonym table for code-specific terms:
```rust
static SYNONYMS: &[(&str, &[&str])] = &[
    ("auth", &["authentication", "authorize", "login"]),
    ("err", &["error", "exception", "failure"]),
    ("config", &["configuration", "settings", "options"]),
    ("db", &["database", "storage", "persistence"]),
    ("req", &["request", "http", "api"]),
    ("res", &["response", "reply", "result"]),
    // ... ~50 entries covers 90% of code abbreviations
];
```

Query-time expansion: "auth error" → also searches for "authentication error", "authorize error", "login error". Zero model overhead.

### 2.2 Dense Embedding (existing, model swap)

**Swap arctic-embed-s (33M, 384d) → mdbr-leaf-ir (23M, 768d with MRL→256d).**

Rationale already covered in prior analysis. Smaller binary, better BEIR scores, same BERT architecture so Burn module changes are minimal. Ship 256d embeddings via MRL for ~33% storage reduction vs current 384d.

**Future option:** Asymmetric encoding. Use snowflake-arctic-embed-m-v1.5 (110M) for document embedding at index time (slow path, done once), leaf-ir (23M) for query embedding at search time (fast path). The LEAF paper shows this outperforms symmetric in 11/14 BEIR datasets.

### 2.3 Graph Walk (NEW)

Given a query, find initial seed chunks via BM25 or embedding, then walk the code graph to find structurally related chunks.

```rust
fn graph_retrieve(
    seeds: &[ChunkId],
    graph: &CodeGraph,
    max_hops: usize,    // default: 2
    max_results: usize,  // default: 20
) -> Vec<(ChunkId, f32)> {
    // Personalized PageRank from seed nodes
    // Edge weights decay: hop 1 = 1.0, hop 2 = 0.5
    // EdgeKind weights: Calls=1.0, Imports=0.8, TypeRef=0.6, CoModified=0.4
}
```

**Why Personalized PageRank:** It naturally handles the "radiating outward from seed nodes" pattern. Chunks that are reachable from multiple seed nodes get higher scores. The algorithm is well-understood, fast (converges in ~10 iterations for small graphs), and deterministic.

**Effort estimate:** Low-medium. The graph is already built in Layer 1. PageRank is ~50 lines of Rust.

### 2.4 Symbol Search (NEW)

Direct symbol lookup — bypasses text search entirely.

```rust
fn symbol_search(query: &str, index: &IndexFile) -> Vec<(ChunkId, f32)> {
    // Fuzzy match against symbol_defs keys
    // "verify_tok" matches "auth::jwt::verify_token"
    // "Claims" matches "auth::jwt::Claims"
    // Uses Jaro-Winkler or Levenshtein with prefix bonus
}
```

**Why a separate engine:** Agents frequently search for exact or near-exact symbol names. BM25 handles this poorly because it tokenizes `verify_token` into `["verify", "token"]` and the IDF of "token" is terrible in a codebase. Symbol search treats the full qualified name as the match unit.

### 2.5 Type/Signature Search (NEW, stretch goal)

Search by function signature pattern:

```
"fn(string, bytes) -> Result"  →  matches verify_token(token: &str, key: &[u8]) -> Result<Claims>
"impl Display for *"           →  matches all Display implementations
"async fn * -> Stream"         →  matches all async streaming functions
```

This is a **structural query language** over the AST metadata. Not fuzzy text matching — pattern matching over typed signatures.

**Implementation:** Parse the query into a signature pattern, match against `signature` fields in StructuralChunk using a simple pattern grammar. No embeddings, no ML.

**Effort estimate:** Medium. The signature extraction from tree-sitter is the hard part. The matching is straightforward once you have structured signatures.

---

## Layer 3: Multi-Signal Fusion (RRF++)

**Current state:** RRF with k=60 merges BM25 rank and semantic rank.

**New state:** RRF over N signals with per-signal weighting and quality boosting.

```rust
fn rrf_plus(
    results: &[(&str, Vec<(ChunkId, f32)>)],  // (engine_name, ranked_results)
    weights: &HashMap<String, f32>,             // per-engine weights
    quality_scores: &HashMap<ChunkId, f32>,     // index-time quality
    k: usize,                                    // RRF constant, still 60
) -> Vec<(ChunkId, f32)> {
    // For each chunk that appears in any result list:
    // score = Σ (weight_i / (k + rank_i)) + quality_boost * quality_score
    //
    // Default weights:
    //   bm25: 1.0
    //   dense: 1.0
    //   graph: 0.6
    //   symbol: 1.5  (high weight — exact symbol match is strong signal)
    //   type_sig: 1.2
    //
    // quality_boost: 0.3 (enough to break ties, not enough to override relevance)
}
```

**Why this works:** RRF is already score-distribution-agnostic. Adding more signals doesn't require normalization. Each engine contributes its rank, weighted by how much we trust that signal type. The quality score acts as a tiebreaker that surfaces well-maintained code over dead code.

**Critical insight:** The weights should be **query-dependent** in v2. A query that looks like a symbol name (camelCase, no spaces) should upweight symbol search. A query that looks like natural language should upweight dense. A query with type annotations should upweight type search. This is Layer 4's job.

---

## Layer 4: Query Intelligence

### 4.1 Query Intent Classification (no ML)

Classify every incoming query into one of four intents using pattern matching:

```rust
enum QueryIntent {
    Symbol,      // "parseConfig", "auth::jwt::verify_token", "MyClass.method"
    Structural,  // "async functions that return Stream", "impl Trait for *"
    Semantic,    // "how does authentication work", "error handling strategy"
    Keyword,     // "TODO fixme", "SAFETY:", "unsafe"
}

fn classify_query(query: &str) -> QueryIntent {
    if looks_like_symbol(query) { return QueryIntent::Symbol; }
    if has_type_patterns(query) { return QueryIntent::Structural; }
    if is_natural_language(query) { return QueryIntent::Semantic; }
    QueryIntent::Keyword
}

fn looks_like_symbol(q: &str) -> bool {
    // Contains camelCase, snake_case, ::, or . with no spaces
    // or matches known symbol patterns
    let no_spaces = !q.contains(' ');
    let has_case_boundary = /* camelCase regex */;
    let has_separator = q.contains("::") || q.contains('.') || q.contains('_');
    no_spaces && (has_case_boundary || has_separator)
}

fn is_natural_language(q: &str) -> bool {
    // Contains common English words: "how", "what", "where", "does", "the"
    // Has spaces and sentence-like structure
    let words: Vec<&str> = q.split_whitespace().collect();
    words.len() >= 3 && has_stopwords(&words)
}
```

**Per-intent engine routing:**

| Intent | BM25 | Dense | Graph | Symbol | Type |
|--------|-------|-------|-------|--------|------|
| Symbol | 0.3 | 0.2 | 0.5 | **2.0** | 0.0 |
| Structural | 0.2 | 0.3 | 0.3 | 0.5 | **2.0** |
| Semantic | 0.5 | **1.5** | 0.8 | 0.2 | 0.0 |
| Keyword | **1.5** | 0.3 | 0.2 | 0.5 | 0.0 |

This is the "adaptive query routing" idea but implemented as a simple lookup table, not a learned model. It will capture 80%+ of the value of a trained router.

### 4.2 Query Expansion (lightweight)

Before dispatching to engines, expand the query:

**For semantic queries:** Add the synonym expansion from 2.1.

**For symbol queries:** Generate variations — `verifyToken` → also search `verify_token`, `VerifyToken`, `VERIFY_TOKEN`. This handles cross-language naming convention differences.

**For all queries:** If the query mentions a module or file path, extract it as a filter:
- "authentication in src/auth" → query="authentication", filter=path_prefix:"src/auth"
- "the jwt module" → query="jwt", filter=symbol_prefix:"jwt" OR path_contains:"jwt"

### 4.3 Result Explanation

For each result, generate a one-line explanation of **why** it matched:

```rust
struct SearchResult {
    chunk_id: String,
    score: f32,
    // NEW
    match_reason: String,  // "Symbol match: auth::jwt::verify_token"
                           // "Called by login_handler which matched your query"
                           // "Semantic similarity: 0.87 (JWT verification logic)"
                           // "BM25: 'authentication' appears 3 times"
}
```

**Why this matters for agents:** LLMs make better decisions about which chunks to retrieve in full (via `get_chunk`) when they understand *why* each result appeared. This reduces unnecessary `get_chunk` calls and keeps the agent's context window focused.

---

## Layer 5: Agent Interface (MCP Tools)

### 5.1 Tool Redesign

**Current tools:** `llmx_index`, `llmx_search`, `llmx_get_chunk`, `llmx_export`

**Proposed tools (backward-compatible — old tools still work, new tools added):**

#### `llmx_search` (enhanced)

Add new parameters:
```json
{
    "query": "string",
    "index_id": "string",
    "intent": "auto | symbol | semantic | keyword | structural",  // NEW, default "auto"
    "include_graph_context": true,    // NEW: also return 1-hop graph neighbors
    "explain": true,                   // NEW: include match_reason per result
    "filters": { /* existing */ },
    "limit": 10,
    "max_tokens": 8000
}
```

#### `llmx_explore` (NEW)

Given a chunk ID, return its structural context: what it calls, what calls it, what module it's in, sibling functions, and the relevant portion of the module tree.

```json
{
    "name": "llmx_explore",
    "description": "Explore the structural neighborhood of a code element. Returns callers, callees, type dependencies, sibling definitions, and module hierarchy. Use after llmx_search to understand how a piece of code fits into the broader codebase.",
    "input": {
        "index_id": "string",
        "chunk_id": "string",
        "depth": 2,            // How many hops in the code graph
        "include_source": false // If true, include chunk content (not just metadata)
    }
}
```

**Why this tool is revolutionary:** This is the tool that turns llmx from a search engine into a **code intelligence engine**. An agent can search for "authentication", get `verify_token`, then `explore` it to discover the entire auth subsystem — without needing to search again. It's navigating the codebase structurally, not just textually.

#### `llmx_symbols` (NEW)

Fast symbol table lookup. Returns all defined symbols matching a pattern, with their locations and types.

```json
{
    "name": "llmx_symbols",
    "description": "List all code symbols (functions, classes, types, constants) matching a pattern. Supports glob patterns and fuzzy matching. Much faster than full search for finding specific definitions.",
    "input": {
        "index_id": "string",
        "pattern": "auth::*",       // Glob pattern over qualified names
        "kind": "function",          // Optional: function, class, type, constant, method
        "limit": 50
    }
}
```

#### `llmx_diff` (NEW, stretch goal)

Given two index IDs (e.g., before and after a change), return what changed: new chunks, modified chunks, removed chunks, and how the code graph changed.

```json
{
    "name": "llmx_diff",
    "description": "Compare two index snapshots. Returns structural changes: new/modified/removed symbols, changed call relationships, and affected modules. Use for understanding impact of code changes.",
    "input": {
        "old_index_id": "string",
        "new_index_id": "string",
        "include_content": false
    }
}
```

### 5.2 Implicit Feedback Loop

**The insight no one is implementing:** Every time an agent calls `llmx_search` followed by `llmx_get_chunk` for specific results, that's **implicit relevance feedback**. The chunks the agent retrieved are relevant; the ones it skipped are not.

Track this in a lightweight feedback store:

```rust
struct FeedbackStore {
    // query_hash → Vec<(chunk_id, was_retrieved: bool)>
    feedback: BTreeMap<u64, Vec<(String, bool)>>,
    // chunk_id → retrieval_count (how often agents actually use this chunk)
    chunk_popularity: BTreeMap<String, u32>,
}
```

**Use at search time:** Boost chunks with high historical retrieval rates. Penalize chunks that frequently appear in results but are never retrieved (agents consistently skip them → they're false positives).

**No ML required.** This is a simple frequency-based popularity signal that feeds into the RRF++ quality score. It's the same insight behind Google's click-through rate signals, but applied to agent behavior.

---

## Implementation Phases

### Phase 7A: Structural Foundation (2-3 weeks)

**Goal:** AST-native chunking for top 5 languages, code graph construction.

1. Extend `chunk.rs` to populate `StructuralChunk` fields for Rust, Python, Go, TypeScript, JavaScript using existing tree-sitter infrastructure
2. Build `CodeGraph` from cross-referencing exports/imports/calls across chunks
3. Symbol resolution (exact match first, fuzzy later)
4. Add `symbol_defs` and `symbol_refs` inverted indexes to `IndexFile`
5. Synthetic query generation via templates

**Ship criterion:** `llmx_index` produces structural metadata. Existing search still works unchanged. New data is present but not yet used in search ranking.

**Risk:** Cross-file symbol resolution accuracy. Mitigation: start with intra-file resolution (always correct) and add cross-file as a separate flag.

### Phase 7B: Multi-Engine Search (2 weeks)

**Goal:** Symbol search and graph walk engines, RRF++ fusion.

1. Implement `symbol_search` with Jaro-Winkler fuzzy matching
2. Implement `graph_retrieve` with Personalized PageRank
3. Extend RRF to accept N engines with weights
4. Implement query intent classification
5. Wire intent-based weight routing
6. Add `explain` field to search results

**Ship criterion:** `llmx_search` with `intent: "auto"` produces measurably better results than current hybrid on a test suite of 50 queries across 5 real codebases.

### Phase 7C: New MCP Tools (1 week)

**Goal:** `llmx_explore`, `llmx_symbols`, enhanced `llmx_search` parameters.

1. Implement `llmx_explore` backed by CodeGraph traversal
2. Implement `llmx_symbols` backed by `symbol_defs` index
3. Add `include_graph_context`, `explain`, `intent` params to `llmx_search`
4. Implicit feedback tracking (write path)

**Ship criterion:** Agent (Claude) can navigate a codebase using `search` → `explore` → `get_chunk` flow and reaches relevant code faster than search-only flow.

### Phase 7D: Embedding Upgrade (1 week)

**Goal:** Swap arctic-embed-s → mdbr-leaf-ir, 256d MRL.

1. Swap model weights in `models/` directory
2. Adjust `embeddings.rs` for new model dimensions
3. Add MRL truncation (768d → 256d) with L2 renormalization
4. Update index format version (triggers reindex for existing users)
5. Benchmark: latency, memory, and retrieval quality vs old model

**Ship criterion:** Measurable improvement on BEIR-style evaluation over llmx test corpora. No regression in indexing speed. Binary size decrease (23M vs 33M params).

### Phase 7E: Polish & Feedback (1 week)

**Goal:** Implicit feedback loop, quality scoring, type/signature search (stretch).

1. Implement FeedbackStore with persistence
2. Wire feedback into RRF++ quality boost
3. Index-time quality scoring (doc comments, test coverage, complexity, recency)
4. (Stretch) Type/signature search engine and query parser

---

## What This Looks Like In Practice

**Before (current llmx):**

```
Agent: llmx_search("how does authentication work")
→ Returns 10 chunks containing the word "authentication" ranked by BM25+embedding
Agent: llmx_get_chunk(chunk_3)  // reads one, hopes it's the right one
Agent: llmx_search("jwt verify")  // has to search again to find related code
Agent: llmx_get_chunk(chunk_7)
Agent: llmx_search("login handler")  // and again...
```

**After (revolutionary llmx):**

```
Agent: llmx_search("how does authentication work", explain=true, include_graph_context=true)
→ Returns:
  1. auth::jwt::verify_token (semantic match, 0.91)
     Graph context: called by login_handler, uses Claims type, imports ring::hmac
  2. auth::middleware::require_auth (graph: 1 hop from verify_token)
     Reason: "Called by 12 route handlers, calls verify_token"
  3. auth::jwt::Claims (type dependency of verify_token)
     Reason: "Return type of verify_token, referenced by 8 functions"

Agent: llmx_explore(chunk_id="auth::jwt::verify_token", depth=2)
→ Returns the entire auth subsystem structure:
   Module auth::jwt: verify_token, sign_token, Claims, JwtError
   Callers: login_handler, refresh_handler, middleware::require_auth
   Dependencies: ring::hmac, serde_json, config::jwt_secret

Agent: llmx_get_chunk("auth::middleware::require_auth")  // knows exactly what to read
```

Three tool calls instead of six. Better results. The agent understands the code structure, not just text matches.

---

## Key Design Principles

1. **Algorithms first, ML only when >85% accuracy fails.** Every new engine uses deterministic algorithms (tree-sitter, PageRank, Jaro-Winkler, pattern matching). ML (embeddings) is one signal among five, not the foundation.

2. **Zero network calls.** Everything runs locally. No API-dependent features.

3. **Backward compatible.** Existing `llmx_search` with default parameters returns identical results to current implementation. New features are opt-in via new parameters and new tools.

4. **Incremental value.** Each phase ships independently and improves the tool. Phase 7A alone (better chunks) improves search quality even without new engines.

5. **Observable.** The `explain` field and `llmx_explore` tool make the system's decisions transparent to both agents and humans.

---

## Competitive Position

After this work, llmx is no longer competing with grep, ripgrep, or generic vector search tools. It's competing with GitHub Copilot's semantic indexing, Sourcegraph's code intelligence, and Cursor's codebase understanding — but running **entirely locally**, with **zero cloud dependency**, inside any MCP-compatible agent.

That's the product: **Sourcegraph-class code intelligence as a local MCP server.**

No one else is building this. The closest is Sourcegraph's own code graph, but it requires a server. GitHub's semantic indexing is proprietary and cloud-only. Every open-source code search tool (ripgrep, ast-grep, codesearch) is single-signal. llmx would be the first open tool that fuses text, semantic, structural, and graph signals into a single local-first search experience designed for AI agents.

---

## Open Questions

1. **Index size growth.** Structural metadata + code graph + synthetic queries will increase index size. Estimate: 3-5x current size. Acceptable? If not, make graph/synthetic queries optional features.

2. **Tree-sitter grammar licensing.** Most are MIT/Apache. Verify all 10 target languages before committing.

3. **Feedback persistence.** Should implicit feedback survive across index rebuilds? Probably yes (keyed by symbol name, not chunk ID, so it's stable across reindexes).

4. **WASM path.** How much of this works in the browser? AST chunking: yes (tree-sitter compiles to WASM). Code graph: yes (pure data structure). New engines: yes (all algorithmic). The only question is whether the combined index size fits in browser memory for large repos.

5. **Evaluation methodology.** Need a benchmark suite of real queries against real codebases with human-judged relevance. Consider building this from the implicit feedback data once Phase 7E ships.
