# ![search](.github/assets/icons/search-24x24.png) llmx

**Local-first codebase indexer with semantic search and chunk exports for agent consumption**

Transform large codebases into searchable, intelligently-chunked datasets with real neural network embeddings running entirely in your browser via WebGPU. No server, no API calls, no data leaving your machine.

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Key Features

- **![search](.github/assets/icons/search-24x24.png) Neural Semantic Search** - Snowflake Arctic embeddings with WebGPU acceleration, same quality as server-side solutions
- **![lightning](.github/assets/icons/lightning-24x24.png) Hybrid Search** - Combines BM25 + vector search with RRF (Reciprocal Rank Fusion) for best results
- **![hierarchy](.github/assets/icons/hierarchy-24x24.png) Smart Chunking** - Deterministic chunking by file type (functions, headings, JSON keys)
- **![document](.github/assets/icons/businessicons-png-24-file-1-business-office-ui-24x24.png) Semantic Exports** - Hierarchical outline format with function names and heading breadcrumbs
- **![lock](.github/assets/icons/lock-24x24.png) Privacy-First** - Zero network calls, all processing in-browser via WASM
- **![lightning](.github/assets/icons/lightning-24x24.png) Fast** - Sub-second indexing, ~50ms embedding inference with GPU acceleration
- **![download](.github/assets/icons/down-24x24.png) Agent-Ready** - Exports designed for selective retrieval, not bulk ingestion

---

## The Problem

LLMs have limited context windows. Loading an entire codebase is:
- **Token-expensive** - Wastes context on irrelevant code
- **Slow** - Reading hundreds of files takes time
- **Inefficient** - Agents can't filter until after reading everything

## The Solution

llmx builds a **searchable index** with **semantic chunk exports** that enable agents to:

1. **Scan** the manifest (`llm.md`) to understand structure
2. **Search** for relevant concepts using BM25
3. **Retrieve** only the specific chunks needed
4. **Navigate** via function names, heading hierarchies, and file paths

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

Agents can **scan headings, function names, and file types** to select relevant chunks—without opening any files.

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Quick Start

### Build WASM

```bash
cd ingestor-wasm
wasm-pack build --target web --out-dir ../web/pkg
```

### Run Web UI

```bash
python3 -m http.server 8001 --bind 127.0.0.1 --directory web
```

Open `http://127.0.0.1:8001` in your browser.

### Index a Codebase

1. **Select folder** (Chromium) or **drag files** (Firefox/all browsers)
2. **Wait for indexing** (sub-second for typical repos)
3. **Search** using the query box
4. **Export** → Download an export bundle named after the selected folder (e.g. `my-repo.llmx-1a2b3c4d.zip`)

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
4. Download `*.index.json` from the UI only if you need the full index structure

### For Humans

- **Browse** the web UI for real-time search
- **Export** for offline analysis
- **Share** the downloaded `*.llmx-<id8>.zip` bundle with team members (no server needed)

---

## ![gear](.github/assets/icons/gear-24x24.png) How It Works

### Chunking Strategy

llmx chunks files **deterministically by type**:

| File Type | Chunking Method |
|-----------|----------------|
| **JavaScript/TypeScript** | Function/class declarations (via tree-sitter or fallback) |
| **Markdown** | Heading boundaries with ancestry preserved |
| **JSON** | Top-level keys or array ranges (max 50 elements) |
| **HTML** | Heading tags, scripts/styles stripped |
| **Text** | Paragraph boundaries |
| **Images** | Indexed by path, bytes included in export |

### Search

**Hybrid search** combining two approaches:

1. **BM25 (Keyword Search)**:
   - Term frequency (TF)
   - Inverse document frequency (IDF)
   - Document length normalization
   - Fast lexical matching

2. **Neural Semantic Search**:
   - Snowflake Arctic Embed (384 dimensions, INT8 quantized)
   - WebGPU-accelerated inference (~50ms per query)
   - Falls back to CPU → hash-based → BM25-only
   - Understands meaning, not just keywords

3. **RRF Fusion**:
   - Reciprocal Rank Fusion combines both rankings
   - Weighted blending for optimal results
   - Better than either method alone

Runs fully client-side in **WASM** with GPU acceleration.

### Export Formats

| Format | Contents | Use Case |
|--------|----------|----------|
| **llm.md** | Semantic manifest with outline | Quick scanning, agent navigation |
| **manifest.json** | Optimized columnar format | Machine parsing, tooling |
| **index.json** | Full index + inverted index | Offline search, backup |
| **export.zip** | All above + chunk files + images | Complete portable package |

---

## ![gear](.github/assets/icons/gear-24x24.png) Technical Details

- **Language**: Rust (core), JavaScript (WASM bindings, web UI)
- **Architecture**: Client-only, no server required
- **ML Framework**: Burn 0.20 (compiles to WASM)
- **Embedding Model**: Snowflake Arctic Embed Small (384-dim, INT8 quantized to ~9MB)
- **Storage**: IndexedDB (persistent) or in-memory
- **Search**: Hybrid (BM25 + neural embeddings) with RRF fusion
- **Chunking**: Deterministic, content-hash based IDs
- **Performance**:
  - Indexing: ~500ms for 10MB codebase
  - Embeddings: ~50ms per query (WebGPU)
  - Package: 2.4 MB WASM (model weights loaded separately from CDN)

### Browser Compatibility

#### File Selection
- **Chromium** (Chrome, Edge): Full support (`showDirectoryPicker`)
- **WebKit** (Safari): Folder input via `webkitdirectory`
- **Firefox**: File selection or drag-and-drop (no folder picker)

#### Semantic Search
- **WebGPU** (Chrome 113+, Edge 113+): GPU-accelerated embeddings (~50ms)
- **CPU Fallback**: All modern browsers with WASM support (~100-200ms)
- **Hash Fallback**: Universal compatibility (deterministic, instant)
- **BM25 Only**: Always available as final fallback

Module workers fall back to main thread if unavailable.

---

## ![folder](.github/assets/icons/closed-folder-24x24.png) Development

### Project Structure

```
llmx/
├── ingestor-core/      # Rust library (chunking, indexing, search, RRF)
│   ├── src/
│   │   ├── chunk.rs         # Chunking logic
│   │   ├── index.rs         # Indexing + hybrid search
│   │   ├── rrf.rs           # Reciprocal Rank Fusion
│   │   ├── embeddings.rs    # Embedding abstractions
│   │   └── mcp/             # MCP server tools
├── ingestor-wasm/      # WASM bindings + Burn embeddings
│   ├── src/
│   │   ├── lib.rs           # WASM exports
│   │   └── embeddings_burn.rs  # Burn-based neural embeddings
│   ├── build.rs         # ONNX model download & conversion
│   └── .cargo/          # WASM build configuration
├── web/                # Browser UI
│   ├── app.js          # Main UI logic
│   ├── worker.js       # Web Worker for WASM
│   └── pkg/            # Built WASM artifacts
└── docs/               # Specifications and usage guides
```

### Build

```bash
# Set model URL (required for WASM builds with embeddings)
export LLMX_EMBEDDING_MODEL_URL="https://your-cdn.com/arctic-embed-s-q8.bin"

# Build WASM (includes neural embedding support)
cd ingestor-wasm
wasm-pack build --target web --release

# Development build (faster, larger)
wasm-pack build --target web --dev

# Run tests
cd ingestor-core
cargo test
```

**Security Note on Model URLs:**
- The `LLMX_EMBEDDING_MODEL_URL` is embedded into the WASM binary at build time
- Use **only public, non-authenticated URLs** (e.g., HuggingFace, public CDN)
- Never use signed URLs or URLs with authentication tokens
- The URL will be visible to anyone inspecting the WASM binary
- For production, host models on a public CDN or use HuggingFace directly

**Build-time Model Download:**
The build script automatically downloads and converts the model:
1. Downloads safetensors from HuggingFace (if not cached)
2. Converts to Burn binary format with INT8 quantization
3. Stores in `ingestor-wasm/models/` directory
4. Model is loaded at runtime from the CDN URL specified above

### Testing

```bash
cargo test --package ingestor-core --lib --tests
```

All 11 tests pass, including format validation and semantic context verification.

---

## ![lock](.github/assets/icons/lock-24x24.png) Privacy & Security

### Runtime Privacy
- **Zero network calls during indexing** - Your code never leaves your machine
- **No external dependencies** for core indexing functionality
- **Content treated as untrusted** - Prompt injection resistant UI
- **Deterministic output** - Same input = same index every time
- **IndexedDB caching** - Model weights cached locally after first download

### Build-Time Security
- **Model URLs embedded at build time** - URLs are visible in WASM binary
  - Only use **public, non-authenticated URLs** for model sources
  - Current setup uses public HuggingFace model repositories
  - Never embed signed URLs or authentication tokens
- **Model integrity verification** - SHA-256 validation prevents tampering (planned)
- **Supply chain security** - Models loaded from trusted sources (HuggingFace)
- **Quantization** - INT8 quantization reduces model size with minimal quality loss

### Security Notes
⚠️ **WASM binaries are inspectable** - Any URLs or constants in the build are visible to users. This is by design for transparency, but means secrets must never be embedded. Our current architecture uses only public model repositories and is safe for production use.

---

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

---

## License

MIT License - See LICENSE file for details

---

## Contributing

Contributions welcome! Please:

1. Read the [INGESTION_SPEC.md](docs/INGESTION_SPEC.md) for architecture
2. Check existing issues before opening new ones
3. Run tests before submitting PRs
4. Follow the existing code style

---

## Roadmap

### Phase 6 (Complete)
- [x] Neural semantic search with Burn framework
- [x] WebGPU-accelerated embeddings
- [x] Hybrid search with RRF fusion
- [x] WASM build pipeline
- [x] Model quantization (INT8)
- [x] IndexedDB caching for models

### Phase 7 (Current - Security Hardening)
- [ ] Implement SHA-256 model integrity verification
- [ ] Add download size limits and rate limiting
- [ ] Improve error handling in MCP server (remove panics)
- [ ] Add cancellation support for async operations
- [ ] Browser integration testing across all platforms

### Future Phases
- [ ] Performance optimizations (fused QKV, attention mask broadcasting)
- [ ] Model configuration flexibility (support multiple model sizes)
- [ ] Add LSP/tree-sitter symbol extraction for more languages
- [ ] Support image OCR for screenshot indexing
- [ ] Add CLI for headless indexing
- [ ] MCP server hardening and production deployment
- [ ] Support for more file types (Python, Go, Rust, etc.)

---

**Made for agents, by humans** ![lightning](.github/assets/icons/lightning-24x24.png)
