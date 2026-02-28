---
chunk_index: 844
ref: "dbfac67ead19"
id: "dbfac67ead19b1a97a7813fdc525476749645c7e19f0d420d48da0155739c0f4"
slug: "stable-backport-patches--web-app-js-prevent-unnecessary-page-reloads"
path: "/home/zack/dev/llmx/docs/STABLE_BACKPORT_PATCHES.md"
kind: "markdown"
lines: [105, 139]
token_estimate: 290
content_sha256: "c955308987e42160b455856d3c58fb32292dba1c24c1696283c48aee15abf43b"
compacted: false
heading_path: ["LLMX Stable - Exact Code Patches to Apply","PATCH 3: Settings Optimization (Priority: MEDIUM)","web/app.js - Prevent unnecessary page reloads"]
symbol: null
address: null
asset_path: null
---

### web/app.js - Prevent unnecessary page reloads

**Location:** In settings apply button handler (find where settings are applied)

**ADD THIS CHECK before window.location.href assignment:**

```javascript
// Check if settings actually changed
const currentParams = new URLSearchParams(window.location.search);
const currentEmbeddings = currentParams.get("embeddings") === "1";
const currentCpu = currentParams.get("cpu") === "1";
const currentForceWebgpu = currentParams.get("force_webgpu") === "1";
const currentAutoEmbeddings = currentParams.get("auto_embeddings") === "1";

const newEmbeddings = newParams.get("embeddings") === "1";
const newCpu = newParams.get("cpu") === "1";
const newForceWebgpu = newParams.get("force_webgpu") === "1";
const newAutoEmbeddings = newParams.get("auto_embeddings") === "1";

if (currentEmbeddings === newEmbeddings &&
    currentCpu === newCpu &&
    currentForceWebgpu === newForceWebgpu &&
    currentAutoEmbeddings === newAutoEmbeddings) {
  setStatus("Settings unchanged");
  return;
}

// Only reload if settings actually changed
window.location.href = newUrl;
```

**Impact:** Better UX, no wasted page reloads

---