---
chunk_index: 858
ref: "59ffc6f2aa0c"
id: "59ffc6f2aa0cf94661d52702ac2bc0eb349bdb73c844e86a967c0c7b77a6d3bf"
slug: "stable-backport-patches--web-app-js-ensure-button-triggers-file-selec"
path: "/home/zack/dev/llmx/docs/STABLE_BACKPORT_PATCHES.md"
kind: "markdown"
lines: [326, 351]
token_estimate: 138
content_sha256: "fffe7745ddde7e40945e6ae02807cda4f67c645662e76a4062832bf7356a4562"
compacted: false
heading_path: ["LLMX Stable - Exact Code Patches to Apply","PATCH 7: Button Selection Fix (Priority: MEDIUM)","web/app.js - Ensure button triggers file selection"]
symbol: null
address: null
asset_path: null
---

### web/app.js - Ensure button triggers file selection

**Location:** Button click handler for folder/file selection

**VERIFY this pattern exists:**
```javascript
elements.selectFolder.addEventListener("click", () => {
  // Should trigger file input
  elements.folderInput.click();
  // OR
  elements.filesInput.click();
});
```

**If missing or broken, ADD:**
```javascript
elements.selectFolder.addEventListener("click", (e) => {
  e.preventDefault();
  elements.filesInput.click();
});
```

**Impact:** Reliable file selection button behavior

---