/// Phase 6: Burn-based embeddings with WebGPU acceleration
///
/// Provides real semantic embeddings using arctic-embed-s model
/// running on WebGPU in the browser.

use burn::tensor::{backend::Backend, Int, Tensor, TensorData};
#[cfg(feature = "ndarray-backend")]
use burn_ndarray::{NdArray, NdArrayDevice};
#[cfg(feature = "wgpu-backend")]
use burn_wgpu::{Wgpu, WgpuDevice};
use crate::model_loader::{fetch_with_cache, Model, MODEL_ID};
#[cfg(feature = "wgpu-backend")]
use crate::model_loader::load_model;
#[cfg(feature = "ndarray-backend")]
use crate::model_loader::load_model_cpu;
use tokenizers::Tokenizer;
use wasm_bindgen::prelude::*;

/// Embedding dimension for arctic-embed-s
pub const EMBEDDING_DIM: usize = 384;

/// Maximum sequence length for the model
const MAX_SEQ_LENGTH: usize = 512;

fn set_panic_hook_once() {
    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                web_sys::console::error_1(&JsValue::from_str(&info.to_string()));
            }));
        });
    }
}

#[cfg(all(feature = "wgpu-backend", target_arch = "wasm32"))]
fn webgpu_opt_in_enabled() -> bool {
    let global = js_sys::global();
    let key = JsValue::from_str("LLMX_ENABLE_WEBGPU");
    let value = js_sys::Reflect::get(&global, &key).ok();

    let Some(value) = value else {
        return false;
    };

    if let Some(flag) = value.as_bool() {
        return flag;
    }

    if let Some(flag) = value.as_f64() {
        return flag != 0.0;
    }

    if let Some(flag) = value.as_string() {
        let flag = flag.trim().to_ascii_lowercase();
        return matches!(flag.as_str(), "1" | "true" | "yes" | "on");
    }

    false
}

#[cfg(all(feature = "wgpu-backend", not(target_arch = "wasm32")))]
fn webgpu_opt_in_enabled() -> bool {
    true
}

/// Tokenizer URL (prefer same-origin to avoid CORS / third-party outages).
/// Both primary and fallback URLs must serve identical files with matching SHA256.
/// If HuggingFace updates the tokenizer, update both TOKENIZER_URL_FALLBACK and
/// TOKENIZER_SHA256 to match, and ensure the local ./models/tokenizer.json is updated.
const TOKENIZER_URL_PRIMARY: &str = "./models/tokenizer.json";
const TOKENIZER_URL_FALLBACK: &str =
    "https://huggingface.co/Snowflake/snowflake-arctic-embed-s/resolve/main/tokenizer.json";
const TOKENIZER_CACHE_KEY: &str = "arctic-embed-s-tokenizer-v1";
const TOKENIZER_SHA256: &str = "91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854";
const MAX_TOKENIZER_BYTES: usize = 5 * 1024 * 1024;
const ALLOWED_TOKENIZER_ORIGINS: [&str; 1] = ["https://huggingface.co/"];

/// Backend-agnostic embedding generator trait
pub trait EmbeddingBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue>;
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, JsValue>;
    fn dimension(&self) -> usize;
}

/// WebGPU-accelerated embedding generator
#[cfg(feature = "wgpu-backend")]
pub struct WgpuEmbeddingGenerator {
    model: Model<Wgpu>,
    tokenizer: Tokenizer,
    device: WgpuDevice,
}

#[cfg(feature = "wgpu-backend")]
impl WgpuEmbeddingGenerator {
    /// Initialize WebGPU embedding generator
    /// Note: WgpuDevice::default() may panic in WASM if:
    /// - WebGPU adapter is unavailable
    /// - Device creation fails
    /// Panics are converted to JsValue errors at the boundary.
    pub async fn new() -> Result<Self, JsValue> {
        web_sys::console::log_1(&JsValue::from_str("WebGPU init: requesting adapter and device..."));

        // Initialize WebGPU device - will panic if adapter/device unavailable
        // This panic is caught and converted to JsValue error in WASM
        let device = WgpuDevice::default();

        web_sys::console::log_1(&JsValue::from_str("WebGPU init: loading tokenizer..."));

        let tokenizer_bytes = fetch_tokenizer_bytes().await?;
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| {
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "Tokenizer load failed: {e}"
                )));
                JsValue::from_str("Failed to load tokenizer")
            })?;

        web_sys::console::log_1(&JsValue::from_str("WebGPU init: loading model..."));
        web_sys::console::warn_1(&JsValue::from_str(
            "WebGPU init: model loading may panic in WASM (known Burn/WGPU issue)",
        ));

        let model = load_model(&device).await?;

        web_sys::console::log_1(&JsValue::from_str("WebGPU init: embedder ready"));
        Ok(Self { model, tokenizer, device })
    }
}

#[cfg(feature = "wgpu-backend")]
impl EmbeddingBackend for WgpuEmbeddingGenerator {
    fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue> {
        embed_with_model(&self.model, &self.tokenizer, &self.device, text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, JsValue> {
        embed_batch_with_model(&self.model, &self.tokenizer, &self.device, texts)
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// CPU fallback embedding generator (NdArray backend)
#[cfg(feature = "ndarray-backend")]
pub struct CpuEmbeddingGenerator {
    model: Model<NdArray>,
    tokenizer: Tokenizer,
    device: NdArrayDevice,
}

#[cfg(feature = "ndarray-backend")]
impl CpuEmbeddingGenerator {
    pub async fn new() -> Result<Self, JsValue> {
        let device = NdArrayDevice::default();

        let tokenizer_bytes = fetch_tokenizer_bytes().await?;
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| {
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "Tokenizer load failed: {e}"
                )));
                JsValue::from_str("Failed to load tokenizer")
            })?;

        let model = load_model_cpu(&device).await?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }
}

#[cfg(feature = "ndarray-backend")]
impl EmbeddingBackend for CpuEmbeddingGenerator {
    fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue> {
        embed_with_model(&self.model, &self.tokenizer, &self.device, text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, JsValue> {
        embed_batch_with_model(&self.model, &self.tokenizer, &self.device, texts)
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// Hash-based fallback (Phase 5 compatibility)
pub struct HashEmbeddingGenerator;

impl HashEmbeddingGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl EmbeddingBackend for HashEmbeddingGenerator {
    fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue> {
        // Use the existing hash-based embedding from ingestor-core
        // This is the Phase 5 fallback
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();

        let mut embedding = Vec::with_capacity(EMBEDDING_DIM);
        for i in 0..EMBEDDING_DIM {
            let idx = i % hash.len();
            let value = (hash[idx] as f32 - 128.0) / 128.0;
            embedding.push(value);
        }

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        Ok(embedding)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, JsValue> {
        texts.iter()
            .map(|text| self.embed(text))
            .collect()
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// Smart embedding generator with automatic fallback
pub enum SmartEmbeddingGenerator {
    #[cfg(feature = "wgpu-backend")]
    WebGpu(WgpuEmbeddingGenerator),
    #[cfg(feature = "ndarray-backend")]
    Cpu(CpuEmbeddingGenerator),
    Hash(HashEmbeddingGenerator),
}

impl SmartEmbeddingGenerator {
    /// Create embedding generator with automatic fallback chain:
    /// WebGPU → CPU → Hash-based
    pub async fn new() -> Self {
        set_panic_hook_once();

        // Try WebGPU first (may panic in WASM with Burn 0.21 - known issue)
        #[cfg(feature = "wgpu-backend")]
        {
            if webgpu_opt_in_enabled() {
                web_sys::console::log_1(&JsValue::from_str("WebGPU init: enabled; attempting..."));
                match WgpuEmbeddingGenerator::new().await {
                    Ok(gen) => {
                        web_sys::console::log_1(&JsValue::from_str("Embeddings backend: WebGPU"));
                        return Self::WebGpu(gen);
                    }
                    Err(e) => {
                        web_sys::console::warn_1(&JsValue::from_str(&format!(
                            "WebGPU init failed; falling back to CPU: {e:?}"
                        )));
                    }
                }
            } else {
                web_sys::console::log_1(&JsValue::from_str(
                    "WebGPU init: disabled (set LLMX_ENABLE_WEBGPU=true or use ?webgpu=1); using CPU",
                ));
            }
        }

        // Fall back to CPU (reliable in WASM)
        #[cfg(feature = "ndarray-backend")]
        if let Ok(gen) = CpuEmbeddingGenerator::new().await {
            web_sys::console::log_1(&JsValue::from_str("Embeddings backend: CPU"));
            return Self::Cpu(gen);
        }

        // Last resort: hash-based
        web_sys::console::warn_1(&JsValue::from_str(
            "Embeddings backend: hash (models unavailable)",
        ));
        Self::Hash(HashEmbeddingGenerator::new())
    }
}

impl EmbeddingBackend for SmartEmbeddingGenerator {
    fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue> {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::WebGpu(gen) => gen.embed(text),
            #[cfg(feature = "ndarray-backend")]
            Self::Cpu(gen) => gen.embed(text),
            Self::Hash(gen) => gen.embed(text),
        }
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, JsValue> {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::WebGpu(gen) => gen.embed_batch(texts),
            #[cfg(feature = "ndarray-backend")]
            Self::Cpu(gen) => gen.embed_batch(texts),
            Self::Hash(gen) => gen.embed_batch(texts),
        }
    }

    fn dimension(&self) -> usize {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::WebGpu(gen) => gen.dimension(),
            #[cfg(feature = "ndarray-backend")]
            Self::Cpu(gen) => gen.dimension(),
            Self::Hash(gen) => gen.dimension(),
        }
    }
}

fn embed_with_model<B: Backend>(
    model: &Model<B>,
    tokenizer: &Tokenizer,
    device: &B::Device,
    text: &str,
) -> Result<Vec<f32>, JsValue> {
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| {
            web_sys::console::error_1(&JsValue::from_str(&format!("Tokenization failed: {e}")));
            JsValue::from_str("Failed to tokenize input")
        })?;

    let input_ids: Vec<i64> = encoding
        .get_ids()
        .iter()
        .take(MAX_SEQ_LENGTH)
        .map(|&id| id as i64)
        .collect();

    let attention_mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .take(MAX_SEQ_LENGTH)
        .map(|&mask| mask as i64)
        .collect();

    if input_ids.is_empty() {
        return Err(JsValue::from_str("Tokenization produced no input ids"));
    }

    let seq_len = input_ids.len();
    let input_ids = Tensor::<B, 2, Int>::from_ints(
        TensorData::new(input_ids, [1, seq_len]),
        device,
    );
    let attention_mask = Tensor::<B, 2, Int>::from_ints(
        TensorData::new(attention_mask, [1, seq_len]),
        device,
    );

    let hidden = model.forward(input_ids, attention_mask.clone());
    let pooled = mean_pool(hidden, attention_mask);
    let normalized = l2_normalize(pooled);

    let data = normalized.into_data();
    data.to_vec::<f32>()
        .map_err(|err| {
            web_sys::console::error_1(&JsValue::from_str(&format!(
                "Embedding read failed: {err:?}"
            )));
            JsValue::from_str("Failed to read embedding")
        })
}

fn embed_batch_with_model<B: Backend>(
    model: &Model<B>,
    tokenizer: &Tokenizer,
    device: &B::Device,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, JsValue> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let mut encoded_ids: Vec<Vec<i64>> = Vec::with_capacity(texts.len());
    let mut encoded_masks: Vec<Vec<i64>> = Vec::with_capacity(texts.len());
    let mut max_len = 0usize;

    for text in texts {
        let encoding = tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| {
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "Tokenization failed: {e}"
                )));
                JsValue::from_str("Failed to tokenize input")
            })?;

        let ids: Vec<i64> = encoding
            .get_ids()
            .iter()
            .take(MAX_SEQ_LENGTH)
            .map(|&id| id as i64)
            .collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .take(MAX_SEQ_LENGTH)
            .map(|&mask| mask as i64)
            .collect();

        if ids.is_empty() {
            return Err(JsValue::from_str("Tokenization produced no input ids"));
        }

        max_len = max_len.max(ids.len());
        encoded_ids.push(ids);
        encoded_masks.push(mask);
    }

    let batch_size = encoded_ids.len();
    let mut flat_ids: Vec<i64> = Vec::with_capacity(batch_size * max_len);
    let mut flat_masks: Vec<i64> = Vec::with_capacity(batch_size * max_len);

    for (mut ids, mut masks) in encoded_ids.into_iter().zip(encoded_masks) {
        ids.resize(max_len, 0);
        masks.resize(max_len, 0);
        flat_ids.extend_from_slice(&ids);
        flat_masks.extend_from_slice(&masks);
    }

    let input_ids = Tensor::<B, 2, Int>::from_ints(
        TensorData::new(flat_ids, [batch_size, max_len]),
        device,
    );
    let attention_mask = Tensor::<B, 2, Int>::from_ints(
        TensorData::new(flat_masks, [batch_size, max_len]),
        device,
    );

    let hidden = model.forward(input_ids, attention_mask.clone());
    let pooled = mean_pool(hidden, attention_mask);
    let normalized = l2_normalize(pooled);

    let data = normalized.into_data();
    let flat = data.to_vec::<f32>().map_err(|err| {
        web_sys::console::error_1(&JsValue::from_str(&format!(
            "Embedding batch read failed: {err:?}"
        )));
        JsValue::from_str("Failed to read embeddings")
    })?;

    if flat.len() != batch_size * EMBEDDING_DIM {
        return Err(JsValue::from_str("Embedding batch size mismatch"));
    }

    let mut outputs = Vec::with_capacity(batch_size);
    for chunk in flat.chunks(EMBEDDING_DIM) {
        outputs.push(chunk.to_vec());
    }

    Ok(outputs)
}

fn mean_pool<B: Backend>(hidden: Tensor<B, 3>, attention_mask: Tensor<B, 2, Int>) -> Tensor<B, 2> {
    // Convert attention_mask to float and expand dimensions for broadcasting
    // attention_mask: 1 = valid token, 0 = padding
    let mask = attention_mask.float().unsqueeze_dim::<3>(2);

    // Mask hidden states: multiply by mask to zero out padding positions
    // Note: Could potentially use mask_fill if available in future Burn versions
    let masked = hidden * mask.clone();

    // Compute mean over sequence length, excluding padding
    let sum = masked.sum_dim(1);
    let denom = mask.sum_dim(1).clamp_min(1e-6);
    let pooled = sum / denom;
    pooled.squeeze_dim::<2>(1)
}

fn l2_normalize<B: Backend>(embeddings: Tensor<B, 2>) -> Tensor<B, 2> {
    let norm = embeddings
        .clone()
        .powf_scalar(2.0)
        .sum_dim(1)
        .sqrt()
        .clamp_min(1e-12);
    embeddings / norm
}

/// Fetch resource from CDN and cache in IndexedDB
async fn load_or_fetch_from_cdn(url: &str, cache_key: &str) -> Result<Vec<u8>, JsValue> {
    fetch_with_cache(
        url,
        cache_key,
        TOKENIZER_SHA256,
        &ALLOWED_TOKENIZER_ORIGINS,
        MAX_TOKENIZER_BYTES,
    )
    .await
}

async fn fetch_tokenizer_bytes() -> Result<Vec<u8>, JsValue> {
    match load_or_fetch_from_cdn(TOKENIZER_URL_PRIMARY, TOKENIZER_CACHE_KEY).await {
        Ok(bytes) => Ok(bytes),
        Err(primary_err) => {
            web_sys::console::warn_1(&JsValue::from_str(&format!(
                "Tokenizer primary fetch failed, falling back: {primary_err:?}"
            )));
            load_or_fetch_from_cdn(TOKENIZER_URL_FALLBACK, TOKENIZER_CACHE_KEY).await
        }
    }
}

/// WASM bindings for browser
#[wasm_bindgen]
pub struct Embedder {
    inner: SmartEmbeddingGenerator,
}

#[wasm_bindgen]
impl Embedder {
    /// Create new embedder with automatic backend selection
    ///
    /// Use this factory method instead of `new()` to avoid TypeScript issues
    /// with async constructors.
    ///
    /// # Example
    /// ```javascript
    /// const embedder = await Embedder.create();
    /// ```
    #[wasm_bindgen]
    pub async fn create() -> Result<Embedder, JsValue> {
        set_panic_hook_once();
        let inner = SmartEmbeddingGenerator::new().await;
        Ok(Embedder { inner })
    }

    /// Generate embedding for a single text
    #[wasm_bindgen]
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, JsValue> {
        self.inner.embed(text)
    }

    /// Generate embeddings for multiple texts
    #[wasm_bindgen(js_name = embedBatch)]
    pub fn embed_batch(&self, texts: Vec<String>) -> Result<JsValue, JsValue> {
        let embeddings = self.inner.embed_batch(&texts)?;

        // Convert Vec<Vec<f32>> to JS array
        let js_array = js_sys::Array::new();
        for embedding in embeddings {
            let js_vec = js_sys::Float32Array::from(&embedding[..]);
            js_array.push(&js_vec);
        }

        Ok(js_array.into())
    }

    /// Get embedding dimension
    #[wasm_bindgen]
    pub fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    /// Get model identifier
    #[wasm_bindgen(js_name = modelId)]
    pub fn model_id(&self) -> String {
        match &self.inner {
            #[cfg(feature = "wgpu-backend")]
            SmartEmbeddingGenerator::WebGpu(_) => MODEL_ID.to_string(),
            #[cfg(feature = "ndarray-backend")]
            SmartEmbeddingGenerator::Cpu(_) => MODEL_ID.to_string(),
            SmartEmbeddingGenerator::Hash(_) => "hash-based-v1".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::record::Recorder;
    use burn::module::Module;

    #[cfg(all(feature = "ndarray-backend", not(target_arch = "wasm32")))]
    #[test]
    fn test_cpu_embeddings_deterministic() {
        use burn_ndarray::NdArrayDevice;

        // This test verifies that dropout is truly disabled (DROPOUT_PROB = 0.0)
        // and that embeddings are deterministic across multiple runs
        let device = NdArrayDevice::default();

        // Load model and tokenizer
        let model_bytes = include_bytes!("../models/arctic-embed-s.bin");
        let tokenizer_bytes = include_bytes!("../models/tokenizer.json");

        let tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
            .expect("Failed to load tokenizer from embedded file");

        let recorder = burn::record::BinBytesRecorder::<
            burn::record::FullPrecisionSettings,
            Vec<u8>,
        >::default();
        let record = recorder
            .load(model_bytes.to_vec(), &device)
            .expect("Failed to load model record");

        let model = crate::bert::Model::new(&device).load_record(record);

        // Helper function to generate embedding
        let embed_text = |text: &str| -> Vec<f32> {
            let encoding = tokenizer
                .encode(text, true)
                .expect("Failed to tokenize");

            let input_ids: Vec<i32> = encoding.get_ids().iter().map(|&id| id as i32).collect();
            let attention_mask: Vec<i32> = encoding
                .get_attention_mask()
                .iter()
                .map(|&m| m as i32)
                .collect();

            let batch_size = 1;
            let seq_len = input_ids.len();

            let input_ids_tensor = burn::tensor::Tensor::<burn_ndarray::NdArray, 2, burn::tensor::Int>::from_data(
                burn::tensor::TensorData::new(input_ids, [batch_size, seq_len]),
                &device,
            );

            let attention_mask_tensor = burn::tensor::Tensor::<burn_ndarray::NdArray, 2, burn::tensor::Int>::from_data(
                burn::tensor::TensorData::new(attention_mask.clone(), [batch_size, seq_len]),
                &device,
            );

            let hidden = model.forward(input_ids_tensor, attention_mask_tensor.clone());
            let pooled = mean_pool(hidden, attention_mask_tensor);
            let normalized = l2_normalize(pooled);

            normalized
                .to_data()
                .to_vec::<f32>()
                .unwrap()
        };

        // Test 1: Same input produces identical embeddings
        let test_text = "The quick brown fox jumps over the lazy dog";
        let emb1 = embed_text(test_text);
        let emb2 = embed_text(test_text);
        let emb3 = embed_text(test_text);

        assert_eq!(
            emb1.len(),
            emb2.len(),
            "Embeddings must have same dimension"
        );
        assert_eq!(
            emb1.len(),
            384,
            "Expected 384-dimensional embeddings for arctic-embed-s"
        );

        // Verify exact equality (no randomness from dropout)
        for (i, ((&v1, &v2), &v3)) in emb1.iter().zip(emb2.iter()).zip(emb3.iter()).enumerate() {
            assert_eq!(
                v1, v2,
                "Embedding dimension {} differs between run 1 and 2: {} vs {}",
                i, v1, v2
            );
            assert_eq!(
                v1, v3,
                "Embedding dimension {} differs between run 1 and 3: {} vs {}",
                i, v1, v3
            );
        }

        // Test 2: Different inputs produce different embeddings
        let text_a = "Machine learning with Rust";
        let text_b = "Deep learning with Python";
        let emb_a = embed_text(text_a);
        let emb_b = embed_text(text_b);

        // Calculate cosine similarity (should be < 1.0 for different texts)
        let dot_product: f32 = emb_a.iter().zip(emb_b.iter()).map(|(&a, &b)| a * b).sum();
        assert!(
            dot_product < 0.99,
            "Different texts should produce different embeddings (cosine similarity: {})",
            dot_product
        );

        // Test 3: Embeddings are normalized (L2 norm = 1.0)
        let norm_sq: f32 = emb1.iter().map(|&v| v * v).sum();
        let norm = norm_sq.sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "Embeddings should be L2-normalized (norm: {})",
            norm
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_mean_pool_correctness() {
        use burn_ndarray::{NdArray, NdArrayDevice};

        let device = NdArrayDevice::default();

        // Create mock hidden states: [batch_size=1, seq_len=3, hidden_dim=4]
        let hidden_data = vec![
            1.0, 2.0, 3.0, 4.0, // token 1
            5.0, 6.0, 7.0, 8.0, // token 2
            9.0, 10.0, 11.0, 12.0, // token 3 (padding)
        ];
        let hidden = burn::tensor::Tensor::<NdArray, 3>::from_data(
            burn::tensor::TensorData::new(hidden_data, [1, 3, 4]),
            &device,
        );

        // Attention mask: [1, 1, 0] - third token is padding
        let attention_mask = burn::tensor::Tensor::<NdArray, 2, burn::tensor::Int>::from_data(
            burn::tensor::TensorData::new(vec![1, 1, 0], [1, 3]),
            &device,
        );

        let pooled = mean_pool(hidden, attention_mask);
        let result: Vec<f32> = pooled.to_data().to_vec::<f32>().unwrap();

        // Expected: mean of first two tokens only (third is masked)
        // (1+5)/2=3, (2+6)/2=4, (3+7)/2=5, (4+8)/2=6
        let expected = vec![3.0, 4.0, 5.0, 6.0];

        assert_eq!(result.len(), expected.len(), "Output dimension mismatch");
        for (i, (&actual, &exp)) in result.iter().zip(expected.iter()).enumerate() {
            assert!(
                (actual - exp).abs() < 1e-5_f32,
                "Dimension {} incorrect: expected {}, got {}",
                i,
                exp,
                actual
            );
        }
    }
}
