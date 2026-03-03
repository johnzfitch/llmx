//! Embedding generation for semantic search using Burn framework.
//!
//! Phase 6: Pure Rust embeddings using arctic-embed-s model (384-dim).
//! Model is compiled into the binary at build time.

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
use tokenizers::Tokenizer;

#[cfg(feature = "ndarray-backend")]
use burn_ndarray::{NdArray, NdArrayDevice};

#[cfg(feature = "wgpu-backend")]
use burn_wgpu::{Wgpu, WgpuDevice};

#[cfg(feature = "embeddings")]
use crate::bert::Model;

/// Embedding dimension for arctic-embed-s
pub const EMBEDDING_DIM: usize = 384;

/// Maximum sequence length for the model
#[cfg(feature = "embeddings")]
const MAX_SEQ_LENGTH: usize = 512;

/// Model identifier for index metadata
#[cfg(feature = "embeddings")]
pub const MODEL_ID: &str = "arctic-embed-s-v1";

/// Embedded model binary (compiled at build time)
#[cfg(feature = "embeddings")]
static MODEL_BYTES: &[u8] = include_bytes!("../models/arctic-embed-s.bin");

/// Embedded tokenizer (compiled at build time)
#[cfg(feature = "embeddings")]
static TOKENIZER_BYTES: &[u8] = include_bytes!("../models/tokenizer.json");

// Thread-local model instance (lazy loaded per thread)
#[cfg(feature = "ndarray-backend")]
thread_local! {
    static MODEL: RefCell<Option<EmbeddingModel<NdArray>>> = const { RefCell::new(None) };
}

#[cfg(all(feature = "wgpu-backend", not(feature = "ndarray-backend")))]
thread_local! {
    static MODEL: RefCell<Option<EmbeddingModel<Wgpu>>> = const { RefCell::new(None) };
}

/// Embedding model with tokenizer
#[cfg(feature = "embeddings")]
pub struct EmbeddingModel<B: Backend> {
    model: Model<B>,
    tokenizer: Tokenizer,
    device: B::Device,
}

#[cfg(feature = "ndarray-backend")]
impl EmbeddingModel<NdArray> {
    /// Load model from embedded bytes
    pub fn load() -> Result<Self> {
        let device = NdArrayDevice::default();

        let tokenizer = Tokenizer::from_bytes(TOKENIZER_BYTES)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        let recorder = BinBytesRecorder::<FullPrecisionSettings, Vec<u8>>::default();
        let record: <Model<NdArray> as Module<NdArray>>::Record = recorder
            .load(MODEL_BYTES.to_vec(), &device)
            .context("Failed to load model record")?;

        let model = Model::new(&device).load_record(record);

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }
}

#[cfg(all(feature = "wgpu-backend", not(feature = "ndarray-backend")))]
impl EmbeddingModel<Wgpu> {
    /// Load model from embedded bytes
    pub fn load() -> Result<Self> {
        let device = WgpuDevice::default();

        let tokenizer = Tokenizer::from_bytes(TOKENIZER_BYTES)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        let recorder = BinBytesRecorder::<FullPrecisionSettings, Vec<u8>>::default();
        let record: <Model<Wgpu> as Module<Wgpu>>::Record = recorder
            .load(MODEL_BYTES.to_vec(), &device)
            .context("Failed to load model record")?;

        let model = Model::new(&device).load_record(record);

        Ok(Self {
            model,
            tokenizer,
            device,
        })
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

/// Access the thread-local model, initializing if needed
#[cfg(feature = "ndarray-backend")]
fn with_model<T, F: FnOnce(&EmbeddingModel<NdArray>) -> Result<T>>(f: F) -> Result<T> {
    MODEL.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(EmbeddingModel::load()?);
        }
        f(opt.as_ref().unwrap())
    })
}

#[cfg(all(feature = "wgpu-backend", not(feature = "ndarray-backend")))]
fn with_model<T, F: FnOnce(&EmbeddingModel<Wgpu>) -> Result<T>>(f: F) -> Result<T> {
    MODEL.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(EmbeddingModel::load()?);
        }
        f(opt.as_ref().unwrap())
    })
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

/// Generate embedding for text.
///
/// Uses Burn model when embeddings feature is enabled,
/// falls back to hash-based pseudo-embeddings otherwise.
#[cfg(feature = "embeddings")]
pub fn generate_embedding(text: &str) -> Vec<f32> {
    match with_model(|m| m.embed(text)) {
        Ok(emb) => emb,
        Err(e) => {
            eprintln!("Embedding error (using fallback): {}", e);
            generate_embedding_fallback(text)
        }
    }
}

#[cfg(not(feature = "embeddings"))]
pub fn generate_embedding(text: &str) -> Vec<f32> {
    generate_embedding_fallback(text)
}

/// Hash-based fallback embedding (deterministic but not semantic).
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
pub fn generate_embeddings(texts: &[&str]) -> Vec<Vec<f32>> {
    match with_model(|m| m.embed_batch(texts)) {
        Ok(embs) => embs,
        Err(e) => {
            eprintln!("Batch embedding error (using fallback): {}", e);
            texts.iter().map(|t| generate_embedding_fallback(t)).collect()
        }
    }
}

#[cfg(not(feature = "embeddings"))]
pub fn generate_embeddings(texts: &[&str]) -> Vec<Vec<f32>> {
    texts.iter().map(|t| generate_embedding_fallback(t)).collect()
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
    let normalized = l2_normalize(pooled);

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
    let normalized = l2_normalize(pooled);

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
    #[ignore] // Requires model to be built
    fn test_burn_embedding() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let emb = generate_embedding(text);

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

        let emb1 = generate_embedding(similar1);
        let emb2 = generate_embedding(similar2);
        let emb3 = generate_embedding(different);

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
