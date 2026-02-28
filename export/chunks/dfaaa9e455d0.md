---
chunk_index: 287
ref: "dfaaa9e455d0"
id: "dfaaa9e455d09e55851fa3c5208127bec251950057b448d4396f156a36ba64ad"
slug: "p6-directions--dependencies"
path: "/home/zack/dev/llmx/docs/P6_DIRECTIONS.md"
kind: "markdown"
lines: [39, 57]
token_estimate: 122
content_sha256: "e6fce25894f18f18fd3f96d87e6a7bdbd604d72de61c41b56225433b46c08df9"
compacted: false
heading_path: ["Phase 6: Burn + WebGPU Embeddings & Advanced Hybrid Search","2. Burn Framework Integration","Dependencies"]
symbol: null
address: null
asset_path: null
---

### Dependencies
```toml
[dependencies]
burn = { version = "0.15", default-features = false }
burn-ndarray = { version = "0.15", optional = true }  # CPU fallback
burn-wgpu = { version = "0.15", optional = true }     # WebGPU

# For ONNX import (build-time only)
burn-import = { version = "0.15" }

# Tokenizer (works in WASM)
tokenizers = { version = "0.20", default-features = false }

[features]
default = ["wgpu"]
wgpu = ["burn-wgpu"]
cpu = ["burn-ndarray"]  # Fallback for no-GPU
```