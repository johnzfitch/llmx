---
chunk_index: 486
ref: "2c8be1b4e5ef"
id: "2c8be1b4e5efe3599ab75926c411a63cb19c4ae56a9b4056c038c3c45e36d3fc"
slug: "phase-4-baseline-benchmarks--stats-computation"
path: "/home/zack/dev/llmx/docs/PHASE_4_BASELINE_BENCHMARKS.md"
kind: "markdown"
lines: [52, 63]
token_estimate: 79
content_sha256: "7bc4dfd453509532539898ff9764053252e548a5c67aeabd560f4d6b7cb6d3a1"
compacted: false
heading_path: ["Phase 4 Baseline Performance Benchmarks","Results Summary","Stats Computation"]
symbol: null
address: null
asset_path: null
---

### Stats Computation

| File Count | Time (avg) | Notes |
|-----------|-----------|-------|
| 10 files | 3.6 ns | Nearly instant |
| 50 files | 11.7 ns | Nearly instant |
| 100 files | 23.4 ns | Nearly instant |

**Key Insights:**
- Stats computation is negligible overhead
- O(1) complexity with pre-computed data