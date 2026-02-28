---
chunk_index: 1009
ref: "2a7cbba0ff1c"
id: "2a7cbba0ff1c1158b89c0d3cfea6ad57d2ba8f6bf9d23ab249ec7113596dfdb3"
slug: "llmx-v5-design-prompt--option-b-semantic-bundles"
path: "/home/zack/dev/llmx/docs/llmx-v5-design-prompt.md"
kind: "markdown"
lines: [130, 139]
token_estimate: 75
content_sha256: "6913ad62bda8faf2f96ff57c06dcea8ff466675eda2d6d28e27c8c158551a372"
compacted: false
heading_path: ["LLMX v5 Format Redesign - External Review Prompt","Proposed Directions","Option B: Semantic Bundles"]
symbol: null
address: null
asset_path: null
---

### Option B: Semantic Bundles
Group by meaning, not by arbitrary chunks.
```
project.llmx/
├── README.md           # Manifest + overview (500 tokens)
├── core_45k.md         # Core functionality bundled
├── utils_12k.md        # Utilities bundled
└── docs_8k.md          # Documentation bundled
```