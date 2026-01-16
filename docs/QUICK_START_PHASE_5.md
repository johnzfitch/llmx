# Phase 5 Quick Start Guide

**TL;DR**: Add semantic search to existing BM25 search. Keep 4 tools. Target: <100ms.

---

## 🚀 30-Second Start

```bash
# 1. Read this first
cat docs/PHASE_5_DIRECTIONS.md

# 2. Understand current search
code src/mcp/tools.rs:254        # llmx_search_handler
code src/index.rs                # BM25 implementation

# 3. Check baseline performance
cat docs/PHASE_4_BASELINE_BENCHMARKS.md

# 4. Start implementing
# See "Implementation Steps" below
```

---

## 📋 Implementation Steps

### Step 1: Add Dependencies (5 min)
```toml
# In ingestor-core/Cargo.toml
[dependencies]
ort = "1.16"              # ONNX runtime
tokenizers = "0.15"       # Tokenization
ndarray = "0.15"          # Vector operations
```

### Step 2: Modify Data Structures (10 min)
```rust
// src/model.rs

pub struct IndexFile {
    // ... existing fields ...
    pub embeddings: Option<Vec<Vec<f32>>>,  // NEW: One per chunk
}

pub struct SearchInput {
    // ... existing fields ...
    pub use_semantic: Option<bool>,  // NEW: Enable hybrid search
}
```

### Step 3: Add Embedding Generation (2-3 hours)
```rust
// src/embeddings.rs (new file)

pub struct EmbeddingGenerator {
    model: ort::Session,
    tokenizer: Tokenizer,
}

impl EmbeddingGenerator {
    pub fn new() -> Result<Self> {
        // Load ONNX model (all-MiniLM-L6-v2 recommended)
        // 384-dimensional embeddings
    }

    pub fn generate(&self, text: &str) -> Result<Vec<f32>> {
        // Tokenize → ONNX inference → L2 normalize
    }
}
```

### Step 4: Update Indexing Pipeline (1-2 hours)
```rust
// src/mcp/tools.rs:173

pub fn llmx_index_handler(...) -> Result<IndexOutput> {
    // ... existing indexing ...

    // NEW: Generate embeddings
    let generator = EmbeddingGenerator::new()?;
    let mut embeddings = Vec::new();
    for chunk in &index.chunks {
        embeddings.push(generator.generate(&chunk.content)?);
    }
    index.embeddings = Some(embeddings);

    // ... save index ...
}
```

### Step 5: Add Vector Search (2-3 hours)
```rust
// src/index.rs (add new function)

pub fn vector_search(
    chunks: &[Chunk],
    embeddings: &[Vec<f32>],
    query_embedding: &[f32],
    limit: usize,
) -> Vec<SearchResult> {
    // Compute cosine similarity for each chunk
    // Sort by similarity (descending)
    // Return top N
}
```

### Step 6: Implement Hybrid Ranking (2-3 hours)
```rust
// src/index.rs (add new function)

pub fn hybrid_search(
    chunks: &[Chunk],
    inverted_index: &InvertedIndex,
    embeddings: &[Vec<f32>],
    query: &str,
    query_embedding: &[f32],
    limit: usize,
) -> Vec<SearchResult> {
    // 1. Get BM25 scores
    let bm25_results = search_index(...);

    // 2. Get semantic scores
    let semantic_results = vector_search(...);

    // 3. Normalize both scores to [0, 1]
    // 4. Combine: score = 0.5 * bm25 + 0.5 * semantic
    // 5. Re-rank and return top N
}
```

### Step 7: Update Search Handler (1 hour)
```rust
// src/mcp/tools.rs:254

pub fn llmx_search_handler(...) -> Result<SearchOutput> {
    let index = store.load(&input.index_id)?;

    let search_results = if input.use_semantic == Some(true) {
        // NEW: Hybrid search
        let generator = EmbeddingGenerator::new()?;
        let query_embedding = generator.generate(&input.query)?;

        hybrid_search(
            &index.chunks,
            &index.inverted_index,
            index.embeddings.as_ref().unwrap(),
            &input.query,
            &query_embedding,
            limit * 2,
        )
    } else {
        // Existing BM25-only search
        search(&index, &input.query, filters, limit * 2)
    };

    // ... apply token budget (keep existing logic) ...
}
```

### Step 8: Test & Benchmark (2 hours)
```bash
# 1. Unit tests
cargo test

# 2. Add semantic benchmarks
# Edit benches/baseline.rs, add:
# - benchmark_embedding_generation
# - benchmark_vector_search
# - benchmark_hybrid_search

# 3. Run benchmarks
cargo bench --bench baseline

# 4. Compare to Phase 4 baseline
# Target: <100ms for hybrid search
```

---

## 🎯 Success Criteria

### Must Have
- [x] Embeddings generated during indexing
- [x] Vector search working
- [x] Hybrid ranking implemented
- [x] `use_semantic` flag working
- [x] Token budgeting still works

### Performance
- [x] Hybrid search: <100ms (target)
- [x] BM25-only: No regression (should be 1.9-26.5µs still)
- [x] Memory: <2× Phase 4

### Quality
- [x] Tests passing
- [x] Clippy clean
- [x] Documentation updated

---

## ⚠️ Common Pitfalls

1. **Don't add 5th tool** - Enhance `llmx_search`, don't create `llmx_semantic_search`
2. **Don't break token budgeting** - Apply budget after hybrid ranking
3. **Don't store embeddings if too large** - Consider trade-offs (disk vs computation)
4. **Don't ignore BM25 mode** - Make `use_semantic` optional, default to BM25

---

## 📊 Key Metrics to Track

### Phase 4 Baseline (maintain these)
- Index 10 files: 44µs
- Index 100 files: 1.5ms
- Search (BM25): 1.9-26.5µs

### Phase 5 Targets (new)
- Embedding generation: <50ms per chunk
- Vector search: <20ms
- Hybrid search total: <100ms

---

## 📚 Read These Files

### Must Read (30 min)
1. `docs/PHASE_5_DIRECTIONS.md` - Full implementation plan
2. `src/mcp/tools.rs:254` - Current search handler
3. `src/index.rs` - BM25 implementation

### Should Read (15 min)
4. `docs/PHASE_4_BASELINE_BENCHMARKS.md` - Performance targets
5. `docs/AGENT_HANDOFF.md` - Full context

### Reference (as needed)
6. `src/model.rs` - Data structures
7. `src/mcp/storage.rs` - IndexStore design

---

## 🔧 Quick Commands

```bash
# Build & test
cargo build --release --features mcp --bin llmx-mcp
cargo test
cargo clippy --all-features

# Benchmarks
cargo bench --bench baseline

# Documentation
cargo doc --open --features mcp

# Test with Claude Code
# 1. Build: cargo build --release --features mcp --bin llmx-mcp
# 2. Add to ~/.claude/mcp.json (see PHASE_4_MCP_VERIFICATION.md)
# 3. Ask: "Index llmx and search for 'BM25 scoring' using semantic search"
```

---

## 💡 Design Decisions

### Embedding Model
**Recommended**: `all-MiniLM-L6-v2`
- Size: 80MB ONNX model
- Dimensions: 384 (good balance)
- Speed: ~10ms per chunk
- Quality: Good for code search

**Alternative**: `all-mpnet-base-v2`
- Size: 420MB
- Dimensions: 768
- Speed: ~30ms per chunk
- Quality: Better, but slower

### Hybrid Scoring
**Simple approach** (start here):
```rust
final_score = 0.5 * normalize(bm25_score) + 0.5 * normalize(semantic_score)
```

**Advanced** (optimize later):
- Reciprocal Rank Fusion (RRF)
- Learned weights per query type
- Dynamic weight adjustment

### Storage Strategy
**Option 1**: Store embeddings in IndexFile (simple)
- Pro: Fast search (no regeneration)
- Con: Larger disk usage (~1.5KB per chunk)

**Option 2**: Regenerate on load (Phase 4 pattern)
- Pro: Smaller disk usage
- Con: ~10ms per chunk load time

**Recommendation**: Start with Option 1, optimize later if disk space is an issue.

---

## 🚦 Phase 5 Workflow

```
Day 1-2: Setup & Understanding
├─ Read docs
├─ Understand current implementation
└─ Add dependencies

Day 3-5: Embedding Generation
├─ Load ONNX model
├─ Implement tokenization
└─ Test on sample chunks

Day 6-8: Vector Search
├─ Cosine similarity
├─ Efficient search algorithm
└─ Benchmark

Day 9-11: Hybrid Ranking
├─ Combine BM25 + semantic
├─ Normalize scores
└─ Test relevance

Day 12-14: Integration & Testing
├─ Update search handler
├─ Add benchmarks
├─ Test with Claude Code
└─ Documentation

Total: 2-3 weeks
```

---

## ✅ Final Checklist

Before starting:
- [ ] Read `PHASE_5_DIRECTIONS.md`
- [ ] Understand current search (tools.rs:254)
- [ ] Review baseline benchmarks

During Phase 5:
- [ ] All Phase 4 tests still passing
- [ ] New semantic tests added
- [ ] Benchmarks show <100ms hybrid search
- [ ] Documentation updated

Before marking Phase 5 complete:
- [ ] Manual Claude Code test passing
- [ ] Clippy clean
- [ ] Performance targets met
- [ ] Handoff document for Phase 6

---

**Ready? Start with `docs/PHASE_5_DIRECTIONS.md`! 🎯**
