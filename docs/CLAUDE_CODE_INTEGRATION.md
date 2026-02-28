# llmx + Claude Code Integration

> Token-efficient codebase exploration via semantic chunking

## Overview

llmx creates searchable semantic chunks from codebases. When integrated with Claude Code, Explore agents can search a pre-built index instead of reading files one-by-one, reducing token consumption by **94%**.

```
Without llmx:  Agent reads 50 files → 84,000 tokens, 45 API turns
With llmx:     Agent searches index → 5,000 tokens, 8 API turns
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Claude Code                               │
├─────────────────────────────────────────────────────────────────┤
│  Explore Agent                                                   │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────────┐  │
│  │ Check index │───▶│ Search chunks│───▶│ Read specific file│  │
│  └─────────────┘    └──────────────┘    └───────────────────┘  │
│         │                  │                                     │
│         ▼                  ▼                                     │
│  ~/.claude/indexes/   llmx MCP Server                           │
│  └── project.md       └── mcp__llmx__search                     │
│                       └── mcp__llmx__read_chunk                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                         llmx                                     │
├─────────────────────────────────────────────────────────────────┤
│  CLI                           MCP Server                        │
│  ┌──────────────────┐         ┌──────────────────┐              │
│  │ llmx index       │         │ llmx serve       │              │
│  │ llmx export      │         │   --index-dir    │              │
│  │ llmx search      │         │   --port 8080    │              │
│  └──────────────────┘         └──────────────────┘              │
│           │                            │                         │
│           ▼                            ▼                         │
│  ~/.claude/indexes/           stdio/SSE transport               │
│  └── <project>/                                                  │
│      ├── manifest.md          Tools exposed:                     │
│      ├── chunks/              - llmx_search(query)              │
│      │   ├── 0001.md          - llmx_read_chunk(id)             │
│      │   ├── 0002.md          - llmx_list_symbols()             │
│      │   └── ...                                                 │
│      └── symbols.json                                            │
└─────────────────────────────────────────────────────────────────┘
```

## Scaffolded Directory Structure

```
~/.claude/
├── indexes/                          # Pre-built project indexes
│   ├── llmx.md                       # Lightweight manifest (generate-manifest.sh)
│   ├── bartender.md
│   └── llmx/                         # Full llmx index (llmx index)
│       ├── manifest.md               # Project overview + chunk references
│       ├── symbols.json              # Function/class index with chunk IDs
│       └── chunks/
│           ├── 0001.md               # Semantic chunk: src/main.rs
│           ├── 0002.md               # Semantic chunk: src/lib.rs (part 1)
│           ├── 0003.md               # Semantic chunk: src/lib.rs (part 2)
│           └── ...
│
├── hooks/
│   ├── explore-guard                 # Blocks find/grep-r, suggests llmx
│   ├── read-compress                 # Compresses large reads for Explore agents
│   ├── generate-manifest.sh          # Lightweight indexer (tokei/tree/rg)
│   └── llmx-index                    # (future) Auto-index on SessionStart
│
├── mcp.json                          # MCP server configuration
│   └── llmx server entry
│
└── settings.json
    └── permissions for llmx tools

~/dev/llmx/
├── src/
│   ├── main.rs                       # CLI entry point
│   ├── lib.rs                        # Core library
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── index.rs                  # llmx index command
│   │   ├── export.rs                 # llmx export command
│   │   ├── search.rs                 # llmx search command
│   │   └── serve.rs                  # llmx serve (MCP server)
│   ├── indexer/
│   │   ├── mod.rs
│   │   ├── chunker.rs                # Semantic chunking logic
│   │   ├── symbols.rs                # Symbol extraction
│   │   └── manifest.rs               # Manifest generation
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── server.rs                 # MCP protocol handler
│   │   ├── tools.rs                  # Tool definitions
│   │   └── transport.rs              # stdio/SSE transports
│   └── search/
│       ├── mod.rs
│       └── semantic.rs               # Semantic search (embeddings optional)
├── Cargo.toml
└── docs/
    └── CLAUDE_CODE_INTEGRATION.md    # This file
```

## Integration Points

### 1. CLI Commands

```bash
# Index a project (generates chunks + manifest)
llmx index ~/dev/project --output ~/.claude/indexes/project/

# Quick export (single llm.md file, no chunks)
llmx export ~/dev/project > ~/.claude/indexes/project.md

# Search an index
llmx search ~/.claude/indexes/project/ "authentication handler"

# Start MCP server
llmx serve --index-dir ~/.claude/indexes --stdio
```

### 2. MCP Server Configuration

Add to `~/.claude/mcp.json`:

```json
{
  "mcpServers": {
    "llmx": {
      "type": "stdio",
      "command": "/home/zack/dev/llmx/target/release/mcp_server",
      "args": [],
      "env": {
        "LLMX_STORAGE_DIR": "/home/zack/.llmx/indexes"
      }
    }
  }
}
```

### 3. MCP Tools (Already Implemented)

| Tool | Description | Key Parameters |
|------|-------------|----------------|
| `llmx_index` | Create/update index from file paths | `paths: string[]`, `options?: {chunk_target_chars, max_file_bytes}` |
| `llmx_search` | Search with token-budgeted inline content | `index_id: string`, `query: string`, `max_tokens?: number` (default 16K), `use_semantic?: bool` |
| `llmx_explore` | List files, outline, or symbols | `index_id: string`, `mode: "files"\|"outline"\|"symbols"`, `path_filter?: string` |
| `llmx_manage` | List or delete indexes | `action: "list"\|"delete"`, `index_id?: string` |

#### Search Response Format

```json
{
  "results": [
    {
      "chunk_id": "abc123",
      "score": 0.85,
      "path": "src/auth/handler.rs",
      "start_line": 45,
      "end_line": 120,
      "content": "pub async fn authenticate...",
      "symbol": "authenticate",
      "heading_path": ["Authentication", "Handlers"]
    }
  ],
  "truncated_ids": ["def456", "ghi789"],
  "total_matches": 15
}
```

### 4. Hook Integration

Update `~/.claude/hooks/explore-guard` to suggest llmx:

```bash
# Add after existing checks:

# Suggest llmx if index exists
PROJECT_NAME=$(basename "$PWD")
INDEX_DIR="$HOME/.claude/indexes/$PROJECT_NAME"
if [[ -d "$INDEX_DIR" ]] && [[ "$COMMAND" =~ (rg|grep|fd|find) ]]; then
    echo "TIP: llmx index exists. Use: mcp__llmx__search 'your query'" >&2
    # Don't block, just suggest
fi
```

### 5. SessionStart Auto-Index

Add to `~/.claude/hooks/session-start`:

```bash
#!/bin/bash
# Auto-index current project if not recently indexed

PROJECT_NAME=$(basename "$PWD")
INDEX_DIR="$HOME/.claude/indexes/$PROJECT_NAME"
MANIFEST="$INDEX_DIR/manifest.md"

# Skip if indexed within last 24 hours
if [[ -f "$MANIFEST" ]]; then
    AGE=$(( $(date +%s) - $(stat -c %Y "$MANIFEST") ))
    (( AGE < 86400 )) && exit 0
fi

# Background index (don't block session start)
if command -v llmx &>/dev/null; then
    llmx index "$PWD" --output "$INDEX_DIR" &>/dev/null &
fi
```

## Chunk Format

Each chunk file follows a consistent format:

```markdown
---
id: abc123def
file: src/auth/handler.rs
lines: 45-120
symbols: [authenticate, verify_token, AuthError]
dependencies: [src/auth/types.rs, src/db/users.rs]
---

# authenticate

Handles user authentication via JWT tokens.

## Code

\`\`\`rust
pub async fn authenticate(req: &Request) -> Result<User, AuthError> {
    let token = extract_token(req)?;
    let claims = verify_token(&token)?;
    // ...
}
\`\`\`

## References

- Called by: `src/routes/api.rs:handle_request`
- Calls: `verify_token`, `extract_token`
```

## Agent Workflow

### Before llmx

```
1. Glob("**/*.rs")                    → 47 files
2. Read("src/main.rs")                → 500 lines
3. Read("src/lib.rs")                 → 800 lines
4. Grep("authenticate")               → 12 matches
5. Read("src/auth/mod.rs")            → 200 lines
6. Read("src/auth/handler.rs")        → 300 lines
... 40 more reads ...
Total: 45 API turns, 84,000 tokens
```

### With llmx

```
1. mcp__llmx__get_manifest("project") → Overview + structure
2. mcp__llmx__search("authenticate")  → 3 relevant chunks
3. mcp__llmx__read_chunk("abc123")    → Just the auth handler
4. Read("src/auth/handler.rs", 45-60) → Specific lines if needed
Total: 4-8 API turns, 5,000 tokens
```

## Current State

llmx already has significant infrastructure:

```
ingestor-core/src/
├── bin/
│   └── mcp_server.rs       ✓ MCP server binary
├── mcp/
│   ├── mod.rs              ✓ MCP module
│   ├── storage.rs          ✓ Index storage layer
│   └── tools.rs            ✓ Tool definitions
├── chunk.rs                ✓ Semantic chunking
├── embeddings.rs           ✓ Embedding generation
├── export.rs               ✓ Export functionality
├── index.rs                ✓ Indexing logic
└── lib.rs                  ✓ Core library

ingestor-wasm/models/
├── arctic-embed-s.safetensors    ✓ Pre-trained model
├── bge-small-en-v1.5.onnx        ✓ ONNX model
└── tokenizer.json                ✓ Tokenizer
```

## Implementation Roadmap

### Phase 1: MCP Server ✅ COMPLETE
- [x] `mcp_server.rs` - Full stdio MCP server
- [x] `llmx_index` - Create/update indexes from paths
- [x] `llmx_search` - Token-budgeted search (16K default)
- [x] `llmx_explore` - List files/outline/symbols
- [x] `llmx_manage` - List/delete indexes
- [x] Index persistence in ~/.llmx/indexes/
- [x] In-memory cache for performance

### Phase 2: Claude Code Hooks ✅ COMPLETE
- [x] `explore-guard` - Blocks find/grep-r for Explore agents
- [x] `read-compress` - Summarizes large file reads
- [x] `generate-manifest.sh` - Lightweight project overview
- [x] CLAUDE.md updated with explore agent guidance
- [x] Permissions for tree, tokei added

### Phase 3: Integration (TODO)
- [ ] Add llmx to ~/.claude/mcp.json
- [ ] Test MCP server with Claude Code
- [ ] Add SessionStart auto-index hook
- [ ] Update explore-guard to suggest llmx when index exists

### Phase 4: Optional Enhancements
- [x] Semantic search with embeddings (use_semantic flag)
- [x] Pre-trained models (arctic-embed-s, bge-small)
- [ ] CLI wrapper for headless indexing
- [ ] Hybrid BM25 + semantic ranking tuning

## Configuration

### Environment Variables

```bash
LLMX_INDEX_DIR=~/.claude/indexes    # Default index location
LLMX_CHUNK_SIZE=500                  # Lines per chunk (default)
LLMX_IGNORE_PATTERNS=node_modules,.git,target,dist
```

### Project-Specific Config

Create `.llmx.toml` in project root:

```toml
[index]
chunk_size = 300
include = ["src/**", "lib/**"]
exclude = ["**/*_test.rs", "**/*.generated.*"]

[symbols]
extract = ["function", "class", "struct", "trait", "interface"]

[output]
format = "markdown"  # or "json"
```

## Metrics

Track these to measure improvement:

```bash
# After running an Explore agent:
jq -r '.message.content[]? | select(.type == "tool_use") | .name' \
    ~/.claude/projects/*/subagents/agent-*.jsonl | sort | uniq -c

# Token count per agent
wc -c ~/.claude/projects/*/subagents/agent-*.jsonl

# Target metrics:
# - <8 API turns per Explore agent
# - <5,000 tokens per Explore agent
# - 0 Glob/Grep calls (replaced by llmx_search)
```

## Quick Start

```bash
# 1. Build MCP server
cd ~/dev/llmx
cargo build --release --features mcp -p ingestor-core

# 2. Create storage directory
mkdir -p ~/.llmx/indexes

# 3. Add MCP server to Claude Code
# Edit ~/.claude/mcp.json (create if doesn't exist):
{
  "mcpServers": {
    "llmx": {
      "type": "stdio",
      "command": "/home/zack/dev/llmx/target/release/mcp_server",
      "args": [],
      "env": {
        "LLMX_STORAGE_DIR": "/home/zack/.llmx/indexes"
      }
    }
  }
}

# 4. Restart Claude Code to load MCP server
# Then use in conversation:
#   "Index ~/dev/myproject"  → calls llmx_index
#   "Search for authentication"  → calls llmx_search
#   "List symbols in the project"  → calls llmx_explore

# 5. Verify with claude --mcp-debug
claude --mcp-debug  # Shows loaded MCP servers
```

### Usage Examples

```
User: Index my llmx project
Agent: [calls mcp__llmx__llmx_index with paths=["/home/zack/dev/llmx"]]
       → Created index abc123 with 47 files, 312 chunks

User: Search for chunking logic
Agent: [calls mcp__llmx__llmx_search with query="chunking logic"]
       → Returns top results with inline content (16K token budget)

User: What functions are in this project?
Agent: [calls mcp__llmx__llmx_explore with mode="symbols"]
       → Returns sorted list of function/class names
```
