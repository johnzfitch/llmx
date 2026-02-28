---
chunk_index: 803
ref: "c77f33dbbbea"
id: "c77f33dbbbea1c406dcb1843467233d65aa2906decdc2d75ea512867329c7913"
slug: "specho-v2-analysis--core-system-45-dimensional-feature-extractio"
path: "/home/zack/dev/llmx/docs/SPECHO_V2_ANALYSIS.md"
kind: "markdown"
lines: [14, 31]
token_estimate: 238
content_sha256: "4f760ebd05cd73053523d15f24827d85ae6c2ecd351127a625a4c5624269d87e"
compacted: false
heading_path: ["specho-v2 Analysis Report","Architecture Overview","Core System: 45-Dimensional Feature Extraction"]
symbol: null
address: null
asset_path: null
---

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