---
chunk_index: 64
ref: "eda60e22fd6a"
id: "eda60e22fd6ab01c6336d220d732919051cfb2244667916f12f406526ee267c9"
slug: "claude-model-selection-for-c--assistant"
path: "/home/zack/dev/llmx/claude_Model selection for current project_2026-01-17.md"
kind: "markdown"
lines: [603, 635]
token_estimate: 325
content_sha256: "e9d015b68fb3c3baedb8838e289bdae6215e0229f514fde0f18b2e8fb16d822f"
compacted: false
heading_path: ["Model selection for current project","Assistant"]
symbol: null
address: null
asset_path: null
---

## Assistant

The user is asking whether to hardcode bge-small-en-v1.5 or support multiple models. Let me think about this practically.

For a Phase 6 implementation focused on browser deployment:

1. **Hardcode bge-small-en-v1.5** is the right call because:
   - 384-dim matches Phase 5 hash-based (no index format changes)
   - 33MB is reasonable for browser download
   - One model = one test surface
   - Can always add more later

2. **Multiple models** adds:
   - UI for model selection
   - Multiple tokenizer.json files
   - Different embedding dimensions to handle
   - More testing permutations
   - Larger bundle/CDN hosting

The user seems to want simplicity ("dump files, get results instantly"). Hardcoding aligns with that.

Recommendation: Option 1, hardcode bge-small-en-v1.5. Ship it, validate it works, add model selection in a future phase if users ask for it.**Option 1. Hardcode it.**

Reasons:
- 384-dim matches Phase 5 hash-based → no index format changes
- 33MB is acceptable browser download
- One model = one test matrix
- Burn's ONNX import generates model-specific Rust code at compile time anyway

Add model selection later if users actually ask for it. They won't care which embedding model powers their search—they care that it works fast.

Ship → validate → iterate.