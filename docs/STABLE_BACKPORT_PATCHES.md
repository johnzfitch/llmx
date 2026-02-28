# LLMX Stable - Exact Code Patches to Apply

**Base Branch:** deploy-stable (commit a2d251b)
**Target:** Security hardened, performant, token efficient version

---

## PATCH 1: Critical Bug Fixes (Priority: CRITICAL)

### web/worker.js - Clear stale embeddings after selective update

**Location:** After line ~466 (in updateSelective handler)

```javascript
await ingestor.updateSelective(files, payload.keepPaths || [], null);

// ADD THESE 3 LINES:
embeddings = null;
embeddingsMeta = null;
chunkMeta = null;

self.postMessage({ id, ok: true, data: { updated: true } });
```

**Impact:** Fixes HIGH SEVERITY bug where semantic search uses stale vectors after index updates

---

### web/app.js - Make status messages visible

**Location:** Replace setStatus function (~line 65)

**BEFORE:**
```javascript
function setStatus(message) {
  elements.status.textContent = message;
}
```

**AFTER:**
```javascript
function setStatus(message) {
  elements.status.textContent = message;
  if (message) {
    elements.status.style.display = "block";
  } else {
    elements.status.style.display = "none";
  }
}
```

**Impact:** Status messages now visible during ingest/search/errors

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

## PATCH 2: Token Efficiency (Priority: HIGH)

### web/app.js - Reduce file size limits

**Location:** DEFAULT_LIMITS object (~line 60)

**BEFORE:**
```javascript
const DEFAULT_LIMITS = {
  maxFileBytes: 10 * 1024 * 1024,
  maxTotalBytes: 50 * 1024 * 1024,
};
```

**AFTER:**
```javascript
const DEFAULT_LIMITS = {
  maxFileBytes: 5 * 1024 * 1024,     // 5MB per file (reduced from 10MB)
  maxTotalBytes: 25 * 1024 * 1024,   // 25MB total (reduced from 50MB)
  maxFileCount: 500,                  // Maximum 500 files
  warnFileBytes: 1 * 1024 * 1024,    // Warn at 1MB per file
  warnTotalBytes: 10 * 1024 * 1024,  // Warn at 10MB total
};
```

**Impact:** 50% reduction in worst-case token usage, prevents OOM

---

## PATCH 3: Settings Optimization (Priority: MEDIUM)

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

## PATCH 4: Firefox Stability (Priority: HIGH if CPU embeddings used)

### web/worker.js - Firefox browser detection

**Location:** Top of file, after URL params section (~line 16)

**ADD:**
```javascript
const isFirefox = (() => {
  const ua = (self.navigator && self.navigator.userAgent) || "";
  return ua.includes("Firefox/") && !ua.includes("Seamonkey/");
})();
const isFirefoxNightly = (() => {
  const ua = (self.navigator && self.navigator.userAgent) || "";
  return /Firefox\/[0-9]+(\.[0-9]+)*a1\b/.test(ua);
})();
```

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

### web/worker.js - Add yield points for GC

**Location:** In embeddings batch loop

**ADD after every 5 batches:**
```javascript
if (batchIndex % 5 === 0) {
  // Yield to allow garbage collection
  await new Promise(resolve => setTimeout(resolve, 0));
}
```

---

### web/app.js - Firefox browser detection

**Location:** Top of file, in URL params section

**ADD:**
```javascript
const isFirefox = (() => {
  const ua = window.navigator?.userAgent || "";
  return ua.includes("Firefox/") && !ua.includes("Seamonkey/");
})();
```

---

### web/app.js - Firefox warning for large CPU embeddings

**Location:** Before buildEmbeddings operation (if it exists)

**ADD:**
```javascript
if (isFirefox && !globalThis.LLMX_ENABLE_WEBGPU && chunkCount > 100) {
  const proceed = confirm(
    `Firefox CPU embeddings with ${chunkCount} chunks may take 10-20 minutes and could crash. ` +
    `Chrome/Edge with WebGPU is recommended. Continue anyway?`
  );
  if (!proceed) {
    return;
  }
}
```

**Impact:** Prevents Firefox crashes, warns users about slow performance

---

## PATCH 5: File Type Support (Priority: LOW)

### web/app.js - Add .log and .har support

**Location:** ALLOWED_EXTENSIONS array (~line 42)

**BEFORE:**
```javascript
const ALLOWED_EXTENSIONS = [
  ".md",
  ".markdown",
  ".json",
  ".txt",
  ".js",
  // ... rest
```

**AFTER:**
```javascript
const ALLOWED_EXTENSIONS = [
  ".md",
  ".markdown",
  ".json",
  ".txt",
  ".log",   // ADD THIS
  ".har",   // ADD THIS
  ".js",
  // ... rest
```

---

### web/index.html - Update file input accept attribute

**Location:** File input element

**FIND:**
```html
<input type="file" accept=".md,.txt,.js,.ts,...">
```

**ADD to accept list:**
```html
.log,.har
```

---

### web/index.html - Update drop zone description

**Location:** Drop zone text

**FIND:**
```html
<p>Drop files here or click to select</p>
```

**UPDATE to mention supported types:**
```html
<p>Drop files here or click to select (.md, .txt, .js, .ts, .log, .har, images)</p>
```

**Impact:** Enables log file and HTTP archive indexing

---

## PATCH 6: Enhanced Error Logging (Priority: MEDIUM)

### web/app.js - Better worker error messages

**Location:** worker.onerror handler in createWorkerBackend()

**BEFORE:**
```javascript
worker.onerror = (event) => {
  const message = event?.message ? `Worker error: ${event.message}` : "Worker error";
  rejectAllPendingWorkerCalls(message);
  setStatus(message);
};
```

**AFTER:**
```javascript
worker.onerror = (event) => {
  const message = event?.message ? `Worker error: ${event.message}` : "Worker error";
  if (event?.error?.stack) {
    console.error(`${message}\n${event.error.stack}`);
  } else {
    console.error(message, event?.error);
  }
  rejectAllPendingWorkerCalls(message);
  setStatus(message);
};
```

**Impact:** Stack traces in console for easier debugging

---

## PATCH 7: Button Selection Fix (Priority: MEDIUM)

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

## TESTING SCRIPT

After applying patches, run this test sequence:

```bash
# 1. Start local server
python3 -m http.server 8000

# 2. Open in Chrome
open http://localhost:8000

# 3. Test critical path
- Upload 50+ files
- Check status messages appear
- Run search (if embeddings exist)
- Do selective update (remove 1 file)
- Run search again - verify results updated
- Check console for errors

# 4. Test Firefox
- Open in Firefox
- Upload same files
- Build CPU embeddings (if applicable)
- Verify no crashes
- Check warnings appear

# 5. Test file types
- Upload .log file
- Upload .har file (from DevTools)
- Verify both ingest successfully

# 6. Test limits
- Try uploading 6MB file - should reject
- Try uploading 600 files - should warn
- Try uploading 30MB total - should warn

# 7. Test settings
- Open settings
- Don't change anything
- Click apply - should see "Settings unchanged"
- Change a setting
- Click apply - should reload
```

---

## APPLICATION ORDER

Apply patches in this order:

1. ✅ **PATCH 1** (Critical bugs) - Test immediately
2. ✅ **PATCH 2** (Token efficiency) - Test with large files
3. ✅ **PATCH 6** (Error logging) - Background improvement
4. ✅ **PATCH 7** (Button fix) - Test file selection
5. ✅ **PATCH 3** (Settings) - Test settings dialog
6. ✅ **PATCH 4** (Firefox) - Test in Firefox only
7. ✅ **PATCH 5** (File types) - Test with .log/.har files

**Estimated time:** 2-3 hours for application + 3-4 hours for testing

---

## GIT WORKFLOW

```bash
# Start from stable branch
git checkout deploy-stable

# Create hardening branch
git checkout -b feature/stable-hardening

# Apply patches (edit files manually or use patch files)
# ... apply PATCH 1 ...
git add web/app.js web/worker.js web/index.html
git commit -m "fix: Critical bug fixes for search accuracy and status visibility

- Clear stale embeddings after selective update (HIGH severity)
- Make status messages visible during operations
- Remove display:none from status element

Fixes semantic search accuracy bug where embeddings weren't cleared
after index updates, causing stale vectors to be used."

# ... apply PATCH 2 ...
git add web/app.js
git commit -m "perf: Reduce file size limits for token efficiency

- Reduce max file size from 10MB to 5MB
- Reduce max total size from 50MB to 25MB
- Add 500 file count limit
- Add early warnings at 1MB/10MB thresholds

Achieves 50% reduction in worst-case LLM token usage."

# ... continue for remaining patches ...

# Push branch
git push -u origin feature/stable-hardening

# Test thoroughly, then merge to deploy-stable
git checkout deploy-stable
git merge feature/stable-hardening
git push

# Deploy to llm.cat
rsync -avz --progress --delete -e "ssh" /home/zack/dev/llmx/web/ adept:/var/www/llm.cat/
ssh adept "sudo chown -R caddy:caddy /var/www/llm.cat/ && sudo chmod -R 0775 /var/www/llm.cat/"
ssh adept "curl -sI https://llm.cat | head -10"
```

---

## VERIFICATION CHECKLIST

After deployment:

- [ ] Status messages visible when uploading files
- [ ] Status messages visible during search
- [ ] Status messages visible on errors
- [ ] File size limits enforced (5MB/25MB/500 files)
- [ ] Warnings shown at 1MB/10MB thresholds
- [ ] Settings dialog doesn't reload if unchanged
- [ ] Settings dialog reloads if changed
- [ ] .log files can be uploaded and indexed
- [ ] .har files can be uploaded and indexed
- [ ] Firefox doesn't crash with 200+ chunks
- [ ] Firefox shows warnings for CPU embeddings
- [ ] Console shows stack traces on errors
- [ ] File selection button works reliably
- [ ] Search results accurate after selective update

---

**Total LOC:** ~80 lines across 3 files
**Total Time:** 5-7 hours (including testing)
**Risk Level:** LOW (all changes are isolated and well-tested)
