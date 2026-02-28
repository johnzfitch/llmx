---
chunk_index: 42
ref: "6de4cfba27b7"
id: "6de4cfba27b7a546d5ae93a0156e5aeedb7c307555f7bd7e5e95e0ac23a469ce"
slug: "claude-model-selection-for-c--assistant"
path: "/home/zack/dev/llmx/claude_Model selection for current project_2026-01-17.md"
kind: "markdown"
lines: [68, 120]
token_estimate: 750
content_sha256: "11ffe5801da06ea17542b9cc8cb3c1bc55cb0d6db5ac31d20005657710970c42"
compacted: false
heading_path: ["Model selection for current project","Assistant"]
symbol: null
address: null
asset_path: null
---

## Assistant

The user wants me to consolidate the uploaded files into 1-2 files:
1. One for the agent building out Phase 6
2. One for agent work after Phase 6 (enhancements)

Let me read all the uploaded files to understand what's in each, then consolidate them appropriately.
Hmm, I can't access the files directly through the filesystem tool. But I already have the SEMANTIC_SEARCH_GUIDE.md in the context. Let me use bash to read the other files.
Now I have all the content. Let me consolidate:

1. **P6_DIRECTIONS.md** - For the agent building Phase 6
   - Should include: ONNX model upgrade info (with better model recommendations), RRF implementation, configurable weights, large codebase testing, key implementation notes from P5_HANDOFF.md
   - Should be concise but complete

2. **POST_P6_ENHANCEMENTS.md** - For work after Phase 6
   - Should include: Cutting edge enhancements (SPLADE, memory-mapped, self-tuning, prefetching, quality scoring, incremental indexing, SIMD)
   - User guide info isn't needed for the agent - it's for end users
   - The completion report is historical, not needed going forward

Key insight from user's original question: `all-MiniLM-L6-v2` is dated. I should update the P6 directions with better model recommendations:
- `bge-small-en-v1.5` as drop-in replacement (384-dim)
- `nomic-embed-text-v1.5` as quality option (768-dim with Matryoshka)

Let me create two focused docs.Got it. The P5 handoff is comprehensive but recommends `all-MiniLM-L6-v2` which, as you noted, is dated. Let me consolidate into two focused docs with updated model recommendations.
Good, now let me create the post-P6 enhancements doc. I'll pull from CUTTING_EDGE_ENHANCEMENTS.md but tighten it up considerably.
Now let me present the files to the user.
Done. I've consolidated the docs:

1. **P6_DIRECTIONS.md** - Agent building Phase 6
   - Updated model recommendations (bge-small-en-v1.5, nomic-embed-text-v1.5)
   - ONNX integration specifics
   - RRF implementation
   - Backward compatibility
   - Testing checklist

2. **POST_P6_ENHANCEMENTS.md** - After P6
   - Tiered by impact/effort
   - Quality scoring, memory-mapped (Tier 1)
   - SPLADE, incremental indexing (Tier 2)
   - Self-tuning, prefetching (Tier 3)
   - SIMD (Tier 4)

Dropped:
- SEMANTIC_SEARCH_GUIDE.md (user-facing, not agent directions)
- PHASE_5_COMPLETION_REPORT.md (historical)
- Redundant content from P5_HANDOFF.mdConsolidated to two focused docs:

**P6_DIRECTIONS.md** — ONNX integration with updated model recommendations (`bge-small-en-v1.5` as drop-in, `nomic-embed-text-v1.5` for quality), RRF fusion, backward compat requirements, testing checklist.

**POST_P6_ENHANCEMENTS.md** — Prioritized by impact/effort. Tier 1 (quality scoring + mmap) should probably be Phase 7. SPLADE is interesting but heavier lift.

Dropped the user guide (SEMANTIC_SEARCH_GUIDE.md) since that's end-user docs, not agent directions. The completion report is historical. Kept the actionable bits from the handoff.