---
chunk_index: 786
ref: "b8b6f70adb1b"
id: "b8b6f70adb1b4bbf200cf1993f52d2712b9d5019ead11c9648b5241a7c430e22"
slug: "semantic-search-guide--automatic-embedding-generation"
path: "/home/zack/dev/llmx/docs/SEMANTIC_SEARCH_GUIDE.md"
kind: "markdown"
lines: [326, 341]
token_estimate: 61
content_sha256: "be86b4327151cdaca3f9aeda30902c5b40269237e1cbd6329a495d4ba48bb567"
compacted: false
heading_path: ["Semantic Search User Guide","Migration from Phase 4","Automatic Embedding Generation"]
symbol: null
address: null
asset_path: null
---

### Automatic Embedding Generation

All new indexes automatically include embeddings:

```bash
# Phase 4
llmx_index → Creates index with BM25 only

# Phase 5
llmx_index → Creates index with BM25 + embeddings
```

No configuration required.

---