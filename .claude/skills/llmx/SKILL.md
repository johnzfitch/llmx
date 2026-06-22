---
name: llmx
description: Use llmx for token-efficient code intelligence on a codebase — semantic/keyword search, symbol lookup, and call-graph traversal via its local MCP tools (llmx_status, llmx_search, llmx_lookup, llmx_refs, llmx_explore, llmx_symbols, llmx_get_chunk, llmx_index, llmx_manage). Prefer it over raw grep/Read when exploring an unfamiliar or large repo, answering "where is X defined", "who calls Y", "what does this module do", or when you need semantic ("find the code that handles auth retries") rather than exact-string matches. Indexing and on-device embeddings run locally; no code leaves the machine.
user-invocable: true
---

# Using llmx

llmx is a local **MCP code-intelligence server**. It indexes a codebase into structural
chunks, builds a symbol table and a call/import graph, and serves on-device neural
embeddings (`mdbr-leaf-ir`) for semantic search. Everything runs locally — no code is
uploaded.

The point is **token efficiency and precision**: instead of reading whole files or
grepping blindly, you retrieve the few chunks that matter, resolve symbols exactly, and
walk the graph to understand relationships.

## When to reach for llmx

Use it when:
- Exploring an unfamiliar or large repo ("what does this codebase do", "where's the entry point").
- The query is **semantic**, not exact-string: "code that handles auth token refresh", "the retry/backoff logic". Plain `grep` can't find these; llmx semantic search can.
- You need **structure**: "where is `parseConfig` defined", "who calls `run_command`", "what imports this module".
- You want to spend few tokens: llmx returns ranked chunks with line ranges, not whole files.

Stick with `grep`/Read when: the repo is tiny, you already know the exact file/line, or you need a literal string match across every file (grep is fine and cheaper for that).

## Step 0 — Confirm availability and readiness

Always start with **`llmx_status`**. It tells you:
- `readiness_tier` (0–3) — how good answers will be (see below).
- `files_indexed` / `files_total`, `symbols_indexed`, `embeddings_ready`, `languages`.
- `background_tasks` — whether indexing is still running.

If the `llmx_*` tools are **not available at all**, the MCP server isn't registered for
this session — see *Setup* at the bottom; tell the user rather than silently falling back.

If status shows **no index / tier 0 / cwd not indexed**, build one with **`llmx_index`**
(see below) before searching.

## Readiness tiers

llmx never blocks — every tool returns the best available answer for the current tier:

- **Tier 0** — manifest only. `llmx_search` matches filenames/paths; `llmx_explore` lists the tree.
- **Tier 1** — parsed symbols (intra-file). `llmx_lookup`, `llmx_symbols`, intra-file `llmx_refs`.
- **Tier 2** — cross-file symbol index + graph. `llmx_lookup` resolves across files; `llmx_refs` gives real callers/callees/importers with edge types.
- **Tier 3** — embeddings ready. `llmx_search` with semantic/hybrid retrieval.

Check the `readiness_tier` field in each response: a low tier means the answer is partial,
not wrong — re-run later (or after `llmx_index`) for better results.

## Index resolution (important)

Most tools accept `index_id` and `loc`, **both optional**. If you omit them, llmx resolves
the index from the **current working directory** (walking up to find an indexed root). So
when you're working inside an already-indexed project, just call `llmx_search`/`llmx_lookup`
with the query and skip `index_id`. Pass `loc` (a path) to target a different project, or
`index_id` to pin an exact index.

## Tools

### `llmx_status`
Index readiness, counts, languages, background jobs. Call first. No args.

### `llmx_index`
Create/update an index. `paths`: files or dirs to index. Optional `options`
(`chunk_target_chars`, `max_file_bytes`, `max_total_bytes`). **Async** — returns a
`job_id` immediately. Poll with `llmx_manage(action: "job_status", index_id: "<job_id>")`
until done, or call `llmx_status` to watch `background_tasks`.

### `llmx_search` — the workhorse
Ranked chunks with inline content and line ranges.
- `query` (required).
- `strategy`: `auto` (default) | `bm25` (keyword) | `semantic` | `hybrid`.
- `use_semantic: true` and/or `intent` (`auto|symbol|semantic|keyword`) to steer routing.
- `filters`: `{ path_prefix, kind, symbol_prefix, heading_prefix }`.
- `limit` (default 10), `max_tokens` (inline-content budget, default 8000).
- `explain: true` to get a `match_reason` per result.

Use `bm25` for known identifiers/strings; `semantic`/`hybrid` for conceptual queries
("where do we validate JWTs"). Semantic/hybrid needs tier 3 (embeddings ready).

### `llmx_lookup` — resolve a symbol to its definition
`symbol` (exact or `prefix*`), optional `kind`, `path_prefix`, `limit`. Best for
"find function `parseConfig`", "where is class `AuthService`". Cross-file at tier 2+.

### `llmx_refs` — call/import graph traversal
`symbol` + `direction`: `callers` | `callees` | `importers` | `imports` | `type_users`.
`depth` (hops, default 1), `limit`. Answers "who calls this", "what does this depend on".
Needs tier 2 for precise cross-file edges.

### `llmx_symbols` — browse the symbol table by pattern
`pattern`: exact `foo`, prefix `foo*`, or substring `*foo*`. Filter by `ast_kind`
(function, method, class, interface, type, enum, constant, variable, test) and
`path_prefix`. Good for surveying an area ("all the `*Handler` functions").

### `llmx_explore` — structural overview
`mode`: `files` | `outline` | `symbols`, optional `path_filter`. Use to orient in a new
repo before drilling in.

### `llmx_get_chunk` — fetch full content
`chunk_id` (id, ref, or prefix). Search/symbol results include `chunk_id`; pull the full
chunk when a snippet isn't enough — cheaper than reading the whole file.

### `llmx_manage` — index lifecycle
`action`: `list` | `delete` | `stats` | `job_status`. For `job_status`, pass the job ID in
`index_id`. `stats` gives file/chunk/symbol/edge breakdowns.

## Typical workflow

1. `llmx_status` — check tier and that the cwd is indexed.
2. If not indexed: `llmx_index({ paths: ["."] })`, then poll `llmx_manage(job_status)` / `llmx_status`.
3. Orient: `llmx_explore({ mode: "outline" })` or `llmx_symbols({ pattern: "*" , ast_kind: "class" })`.
4. Find: `llmx_search` (semantic/hybrid for concepts, bm25 for identifiers) or `llmx_lookup` for an exact symbol.
5. Understand relationships: `llmx_refs({ symbol, direction: "callers" })`.
6. Pull detail only when needed: `llmx_get_chunk({ chunk_id })`.

Lead with search/lookup over reading files; widen `limit`/`max_tokens` only if the first
pass misses.

## Examples

- "Where do we handle rate-limit retries?" → `llmx_search({ query: "rate limit retry backoff", strategy: "semantic" })`
- "Find the `BackendClient` struct." → `llmx_lookup({ symbol: "BackendClient", kind: "class" })`
- "Who calls `codex_exec`?" → `llmx_refs({ symbol: "codex_exec", direction: "callers" })`
- "List every test." → `llmx_symbols({ pattern: "*", ast_kind: "test" })`
- "Index this repo first." → `llmx_index({ paths: ["."] })` then poll.

## Setup (if the tools aren't available)

The `llmx_*` tools come from the `llmx-mcp` server. To make them live for a session it must
be built and registered:

```bash
# Build the native server (CPU embeddings; model is committed under ingestor-core/models/, no download)
cargo build --release -p llmx-mcp --bin llmx-mcp \
  --no-default-features --features treesitter,mcp,mcp-http,cli,ndarray-backend
# (omit the flags for the default GPU/wgpu build; or: cargo install --locked llmx-mcp)

# Register for MCP discovery (repo root)
echo '{"mcpServers":{"llmx":{"command":"llmx-mcp"}}}' > .mcp.json
```

Registered MCP tools are namespaced by the server, e.g. `mcp__llmx__llmx_search`. On-device
embeddings (`mdbr-leaf-ir`) power tier-3 semantic search; if `embeddings_ready` is false in
`llmx_status`, semantic queries fall back to keyword ranking until the model/index is ready.

> This skill lives in `.claude/skills/llmx/`. Copy it to `~/.claude/skills/llmx/` to use
> llmx from any project.
