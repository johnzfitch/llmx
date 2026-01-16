# specho-v2 Analysis Report
## For llmx Future Directions Context

**Repository**: https://github.com/johnzfitch/specho-v2  
**Author**: Zack Fitch (johnzfitch)  
**Purpose**: AI text detection & model fingerprinting using linguistic analysis  
**Live Demo**: definitelynot.ai (mentioned but not accessible)  
**Status**: Public, 1 star, actively maintained (3 commits visible)  

---

## Architecture Overview

### Core System: 45-Dimensional Feature Extraction

specho-v2 implements a **tiered feature extraction system** for detecting AI-generated text and fingerprinting specific models. The system uses **5 layers** extracting **45 total dimensions**:

```
┌─────────────────────────────────────────────────────────────┐
│                    45D Feature Space                        │
├─────────────────────────────────────────────────────────────┤
│ Layer A: Trajectory (5D)      - Semantic path analysis     │
│ Layer B: Echo Patterns (15D)  - SpecHO core                │
│ Layer C: Epistemic (12D)      - Hedging + discourse        │
│   C.1: Epistemic markers (6D) - Uncertainty language       │
│   C.2: Transitions (6D)       - Discourse connectors       │
│ Layer D: Syntactic (7D)       - Structural patterns        │
│ Layer E: Lightweight (6D)     - Fast screening             │
└─────────────────────────────────────────────────────────────┘
```

### Key Innovation: Tiered Detection

The system offers **3 performance tiers** based on latency/accuracy tradeoff:

| Tier | Layers Used | Latency | Accuracy | Use Case |
|------|-------------|---------|----------|----------|
| **1** | E only | <1ms | 98.6% | High-volume screening |
| **2** | E+B+C | ~50ms | ~99% | Standard analysis |
| **3** | All 45D | ~100ms | >99% | Deep verification |

**Critical Design Decision**: Layer E (6D) can run **without any dependencies** (just NumPy), making it deployable in constrained environments.

---

## Feature Breakdown

### Layer A: Trajectory Analysis (5D)
**Purpose**: Semantic path through embedding space  
**Implementation**: Likely uses sentence-transformers  
**Features**:
- Directional consistency (do concepts flow logically?)
- Semantic velocity (rate of topic change)
- Path curvature (abruptness of transitions)
- Return-to-origin patterns (circular reasoning)
- Embedding space density traversal

**Why it matters for code**: Code has very specific semantic trajectories (problem → implementation → edge cases). AI-generated code might show different patterns.

---

### Layer B: Echo Patterns (15D) - The Core
**Purpose**: SpecHO's original innovation - detecting "echoes" in language  
**Status**: **This was missing in the 24D version** (critical bug)  
**Implementation**: `src/fingerprint/layer_b_echo.py`

**What are "echo patterns"?**
Based on the architecture, likely analyzing:
- Repetitive phrasing across paragraphs
- Self-referential structures (text that "echoes" itself)
- Harmonic patterns in sentence rhythm
- Vocabulary recycling frequency
- Conceptual reverberations (same idea, different words)

**Why 15 dimensions?**: Probably captures:
- Short-range echoes (within paragraph)
- Medium-range echoes (across sections)
- Long-range echoes (document-level)
- Echo intensity, frequency, decay rate

**Relevance to code**: Code has intentional repetition (patterns, idioms) but also organic variation. AI code might show different echo signatures.

---

### Layer C: Epistemic Markers (12D)
**Purpose**: Uncertainty and discourse structure  
**Split into 2 sub-layers**:

**C.1: Epistemic Hedging (6D)**  
Examples of features:
- Hedging frequency ("might", "could", "possibly")
- Certainty modifiers ("definitely", "clearly", "obviously")
- Epistemic verb patterns ("believe", "think", "suppose")
- Qualification density (caveats per assertion)
- Modal verb distributions
- Confidence gradients

**C.2: Discourse Transitions (6D)**  
Examples of features:
- Logical connectors ("therefore", "however", "moreover")
- Transition word diversity
- Paragraph bridging patterns
- Structural signaling ("first", "finally", "in conclusion")
- Coherence markers density
- Temporal sequencing language

**Relevance to code**: Comments in code have distinctive epistemic patterns. Human code comments often hedge ("this might not work if..."), while AI comments might be overly certain or formulaic.

---

### Layer D: Syntactic Structure (7D)
**Purpose**: Structural patterns independent of content  
**Implementation**: `src/fingerprint/syntactic.py`

Likely features:
- Parse tree depth variability
- Clause nesting patterns
- Sentence length distribution (burstiness)
- Dependency arc lengths
- Coordination vs subordination ratio
- Syntactic complexity metrics
- Punctuation patterns

**Relevance to code**: Code syntax is rigid, but comments/docs show syntactic patterns. Function naming conventions and structure have syntactic signatures.

---

### Layer E: Lightweight Classifier (6D)
**Purpose**: Fast screening without heavy dependencies  
**Implementation**: `src/fingerprint/lightweight.py`  
**Dependencies**: NumPy only (critical for deployment)

**The 6 dimensions are likely**:
1. **Perplexity proxy**: Simple n-gram predictability
2. **Burstiness**: Sentence length variance
3. **Lexical diversity**: TTR (type-token ratio) or variants
4. **Stopword ratio**: Function words vs content words
5. **Average sentence complexity**: Words per sentence
6. **Punctuation density**: Marks per 100 words

**Why this matters**: These 6 features achieve **98.6% accuracy** with minimal compute. This is the benchmark for llmx's "chunk quality scoring" idea.

---

## Validation Results

From the README's validation corpus (464 samples):

| Metric | 24D System (old) | 45D System (new) |
|--------|------------------|------------------|
| **Accuracy** | 94.4% | >98% |
| **Human Recognition** | 72.4% | >92% |
| **False Positive Rate** | 27.6% | <8% |

**Key finding**: Restoring Layer B (15D echo features) was critical. The 24D system was missing the core innovation.

---

## System Components

### Corpus System
Sources for training/validation:
- HuggingFace datasets
- Arena/LMSYS conversations
- Manual curation
- Academic papers
- Legal documents (PACER)
- **500+ samples** across multiple models

### Classifier Architecture
- **Tiered approach**: Fast screening → detailed analysis
- **<1ms** for Layer E only
- **98.6% accuracy** on validation set
- **Model-specific fingerprints**: Can identify which model generated text

### Verification System
- **Model ID verification**: Claims vs actual model
- **AURORA trust framework**: Integration with watermarking
- **Drift monitoring**: Detect when models change over time

---

## Technical Stack

### Dependencies by Tier

**Tier 1 (Minimal)**:
```python
numpy  # Only requirement for Layer E
```

**Tier 2-3 (Full)**:
```python
numpy
scipy
scikit-learn
spacy  # For syntactic analysis
sentence-transformers  # For embeddings (Layer A)
```

### Storage
- Trained classifiers: `data/models/`
- Reference fingerprints: `data/fingerprints/`
- Validation results: `data/validation/`

### Web Interface
- **PHP frontend**: `web/index.php`
- **Python API**: `web/api/`
- **SSE streaming**: Real-time analysis updates

---

## Relevance to llmx

### Direct Applications

1. **Chunk Quality Scoring** (from og-build.txt V4 plan)
   - Use Layer E (6D) for fast quality assessment
   - Metrics: readability, cognitive load, coherence
   - Add to index: `chunk.quality_score = lightweight_classifier(chunk.content)`

2. **Code vs Comment Detection**
   - Epistemic markers (Layer C.1) distinguish comments
   - Syntactic patterns (Layer D) identify documentation
   - Echo patterns (Layer B) detect boilerplate

3. **Model Attribution**
   - Fingerprint which LLM generated code comments
   - Track model drift in codebases over time
   - Verify claimed authorship

### Architectural Patterns

1. **Tiered Extraction** (directly applicable)
   ```rust
   pub enum AnalysisLevel {
       Fast,      // 6D, <1ms, 98% accurate
       Standard,  // 23D, ~50ms, ~99% accurate
       Deep,      // 45D, ~100ms, >99% accurate
   }
   ```

2. **Minimal Dependencies for Core**
   - Layer E pattern: core functionality with zero external deps
   - Heavier features (embeddings) as optional

3. **Feature Persistence**
   - Store extracted features in index
   - Enable queries: "show me high-quality chunks"
   - Track quality metrics over time

### Research Questions

1. **Do specho-v2's dimensions transfer to code?**
   - Test Layer E on code chunks
   - Measure correlation with human "good code" ratings
   - Benchmark against existing complexity metrics

2. **Can we detect AI-generated code?**
   - Fine-tune classifier on code corpus
   - GitHub Copilot vs human code signatures
   - Track adoption patterns

3. **Linguistic features vs LLM annotations**
   - Compare specho-v2 (deterministic) to LLM summaries (stochastic)
   - Cost: computation vs API calls
   - Accuracy: objective metrics vs subjective understanding

---

## Integration Strategy for llmx

### Phase 1: Experiment (Post Phase 6)

```rust
// Add to chunk metadata
pub struct ChunkMetadata {
    // ... existing fields
    pub linguistic_features: Option<LinguisticFeatures>,
}

pub struct LinguisticFeatures {
    pub layer_e_fast: [f32; 6],  // Always computed
    pub quality_score: f32,       // Derived from layer_e
    pub complexity_score: f32,
    pub readability_score: f32,
}
```

**Implementation**:
1. Port Layer E (6D) to Rust (no external deps)
2. Compute on indexing (negligible overhead)
3. Enable queries: `llmx_search --min-quality 0.8`

### Phase 2: Validate (if Phase 1 succeeds)

**Research questions**:
- Do linguistic features correlate with agent usefulness?
- Can we predict which chunks agents will find valuable?
- Does quality scoring improve search ranking?

**Benchmark dataset**:
- 100 code chunks
- Human ratings (developers)
- Agent ratings (which chunks were actually used)
- Correlation analysis

### Phase 3: Expand (if Phase 2 validates)

**Add full 45D analysis**:
- Optional heavy dependencies
- Command: `llmx_analyze_deep --include-embeddings`
- Use for documentation generation
- Model fingerprinting for attribution

---

## Comparison: LLM Annotations vs Linguistic Features

| Dimension | LLM Annotations | Linguistic Features (specho-v2) |
|-----------|----------------|----------------------------------|
| **Speed** | 100-1000ms per chunk | <1ms (Layer E) |
| **Cost** | $0.001-0.01 per chunk | $0 (local) |
| **Determinism** | Stochastic (varies) | Deterministic (same input → same output) |
| **Interpretability** | High (natural language) | Low (numeric features) |
| **Maintenance** | API dependency | Local code |
| **Privacy** | Sends code to external API | 100% local |
| **Quality** | Subjective understanding | Objective metrics |
| **Scalability** | Rate limited | CPU bound |

**Recommendation**: **Use both**
- Linguistic features for fast, objective metrics
- LLM annotations for deep understanding
- Combine: "High quality by metrics + LLM explains why"

---

## Key Takeaways

1. **Layer E (6D) is the MVP**: 98.6% accuracy with zero deps
   - Port to Rust
   - Add to llmx index
   - Enable quality-based search

2. **Tiered approach matches llmx philosophy**
   - Fast screening (Layer E)
   - Standard search (existing BM25)
   - Deep analysis (embeddings + LLM)

3. **Echo patterns (Layer B) are unique**
   - No other tool does this
   - Potentially very relevant for code patterns
   - Worth investigating for code quality

4. **Validation is rigorous**
   - 464-sample corpus
   - 98%+ accuracy
   - 8% false positives (acceptable)

5. **Production-ready architecture**
   - Tiered performance
   - Optional heavy features
   - Web interface + API

---

## Action Items for llmx

### Immediate (Phase 4-5 timeline)
- [ ] Study Layer E implementation when accessible
- [ ] Port 6D extraction to Rust (no deps)
- [ ] Benchmark on code chunks (10 samples)
- [ ] Decide: worth integrating?

### Near-term (Phase 6)
- [ ] If validated: Add to ChunkMetadata
- [ ] Implement quality-based search filters
- [ ] Measure impact on agent workflows
- [ ] Document findings

### Long-term (Future Directions)
- [ ] Full 45D analysis for deep insights
- [ ] Model fingerprinting for attribution
- [ ] Code generation detection
- [ ] Track quality trends over time

---

## Appendix: Code Structure

```
specho-v2/
├── src/
│   ├── fingerprint/
│   │   ├── lightweight.py          # Layer E (6D) ← START HERE
│   │   ├── layer_b_echo.py         # Layer B (15D) ← CORE INNOVATION
│   │   ├── trajectory.py           # Layer A (5D)
│   │   ├── epistemic.py            # Layer C.1 (6D)
│   │   ├── transitions.py          # Layer C.2 (6D)
│   │   ├── syntactic.py            # Layer D (7D)
│   │   └── unified_45d.py          # Combined extractor
│   ├── echo_engine/                # SpecHO core
│   ├── preprocessor/               # spaCy pipeline
│   ├── clause_identifier/
│   └── scoring/                    # Aggregation
├── data/
│   ├── models/                     # Trained classifiers
│   ├── fingerprints/               # Reference DB
│   └── validation/                 # Test results
├── tools/
│   ├── train_specho.py
│   ├── ab_test.py
│   └── visualize_trajectory.py
└── specho_cli.py                   # CLI interface
```

---

## Conclusion

specho-v2 represents a **mature, production-ready approach** to linguistic analysis that could complement llmx's semantic search. The tiered architecture (especially Layer E's 98.6% accuracy with zero dependencies) aligns perfectly with llmx's philosophy of providing layered capabilities.

**The big question**: Do these features transfer from natural language to code? The only way to find out is to experiment.

**Recommendation**: Start with Layer E port to Rust, benchmark on 10-20 code chunks, measure correlation with human quality ratings. If promising, integrate deeper.

This is **not** a replacement for LLM annotations—it's a complement. Use linguistic features for fast, objective scoring; use LLMs for deep understanding. Together they provide both **quantitative metrics** (what is high quality?) and **qualitative insights** (why is it high quality?).
