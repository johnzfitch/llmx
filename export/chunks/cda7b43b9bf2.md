---
chunk_index: 643
ref: "cda7b43b9bf2"
id: "cda7b43b9bf246f871f49e6674d8dc89dcba84edf09a65211d01f799e836f51f"
slug: "phase-5-completion-report--hybrid-search-src-index-rs-255-327"
path: "/home/zack/dev/llmx/docs/PHASE_5_COMPLETION_REPORT.md"
kind: "markdown"
lines: [93, 113]
token_estimate: 146
content_sha256: "638c91ecb145303930536da32245fd27cb2599915bbbc35121657c275cb98658"
compacted: false
heading_path: ["Phase 5 Completion Report: Semantic Search Integration","Implementation Summary","3. Search Algorithms","Hybrid Search (src/index.rs:255-327)"]
symbol: null
address: null
asset_path: null
---

#### Hybrid Search (src/index.rs:255-327)
```rust
pub fn hybrid_search(
    chunks: &[Chunk],
    inverted: &BTreeMap<String, TermEntry>,
    chunk_refs: &BTreeMap<String, String>,
    embeddings: &[Vec<f32>],
    query: &str,
    query_embedding: &[f32],
    filters: &SearchFilters,
    limit: usize,
) -> Vec<SearchResult>
```

**Strategy**: Linear score combination
- Runs both BM25 and semantic search
- Normalizes BM25 scores to [0, 1]
- Combines: `final_score = 0.5 * normalized_bm25 + 0.5 * semantic_similarity`
- Merges and re-ranks results
- Returns top-N by combined score