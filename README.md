# ![search](.github/assets/icons/search-24x24.png) llmx

**Local-first codebase indexer with semantic search and chunk exports for agent consumption**

Transform large codebases into searchable, intelligently-chunked datasets with real neural-network embeddings running entirely on-device. llmx ships as a native MCP server and CLI: no server-side indexing, no code upload, and no data leaving your machine.

**Proof:** [7,147 files indexed in 31 MB -> 1,625 tokens retrieved (99.98% savings)](#-real-world-example-apple-hig-corpus) | [180+ tests](ingestor-core/tests/) | [MCP server](ingestor-core/src/bin/mcp_server.rs)

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Key Features

- **![search](.github/assets/icons/search-24x24.png) Neural Semantic Search** - Burn-powered `mdbr-leaf-ir` embeddings running on-device (CPU by default, optional GPU acceleration)
- **![lightning](.github/assets/icons/lightning-24x24.png) Hybrid Search** - Combines BM25 + vector search with RRF (Reciprocal Rank Fusion) for best results
- **![hierarchy](.github/assets/icons/hierarchy-24x24.png) Smart Chunking** - Deterministic chunking by file type (functions, headings, JSON keys)
- **![document](.github/assets/icons/businessicons-png-24-file-1-business-office-ui-24x24.png) Semantic Exports** - Hierarchical outline format with function names and heading breadcrumbs
- **![lock](.github/assets/icons/lock-24x24.png) Privacy-First** - Your code stays local; the embedding model ships with the binary, so indexing makes zero network calls
- **![download](.github/assets/icons/down-24x24.png) Agent-Ready** - MCP tools and exports designed for selective retrieval, not bulk ingestion

---

## The Problem

LLMs have limited context windows. Loading an entire codebase is:
- **Token-expensive** - Wastes context on irrelevant code
- **Slow** - Reading hundreds of files takes time
- **Inefficient** - Agents can't filter until after reading everything

## The Solution

llmx builds a **searchable index** with **semantic chunk exports** that enable agents to:

1. **Scan** the manifest (`llm.md`) to understand structure
2. **Search** for relevant concepts using BM25, vectors, or symbol lookup
3. **Retrieve** only the specific chunks needed
4. **Navigate** via function names, heading hierarchies, and the call/import graph

---

## ![hierarchy](.github/assets/icons/hierarchy-24x24.png) Semantic Outline Format

llmx exports token-efficient manifests (`llm.md` + `manifest.llm.tsv`) with semantic labels for intelligent chunk selection:

### Code Files
```
### src/auth.js (js, 47 lines)
- c0001 (1-15) `loginUser()`
- c0002 (17-30) `validateToken()`
- c0003 (32-47) `logout()`
```

### Markdown Documentation
```
### docs/api-reference.md (md, 234 lines)
- c0004 (1-45) API Reference
- c0005 (46-102) API Reference > Authentication
- c0006 (103-156) API Reference > Rate Limiting > Quotas
```

Agents can **scan headings, function names, and file types** to select relevant chunks--without opening any files.

---

## ![download](.github/assets/icons/down-24x24.png) Installation

### From <abbr title="Rust package registry">crates.io</abbr>

```bash
cargo install --locked llmx-mcp
```

### Homebrew (macOS/Linux)

```bash
brew install johnzfitch/llmx/llmx
```

### Arch Linux (<abbr title="Arch User Repository">AUR</abbr>)

```bash
yay -S llmx-bin   # or paru, pakku, etc.
```

All methods install both `llmx` (CLI) and `llmx-mcp` (MCP server).

<details>
<summary>MCP Server Setup (Claude Code, Cursor, etc.)</summary>

Create a `.mcp.json` in your project root for MCP discovery:

```bash
echo '{"mcpServers":{"llmx":{"command":"llmx-mcp"}}}' > .mcp.json
```

Then restart your MCP client. The first session auto-starts a shared backend on `localhost:19100` so multiple sessions share one index in memory instead of each loading its own copy.

| Environment Variable | Effect |
|---|---|
| `LLMX_PORT` | Override backend port (default `19100`) |
| `LLMX_NO_AUTOSTART=1` | Disable auto-start, run standalone per-session |
| `LLMX_STORAGE_DIR` | Override index storage location |

The server provides:

<dl>
  <dt><code>llmx_status</code></dt>
  <dd>Index readiness, file counts, and background task progress</dd>
  <dt><code>llmx_search</code></dt>
  <dd>Semantic / keyword / hybrid search with token-budgeted inline content</dd>
  <dt><code>llmx_lookup</code></dt>
  <dd>Exact or prefix symbol resolution by name</dd>
  <dt><code>llmx_refs</code></dt>
  <dd>Graph traversal &mdash; callers, callees, imports, type references</dd>
  <dt><code>llmx_explore</code> / <code>llmx_symbols</code> / <code>llmx_get_chunk</code> / <code>llmx_index</code> / <code>llmx_manage</code></dt>
  <dd>Structure browsing, symbol tables, full-chunk fetch, and index lifecycle</dd>
</dl>

</details>

<details>
<summary>Build from Source</summary>

```bash
git clone https://github.com/johnzfitch/llmx.git
cd llmx

# Default build: GPU-capable embeddings (wgpu/Metal/Vulkan) with CPU fallback
cargo build --release -p llmx-mcp --bin llmx --bin llmx-mcp

# CPU-only build: lighter and faster to compile, embeddings run on CPU
cargo build --release -p llmx-mcp --bin llmx --bin llmx-mcp \
  --no-default-features --features treesitter,mcp,mcp-http,cli,ndarray-backend
```

Binaries output to `target/release/llmx` and `target/release/llmx-mcp`. The embedding
model is committed under `ingestor-core/models/`, so builds are fully offline -- no model
download step.

</details>

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Quick Start

### CLI Usage

```bash
# Index a codebase
llmx index ./my-project

# Search with token budget
llmx search "authentication login" --limit 10 --max-tokens 4000

# Explore structure
llmx explore files
llmx explore symbols --path src/

# Export for agents
llmx export --format zip -o ./export.zip
```

### MCP Usage (agents)

Once registered (see *MCP Server Setup* above), an agent calls `llmx_status` to check
index readiness, then `llmx_search` / `llmx_lookup` / `llmx_refs` to retrieve only the
chunks it needs. Indexing runs as a background job and the server stays responsive,
returning the best available results as the index warms up.

---

## ![folder](.github/assets/icons/closed-folder-24x24.png) Usage

### For Agents

Give the agent an export bundle (compact, recommended):

```
llmx-export/
├── llm.md              # Compact pointer manifest (recommended)
├── manifest.llm.tsv    # Token-efficient chunk table for LLMs
└── chunks/
    ├── c0001.md        # Chunk body (minimal header + content)
    └── ...
```

**Agent workflow:**
1. Read `llm.md` for the compact workflow and artifact pointers
2. Scan `manifest.llm.tsv` to identify relevant files/chunks by label
3. Open only the matching `chunks/<ref>.md` files

### For Humans

- **Search** from the CLI or via your MCP-enabled editor
- **Export** for offline analysis
- **Share** the exported `*.llmx-<id8>.zip` bundle with team members (no server needed)

---

## ![gear](.github/assets/icons/gear-24x24.png) How It Works

```mermaid
flowchart LR
    A[Codebase] --> B[Chunker]
    B --> C[Index + BM25 + Embeddings]
    C --> D[llm.md manifest]
    D --> E[Agent queries]
    E --> F[Relevant chunks only]
```

### Chunking Strategy

llmx chunks files **deterministically by type**:

| File Type | Chunking Method |
|-----------|----------------|
| **JavaScript/TypeScript** | Function/class declarations (via tree-sitter or fallback) |
| **Rust / Python / Go / Java / C / C++ / C#** | Symbol-aware via tree-sitter |
| **Markdown** | Heading boundaries with ancestry preserved |
| **JSON** | Top-level keys or array ranges (max 50 elements) |
| **HTML** | Heading tags, scripts/styles stripped |
| **Text** | Paragraph boundaries |
| **Images** | Indexed by path, bytes included in export |

### Search

**Hybrid search** combining two approaches:

1. **BM25 (Keyword Search)** - TF/IDF with document-length normalization; fast lexical matching.
2. **Neural Semantic Search** - `mdbr-leaf-ir` via Burn (`768` dimensions, INT8 `Q8S` quantized), running on-device. Understands meaning, not just keywords.
3. **RRF Fusion** - Reciprocal Rank Fusion combines both rankings for results better than either method alone.

Embeddings run natively: CPU by default, or GPU-accelerated (wgpu over Metal/Vulkan/DX12)
when built with the `wgpu-backend` feature.

### Export Formats

| Format | Contents | Use Case |
|--------|----------|----------|
| **llm.md** | Semantic manifest with outline | Quick scanning, agent navigation |
| **manifest.json** | Optimized columnar format | Machine parsing, tooling |
| **index.json** | Full index + inverted index | Offline search, backup |
| **export.zip** | All above + chunk files + images | Complete portable package |

---

## ![document](.github/assets/icons/businessicons-png-24-file-1-business-office-ui-24x24.png) Real-World Example: Apple HIG Corpus

Tested on the **Apple Human Interface Guidelines archive** (1980-2009):

| Metric | Value |
|--------|-------|
| **Files** | 7,147 |
| **Chunks** | 21,369 |
| **Raw size** | 31 MB (~7.8M tokens) |

### Token Savings

| Access Method | Tokens | Savings |
|---------------|--------|---------|
| Read all files | ~7,800,000 | -- |
| Scan manifest (`llm.md`) | ~208,000 | **97%** |
| Targeted search (3 queries) | ~1,625 | **99.98%** |

The agent found relevant content spanning **4 decades** using **0.02%** of the total corpus tokens.

---

## ![gear](.github/assets/icons/gear-24x24.png) Technical Details

- **Language**: Rust
- **Architecture**: Native MCP server + CLI, with an auto-started local REST backend so multiple sessions share one in-memory index
- **ML Framework**: Burn (Rust-native)
- **Embedding Model**: `mdbr-leaf-ir` (`768`-dim output; native `f32` and `q8` artifacts committed under `ingestor-core/models/`)
- **Embedding Backends**: `ndarray` (CPU) or `wgpu` (GPU via Metal/Vulkan/DX12)
- **Storage**: On-disk index store (default `~/.local/share/llmx/indexes`, configurable via `LLMX_STORAGE_DIR`)
- **Search**: Hybrid (BM25 + neural embeddings) with RRF fusion
- **Chunking**: Deterministic, content-hash based IDs
- **Integrity**: The model id/SHA-256 is derived at build time; indexes record the model they were built with and reject mismatches (re-index after a model change)

---

## ![folder](.github/assets/icons/closed-folder-24x24.png) Development

### Project Structure

```
llmx/
├── ingestor-core/      # Rust crate: chunking, indexing, search, RRF, MCP + CLI
│   ├── src/
│   │   ├── index.rs         # Indexing + hybrid search
│   │   ├── chunk/           # Per-language chunkers (tree-sitter)
│   │   ├── embeddings*.rs    # Native Burn embeddings
│   │   ├── mcp/             # MCP server tools
│   │   └── bin/
│   │       ├── mcp_server.rs # llmx-mcp (MCP server + REST backend)
│   │       └── llmx.rs       # llmx (CLI)
│   ├── models/         # Committed mdbr-leaf-ir artifacts (f32 + q8 + tokenizer)
│   └── build.rs        # Verifies committed model + emits model id/sha256
├── docs/               # Specifications and usage guides
└── pkg/                # Packaging (Homebrew, AUR)
```

### Build & Test

```bash
# Core library tests
cargo test --package ingestor-core

# CLI integration tests
cargo test --features cli

# MCP protocol tests
cargo test --features mcp
```

180+ tests covering token savings, CLI commands, MCP protocol, edge cases, and all 30+ file types.

---

## ![lock](.github/assets/icons/lock-24x24.png) Privacy & Security

- **Zero network calls during indexing** - Your code never leaves your machine, and the embedding model ships with the binary (no runtime download)
- **No external dependencies** for core indexing functionality
- **Content treated as untrusted** - prompt-injection-resistant handling
- **Deterministic output** - Same input = same index every time
- **Model integrity verification** - the build derives a SHA-256 for the committed model; indexes are tagged with their model and refuse to serve semantic results on a mismatch

---

<details>
<summary><strong>Export Format Details</strong></summary>

## ![download](.github/assets/icons/down-24x24.png) Export Details

### llm.md Format (v1.0)

**Header:**
```markdown
# llm.md (pointer manifest)

Index ID: <sha256>
Files: 42  Chunks: 187

Chunk files live under `chunks/` and are named `{ref}.md`.
Prefer search to find refs, then open only the referenced chunk files.
```

**File sections:**
```markdown
### src/utils.ts (js, 89 lines)
- abc123def (1-20) `parseDate()`
- ghi456jkl (22-45) `formatCurrency()`
```

**Chunk files** (`chunks/<ref>.md`):
```yaml
---
ref: abc123def
id: <full-sha256>
slug: parseDate
path: src/utils.ts
kind: java_script
lines: [1, 20]
token_estimate: 145
heading_path: []
symbol: parseDate
---

export function parseDate(input) {
  // ... function body
}
```

</details>

---

## License

MIT License - See LICENSE file for details

---

## Contributing

Contributions welcome! Please:

1. Read the specs under [docs/](docs/) for architecture
2. Check existing issues before opening new ones
3. Run tests before submitting PRs
4. Follow the existing code style

---

**Made for agents, by humans** ![lightning](.github/assets/icons/lightning-24x24.png)
