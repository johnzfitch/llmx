---
chunk_index: 744
ref: "e67bc18cadce"
id: "e67bc18cadce7ed4ba8029144a553fd549a0729c96ee28eeeac53c38de9b566f"
slug: "quick-start-phase-5--hybrid-scoring"
path: "/home/zack/dev/llmx/docs/QUICK_START_PHASE_5.md"
kind: "markdown"
lines: [279, 289]
token_estimate: 66
content_sha256: "131231236f8bca58b0b67b8d79e69157ed9ae76fe903b2dccff07c7708a540fe"
compacted: false
heading_path: ["Phase 5 Quick Start Guide","💡 Design Decisions","Hybrid Scoring"]
symbol: null
address: null
asset_path: null
---

### Hybrid Scoring
**Simple approach** (start here):
```rust
final_score = 0.5 * normalize(bm25_score) + 0.5 * normalize(semantic_score)
```

**Advanced** (optimize later):
- Reciprocal Rank Fusion (RRF)
- Learned weights per query type
- Dynamic weight adjustment