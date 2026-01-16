# Phase 5: Semantic Search & Embeddings Integration

## Overview
Add semantic search capabilities alongside existing BM25, enabling hybrid search that combines keyword matching with meaning-based retrieval.

## Background
BM25 excels at exact keyword matching but struggles with:
- Synonyms ("fix" vs "repair" vs "correct")
- Conceptual queries ("error handling patterns")
- Code semantics (finding similar logic patterns)

Embeddings complement BM25 by capturing semantic meaning.

## Primary Objectives

### 1. Embedding Generation Pipeline
**Goal**: Generate and store embeddings for all indexed chunks

**Tasks**:
- [ ] Add embedding model integration (options: local ONNX, OpenAI, Anthropic)
- [ ] Implement batched embedding generation to optimize API calls
- [ ] Store embeddings in index format (consider binary format for size)
- [ ] Add embedding version tracking for cache invalidation

**Model Selection Criteria**:
```rust
// Priority order for embedding providers
1. Local ONNX (e5-small-v2, all-MiniLM-L6) - fast, free, private
2. Voyage AI - optimized for code, excellent quality
3. OpenAI text-embedding-3-small - good balance
4. Claude embeddings - when available
```

**Storage Schema**:
```rust
pub struct ChunkWithEmbedding {
    pub chunk_id: String,
    pub content: String,
    pub embedding: Vec<f32>,  // 384 or 768 dimensions
    pub embedding_model: String,  // track model version
    pub embedding_timestamp: u64,
}
```

### 2. Vector Search Implementation
**Goal**: Fast similarity search over embeddings

**Approach Options**:
- **Simple**: Brute force cosine similarity (works for <10K chunks)
- **Scalable**: HNSW index via `hnswlib-rs` (works for 100K+ chunks)
- **Hybrid**: Start simple, upgrade when needed

**Tasks**:
- [ ] Implement cosine similarity search
- [ ] Add HNSW indexing (optional, for large codebases)
- [ ] Profile search performance (target: <100ms for 10K chunks)
- [ ] Add similarity score threshold configuration

**Search Interface**:
```rust
pub struct VectorSearchInput {
    pub query: String,
    pub index_name: String,
    pub top_k: usize,
    pub min_similarity: f32,  // threshold for results
}

pub struct VectorSearchResult {
    pub chunk_id: String,
    pub similarity_score: f32,
    pub chunk_content: String,
}
```

### 3. Hybrid Search Strategy
**Goal**: Combine BM25 and semantic search intelligently

**Fusion Approaches**:
1. **Reciprocal Rank Fusion (RRF)** - simple, works well
2. **Linear combination** - weight BM25 vs vector scores
3. **Cascade** - BM25 first, then rerank with vectors

**Implementation**:
```rust
pub struct HybridSearchInput {
    pub query: String,
    pub index_name: String,
    pub strategy: HybridStrategy,  // RRF, Linear, Cascade
    pub bm25_weight: f32,  // default 0.5
    pub semantic_weight: f32,  // default 0.5
    pub top_k: usize,
}

pub enum HybridStrategy {
    ReciprocalRankFusion { k: usize },
    LinearCombination,
    Cascade { bm25_top_k: usize },
}
```

**Tasks**:
- [ ] Implement RRF fusion (start here, it's robust)
- [ ] Add linear combination option
- [ ] Make strategy configurable per search
- [ ] Benchmark hybrid vs pure BM25 on test queries

### 4. Tool Updates for Semantic Search
**New tools to add**:
- [ ] `llmx_semantic_search` - vector-only search
- [ ] `llmx_hybrid_search` - combined BM25 + semantic (recommended default)
- [ ] `llmx_generate_embeddings` - manually trigger embedding generation

**Enhanced `llmx_search`**:
- [ ] Add `use_semantic` flag (default: false for backwards compat)
- [ ] Add `hybrid_strategy` parameter
- [ ] Return separate BM25 and semantic scores in results

### 5. Embedding Cache & Updates
**Goal**: Avoid regenerating embeddings unnecessarily

**Cache Strategy**:
```rust
pub struct EmbeddingCache {
    // Map from (content_hash, model_id) -> embedding
    cache: HashMap<(String, String), Vec<f32>>,
    max_size: usize,  // LRU eviction
}
```

**Tasks**:
- [ ] Implement content-based cache keying (hash of chunk content)
- [ ] Add LRU eviction for cache size limits
- [ ] Persist cache to disk between runs
- [ ] Handle model version changes (invalidate old embeddings)

**Update Logic**:
```
On index update:
  1. Identify changed chunks (content hash comparison)
  2. Generate embeddings only for new/modified chunks
  3. Reuse cached embeddings for unchanged chunks
  4. Update vector index incrementally
```

## Performance Targets
- [ ] Embedding generation: <2s per 100 chunks (with batching)
- [ ] Vector search: <100ms for 10K chunks
- [ ] Hybrid search: <150ms for 10K chunks
- [ ] Index update: <5s for 1000 new chunks (incremental)

## Testing Plan
1. **Unit tests**: 
   - Cosine similarity calculation
   - RRF fusion logic
   - Embedding cache hit/miss

2. **Integration tests**:
   - End-to-end embedding generation
   - Hybrid search returns relevant results
   - Cache persistence across restarts

3. **Quality tests**:
   - Curate 20 test queries with expected results
   - Compare BM25-only vs hybrid search recall/precision
   - Verify semantic search finds synonyms/concepts

4. **Performance tests**:
   - Benchmark search latency at 1K, 10K, 100K chunks
   - Profile embedding generation throughput
   - Measure memory usage with loaded embeddings

## Implementation Order
1. **Week 1**: Embedding generation pipeline + storage
2. **Week 2**: Vector search (cosine similarity, brute force)
3. **Week 3**: RRF hybrid search + tool integration
4. **Week 4**: Embedding cache + incremental updates

## Dependencies
**New crates to add**:
```toml
# For embeddings
ort = "1.16"  # ONNX runtime (local models)
tokenizers = "0.15"  # for local model tokenization

# For vector search
ndarray = "0.15"  # array operations for embeddings
hnsw = { version = "0.11", optional = true }  # scalable vector search

# For HTTP clients (if using API-based embeddings)
reqwest = { version = "0.11", features = ["json"] }
```

## Configuration
Add to `~/.llmx/config.toml`:
```toml
[embeddings]
provider = "local"  # or "openai", "voyage", "anthropic"
model = "e5-small-v2"  # model identifier
batch_size = 32  # chunks per batch
cache_dir = "~/.llmx/embedding_cache"

[search]
default_strategy = "hybrid"  # or "bm25", "semantic"
bm25_weight = 0.5
semantic_weight = 0.5
min_similarity = 0.3  # threshold for semantic results
```

## Success Criteria
- [ ] Can generate embeddings for indexed codebase
- [ ] Semantic search finds conceptually similar code
- [ ] Hybrid search outperforms BM25 on synonym/concept queries
- [ ] Incremental updates reuse cached embeddings
- [ ] Search latency remains <150ms for typical codebases

## Known Challenges
1. **Model selection**: Balance between quality, speed, and privacy
2. **Dimensionality**: Higher dims = better quality but slower search
3. **Token limits**: Long code chunks may need truncation/splitting
4. **Cold start**: First embedding generation is slow; need progress indicator
5. **Cost**: API-based embeddings cost money; local models solve this

## Future Optimizations (Post-Phase 5)
- Quantize embeddings (768 float32 → 768 int8) for 4x memory reduction
- GPU acceleration for embedding generation
- Streaming embeddings generation with progress reporting
- Cross-encoder reranking for top results
- Query expansion using LLM before search

## Resources
- [Sentence Transformers](https://www.sbert.net/) - embedding model reference
- [HNSW paper](https://arxiv.org/abs/1603.09320) - scalable vector search
- [RRF explanation](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)
- Local models: https://huggingface.co/sentence-transformers

## Notes
- Start with local ONNX models for privacy and cost
- RRF is more robust than linear combination (fewer hyperparameters)
- Consider lazy loading embeddings (only in memory when needed)
- Semantic search shines for natural language queries about code
- BM25 still better for exact identifier matching
