---
chunk_index: 236
ref: "0a185cff1bdd"
id: "0a185cff1bdd648ec59c302e1607318af5b0787bda7e364c29d792131e30d8ed"
slug: "ingestion-spec--alternative-deployment-models"
path: "/home/zack/dev/llmx/docs/INGESTION_SPEC.md"
kind: "markdown"
lines: [226, 245]
token_estimate: 212
content_sha256: "3114779e7205c3b03b63039dbc6278f911d5809ce948c9c9e923fe3a08914664"
compacted: false
heading_path: ["LLMX Ingestion Spec","Alternative Deployment Models"]
symbol: null
address: null
asset_path: null
---

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