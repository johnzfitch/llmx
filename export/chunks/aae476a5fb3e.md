---
chunk_index: 290
ref: "aae476a5fb3e"
id: "aae476a5fb3e36bcfca0b44749ea70b837451f929a786580e383c6c3cef96eb6"
slug: "p6-directions--browser-integration-wasm-bindgen"
path: "/home/zack/dev/llmx/docs/P6_DIRECTIONS.md"
kind: "markdown"
lines: [156, 185]
token_estimate: 175
content_sha256: "5141248299c42a8d4023c59a4bbf00c0f5df69b227b388a1cb5fc9c921e7e5ff"
compacted: false
heading_path: ["Phase 6: Burn + WebGPU Embeddings & Advanced Hybrid Search","2. Burn Framework Integration","Browser Integration (wasm-bindgen)"]
symbol: null
address: null
asset_path: null
---

### Browser Integration (wasm-bindgen)
```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Embedder {
    inner: EmbeddingGenerator<Wgpu>,
}

#[wasm_bindgen]
impl Embedder {
    #[wasm_bindgen(constructor)]
    pub async fn new() -> Result<Embedder, JsError> {
        let inner = EmbeddingGenerator::new().await
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Embedder { inner })
    }
    
    #[wasm_bindgen]
    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.inner.embed(text)
    }
    
    #[wasm_bindgen]
    pub fn embed_batch(&self, texts: Vec<String>) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.inner.embed(t)).collect()
    }
}
```