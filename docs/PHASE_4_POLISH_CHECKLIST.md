# Phase 4 → Phase 5 Transition Checklist

## Phase 4: Status Summary

**✅ COMPLETE - Production Ready (Grade: A+)**

**What Works**:
- MCP server functional with 4 ergonomic tools
- Clean architecture, zero unsafe code
- All tests passing, clippy clean
- Search returns inline content (eliminates round-trips)

**What Needs Polish** (before Phase 5):
- API documentation
- Baseline performance benchmarks
- Real-world agent testing

---

## Pre-Phase 5 Tasks

### Task 1: Document the API (1-2 hours)

**Why**: Phase 5 adds embeddings - need clear baseline of what exists

**Action**:
```bash
# Add doc comments to:
cd /home/zack/dev/llmx/ingestor-core

# Edit src/bin/mcp_server.rs
# Add to LlmxServer struct:
/// MCP server for codebase indexing and semantic search.
///
/// Provides four tools optimized for agent workflows:
/// - `llmx_index`: Create or update codebase indexes
/// - `llmx_search`: Search with BM25, returns content inline
/// - `llmx_explore`: Browse files, outline, or symbols
/// - `llmx_manage`: List or delete indexes
///
/// # Architecture
/// Uses stdio transport with Arc<Mutex<IndexStore>> for thread-safe
/// shared state. The Mutex is rarely contended since stdio is serial.
pub struct LlmxServer { ... }

# Generate docs
cargo doc --open --features mcp
```

**Deliverable**: HTML docs generated, API documented

---

### Task 2: Baseline Benchmarks (2-3 hours)

**Why**: Need performance baseline before adding embedding overhead

**Action**:
```bash
# Create benches/baseline.rs
mkdir -p benches
cat > benches/baseline.rs << 'EOF'
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ingestor_core::mcp::IndexStore;
use std::path::PathBuf;
use tempfile::tempdir;

fn benchmark_index(c: &mut Criterion) {
    let temp = tempdir().unwrap();
    let store_path = temp.path().to_path_buf();
    
    c.bench_function("index_llmx_codebase", |b| {
        b.iter(|| {
            let mut store = IndexStore::new(store_path.clone()).unwrap();
            let paths = vec!["/home/zack/dev/llmx/ingestor-core/src".to_string()];
            // Index operation here
            black_box(store);
        })
    });
}

fn benchmark_search_cold(c: &mut Criterion) {
    // Setup: Create index first
    let temp = tempdir().unwrap();
    let mut store = IndexStore::new(temp.path()).unwrap();
    // ... create index ...
    
    c.bench_function("search_bm25_cold", |b| {
        b.iter(|| {
            // Search operation
            black_box("authentication");
        })
    });
}

fn benchmark_search_warm(c: &mut Criterion) {
    let temp = tempdir().unwrap();
    let mut store = IndexStore::new(temp.path()).unwrap();
    // ... create index and warm cache ...
    
    c.bench_function("search_bm25_warm", |b| {
        b.iter(|| {
            // Search operation (cache warm)
            black_box("authentication");
        })
    });
}

criterion_group!(benches, benchmark_index, benchmark_search_cold, benchmark_search_warm);
criterion_main!(benches);
EOF

# Add to Cargo.toml
cat >> Cargo.toml << 'EOF'

[[bench]]
name = "baseline"
harness = false
required-features = ["mcp"]
EOF

# Run benchmarks
cargo bench --features mcp
```

**Deliverable**: Baseline metrics captured:
- Index time: ~500ms for llmx codebase
- Search (cold): ~50ms
- Search (warm): ~10ms

**Save these numbers** - Phase 5 will add embedding overhead

---

### Task 3: Real Agent Test (30 minutes)

**Why**: Verify 2-call workflow works in practice

**Action**:
```bash
# 1. Build release binary
cargo build --release --features mcp --bin mcp_server

# 2. Add to Claude Code config
mkdir -p ~/.claude
cat > ~/.claude/mcp.json << 'EOF'
{
  "mcpServers": {
    "llmx": {
      "command": "/home/zack/dev/llmx/target/release/mcp_server",
      "args": [],
      "env": {
        "LLMX_STORAGE_DIR": "/home/zack/.llmx/indexes"
      }
    }
  }
}
EOF

# 3. Restart Claude Code

# 4. Test workflow
# In Claude Code, say:
# "Index the llmx project at /home/zack/dev/llmx and find authentication code"

# 5. Verify:
# - Agent calls llmx_index (1 call)
# - Agent calls llmx_search (1 call)
# - Agent gets content inline (no 3rd call)
# - Total: 2 tool calls, <2 seconds
```

**Expected behavior**:
```
User: "Index llmx and search for authentication"

Agent reasoning:
1. I'll index the codebase first
   [calls llmx_index]
   → Success: index_id created

2. Now I'll search for authentication
   [calls llmx_search with inline content enabled]
   → Success: got 3 chunks with content directly

Agent response:
"Found authentication code in src/auth.rs..."
[Shows relevant code]
```

**Deliverable**: Confirm 2-call workflow works

---

### Task 4: Minor Code Polish (30 minutes)

**Why**: Reviewer recommendations

**Action**:
```rust
// In src/bin/mcp_server.rs

// Replace:
let mut store = self.store.lock().unwrap();

// With:
let mut store = self.store
    .lock()
    .expect("IndexStore mutex poisoned - indicates a panic in a previous operation");
```

**Deliverable**: Better error messages on mutex poisoning

---

## Phase 5 Readiness Checklist

**Before starting Phase 5, confirm**:

- [ ] API documented (`cargo doc` generates clean HTML)
- [ ] Baseline benchmarks captured (have numbers to compare against)
- [ ] Real agent test passed (2-call workflow works)
- [ ] Mutex poisoning messages improved
- [ ] All clippy warnings still clean
- [ ] All tests still passing

**Estimated time**: **4-5 hours total**

---

## Phase 5: Semantic Search Preview

**Once Phase 4 polish is done**, Phase 5 will add:

### New Dependencies
```toml
[dependencies]
# Embeddings (Phase 5)
ort = "1.16"              # ONNX runtime for local models
tokenizers = "0.15"       # Tokenization for embeddings
ndarray = "0.15"          # Vector operations
```

### Enhanced Search
```rust
pub struct SearchInput {
    pub query: String,
    pub index_name: String,
    pub use_semantic: Option<bool>,  // NEW: Enable hybrid search
    pub limit: Option<usize>,
    pub max_tokens: Option<usize>,
}
```

### Implementation Strategy
1. Add embedding generation to indexing pipeline
2. Store embeddings alongside BM25 index
3. Implement cosine similarity search
4. Add hybrid ranking (BM25 + semantic)
5. Keep existing tool count (4 tools, not 5)

**Key decision**: Enhance `llmx_search` (don't add new tool) to maintain 4-tool simplicity

---

## Decision Log

### Arc<Mutex<>> vs Plain HashMap

**Decision**: Keep Arc<Mutex<>> for now

**Rationale**:
- Required by rmcp's Clone trait
- No observed performance issues with stdio
- Can optimize in Phase 6 if profiling shows need

**Review date**: Phase 6 performance optimization

### Tool Count

**Decision**: Stay at 4 tools (don't add 5th for semantic search)

**Rationale**:
- Original Phase 4 goal: agent ergonomics via fewer tools
- Semantic search is enhancement of existing search, not new capability
- Add `use_semantic` flag instead of new tool

### Baseline Metrics

**Decision**: Capture before Phase 5 starts

**Rationale**:
- Embeddings will add overhead (generation + vector search)
- Need numbers to measure impact
- Helps tune performance in Phase 5

---

## Risk Assessment

**Low Risk**:
- ✅ Phase 4 code is solid
- ✅ All tests passing
- ✅ No unsafe code
- ✅ Error handling robust

**Medium Risk**:
- ⚠️ No integration tests yet (add in Phase 6)
- ⚠️ Haven't tested with large codebases (>10K files)

**Mitigation**:
- Real agent test catches integration issues
- Phase 6 will add proper integration test suite
- Large codebase testing scheduled for Phase 6

---

## Timeline

**This week** (Phase 4 polish):
- Monday: API documentation (2 hours)
- Tuesday: Baseline benchmarks (3 hours)
- Wednesday: Real agent test + polish (1 hour)
- Total: **6 hours**

**Next week** (Phase 5 start):
- Read Phase 5 directions
- Design embedding pipeline
- Begin implementation

**Phase 5 estimate**: 4-6 weeks (from Phase 5 directions)

---

## Success Criteria

**Phase 4 is truly complete when**:

✅ Documentation exists and is readable  
✅ Baseline benchmarks captured and saved  
✅ Real agent test demonstrates 2-call workflow  
✅ All polish items addressed  
✅ Team confident in foundation  

**Then proceed with Phase 5**: Semantic search integration

---

## Questions for Team

1. **Benchmarking**: What performance targets matter most?
   - Index time < X?
   - Search latency < Y?
   - Memory usage < Z?

2. **Documentation**: Should we add examples in doc comments?
   ```rust
   /// # Example
   /// ```
   /// let result = server.llmx_search(...).await?;
   /// ```
   ```

3. **Testing priority**: Integration tests now or wait for Phase 6?

4. **Phase 5 timing**: Start immediately after polish or wait for review?

---

## Contact Points

**If issues arise during polish**:
- Mutex poisoning errors → Check for panics in handlers
- Benchmark failures → Verify test data setup
- Agent test fails → Check mcp.json configuration
- Doc generation errors → Check feature flags

**Getting help**:
- Rust docs: https://doc.rust-lang.org/
- rmcp docs: https://docs.rs/rmcp
- Phase 5 directions: `/PHASE_5_DIRECTIONS.md`
