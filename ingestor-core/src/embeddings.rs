//! Embedding generation for semantic search using Burn framework.
//!
//! Phase 6: Pure Rust embeddings using mdbr-leaf-ir model (768-dim).
//! Model is compiled into the binary at build time.

// Catch the broken feature combination at compile time with a clear message.
// `embeddings` alone compiles code that calls `with_model`, which is only
// defined when a backend (ndarray-backend or wgpu-backend) is also enabled.
#[cfg(all(feature = "embeddings", not(any(feature = "ndarray-backend", feature = "wgpu-backend"))))]
compile_error!(
    "Feature 'embeddings' requires a backend. Enable 'ndarray-backend' (pure Rust, recommended) \
     or 'wgpu-backend' (GPU-accelerated)."
);

#[cfg(any(test, not(feature = "embeddings")))]
use sha2::{Digest, Sha256};

#[cfg(feature = "embeddings")]
use anyhow::{Context, Result};
#[cfg(feature = "embeddings")]
use burn::module::Module;
#[cfg(feature = "embeddings")]
use burn::record::{BinBytesRecorder, FullPrecisionSettings, Recorder};
#[cfg(feature = "embeddings")]
use burn::tensor::{backend::Backend, Int, Tensor, TensorData};
#[cfg(feature = "embeddings")]
use std::cell::RefCell;
#[cfg(feature = "embeddings")]
use std::env;
#[cfg(feature = "embeddings")]
use tokenizers::Tokenizer;

#[cfg(feature = "ndarray-backend")]
use burn_ndarray::{NdArray, NdArrayDevice};

#[cfg(feature = "wgpu-backend")]
use burn_wgpu::{Wgpu, WgpuDevice};

#[cfg(feature = "embeddings")]
use crate::bert::Model;

/// Embedding dimension for mdbr-leaf-ir after projection head
pub const EMBEDDING_DIM: usize = 768;

/// Maximum sequence length for the model
#[cfg(feature = "embeddings")]
const MAX_SEQ_LENGTH: usize = 512;
#[cfg(feature = "embeddings")]
const DEFAULT_CPU_BATCH_SIZE: usize = 16;
#[cfg(feature = "embeddings")]
const DEFAULT_WGPU_BATCH_SIZE: usize = 64;

#[cfg(feature = "embeddings")]
const MODEL_ID_F32: &str = env!("LLMX_MODEL_ID_F32");
#[cfg(feature = "embeddings")]
const MODEL_ID_Q8: &str = env!("LLMX_MODEL_ID_Q8");

/// Embedded model binaries (compiled at build time)
#[cfg(feature = "embeddings")]
static MODEL_BYTES_F32: &[u8] = include_bytes!("../models/mdbr-leaf-ir-f32.bin");
#[cfg(feature = "embeddings")]
static MODEL_BYTES_Q8: &[u8] = include_bytes!("../models/mdbr-leaf-ir-q8.bin");

/// Embedded tokenizer (compiled at build time)
#[cfg(feature = "embeddings")]
static TOKENIZER_BYTES: &[u8] = include_bytes!("../models/tokenizer.json");

#[cfg(feature = "embeddings")]
thread_local! {
    static MODEL: RefCell<Option<&'static RuntimeEmbeddingModel>> = const { RefCell::new(None) };
}

/// Embedding model with tokenizer
#[cfg(feature = "embeddings")]
pub struct EmbeddingModel<B: Backend> {
    model: Model<B>,
    tokenizer: Tokenizer,
    device: B::Device,
}

#[cfg(feature = "embeddings")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelArtifact {
    F32,
    Q8,
}

#[cfg(feature = "embeddings")]
enum RuntimeEmbeddingModel {
    #[cfg(feature = "wgpu-backend")]
    Wgpu(EmbeddingModel<Wgpu>),
    #[cfg(feature = "ndarray-backend")]
    NdArray(EmbeddingModel<NdArray>),
}

#[cfg(feature = "ndarray-backend")]
impl EmbeddingModel<NdArray> {
    /// Load model from embedded bytes
    pub fn load() -> Result<Self> {
        let device = NdArrayDevice::default();
        load_model(MODEL_BYTES_Q8, device)
    }
}

#[cfg(feature = "wgpu-backend")]
impl EmbeddingModel<Wgpu> {
    /// Load model from embedded bytes
    pub fn load() -> Result<Self> {
        let device = WgpuDevice::default();
        load_model(MODEL_BYTES_F32, device)
    }
}

#[cfg(feature = "embeddings")]
impl<B: Backend> EmbeddingModel<B> {
    /// Generate embedding for a single text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        embed_with_model(&self.model, &self.tokenizer, &self.device, text)
    }

    /// Generate embeddings for multiple texts (batched for efficiency)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        embed_batch_with_model(&self.model, &self.tokenizer, &self.device, texts)
    }
}

#[cfg(feature = "embeddings")]
impl RuntimeEmbeddingModel {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(model) => model.embed(text),
            #[cfg(feature = "ndarray-backend")]
            Self::NdArray(model) => model.embed(text),
        }
    }

    fn model_id(&self) -> &'static str {
        model_id_for_artifact(self.model_artifact())
    }

    fn model_artifact(&self) -> ModelArtifact {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(_) => ModelArtifact::F32,
            #[cfg(feature = "ndarray-backend")]
            Self::NdArray(_) => ModelArtifact::Q8,
        }
    }

    fn batch_size(&self) -> usize {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(_) => env_batch_size("LLMX_WGPU_EMBED_BATCH", DEFAULT_WGPU_BATCH_SIZE),
            #[cfg(feature = "ndarray-backend")]
            Self::NdArray(_) => env_batch_size("LLMX_CPU_EMBED_BATCH", DEFAULT_CPU_BATCH_SIZE),
        }
    }

    fn embed_many(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let batch_size = self.batch_size().max(1);
        let mut outputs = Vec::with_capacity(texts.len());
        for batch in texts.chunks(batch_size) {
            let mut batch_embeddings = match self {
                #[cfg(feature = "wgpu-backend")]
                Self::Wgpu(model) => model.embed_batch(batch)?,
                #[cfg(feature = "ndarray-backend")]
                Self::NdArray(model) => model.embed_batch(batch)?,
            };
            outputs.append(&mut batch_embeddings);
        }
        Ok(outputs)
    }
}

#[cfg(feature = "embeddings")]
fn env_batch_size(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(feature = "embeddings")]
fn should_try_wgpu_backend() -> bool {
    let Some(value) = env::var("LLMX_FORCE_CPU").ok() else {
        return true;
    };
    let value = value.trim().to_ascii_lowercase();
    !matches!(value.as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(feature = "embeddings")]
fn load_runtime_model() -> Result<RuntimeEmbeddingModel> {
    #[cfg(feature = "wgpu-backend")]
    if should_try_wgpu_backend() {
        let wgpu_result = std::panic::catch_unwind(EmbeddingModel::<Wgpu>::load);
        match wgpu_result {
            Ok(Ok(model)) => {
                let smoke = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    model.embed("llmx backend probe")
                }));
                match smoke {
                    Ok(Ok(_)) => return Ok(RuntimeEmbeddingModel::Wgpu(model)),
                    Ok(Err(err)) => {
                        eprintln!("WGPU unavailable ({err}), using CPU");
                    }
                    Err(_) => {
                        eprintln!("WGPU unavailable (runtime probe panicked), using CPU");
                    }
                }
            }
            Ok(Err(err)) => eprintln!("WGPU unavailable ({err}), using CPU"),
            Err(_) => eprintln!("WGPU unavailable (initialization panicked), using CPU"),
        }
    }

    #[cfg(feature = "ndarray-backend")]
    {
        return EmbeddingModel::<NdArray>::load().map(RuntimeEmbeddingModel::NdArray);
    }

    #[cfg(not(feature = "ndarray-backend"))]
    anyhow::bail!("No embedding backend compiled")
}

/// Access the thread-local model, initializing if needed.
#[cfg(feature = "embeddings")]
fn with_model<T, F: FnOnce(&RuntimeEmbeddingModel) -> Result<T>>(f: F) -> Result<T> {
    MODEL.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let model = Box::new(load_runtime_model()?);
            *opt = Some(Box::leak(model));
        }
        f(opt.as_ref().unwrap())
    })
}

#[cfg(feature = "embeddings")]
fn load_model<B: Backend>(model_bytes: &[u8], device: B::Device) -> Result<EmbeddingModel<B>> {
    let tokenizer = Tokenizer::from_bytes(TOKENIZER_BYTES)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    let recorder = BinBytesRecorder::<FullPrecisionSettings, Vec<u8>>::default();
    let record: <Model<B> as Module<B>>::Record = recorder
        .load(model_bytes.to_vec(), &device)
        .context("Failed to load model record")?;

    let model = Model::new(&device).load_record(record);

    Ok(EmbeddingModel {
        model,
        tokenizer,
        device,
    })
}

#[cfg(feature = "embeddings")]
fn model_id_for_artifact(artifact: ModelArtifact) -> &'static str {
    match artifact {
        ModelArtifact::F32 => MODEL_ID_F32,
        ModelArtifact::Q8 => MODEL_ID_Q8,
    }
}

/// Compute cosine similarity between two normalized vectors.
///
/// Returns a value in [-1, 1], where 1 is identical.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// L2 normalize a vector.
pub fn normalize(vec: &[f32]) -> Vec<f32> {
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec.to_vec()
    }
}

/// Generate embedding for text using the compiled embedding backend.
#[cfg(feature = "embeddings")]
pub fn generate_embedding(text: &str) -> Result<Vec<f32>> {
    with_model(|m| m.embed(text))
}

#[cfg(not(feature = "embeddings"))]
pub fn generate_embedding(text: &str) -> Vec<f32> {
    generate_embedding_fallback(text)
}

/// Hash-based fallback embedding (deterministic but not semantic).
#[cfg(any(test, not(feature = "embeddings")))]
fn generate_embedding_fallback(text: &str) -> Vec<f32> {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = hasher.finalize();

    let mut embedding = Vec::with_capacity(EMBEDDING_DIM);
    for i in 0..EMBEDDING_DIM {
        let idx = i % hash.len();
        let value = (hash[idx] as f32 - 128.0) / 128.0;
        embedding.push(value);
    }

    normalize(&embedding)
}

/// Generate embeddings for multiple texts.
#[cfg(feature = "embeddings")]
pub fn generate_embeddings(texts: &[&str]) -> Result<Vec<Vec<f32>>> {
    with_model(|m| m.embed_many(texts))
}

#[cfg(feature = "embeddings")]
pub fn runtime_model_id() -> Result<&'static str> {
    with_model(|m| Ok(m.model_id()))
}

#[cfg(not(feature = "embeddings"))]
pub fn generate_embeddings(texts: &[&str]) -> Vec<Vec<f32>> {
    texts.iter().map(|t| generate_embedding_fallback(t)).collect()
}

#[cfg(feature = "embeddings")]
pub fn validate_index_embeddings(index: &crate::IndexFile) -> Result<&[Vec<f32>]> {
    validate_index_embeddings_with_model_id(index, runtime_model_id()?)
}

#[cfg(feature = "embeddings")]
fn validate_index_embeddings_with_model_id<'a>(
    index: &'a crate::IndexFile,
    runtime_model_id: &str,
) -> Result<&'a [Vec<f32>]> {
    let embeddings = index
        .embeddings
        .as_deref()
        .context("Semantic search requires indexed embeddings, but this index has none")?;

    let actual_model = index.embedding_model.as_deref().unwrap_or("unknown");
    if actual_model != runtime_model_id {
        anyhow::bail!(
            "Index embeddings were built with model '{}', but the runtime model is '{}'. Re-index before using semantic search.",
            actual_model,
            runtime_model_id
        );
    }

    if embeddings.len() != index.chunks.len() {
        anyhow::bail!(
            "Index embedding count mismatch: {} vectors for {} chunks",
            embeddings.len(),
            index.chunks.len()
        );
    }

    for (idx, embedding) in embeddings.iter().enumerate() {
        if embedding.len() != EMBEDDING_DIM {
            anyhow::bail!(
                "Index embedding {} has dimension {}, expected {}",
                idx,
                embedding.len(),
                EMBEDDING_DIM
            );
        }
    }

    Ok(embeddings)
}

// Internal implementation functions

#[cfg(feature = "embeddings")]
fn embed_with_model<B: Backend>(
    model: &Model<B>,
    tokenizer: &Tokenizer,
    device: &B::Device,
    text: &str,
) -> Result<Vec<f32>> {
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

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
        anyhow::bail!("Tokenization produced no input ids");
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
    let projected = model.project_embeddings(pooled);
    let normalized = l2_normalize(projected);

    let data = normalized.into_data();
    data.to_vec::<f32>()
        .map_err(|e| anyhow::anyhow!("Failed to read embedding: {:?}", e))
}

#[cfg(feature = "embeddings")]
fn embed_batch_with_model<B: Backend>(
    model: &Model<B>,
    tokenizer: &Tokenizer,
    device: &B::Device,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let mut encoded_ids: Vec<Vec<i64>> = Vec::with_capacity(texts.len());
    let mut encoded_masks: Vec<Vec<i64>> = Vec::with_capacity(texts.len());
    let mut max_len = 0usize;

    for text in texts {
        let encoding = tokenizer
            .encode(*text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

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
            anyhow::bail!("Tokenization produced no input ids");
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
    let projected = model.project_embeddings(pooled);
    let normalized = l2_normalize(projected);

    let data = normalized.into_data();
    let flat = data
        .to_vec::<f32>()
        .map_err(|e| anyhow::anyhow!("Failed to read embeddings: {:?}", e))?;

    if flat.len() != batch_size * EMBEDDING_DIM {
        anyhow::bail!("Embedding batch size mismatch");
    }

    let mut outputs = Vec::with_capacity(batch_size);
    for chunk in flat.chunks(EMBEDDING_DIM) {
        outputs.push(chunk.to_vec());
    }

    Ok(outputs)
}

#[cfg(feature = "embeddings")]
fn mean_pool<B: Backend>(hidden: Tensor<B, 3>, attention_mask: Tensor<B, 2, Int>) -> Tensor<B, 2> {
    // Convert attention_mask to float and expand dimensions for broadcasting
    // attention_mask: 1 = valid token, 0 = padding
    let mask = attention_mask.float().unsqueeze_dim::<3>(2);

    // Mask hidden states: multiply by mask to zero out padding positions
    let masked = hidden * mask.clone();

    // Compute mean over sequence length, excluding padding
    let sum = masked.sum_dim(1);
    let denom = mask.sum_dim(1).clamp_min(1e-6);
    let pooled = sum / denom;
    pooled.squeeze_dim::<2>(1)
}

#[cfg(feature = "embeddings")]
fn l2_normalize<B: Backend>(embeddings: Tensor<B, 2>) -> Tensor<B, 2> {
    let norm = embeddings
        .clone()
        .powf_scalar(2.0)
        .sum_dim(1)
        .sqrt()
        .clamp_min(1e-12);
    embeddings / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize() {
        let vec = vec![3.0, 4.0];
        let normalized = normalize(&vec);
        let norm = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_generate_embedding_fallback() {
        let text = "function hello() { return 'world'; }";
        let embedding = generate_embedding_fallback(text);
        assert_eq!(embedding.len(), EMBEDDING_DIM);

        // Check normalization
        let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);

        // Check determinism
        let embedding2 = generate_embedding_fallback(text);
        assert_eq!(embedding, embedding2);
    }

    #[cfg(feature = "embeddings")]
    #[test]
    fn test_backend_model_ids_are_distinct() {
        assert_ne!(MODEL_ID_F32, MODEL_ID_Q8);
        assert!(MODEL_ID_F32.starts_with("mdbr-leaf-ir-f32-"));
        assert!(MODEL_ID_Q8.starts_with("mdbr-leaf-ir-q8-"));
    }

    #[cfg(feature = "embeddings")]
    #[test]
    fn test_validate_index_embeddings_rejects_backend_mismatch() {
        let mut index = crate::ingest_files(
            vec![crate::FileInput {
                path: "src/lib.rs".to_string(),
                data: b"pub fn greet() {}".to_vec(),
                mtime_ms: None,
                fingerprint_sha256: None,
            }],
            crate::IngestOptions::default(),
        );
        index.embeddings = Some(vec![vec![0.0; EMBEDDING_DIM]; index.chunks.len()]);
        index.embedding_model = Some(MODEL_ID_F32.to_string());

        let err = validate_index_embeddings_with_model_id(&index, MODEL_ID_Q8).unwrap_err();
        assert!(err
            .to_string()
            .contains("Re-index before using semantic search."));
        assert!(err.to_string().contains(MODEL_ID_F32));
        assert!(err.to_string().contains(MODEL_ID_Q8));
    }

    #[cfg(feature = "embeddings")]
    #[test]
    #[ignore] // Requires model to be built
    fn test_burn_embedding() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let emb = generate_embedding(text).unwrap();

        assert_eq!(emb.len(), EMBEDDING_DIM);
        let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[cfg(feature = "embeddings")]
    #[test]
    #[ignore] // Requires model to be built
    fn test_semantic_similarity() {
        let similar1 = "The cat sat on the mat.";
        let similar2 = "A cat is sitting on a rug.";
        let different = "The stock market crashed yesterday.";

        let emb1 = generate_embedding(similar1).unwrap();
        let emb2 = generate_embedding(similar2).unwrap();
        let emb3 = generate_embedding(different).unwrap();

        let sim_similar = cosine_similarity(&emb1, &emb2);
        let sim_different = cosine_similarity(&emb1, &emb3);

        // Similar sentences should have higher similarity
        assert!(
            sim_similar > sim_different,
            "Similar: {}, Different: {}",
            sim_similar,
            sim_different
        );
        assert!(
            sim_similar > 0.5,
            "Similar sentences should have >0.5 similarity"
        );
    }
}
