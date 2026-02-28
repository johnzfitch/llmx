---
chunk_index: 958
ref: "32b4bbecf64b"
id: "32b4bbecf64ba2d17040823333241a0e61523238d87389aa7fc6874c603a0c38"
slug: "llmx-export-evaluation--p0-hybrid-search-hashmap"
path: "/home/zack/dev/llmx/docs/llmx-export-evaluation.md"
kind: "markdown"
lines: [73, 76]
token_estimate: 34
content_sha256: "5fe66c5f92ac25c08db55945b9130a63461a7f03ae0cb6520b16e049979ce50b"
compacted: false
heading_path: ["LLMX Export Evaluation Report","Performance Impact (P0-P4 Optimizations)","P0: Hybrid Search HashMap"]
symbol: null
address: null
asset_path: null
---

### P0: Hybrid Search HashMap
- Index building: O(chunks) once, then O(1) lookups
- Impact: Search over 288 chunks now scales linearly