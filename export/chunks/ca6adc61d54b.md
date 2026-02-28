---
chunk_index: 46
ref: "ca6adc61d54b"
id: "ca6adc61d54bd77dbffdf9dcc8bd2e7d24a4a349a399b9927f8f363a2ecd6ebf"
slug: "claude-model-selection-for-c--assistant"
path: "/home/zack/dev/llmx/claude_Model selection for current project_2026-01-17.md"
kind: "markdown"
lines: [157, 172]
token_estimate: 154
content_sha256: "6704bb32f1590cc6b0e3947726037e35f922bacb47c36314ba8ddd55af01c1b3"
compacted: false
heading_path: ["Model selection for current project","Assistant"]
symbol: null
address: null
asset_path: null
---

## Assistant

The user is clarifying that llm.cat runs entirely client-side with WebGPU. This means:

1. No server-side Rust - everything runs in browser
2. `ort` crate won't work (doesn't compile to WASM)
3. Need WebGPU-compatible inference

Options for WebGPU embeddings in browser:
1. **onnxruntime-web** with WebGPU backend - JS library, works but mixing with Rust WASM
2. **Transformers.js** - Hugging Face's JS library, has WebGPU support now
3. **Burn** - Rust framework that compiles to WASM and has WGPU backend
4. **candle** - HF's Rust ML framework, has WASM support

Given Zack's stack (Rust preference