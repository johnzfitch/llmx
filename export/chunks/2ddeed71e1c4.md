---
chunk_index: 80
ref: "2ddeed71e1c4"
id: "2ddeed71e1c44765389f96a1f52f81fdd5f4a0f7925c287cd457ded92c7ed88b"
slug: "agent-handoff--project-structure"
path: "/home/zack/dev/llmx/docs/AGENT_HANDOFF.md"
kind: "markdown"
lines: [22, 54]
token_estimate: 323
content_sha256: "5ff620c119628624937977f87be59a409d9653df150c3a687dc780cd57cfd27b"
compacted: false
heading_path: ["Agent Handoff Document","Current State","Project Structure"]
symbol: null
address: null
asset_path: null
---

### Project Structure

```
llmx/
├── ingestor-core/              # Core library & MCP server
│   ├── src/
│   │   ├── lib.rs              # Public API (ingest_files, search, etc.)
│   │   ├── chunk.rs            # Chunking logic
│   │   ├── index.rs            # BM25 search & inverted index
│   │   ├── model.rs            # Data structures
│   │   ├── export.rs           # Export formats (llm.md, zip)
│   │   ├── util.rs             # Helper functions
│   │   ├── bin/
│   │   │   └── mcp_server.rs   # MCP stdio server (12MB binary)
│   │   └── mcp/
│   │       ├── mod.rs          # Module exports
│   │       ├── storage.rs      # IndexStore (disk + cache)
│   │       └── tools.rs        # 4 MCP tool handlers
│   ├── benches/
│   │   └── baseline.rs         # Performance benchmarks
│   └── Cargo.toml              # Dependencies
├── docs/
│   ├── PHASE_4_BASELINE_BENCHMARKS.md      # Perf baseline
│   ├── PHASE_4_COMPLETION_REPORT.md        # What was done
│   ├── PHASE_4_MCP_VERIFICATION.md         # Manual test guide
│   ├── PHASE_4_POLISH_CHECKLIST.md         # Original checklist
│   ├── PHASE_5_DIRECTIONS.md               # ⚠️ READ THIS NEXT
│   └── AGENT_HANDOFF.md                    # This document
└── test_mcp_server.sh          # Automated test (partial)
```

---