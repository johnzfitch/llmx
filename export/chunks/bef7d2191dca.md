---
chunk_index: 152
ref: "bef7d2191dca"
id: "bef7d2191dca47052abedc6ecca79402a28b7ab38c4d1b32fc26a36bcc0f0f18"
slug: "claude-code-integration--1-cli-commands"
path: "/home/zack/dev/llmx/docs/CLAUDE_CODE_INTEGRATION.md"
kind: "markdown"
lines: [111, 126]
token_estimate: 100
content_sha256: "45f4605e75cec82121df6057bbe1993660491e71929a2096f9ae92950fe80ad4"
compacted: false
heading_path: ["llmx + Claude Code Integration","Integration Points","1. CLI Commands"]
symbol: null
address: null
asset_path: null
---

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