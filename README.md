# ![search](.github/assets/icons/search-24x24.png) llmx

**Local-first codebase indexer with semantic chunk exports for agent consumption**

Transform large codebases into searchable, intelligently-chunked datasets that agents can navigate efficiently without loading everything into context.

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Key Features

- **![search](.github/assets/icons/search-24x24.png) BM25 Search** - Algorithm-first retrieval without embeddings, fully in-browser
- **![folder](.github/assets/icons/closed-folder-24x24.png) Smart Chunking** - Deterministic chunking by file type (functions, headings, JSON keys)
- **![text](.github/assets/icons/businessicons-png-24-file-1-business-office-ui-24x24.png) Semantic Exports** - Hierarchical outline format with function names and heading breadcrumbs
- **![lock](.github/assets/icons/lock-24x24.png) Privacy-First** - Zero network calls, all processing in-browser via WASM
- **![lightning](.github/assets/icons/lightning-24x24.png) Fast** - Sub-second indexing and search for typical repositories
- **![download](.github/assets/icons/downloads-folder-24x24.png) Agent-Ready** - Exports designed for selective retrieval, not bulk ingestion

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

## ![text](.github/assets/icons/businessicons-png-24-file-1-business-office-ui-24x24.png) Semantic Outline Format

llmx exports an **llm.md manifest** with rich semantic context for intelligent chunk selection:

### Code Files
```
### src/auth.js (js, 47 lines)
- a1b2c3d4 (1-15) `loginUser()`
- e5f6g7h8 (17-30) `validateToken()`
- x9y0z1a2 (32-47) `logout()`
```

### Markdown Documentation
```
### docs/api-reference.md (md, 234 lines)
- p4q5r6s7 (1-45) API Reference
- t8u9v0w1 (46-102) API Reference > Authentication
- a2b3c4d5 (103-156) API Reference > Rate Limiting > Quotas
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
4. **Export** → Download `export.zip` with `llm.md` + `chunks/*.md`

---

## ![folder](.github/assets/icons/closed-folder-24x24.png) Usage

### For Agents

Give the agent the **export directory**:

```
llmx-export/
├── llm.md              # Semantic manifest (scan this first)
├── manifest.json       # Machine-readable index
├── index.json          # Full index with inverted index
└── chunks/
    ├── a1b2c3d4.md     # Individual chunk files
    ├── e5f6g7h8.md
    └── ...
```

**Agent workflow:**
1. Read `llm.md` to understand structure
2. Scan for relevant symbols/headings (e.g., `loginUser()`, `Authentication`)
3. Open only the matching `chunks/<ref>.md` files
4. Use `manifest.json` or `index.json` for programmatic search

### For Humans

- **Browse** the web UI for real-time search
- **Export** for offline analysis
- **Share** `export.zip` with team members (no server needed)

---

## ![database](.github/assets/icons/table-24x24.png) How It Works

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

**BM25-style ranking** with:
- Term frequency (TF)
- Inverse document frequency (IDF)
- Document length normalization
- No embeddings required

Runs fully client-side in **WASM**.

### Export Formats

| Format | Contents | Use Case |
|--------|----------|----------|
| **llm.md** | Semantic manifest with outline | Quick scanning, agent navigation |
| **manifest.json** | Optimized columnar format | Machine parsing, tooling |
| **index.json** | Full index + inverted index | Offline search, backup |
| **export.zip** | All above + chunk files + images | Complete portable package |

---

## ![lightning](.github/assets/icons/lightning-24x24.png) Technical Details

- **Language**: Rust (core), JavaScript (WASM bindings, web UI)
- **Architecture**: Client-only, no server required
- **Storage**: IndexedDB (persistent) or in-memory
- **Search**: Custom BM25 implementation, inverted index
- **Chunking**: Deterministic, content-hash based IDs
- **Performance**: ~500ms for 10MB codebase on modern hardware

### Browser Compatibility

- **Chromium** (Chrome, Edge): Full support (`showDirectoryPicker`)
- **WebKit** (Safari): Folder input via `webkitdirectory`
- **Firefox**: File selection or drag-and-drop (no folder picker)

Module workers fall back to main thread if unavailable.

---

## ![folder](.github/assets/icons/closed-folder-24x24.png) Development

### Project Structure

```
llmx/
├── ingestor-core/      # Rust library (chunking, indexing, export)
├── ingestor-wasm/      # WASM bindings
├── web/                # Browser UI
│   ├── app.js          # Main UI logic
│   ├── worker.js       # Web Worker for WASM
│   └── pkg/            # Built WASM artifacts
└── docs/               # Specifications and usage guides
```

### Build

```bash
# Build WASM
cd ingestor-wasm
wasm-pack build --target web

# Run tests
cd ingestor-core
cargo test
```

### Testing

```bash
cargo test --package ingestor-core --lib --tests
```

All 11 tests pass, including format validation and semantic context verification.

---

## ![lock](.github/assets/icons/lock-24x24.png) Privacy & Security

- **No network calls** - All processing local
- **No external dependencies** for indexing
- **Content treated as untrusted** - Prompt injection resistant UI
- **Deterministic output** - Same input = same index every time

---

## ![download](.github/assets/icons/downloads-folder-24x24.png) Export Details

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

- [ ] Add LSP/tree-sitter symbol extraction for more languages
- [ ] Support image OCR for screenshot indexing
- [ ] Add CLI for headless indexing
- [ ] MCP server for external agent retrieval
- [ ] Support for more file types (Python, Go, Rust, etc.)

---

**Made for agents, by humans** ![lightning](.github/assets/icons/lightning-24x24.png)
