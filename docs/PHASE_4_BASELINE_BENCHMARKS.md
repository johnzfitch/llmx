# Phase 4 Baseline Performance Benchmarks

Baseline performance metrics captured before Phase 5 (semantic search) implementation.

## Test Environment

- Rust: Release build with optimizations
- Benchmark tool: Criterion 0.5
- Date: 2026-01-16

## Results Summary

### Index Creation

| Configuration | Time (avg) | Notes |
|--------------|-----------|-------|
| 10 files × 1KB | 44.4 µs | Very fast for small codebases |
| 50 files × 2KB | 384.6 µs | ~385 µs target for medium projects |
| 100 files × 5KB | 1.53 ms | Scales linearly with file count/size |

**Key Insights:**
- Index creation is **very fast**: < 500 µs for typical small codebases
- Linear scaling: 10x file increase = ~10x time increase
- Well under the 500ms target for 230KB codebase

### Search Performance (BM25)

| Query | Time (avg) | Notes |
|-------|-----------|-------|
| `function` | 1.89 µs | Single token, very fast |
| `test println` | 26.5 µs | Multi-token query |
| `hello world` | 24.2 µs | Multi-token query |

**Key Insights:**
- Search is **extremely fast**: < 30 µs for all queries
- Well under the 10ms target even for complex queries
- Cold cache performance (not measured) will be higher due to disk I/O

### Inverted Index Build

| Chunk Count | Time (avg) | Notes |
|------------|-----------|-------|
| 100 chunks | 22.8 µs | Small index |
| 500 chunks | 132.7 µs | Medium index |
| 1000 chunks | 266.5 µs | Large index |

**Key Insights:**
- Index rebuild is very fast: < 300 µs for 1000 chunks
- Scales linearly: 5x chunks = ~5x time
- Lazy loading with rebuild is practical for all index sizes

### Stats Computation

| File Count | Time (avg) | Notes |
|-----------|-----------|-------|
| 10 files | 3.6 ns | Nearly instant |
| 50 files | 11.7 ns | Nearly instant |
| 100 files | 23.4 ns | Nearly instant |

**Key Insights:**
- Stats computation is negligible overhead
- O(1) complexity with pre-computed data

### Serialization

| Operation | Time (avg) | Notes |
|-----------|-----------|-------|
| Serialize index | 46.1 µs | 50 files, 2KB each |
| Deserialize index | 105.4 µs | 50 files, 2KB each |

**Key Insights:**
- Save index: < 100 µs for JSON serialization
- Load index: ~100 µs for deserialization + rebuild time
- Atomic writes add filesystem overhead (not measured here)

## Performance Goals for Phase 5

### Baseline to Maintain

- [x] Index time: < 500 ms for 230KB codebase ✅ (1.5ms measured)
- [x] Search time: < 10 ms (warm cache) ✅ (26µs measured)
- [x] Save index: < 100 ms ✅ (46µs measured)

### New Metrics to Track in Phase 5

When adding semantic search capabilities:

1. **Embedding Generation**
   - Target: < 50ms per chunk (or batch operation)
   - Track: Cold start vs warm model loading

2. **Vector Search**
   - Target: < 20ms for similarity search
   - Track: Impact of embedding dimensions (384 vs 768)

3. **Hybrid Search**
   - Target: < 50ms total (BM25 + semantic + fusion)
   - Track: Overhead of combining both methods

## Methodology

Benchmarks use Criterion with:
- 100 samples per measurement
- Automatic warmup (3 seconds)
- Statistical outlier detection
- Test data: Generated Rust files with realistic content

## Reproduction

```bash
cd ingestor-core
cargo bench --bench baseline
```

Results are stored in `target/criterion/` with detailed HTML reports.

## Next Steps for Phase 5

1. Add embedding generation benchmarks
2. Add vector search benchmarks
3. Compare hybrid search vs BM25-only
4. Measure memory overhead of storing embeddings
5. Track index file size increase with embeddings
