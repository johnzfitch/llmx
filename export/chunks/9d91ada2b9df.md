---
chunk_index: 781
ref: "9d91ada2b9df"
id: "9d91ada2b9dfa742ba4959b725b2c709da17cadc61b7f41912d666e8de8150f1"
slug: "semantic-search-guide--problem-semantic-search-not-available"
path: "/home/zack/dev/llmx/docs/SEMANTIC_SEARCH_GUIDE.md"
kind: "markdown"
lines: [262, 272]
token_estimate: 62
content_sha256: "771371328ef81f4429dd53525c05970f19ebfc8c419c4ca76a4684e0c8b7c579"
compacted: false
heading_path: ["Semantic Search User Guide","Troubleshooting","Problem: Semantic search not available"]
symbol: null
address: null
asset_path: null
---

### Problem: Semantic search not available

**Cause**: Server built without `embeddings` feature.

**Solution**: Rebuild with `--features mcp` (embeddings included by default):
```bash
cargo build --release --features mcp --bin llmx-mcp
```

---