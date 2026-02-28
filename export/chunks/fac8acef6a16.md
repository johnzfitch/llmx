---
chunk_index: 220
ref: "fac8acef6a16"
id: "fac8acef6a165a56157da156838b1fbdd27a05f6c79656b591ef2f62abbd2592"
slug: "ingestion-spec--javascript-typescript"
path: "/home/zack/dev/llmx/docs/INGESTION_SPEC.md"
kind: "markdown"
lines: [83, 89]
token_estimate: 67
content_sha256: "7576d3e5b90de1b6c1b96614485f49f5f43e348a78f2403ece70a2e45e29e38a"
compacted: false
heading_path: ["LLMX Ingestion Spec","Chunking Rules (Deterministic)","JavaScript / TypeScript"]
symbol: null
address: null
asset_path: null
---

### JavaScript / TypeScript

- Tree-sitter parsing; chunk by function/class/method declarations.
- Best effort symbol extraction via `name` field.
- Fallback to text chunking if parsing fails.
- WASM builds disable tree-sitter and always fall back to text chunking.