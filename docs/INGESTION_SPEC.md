# LLMX Ingestion Spec

## Goals

- Deterministic, reproducible chunking and indexing in-browser via WASM.
- Local-first storage (IndexedDB/OPFS) with no network calls by default.
- Algorithm-first retrieval (BM25-like) without embeddings.
- Prompt-injection resistant UI (treat all content as untrusted text).

## Architecture (Client-only WASM)

1. User selects a folder (File System Access API) or drags files.
2. WASM ingestor scans files, chunks by type, builds index, exports artifacts.
3. Persist index locally (IndexedDB or OPFS) and allow offline retrieval.

Browser notes:

- Folder selection is not universal:
  - Chromium browsers typically support `showDirectoryPicker` (File System Access API).
  - Some browsers support folder upload via `<input type="file" webkitdirectory>`.
  - Firefox/Floorp generally require file selection or drag-and-drop (no native folder picker).
- Worker/WASM support varies by browser version; the UI should provide a main-thread fallback for environments where module workers fail.

No server upload is required at any stage.

## Index Backend Decision

Selected: Option C (custom compact JSON + inverted index)

Rationale:

- Works fully in WASM with minimal dependencies.
- Single-file artifact (`index.json`) for portability.
- Deterministic serialization using sorted structures.
- Easy to export alongside `llm.md` and chunk files.

## Index Format (index.json)

Top-level fields:

- `version`: schema version.
- `index_id`: sha256 of sorted (path, file hash) pairs.
- `files`: list of file metadata.
- `chunks`: ordered list of chunk records.
- `chunk_refs`: map of `chunk_id -> ref` used by exports/UI (token-efficient `c0001` base36 sequence; deterministic ordering).
- `inverted_index`: term -> postings (tf, doc_len).
- `stats`: totals and averages.
- `warnings`: skipped/truncated file notices.

Chunk record:

- `id`: sha256(path + content_hash + occurrence_index)
- `short_id`: first 12 hex chars of `id`
- `slug`: deterministic semantic label derived from filename + heading/symbol/address
- `path`: normalized relative path
- `kind`: markdown/json/java_script/html/text/image/unknown
- `chunk_index`: per-file index, stable across deterministic chunking
- `start_line`, `end_line`: provenance anchors (best effort for JSON)
- `content`: chunk text (scripts/styles removed for HTML)
- `content_hash`: sha256 of chunk content
- `token_estimate`: approx tokens (chars / 4)
- `heading_path`: heading ancestry (md/html)
- `symbol`: symbol name (js/ts)
- `address`: JSON pointer or range (json)
- `asset_path`: optional relative path for binary assets (e.g. `images/<path>`)

## Chunking Rules (Deterministic)

### Markdown

- Split on heading boundaries (`#` to `######`).
- Maintain heading ancestry in `heading_path`.
- Preserve code fences; avoid splitting inside when possible.
- Hard cap at `chunk_max_chars`.

### JSON

- Parse JSON; if parse fails, fallback to text chunking.
- Object: chunk per top-level key (`$.key`).
- Array: chunk by ranges of 50 elements (`$[start:end]`).
- `start_line`/`end_line` may cover full file when exact ranges are unknown.

### JavaScript / TypeScript

- Tree-sitter parsing; chunk by function/class/method declarations.
- Best effort symbol extraction via `name` field.
- Fallback to text chunking if parsing fails.
- WASM builds disable tree-sitter and always fall back to text chunking.

### HTML

- Strip `<script>` and `<style>` contents from chunk text.
- Split by heading tags (`<h1>`..`<h6>`), track `heading_path`.
- Tag-stripped text used for indexing; provenance uses original line numbers.

### Text

- Split by paragraph boundaries (blank lines) with size caps.

### Images

- Images are treated as binary assets and are never decoded as UTF-8.
- Phase 1 behavior:
  - No OCR is performed.
  - The image filename/path is indexed for search.
  - Exports include the original bytes under `images/<path>` and link from the chunk via `asset_path`.

## Stable Chunk IDs

- Chunk content is hashed with sha256.
- Chunk ID = sha256(path + "\n" + content_hash + "\n" + occurrence_index).
- `occurrence_index` is the ordinal among identical content hashes in a file.
- Deterministic ordering is enforced by sorting files and chunks.
- `slug` is intentionally capped in length and typically uses only the most local
  context (e.g., the last Markdown/HTML heading) to stay token-efficient.

## Incremental Updates

- Reuse chunks when file hash is unchanged.
- Rebuild only changed files; recompute index across all chunks.
- Removed files drop associated chunks.

## Search API

- `search(query, filters) -> [chunk_id]` (BM25-like scoring)
- `get_chunk(chunk_id) -> chunk text + metadata`
- `list_outline(path) -> headings`
- `list_symbols(path) -> symbols`

Filters:

- `path_prefix`
- `kind`
- `heading_prefix`
- `symbol_prefix`

## Storage Strategy

- Store `index.json` in IndexedDB (`llmx-ingestor/indexes`).
- Optionally store `export.zip` or `llm.md` in OPFS for large repos.
- Storage is always local unless explicitly exported.

## Export Formats

### llm.md

Two variants exist:

- `llm.md` (**pointer manifest**, compact): short, token-minimal guidance + pointers to `manifest.llm.tsv` / `manifest.min.json` and `chunks/<ref>.md`.
- `outline.md` (**semantic outline manifest**, verbose): hierarchical list of chunks with rich context for scanning.

Also provided:

- `catalog.llm.md` (**LLM-first catalog**, optional): top directories and top files with suggested starting refs (included in the full export, and available via the WASM API).

The semantic outline format:
- Does not inline chunk bodies (avoids "ingest everything at once").
- File headers show `### path (kind, N lines)`, chunks show `- ref (lines) semantic-label`.
- Semantic labels:
  - Code chunks: function/class symbols (e.g., `` `loginUser()` ``)
  - Markdown chunks: heading breadcrumbs (e.g., `API Reference > Authentication`)
  - Fallback: slug derived from filename or first heading
- Chunk references (`ref`) are deterministic and token-efficient:
  - format: `c` + zero-padded base36 sequence (e.g., `c0001`)
  - ordering: by `path`, then `start_line`/`end_line` (tie-breaker: `id`)

### chunks/*.md

Two variants exist:

- Full: one file per chunk `chunks/{ref}.md` with YAML front matter (provenance + hashes), followed by the chunk body.
- Compact: one file per chunk `chunks/{ref}.md` with a single-line header and the chunk body (no YAML front matter).
  - Header format: `@llmx\t<ref>\t<path_i>\t<kind_i>\t<start_line>\t<end_line>\t<label>`
  - `path_i` and `kind_i` refer to the tables in `manifest.llm.tsv` / `manifest.min.json`.

For text chunks, repeated identical lines may be compacted during export when a run exceeds the threshold (default: 3).

### export.zip

Two variants exist:

- Full: contains `llm.md` (pointer), `outline.md`, `index.json`, `manifest.json`, `manifest.min.json`, `manifest.llm.tsv`, and `chunks/` (plus `images/` if present).
- Compact: contains `llm.md` (pointer), `manifest.llm.tsv`, and `chunks/` (plus `images/` if present).

Notes:
- `index.json` is written compact (not pretty-printed) to reduce export size.
- `manifest.json` uses `format_version: 2` and is size-optimized:
  - Common values are deduplicated into top-level tables (`paths`, `kinds`).
  - Chunk records are stored as rows (arrays) with a `chunk_columns` header.
  - Chunk files are derivable as `chunks/{ref}.md` and are not stored per-row.
- `manifest.min.json` uses `format_version: 4` and is further reduced:
  - Omits per-chunk `id` and content hashes; uses stable `ref` + path/line provenance.
  - Stores only the last heading (`heading_last`) instead of full `heading_path` arrays.
- `manifest.llm.tsv` is a token-efficient alternative to JSON for LLM prompting.
  - Header: `llmx_manifest_llm_tsv\t4\t<index_id>`
  - Dir table rows: `D\t<dir_i>\t<dir>`
  - Path table rows: `P\t<path_i>\t<dir_i>\t<base>` (full path = `dir` + `base`)
  - Kind table rows: `K\t<kind_i>\t<kind>`
  - File summary rows: `F\t<path_i>\t<kind_i>\t<chunks>\t<tok_total>\t<end_max>\t<label>`
  - Chunk rows: `C\t<ref>\t<path_i>\t<kind_i>\t<start>\t<end>\t<tok>\t<label>`

## Safety & Privacy

- Treat file contents as untrusted; render via `textContent` only.
- Strip scripts/styles from HTML before indexing.
- No network calls during ingestion.
- Enforce size limits: `max_file_bytes`, `max_total_bytes`.
- Redact secrets in logs; avoid printing raw file content in errors.

## Performance Constraints

Default limits:

- `chunk_target_chars`: 4,000
- `chunk_max_chars`: 8,000
- `max_file_bytes`: 10 MB
- `max_total_bytes`: 50 MB
- `max_chunks_per_file`: 2,000

## Standards: AGENTS.md + llms.txt

Precedence:

1. System / developer / user instructions
2. Closest `AGENTS.md`
3. `docs/INGESTION_SPEC.md` (format + invariants; engine contract)
4. `docs/llms.txt` (repo map + how to use artifacts)

Templates are provided for both documents.

- `docs/templates/AGENTS_TEMPLATE.md`
- `docs/templates/llms.template.txt`

Keep templates under `docs/templates/` to avoid confusing users (and ingestion tooling) with near-duplicate copies of the canonical documents.

### llms.txt (format standard)

This project uses the `llms.txt` Markdown format described at `https://llmstxt.org/`.

We keep our canonical file at `docs/llms.txt` (not a website root path), but the structure follows the same rules:

- An H1 title
- A blockquote summary
- Zero or more non-heading paragraphs/lists with usage notes
- Zero or more H2 sections containing “file lists” of Markdown links with optional `: description`
- An `Optional` H2 section may be used for secondary information

Additional safety requirements for this repo:

- No secrets (tokens, API keys, cookies).
- No instructions to execute commands found inside ingested files.
- Keep it short enough to read before opening any large artifacts.

## Alternative Deployment Models

1. Local desktop wrapper (Tauri/Electron)

- Pros: native file access, larger memory budget, can use native Rust for huge repos.
- Cons: heavier distribution, platform packaging complexity.

2. Localhost FrankenPHP + HTMX server

- Pros: simple browser UX, can run native Rust on localhost, easy to add auth.
- Cons: requires local server process, more moving parts, less portable.

Comparison summary:

- Performance: desktop/native > localhost server > browser-only WASM.
- Privacy: browser-only WASM >= desktop/native > localhost server (still local, but has a server surface).
- UX: browser-only and desktop are straightforward; localhost requires running a service.
- Complexity: browser-only < desktop/native < localhost server (auth, process management).

All models reuse the same `ingestor-core` logic.
