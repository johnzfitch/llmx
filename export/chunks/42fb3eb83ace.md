---
chunk_index: 401
ref: "42fb3eb83ace"
id: "42fb3eb83ace1228a92443dd0510d51f15d3db912a75248f0bf924888bcf6560"
slug: "phase6-implementation--build-errors"
path: "/home/zack/dev/llmx/docs/PHASE6_IMPLEMENTATION.md"
kind: "markdown"
lines: [388, 399]
token_estimate: 83
content_sha256: "1a1aa986c9c322b9f5945083047ba6b257f7eba45b33dca4e5a0cb2ebef08b8b"
compacted: false
heading_path: ["Phase 6 Implementation: Burn + WebGPU Embeddings","Troubleshooting","Build Errors"]
symbol: null
address: null
asset_path: null
---

### Build Errors

**"Failed to download model"**
- Check internet connection
- HuggingFace may be temporarily unavailable
- Try clearing `models/` directory and rebuilding

**"burn-import failed"**
- Verify Burn version matches across all crates
- Check ONNX model is valid (not corrupted download)
- Try `cargo clean` and rebuild