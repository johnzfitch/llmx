---
chunk_index: 306
ref: "6a822ecec9da"
id: "6a822ecec9da4e38bbbc7a73bdb70050af6ba65e196af2788f73da3a6ade96dd"
slug: "p6-directions--7-gotchas"
path: "/home/zack/dev/llmx/docs/P6_DIRECTIONS.md"
kind: "markdown"
lines: [376, 388]
token_estimate: 160
content_sha256: "33d3cc209f674ee6d671ba274cb2ae04e97e374c27a92561e7e2a300033b9d51"
compacted: false
heading_path: ["Phase 6: Burn + WebGPU Embeddings & Advanced Hybrid Search","7. Gotchas"]
symbol: null
address: null
asset_path: null
---

## 7. Gotchas

1. **Burn version compatibility** - burn-import generates code for specific Burn version, keep in sync
2. **WASM memory limits** - Browser WASM has ~4GB limit, watch model + index size
3. **Tokenizer in WASM** - `tokenizers` crate works but disable default features
4. **WebGPU async** - All GPU ops are async in browser, can't block
5. **Safari WebGPU** - Still experimental, test thoroughly
6. **Model warmup** - First inference is slow (shader compilation), do warmup call on init
7. **Attention mask in pooling** - Forgetting this = garbage embeddings
8. **Dimension mismatch** - BGE=384, Nomic=768, don't hardcode

---