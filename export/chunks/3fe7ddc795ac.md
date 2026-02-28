---
chunk_index: 586
ref: "3fe7ddc795ac"
id: "3fe7ddc795ac3ae5ea331133d764566aba7c8f20c2dbbbbdf4c19c9a914c32e3"
slug: "phase-4-directions--known-issues-to-address"
path: "/home/zack/dev/llmx/docs/PHASE_4_DIRECTIONS.md"
kind: "markdown"
lines: [113, 118]
token_estimate: 60
content_sha256: "5804546df99e9d5cbc1343526aa3bcbc44cb7ca30b4751be899626199798febb"
compacted: false
heading_path: ["Phase 4: MCP Server Implementation & Tool Consolidation","Known Issues to Address"]
symbol: null
address: null
asset_path: null
---

## Known Issues to Address
- Search results currently require separate `get_chunks` call
- IndexStore uses unnecessary `Arc<Mutex<>>` for stdio context
- 10 tools create decision paralysis for agents
- No query explanation in search results