---
chunk_index: 840
ref: "effde70c633c"
id: "effde70c633cdb5355840ab58c3f945b7c5d2c4b83c6b6cdcb32de13752ec075"
slug: "stable-backport-patches--web-index-html-remove-display-none-from-stat"
path: "/home/zack/dev/llmx/docs/STABLE_BACKPORT_PATCHES.md"
kind: "markdown"
lines: [56, 73]
token_estimate: 77
content_sha256: "929b90707cd95598abf56f6fb790d7a38646474ea6b8fae52d9b6a15e2bed076"
compacted: false
heading_path: ["LLMX Stable - Exact Code Patches to Apply","PATCH 1: Critical Bug Fixes (Priority: CRITICAL)","web/index.html - Remove display:none from status element"]
symbol: null
address: null
asset_path: null
---

### web/index.html - Remove display:none from status element

**Location:** Find #ingest-status element

**BEFORE:**
```html
<div id="ingest-status" style="display:none;"></div>
```

**AFTER:**
```html
<div id="ingest-status"></div>
```

**Impact:** Allows setStatus() to control visibility dynamically

---