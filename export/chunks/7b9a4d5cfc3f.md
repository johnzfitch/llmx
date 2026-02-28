---
chunk_index: 450
ref: "7b9a4d5cfc3f"
id: "7b9a4d5cfc3f4ed5d5e8c55e79ea5056ea8eef2fe30d7261780881bc46f70033"
slug: "phase6-status--for-full-phase-6"
path: "/home/zack/dev/llmx/docs/PHASE6_STATUS.md"
kind: "markdown"
lines: [451, 479]
token_estimate: 162
content_sha256: "b2e730bee600e53914b6d92dcd1e61eb706543b858ff1a5e629a8482dd99c0f3"
compacted: false
heading_path: ["Phase 6 Implementation Status","💡 Recommendations","For Full Phase 6"]
symbol: null
address: null
asset_path: null
---

### For Full Phase 6

**Priority order:**

1. **Convert ONNX to opset 13** (Critical)
   - Download bge-small-en-v1.5 PyTorch model
   - Export to ONNX with opset 13
   - Verify burn-import works

2. **Solve tokenizer** (Critical)
   - Research esaxx_rs or pure Rust tokenizer
   - Implement basic WordPiece
   - Test in WASM

3. **Complete inference** (High)
   - Implement forward pass
   - Test embeddings match Python
   - Validate in browser

4. **Add caching** (Medium)
   - IndexedDB or Cache API
   - Progress reporting
   - Offline support

5. **Optimize performance** (Low)
   - Can do after it works
   - Batch processing
   - WebWorker