---
chunk_index: 726
ref: "ce3c0bde5b99"
id: "ce3c0bde5b999f3009c9687538e41fbe032d35dae2ae510edef528e1c9eb4e01"
slug: "quick-start-phase-5--step-6-implement-hybrid-ranking-2-3-hours"
path: "/home/zack/dev/llmx/docs/QUICK_START_PHASE_5.md"
kind: "markdown"
lines: [108, 131]
token_estimate: 144
content_sha256: "cc509f688aa1e5bbfaa1256cb33e1a6d0de467c3dcc7de7f496029d25f572164"
compacted: false
heading_path: ["Phase 5 Quick Start Guide","📋 Implementation Steps","Step 6: Implement Hybrid Ranking (2-3 hours)"]
symbol: null
address: null
asset_path: null
---

### Step 6: Implement Hybrid Ranking (2-3 hours)
```rust
// src/index.rs (add new function)

pub fn hybrid_search(
    chunks: &[Chunk],
    inverted_index: &InvertedIndex,
    embeddings: &[Vec<f32>],
    query: &str,
    query_embedding: &[f32],
    limit: usize,
) -> Vec<SearchResult> {
    // 1. Get BM25 scores
    let bm25_results = search_index(...);

    // 2. Get semantic scores
    let semantic_results = vector_search(...);

    // 3. Normalize both scores to [0, 1]
    // 4. Combine: score = 0.5 * bm25 + 0.5 * semantic
    // 5. Re-rank and return top N
}
```