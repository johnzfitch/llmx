---
chunk_index: 161
ref: "d5ac3ccd54a6"
id: "d5ac3ccd54a6102e4f45282f3f3b028d8f32f562cf1263942c3ff8a287a1c2fa"
slug: "claude-code-integration--with-llmx"
path: "/home/zack/dev/llmx/docs/CLAUDE_CODE_INTEGRATION.md"
kind: "markdown"
lines: [264, 273]
token_estimate: 76
content_sha256: "0a0ffffa3ff110d078bf40b0e1950c40897f7abcd16b140b966ef56a9671b0a0"
compacted: false
heading_path: ["llmx + Claude Code Integration","Agent Workflow","With llmx"]
symbol: null
address: null
asset_path: null
---

### With llmx

```
1. mcp__llmx__get_manifest("project") → Overview + structure
2. mcp__llmx__search("authenticate")  → 3 relevant chunks
3. mcp__llmx__read_chunk("abc123")    → Just the auth handler
4. Read("src/auth/handler.rs", 45-60) → Specific lines if needed
Total: 4-8 API turns, 5,000 tokens
```