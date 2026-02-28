---
chunk_index: 743
ref: "1c5afda214fa"
id: "1c5afda214fa8f5bfcc402085aa2931019a109f41d631183034d3e1435410c84"
slug: "quick-start-phase-5--embedding-model"
path: "/home/zack/dev/llmx/docs/QUICK_START_PHASE_5.md"
kind: "markdown"
lines: [266, 278]
token_estimate: 74
content_sha256: "3c63adac6ee157f016049752f70597591a0c44d13257c1f2da682f603c0ce7d5"
compacted: false
heading_path: ["Phase 5 Quick Start Guide","💡 Design Decisions","Embedding Model"]
symbol: null
address: null
asset_path: null
---

### Embedding Model
**Recommended**: `all-MiniLM-L6-v2`
- Size: 80MB ONNX model
- Dimensions: 384 (good balance)
- Speed: ~10ms per chunk
- Quality: Good for code search

**Alternative**: `all-mpnet-base-v2`
- Size: 420MB
- Dimensions: 768
- Speed: ~30ms per chunk
- Quality: Better, but slower