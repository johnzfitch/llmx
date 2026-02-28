---
chunk_index: 847
ref: "dca9111a8fea"
id: "dca9111a8fea9cf91d0f883534576c15651408bbca3ef5708078e02f7e00e814"
slug: "stable-backport-patches--web-worker-js-reduce-batch-size-for-firefox"
path: "/home/zack/dev/llmx/docs/STABLE_BACKPORT_PATCHES.md"
kind: "markdown"
lines: [160, 175]
token_estimate: 79
content_sha256: "9bdbced72714ded52e5cb524d4e7b016f73cd21b7f7edea473af651bd0ce9434"
compacted: false
heading_path: ["LLMX Stable - Exact Code Patches to Apply","PATCH 4: Firefox Stability (Priority: HIGH if CPU embeddings used)","web/worker.js - Reduce batch size for Firefox"]
symbol: null
address: null
asset_path: null
---

### web/worker.js - Reduce batch size for Firefox

**Location:** In buildEmbeddings handler, where batch processing happens

**BEFORE:**
```javascript
const batchSize = 8; // or whatever the current value is
```

**AFTER:**
```javascript
const batchSize = isFirefox ? 1 : 2; // Firefox needs smaller batches
```

---