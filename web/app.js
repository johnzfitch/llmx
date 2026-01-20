const state = {
  backend: null,
  workerReady: false,
  indexLoaded: false,
  busy: false,
  indexId: null,
  sourceLabel: null,
  files: [],
  searchSeq: 0,
};

const elements = {
  status: document.getElementById("ingest-status"),
  selectFolder: document.getElementById("select-folder"),
  folderInput: document.getElementById("folder-input"),
  filesInput: document.getElementById("files-input"),
  dropZone: document.getElementById("drop-zone"),
  query: document.getElementById("query"),
  pathFilter: document.getElementById("path-filter"),
  fileFilter: document.getElementById("file-filter"),
  outlineFilter: document.getElementById("outline-filter"),
  symbolFilter: document.getElementById("symbol-filter"),
  kindFilter: document.getElementById("kind-filter"),
  runSearch: document.getElementById("run-search"),
  buildEmbeddings: document.getElementById("build-embeddings"),
  results: document.getElementById("results"),
  chunkView: document.getElementById("chunk-view"),
  chunkTitle: document.getElementById("chunk-title"),
  chunkContent: document.getElementById("chunk-content"),
  closeChunk: document.getElementById("close-chunk"),
  downloadExport: document.getElementById("download-export"),
  downloadIndexJson: document.getElementById("download-index-json"),
  indexId: document.getElementById("index-id"),
  chunkCount: document.getElementById("chunk-count"),
  warningCount: document.getElementById("warning-count"),
  warningList: document.getElementById("warning-list"),
  backendInfo: document.getElementById("backend-info"),
  settingEmbeddings: document.getElementById("setting-embeddings"),
  settingForceCpu: document.getElementById("setting-force-cpu"),
  settingForceWebgpu: document.getElementById("setting-force-webgpu"),
  settingAutoEmbeddings: document.getElementById("setting-auto-embeddings"),
  applySettings: document.getElementById("apply-settings"),
  resetSettings: document.getElementById("reset-settings"),
};

const urlParams = (() => {
  try {
    return new URL(window.location.href).searchParams;
  } catch {
    return new URLSearchParams();
  }
})();
const embeddingsRequested = urlParams.get("embeddings") === "1";
const forceCpu = urlParams.get("cpu") === "1";
const webGpuParam = urlParams.get("webgpu");
const webGpuRequested = webGpuParam === "1" || (embeddingsRequested && webGpuParam !== "0" && !forceCpu);
const autoEmbeddingsRequested = urlParams.get("auto_embeddings") === "1";
const forceWebGpu = urlParams.get("force_webgpu") === "1";
const isFirefox = (() => {
  const ua = window.navigator?.userAgent || "";
  return ua.includes("Firefox/") && !ua.includes("Seamonkey/");
})();
const isFirefoxNightly = (() => {
  const ua = window.navigator?.userAgent || "";
  // Nightly user agents usually look like: "Firefox/123.0a1"
  return /Firefox\/[0-9]+(\.[0-9]+)*a1\b/.test(ua);
})();
const webGpuAvailable = Boolean(window.navigator && window.navigator.gpu);
globalThis.LLMX_ENABLE_WEBGPU = webGpuRequested && webGpuAvailable;
globalThis.LLMX_ENABLE_EMBEDDINGS = embeddingsRequested;
if (webGpuRequested && !webGpuAvailable) {
  console.warn(
    "WebGPU unavailable (navigator.gpu missing). To use embeddings, either use a WebGPU-capable Chromium browser or add ?cpu=1 to allow slow CPU embeddings."
  );
}
if (webGpuRequested && isFirefox && !isFirefoxNightly && !forceWebGpu) {
  globalThis.LLMX_ENABLE_WEBGPU = false;
  console.warn(
    "WebGPU requested on Firefox, but is disabled by default due to stability issues. Use Chromium, use Firefox Nightly, or add ?force_webgpu=1 to override."
  );
}
if (embeddingsRequested && !globalThis.LLMX_ENABLE_WEBGPU && !forceCpu) {
  console.warn(
    "Embeddings require WebGPU by default. Use a WebGPU-capable Chromium browser, or add ?cpu=1 to allow slow CPU embeddings. On Firefox, add ?force_webgpu=1 to override the default WebGPU disable."
  );
}
const shouldAutoBuildEmbeddings =
  embeddingsRequested && (globalThis.LLMX_ENABLE_WEBGPU || (autoEmbeddingsRequested && forceCpu));

const ALLOWED_EXTENSIONS = [
  ".md",
  ".markdown",
  ".json",
  ".txt",
  ".log",
  ".har",
  ".js",
  ".ts",
  ".tsx",
  ".html",
  ".htm",
  ".png",
  ".jpg",
  ".jpeg",
  ".webp",
  ".gif",
  ".bmp",
];
const SKIP_DIRS = [".git", "node_modules", "target", "dist", "build", ".cache"];
const DEFAULT_LIMITS = {
  maxFileBytes: 5 * 1024 * 1024,     // 5MB per file (reduced from 10MB)
  maxTotalBytes: 25 * 1024 * 1024,   // 25MB total (reduced from 50MB)
  maxFileCount: 500,                  // Maximum 500 files
  warnFileBytes: 1 * 1024 * 1024,    // Warn at 1MB per file
  warnTotalBytes: 10 * 1024 * 1024,  // Warn at 10MB total
};

function setStatus(message) {
  elements.status.textContent = message;
}

function updateBackendInfo(backendType, capabilities) {
  let info = backendType;
  if (capabilities) {
    const parts = [];
    if (capabilities.embeddings) {
      if (capabilities.webgpu) {
        parts.push("WebGPU");
      } else if (capabilities.forceCpu) {
        parts.push("CPU");
      }
    }
    if (parts.length > 0) {
      info += ` (${parts.join(", ")})`;
    }
  }
  elements.backendInfo.textContent = info;
}

function hasFolderPickerSupport() {
  const supportsDirectoryPicker = typeof window.showDirectoryPicker === "function";
  const supportsWebkitDirectory = elements.folderInput && "webkitdirectory" in elements.folderInput;
  return supportsDirectoryPicker || supportsWebkitDirectory;
}

function configureFolderPickerUi() {
  // Button now triggers file selection which is always supported
  return;
}

let workerCallId = 0;
const pendingCalls = new Map();

function formatErrorForUi(error) {
  const message = error instanceof Error ? error.message : String(error || "Unknown error");
  const cleaned = message.replace(/\s+/g, " ").trim();
  return cleaned.length > 240 ? `${cleaned.slice(0, 237)}...` : cleaned;
}

function rejectAllPendingWorkerCalls(message) {
  for (const pending of pendingCalls.values()) {
    pending.reject(new Error(message));
  }
  pendingCalls.clear();
}

function callWorker(op, payload, transfer) {
  if (!state.backend) {
    return Promise.reject(new Error("Backend not initialized"));
  }
  return state.backend.call(op, payload, transfer);
}

function createWorkerBackend() {
  const workerUrl = new URL("./worker.js", import.meta.url);
  // Pass feature flags via URL search params (current approach)
  // Note: Could alternatively pass config via postMessage after worker creation,
  // which would be more robust for blob URLs, but current approach works for module workers
  workerUrl.search = window.location.search || "";
  const worker = new Worker(workerUrl, { type: "module" });
  worker.onmessage = (event) => {
    const msg = event.data || {};
    const pending = pendingCalls.get(msg.id);
    if (!pending) {
      return;
    }
    pendingCalls.delete(msg.id);
    if (msg.ok) {
      pending.resolve(msg.data);
    } else {
      pending.reject(new Error(msg.error || "Worker error"));
    }
  };
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
  worker.onmessageerror = () => {
    const message = "Worker message error (structured clone failed).";
    rejectAllPendingWorkerCalls(message);
    setStatus(message);
  };

  return {
    kind: "worker",
    call(op, payload, transfer) {
      const id = ++workerCallId;
      return new Promise((resolve, reject) => {
        pendingCalls.set(id, { resolve, reject });
        worker.postMessage({ id, op, payload }, transfer || []);
      });
    },
    terminate() {
      rejectAllPendingWorkerCalls("Worker terminated.");
      worker.terminate();
    },
  };
}

// Local backend runs WASM in the main thread (for debugging/comparison)
// Note: Intentionally duplicates worker.js embedding logic for architectural separation
// Worker backend: runs in separate thread, doesn't block UI
// Local backend: runs in main thread, useful for debugging and comparison
async function createLocalBackend() {
  const wasmModule = await import("./pkg/ingestor_wasm.js");
  const wasmUrl = new URL("./pkg/ingestor_wasm_bg.wasm", import.meta.url);
  await wasmModule.default({ module_or_path: wasmUrl });
  const WasmIngestor = wasmModule.Ingestor;
  const WasmEmbedder = wasmModule.Embedder;
  let ingestor = null;
  let embedder = null;
  let embeddings = null; // Float32Array
  let embeddingsMeta = null; // { dim, count, modelId }
  let chunkMeta = null; // Array<{ id, ref, path, kind, start_line, end_line, heading_path, heading_joined, symbol, snippet }>
  let buildEmbeddingsPromise = null;

  function snippet(text, maxChars) {
    if (!text) return "";
    const cleaned = String(text).replace(/\s+/g, " ").trim();
    return cleaned.length > maxChars ? `${cleaned.slice(0, maxChars - 3)}...` : cleaned;
  }

  function passesFilters(chunk, filters) {
    if (!filters) return true;
    if (filters.path_exact && chunk.path !== filters.path_exact) return false;
    if (filters.path_prefix && !chunk.path.startsWith(filters.path_prefix)) return false;
    if (filters.kind && chunk.kind !== filters.kind) return false;
    if (filters.heading_prefix && !chunk.heading_joined.startsWith(filters.heading_prefix)) return false;
    if (filters.symbol_prefix) {
      if (!chunk.symbol) return false;
      if (!chunk.symbol.startsWith(filters.symbol_prefix)) return false;
    }
    return true;
  }

  function shouldUseEmbeddings() {
    if (!embeddings || !embeddingsMeta || !chunkMeta) return false;
    if (!embedder) return false;
    if (embeddingsMeta.modelId !== embedder.modelId()) return false;
    if (embeddingsMeta.count !== chunkMeta.length) return false;
    return true;
  }

  function dotProduct(a, b, bOffset, dim) {
    let sum = 0;
    for (let i = 0; i < dim; i += 1) {
      sum += a[i] * b[bOffset + i];
    }
    return sum;
  }

  function rrfFuse(bm25Results, semanticResults, limit) {
    const k = 60;
    const scores = new Map();

    function addList(results) {
      results.forEach((result, rank) => {
        const prev = scores.get(result.chunk_id) || 0;
        scores.set(result.chunk_id, prev + 1 / (k + rank + 1));
      });
    }

    addList(bm25Results);
    addList(semanticResults);

    return Array.from(scores.entries())
      .map(([chunkId, score]) => ({ chunkId, score }))
      .sort((a, b) => b.score - a.score)
      .slice(0, limit);
  }

  function buildSearchResult(meta, score) {
    return {
      chunk_id: meta.id,
      chunk_ref: meta.ref,
      score,
      path: meta.path,
      start_line: meta.start_line,
      end_line: meta.end_line,
      snippet: meta.snippet,
      heading_path: meta.heading_path,
    };
  }

  async function ensureEmbedder() {
    if (!embedder) {
      embedder = await WasmEmbedder.create();
    }
    return embedder;
  }

  async function buildEmbeddingsIndex() {
    if (!ingestor) {
      throw new Error("No index loaded");
    }
    const embed = await ensureEmbedder();
    const json = ingestor.exportIndexJson();
    const index = JSON.parse(json);

    const chunks = Array.isArray(index.chunks) ? index.chunks : [];
    const refs = index.chunk_refs || {};
    const dim = embed.dimension();
    const modelId = embed.modelId();

    chunkMeta = chunks.map((chunk) => {
      const headingPath = Array.isArray(chunk.heading_path) ? chunk.heading_path : [];
      const headingJoined = headingPath.join("/");
      return {
        id: chunk.id,
        ref: refs[chunk.id] || chunk.short_id || "",
        path: chunk.path || "",
        kind: chunk.kind || null,
        start_line: chunk.start_line || 0,
        end_line: chunk.end_line || 0,
        heading_path: headingPath,
        heading_joined: headingJoined,
        symbol: chunk.symbol || null,
        snippet: snippet(chunk.content || "", 200),
        content: chunk.content || "",
      };
    });

    const count = chunkMeta.length;
    const view = new Float32Array(count * dim);
    const batchSize = 8;

    for (let offset = 0; offset < count; offset += batchSize) {
      const batch = chunkMeta.slice(offset, offset + batchSize);
      const texts = batch.map((item) => item.content);
      const out = embed.embedBatch(texts);
      if (!out || typeof out.length !== "number") {
        throw new Error("Embedding batch returned unexpected type");
      }
      for (let i = 0; i < batch.length; i += 1) {
        const emb = out[i];
        if (!(emb instanceof Float32Array) || emb.length !== dim) {
          throw new Error("Embedding batch returned invalid vector");
        }
        view.set(emb, (offset + i) * dim);
      }

      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    embeddings = view;
    embeddingsMeta = { dim, count, modelId };

    for (const meta of chunkMeta) {
      delete meta.content;
    }

    return embeddingsMeta;
  }

  return {
    kind: "local",
    async call(op, payload) {
      switch (op) {
        case "ping":
          return { ready: true };
        case "initEmbedder": {
          const embed = await ensureEmbedder();
          return { modelId: embed.modelId(), dimension: embed.dimension() };
        }
        case "ingest": {
          const files = (payload.files || []).map((file) => ({
            path: file.path,
            data: new Uint8Array(file.data),
            mtime_ms: file.mtime_ms ?? null,
            fingerprint_sha256: file.fingerprint_sha256 ?? null,
          }));
          ingestor = WasmIngestor.ingest(files, null);
          embeddings = null;
          embeddingsMeta = null;
          chunkMeta = null;
          buildEmbeddingsPromise = null;
          return { indexId: ingestor.indexId() };
        }
        case "updateSelective": {
          if (!ingestor) throw new Error("No index loaded");
          const files = (payload.files || []).map((file) => ({
            path: file.path,
            data: new Uint8Array(file.data),
            mtime_ms: file.mtime_ms ?? null,
            fingerprint_sha256: file.fingerprint_sha256 ?? null,
          }));
          await ingestor.updateSelective(files, payload.keepPaths || [], null);
          embeddings = null;
          embeddingsMeta = null;
          chunkMeta = null;
          buildEmbeddingsPromise = null;
          return { updated: true };
        }
        case "loadIndexJson":
          ingestor = WasmIngestor.fromIndexJson(payload.json);
          embeddings = null;
          embeddingsMeta = null;
          chunkMeta = null;
          buildEmbeddingsPromise = null;
          return { loaded: true };
        case "buildEmbeddings": {
          if (!buildEmbeddingsPromise) {
            buildEmbeddingsPromise = buildEmbeddingsIndex().finally(() => {
              buildEmbeddingsPromise = null;
            });
          }
          const meta = await buildEmbeddingsPromise;
          return { meta };
        }
        case "getEmbeddings": {
          if (!embeddings || !embeddingsMeta) {
            return { embeddings: null };
          }
          const buffer = embeddings.buffer.slice(0);
          return { embeddings: buffer, meta: embeddingsMeta };
        }
        case "setEmbeddings": {
          if (!ingestor) throw new Error("No index loaded");
          const { embeddings: buffer, meta } = payload || {};
          if (!(buffer instanceof ArrayBuffer)) {
            throw new Error("Embeddings payload must be an ArrayBuffer");
          }
          if (!meta || typeof meta.dim !== "number" || typeof meta.count !== "number") {
            throw new Error("Embeddings metadata missing");
          }
          const dim = meta.dim;
          const count = meta.count;
          if (dim <= 0 || count < 0) {
            throw new Error("Embeddings metadata invalid");
          }
          const view = new Float32Array(buffer);
          if (view.length !== dim * count) {
            throw new Error("Embeddings buffer length mismatch");
          }
          const json = ingestor.exportIndexJson();
          const index = JSON.parse(json);
          const chunks = Array.isArray(index.chunks) ? index.chunks : [];
          if (chunks.length !== count) {
            throw new Error("Embeddings count does not match index chunk count");
          }
          const refs = index.chunk_refs || {};
          chunkMeta = chunks.map((chunk) => {
            const headingPath = Array.isArray(chunk.heading_path) ? chunk.heading_path : [];
            const headingJoined = headingPath.join("/");
            return {
              id: chunk.id,
              ref: refs[chunk.id] || chunk.short_id || "",
              path: chunk.path || "",
              kind: chunk.kind || null,
              start_line: chunk.start_line || 0,
              end_line: chunk.end_line || 0,
              heading_path: headingPath,
              heading_joined: headingJoined,
              symbol: chunk.symbol || null,
              snippet: snippet(chunk.content || "", 200),
            };
          });
          embeddings = view;
          embeddingsMeta = meta;
          return { loaded: true };
        }
        case "exportIndexJson":
          if (!ingestor) throw new Error("No index loaded");
          return { json: ingestor.exportIndexJson() };
        case "stats":
          if (!ingestor) throw new Error("No index loaded");
          return { stats: await ingestor.stats() };
        case "warnings":
          if (!ingestor) throw new Error("No index loaded");
          return { warnings: await ingestor.warnings() };
        case "search":
          if (!ingestor) throw new Error("No index loaded");
          {
            const query = payload.query || "";
            const filters = payload.filters || null;
            const limit = payload.limit || 20;

            const bm25Results = await ingestor.search(query, filters, limit * 2);

            if (!embeddings || !embeddingsMeta || !chunkMeta) {
              return { results: bm25Results };
            }

            await ensureEmbedder();
            if (!shouldUseEmbeddings()) {
              return { results: bm25Results };
            }

            const queryEmbedding = embedder.embed(query);
            const dim = embeddingsMeta.dim;

            const semantic = [];
            for (let i = 0; i < chunkMeta.length; i += 1) {
              const meta = chunkMeta[i];
              if (!passesFilters(meta, filters)) continue;
              const score = dotProduct(queryEmbedding, embeddings, i * dim, dim);
              semantic.push({ idx: i, score });
            }

            semantic.sort((a, b) => b.score - a.score);
            const semanticTop = semantic.slice(0, limit * 2).map(({ idx, score }) => {
              const meta = chunkMeta[idx];
              return buildSearchResult(meta, score);
            });

            const merged = rrfFuse(bm25Results, semanticTop, limit);

            const bm25ById = new Map(bm25Results.map((r) => [r.chunk_id, r]));
            const results = merged.map(({ chunkId, score }) => {
              const existing = bm25ById.get(chunkId);
              if (existing) {
                return { ...existing, score };
              }
              const idx = chunkMeta.findIndex((m) => m.id === chunkId);
              if (idx !== -1) {
                return buildSearchResult(chunkMeta[idx], score);
              }
              return {
                chunk_id: chunkId,
                chunk_ref: "",
                score,
                path: "",
                start_line: 0,
                end_line: 0,
                snippet: "",
                heading_path: [],
              };
            });

            return { results };
          }
        case "getChunk":
          if (!ingestor) throw new Error("No index loaded");
          return { chunk: await ingestor.getChunk(payload.chunkId) };
        case "listOutline":
          if (!ingestor) throw new Error("No index loaded");
          return { outline: await ingestor.listOutline(payload.path) };
        case "listSymbols":
          if (!ingestor) throw new Error("No index loaded");
          return { symbols: await ingestor.listSymbols(payload.path) };
        case "exportLlm":
          if (!ingestor) throw new Error("No index loaded");
          return { content: ingestor.exportLlm() };
        case "exportLlmPointer":
          if (!ingestor) throw new Error("No index loaded");
          return { content: ingestor.exportLlmPointer() };
        case "exportOutline":
          if (!ingestor) throw new Error("No index loaded");
          return { content: ingestor.exportLlm() };
        case "exportZip":
          if (!ingestor) throw new Error("No index loaded");
          return { bytes: ingestor.exportZip() };
        case "exportZipCompact":
          if (!ingestor) throw new Error("No index loaded");
          return { bytes: ingestor.exportZipCompact() };
        case "files":
          if (!ingestor) throw new Error("No index loaded");
          return { files: await ingestor.files() };
        case "indexId":
          if (!ingestor) throw new Error("No index loaded");
          return { indexId: ingestor.indexId() };
        default:
          throw new Error(`Unknown op: ${op}`);
      }
    },
    terminate() {},
  };
}

async function initWorker() {
  let initError = null;

  try {
    state.backend = createWorkerBackend();
    const result = await callWorker("ping", {});
    if (!result.ready) {
      throw new Error("Worker did not initialize");
    }
    state.workerReady = true;

    // Query worker capabilities and update UI
    try {
      const caps = await callWorker("getCapabilities", {});
      updateBackendInfo("Worker", caps);
    } catch (capError) {
      console.warn("Failed to get worker capabilities:", capError);
      elements.backendInfo.textContent = "Worker";
    }

    setStatus("Ready for ingestion.");
    await populateSavedIndexes();
    return;
  } catch (error) {
    initError = error;
    try {
      state.backend?.terminate?.();
    } catch {}
    state.backend = null;
  }

  const local = await createLocalBackend();
  const result = await local.call("ping", {});
  if (!result.ready) {
    throw new Error("WASM did not initialize");
  }
  state.backend = local;
  state.workerReady = true;
  const reason = initError ? ` (${formatErrorForUi(initError)})` : "";

  // Update backend info for local mode
  const caps = {
    embeddings: embeddingsRequested,
    webgpu: globalThis.LLMX_ENABLE_WEBGPU,
    forceCpu,
  };
  updateBackendInfo("Local", caps);

  setStatus(`Ready for ingestion (worker disabled)${reason}.`);
  await populateSavedIndexes();
}

if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => {
    if (state.backend) {
      state.backend.terminate();
    }
  });
}

function isAllowedPath(path) {
  const lower = path.toLowerCase();
  if (SKIP_DIRS.some((dir) => lower.includes(`/${dir}/`) || lower.startsWith(`${dir}/`))) {
    return false;
  }
  return ALLOWED_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

async function collectFilesFromInput(fileList) {
  const entries = [];
  let totalBytes = 0;
  let skippedLarge = 0;
  let skippedTotal = 0;
  let skippedCount = 0;
  let rootName = null;
  for (const file of fileList) {
    const path = file.webkitRelativePath || file.name;
    if (!rootName && file.webkitRelativePath) {
      const first = String(file.webkitRelativePath).split("/")[0];
      if (first) {
        rootName = first;
      }
    }
    if (!isAllowedPath(path)) {
      continue;
    }
    if (entries.length >= DEFAULT_LIMITS.maxFileCount) {
      skippedCount += 1;
      continue;
    }
    if (file.size > DEFAULT_LIMITS.maxFileBytes) {
      skippedLarge += 1;
      continue;
    }
    if (totalBytes + file.size > DEFAULT_LIMITS.maxTotalBytes) {
      skippedTotal += 1;
      continue;
    }
    entries.push({ path, file });
    totalBytes += file.size;
  }
  return { entries, skippedLarge, skippedTotal, skippedCount, totalBytes, rootName };
}

async function collectFilesFromHandle(handle, basePath = "", budget = null, rootName = null) {
  const entries = [];
  const shared = budget || {
    totalBytes: 0,
    fileCount: 0,
    skippedLarge: 0,
    skippedTotal: 0,
    skippedCount: 0,
  };
  for await (const [name, entry] of handle.entries()) {
    if (entry.kind === "directory") {
      if (SKIP_DIRS.includes(name)) {
        continue;
      }
      const nested = await collectFilesFromHandle(entry, `${basePath}${name}/`, shared, rootName);
      entries.push(...nested.entries);
      continue;
    }
    const path = `${basePath}${name}`;
    if (!isAllowedPath(path)) {
      continue;
    }
    if (shared.fileCount >= DEFAULT_LIMITS.maxFileCount) {
      shared.skippedCount += 1;
      continue;
    }
    const file = await entry.getFile();
    if (file.size > DEFAULT_LIMITS.maxFileBytes) {
      shared.skippedLarge += 1;
      continue;
    }
    if (shared.totalBytes + file.size > DEFAULT_LIMITS.maxTotalBytes) {
      shared.skippedTotal += 1;
      continue;
    }
    entries.push({ path, file });
    shared.totalBytes += file.size;
    shared.fileCount += 1;
  }
  return {
    entries,
    skippedLarge: shared.skippedLarge,
    skippedTotal: shared.skippedTotal,
    skippedCount: shared.skippedCount,
    totalBytes: shared.totalBytes,
    rootName,
  };
}

function isImagePath(path) {
  const lower = path.toLowerCase();
  return (
    lower.endsWith(".png") ||
    lower.endsWith(".jpg") ||
    lower.endsWith(".jpeg") ||
    lower.endsWith(".webp") ||
    lower.endsWith(".gif") ||
    lower.endsWith(".bmp")
  );
}

function hexEncode(bytes) {
  let out = "";
  for (const b of bytes) {
    out += b.toString(16).padStart(2, "0");
  }
  return out;
}

async function sha256Hex(data) {
  const buffer = data instanceof ArrayBuffer ? data : data.buffer;
  const digest = await crypto.subtle.digest("SHA-256", buffer);
  return hexEncode(new Uint8Array(digest));
}

async function fingerprintFile(file) {
  const size = file.size;
  const headLen = 4096;
  const tailLen = 4096;
  if (size <= headLen + tailLen) {
    const full = await file.arrayBuffer();
    return sha256Hex(full);
  }
  const head = await file.slice(0, headLen).arrayBuffer();
  const tail = await file.slice(size - tailLen, size).arrayBuffer();

  const sizeBytes = new Uint8Array(8);
  const view = new DataView(sizeBytes.buffer);
  view.setBigUint64(0, BigInt(size), true);

  const combined = new Uint8Array(sizeBytes.byteLength + head.byteLength + tail.byteLength);
  combined.set(sizeBytes, 0);
  combined.set(new Uint8Array(head), sizeBytes.byteLength);
  combined.set(new Uint8Array(tail), sizeBytes.byteLength + head.byteLength);
  return sha256Hex(combined);
}

function sanitizeFilenameBase(input) {
  const raw = String(input || "").trim().toLowerCase();
  if (!raw) return "project";
  let out = "";
  let prevDash = false;
  for (const ch of raw) {
    const isOk =
      (ch >= "a" && ch <= "z") ||
      (ch >= "0" && ch <= "9") ||
      ch === "-" ||
      ch === "_" ||
      ch === ".";
    if (isOk) {
      out += ch;
      prevDash = false;
    } else if (!prevDash) {
      out += "-";
      prevDash = true;
    }
  }
  out = out.replace(/^-+/, "").replace(/-+$/, "");
  return out || "project";
}

function inferSourceLabel(entries, collectedMeta) {
  const direct = collectedMeta?.rootName;
  if (direct && String(direct).trim()) {
    return String(direct).trim();
  }

  const counts = new Map();
  for (const entry of entries || []) {
    const path = entry?.path || "";
    const first = path.includes("/") ? path.split("/")[0] : "";
    if (!first) continue;
    counts.set(first, (counts.get(first) || 0) + 1);
  }
  if (!counts.size) return null;
  let best = null;
  let bestCount = 0;
  for (const [name, count] of counts.entries()) {
    if (count > bestCount) {
      best = name;
      bestCount = count;
    }
  }
  const total = (entries || []).length || 1;
  if (best && bestCount / total >= 0.6) {
    return best;
  }
  return null;
}

function exportBaseName() {
  const label = sanitizeFilenameBase(state.sourceLabel || "project");
  const id = typeof state.indexId === "string" ? state.indexId : "";
  const shortId = id ? id.slice(0, 8) : "";
  return shortId ? `${label}.llmx-${shortId}` : `${label}.llmx`;
}

function updateExportUiLabels() {
  const base = exportBaseName();
  if (elements.downloadExport) {
    elements.downloadExport.textContent = `Download ${base}.zip`;
    elements.downloadExport.title = `Download export bundle: ${base}.zip`;
  }
  if (elements.downloadIndexJson) {
    elements.downloadIndexJson.textContent = `Download ${base}.index.json`;
    elements.downloadIndexJson.title = `Download index file: ${base}.index.json`;
  }
}

async function runIngest(entries, collectedMeta) {
  if (!state.workerReady) {
    setStatus("Backend not ready.");
    return;
  }
  if (!entries.length) {
    setStatus(`No supported files found. Accepted: ${ALLOWED_EXTENSIONS.join(", ")}`);
    return;
  }
  const skippedLarge = collectedMeta?.skippedLarge || 0;
  const skippedTotal = collectedMeta?.skippedTotal || 0;
  const skippedCount = collectedMeta?.skippedCount || 0;
  const totalBytes = collectedMeta?.totalBytes || 0;
  const skippedNote =
    skippedLarge || skippedTotal || skippedCount
      ? ` (skipped: ${skippedLarge} too large, ${skippedTotal} over total limit, ${skippedCount} too many files)`
      : "";

  // Warn if approaching limits
  if (totalBytes > DEFAULT_LIMITS.warnTotalBytes) {
    console.warn(`Large upload: ${(totalBytes / 1024 / 1024).toFixed(1)}MB. Browser may slow down.`);
  }
  if (entries.length > DEFAULT_LIMITS.maxFileCount * 0.8) {
    console.warn(`Many files: ${entries.length}. Processing may take time.`);
  }

  const prevByPath = new Map((state.files || []).map((meta) => [meta.path, meta]));
  const currentPaths = new Set(entries.map((e) => e.path));
  let removedCount = 0;
  if (state.indexLoaded) {
    for (const path of prevByPath.keys()) {
      if (!currentPaths.has(path)) {
        removedCount += 1;
      }
    }
  }

  try {
    state.busy = true;
    state.sourceLabel = inferSourceLabel(entries, collectedMeta);

    if (!state.indexLoaded) {
      setStatus(`Ingesting ${entries.length} files${skippedNote}...`);
      const files = [];
      for (let i = 0; i < entries.length; i++) {
        const entry = entries[i];
        const data = await entry.file.arrayBuffer();
        files.push({
          path: entry.path,
          data,
          mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
          fingerprint_sha256: null,
        });
        // Yield to browser every 10 files to prevent freezing
        if (i % 10 === 0 && i > 0) {
          setStatus(`Ingesting ${entries.length} files (${i}/${entries.length})${skippedNote}...`);
          await new Promise(resolve => setTimeout(resolve, 0));
        }
      }
      const transfer = files.map((f) => f.data);
      await callWorker("ingest", { files }, state.backend?.kind === "worker" ? transfer : undefined);
    } else {
      let unchanged = 0;
      let changed = 0;
      let added = 0;
      const keepPaths = [];
      const files = [];

      for (const entry of entries) {
        const prev = prevByPath.get(entry.path);
        const isImage = isImagePath(entry.path);
        if (!prev) {
          added += 1;
          const data = await entry.file.arrayBuffer();
          files.push({
            path: entry.path,
            data,
            mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
            fingerprint_sha256: await fingerprintFile(entry.file),
          });
          continue;
        }

        if (isImage) {
          // Always include image bytes so `export.zip` always contains the assets.
          const data = await entry.file.arrayBuffer();
          files.push({
            path: entry.path,
            data,
            mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
            fingerprint_sha256: prev.fingerprint_sha256 || (await fingerprintFile(entry.file)),
          });
          continue;
        }

        const prevBytes = prev.bytes ?? null;
        if (prevBytes !== null && prevBytes !== entry.file.size) {
          changed += 1;
          const data = await entry.file.arrayBuffer();
          files.push({
            path: entry.path,
            data,
            mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
            fingerprint_sha256: await fingerprintFile(entry.file),
          });
          continue;
        }

        const prevFp = prev.fingerprint_sha256 || null;
        if (prevFp) {
          const fp = await fingerprintFile(entry.file);
          if (fp === prevFp) {
            unchanged += 1;
            keepPaths.push(entry.path);
          } else {
            changed += 1;
            const data = await entry.file.arrayBuffer();
            files.push({
              path: entry.path,
              data,
              mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
              fingerprint_sha256: fp,
            });
          }
          continue;
        }

        // Fallback: if we don't have a fingerprint in the cache yet, be conservative and re-read once.
        changed += 1;
        const data = await entry.file.arrayBuffer();
        files.push({
          path: entry.path,
          data,
          mtime_ms: Number.isFinite(entry.file.lastModified) ? entry.file.lastModified : null,
          fingerprint_sha256: await fingerprintFile(entry.file),
        });
      }

      setStatus(
        `Updating index: ${unchanged} unchanged, ${changed} changed, ${added} new, ${removedCount} removed${skippedNote}...`
      );
      const transfer = files.map((f) => f.data);
      await callWorker(
        "updateSelective",
        { files, keepPaths },
        state.backend?.kind === "worker" ? transfer : undefined
      );
    }

    const statsResult = await callWorker("stats", {});
    const warningsResult = await callWorker("warnings", {});
    const filesResult = await callWorker("files", {});

    const idResult = await callWorker("indexId", {});
    state.indexId = idResult.indexId || null;
    state.files = filesResult.files || [];
    updateExportUiLabels();

    elements.indexId.textContent = state.indexId || "(unknown)";
    elements.chunkCount.textContent = statsResult.stats.total_chunks;
    elements.warningCount.textContent = warningsResult.warnings.length;
    renderWarnings(warningsResult.warnings);
    populateFileFilter();
    await updateOutlineSymbols();
    await populateSavedIndexes();
    state.indexLoaded = true;
    setStatus("Index ready.");
    if (shouldAutoBuildEmbeddings) {
      void callWorker("buildEmbeddings", {}).catch(() => {});
    }
  } catch (error) {
    setStatus(`Ingestion failed: ${formatErrorForUi(error)}`);
  } finally {
    state.busy = false;
  }
}

function renderWarnings(warnings) {
  elements.warningList.replaceChildren();
  if (!warnings.length) {
    return;
  }
  for (const warning of warnings) {
    const div = document.createElement("div");
    div.textContent = `${warning.path}: ${warning.message}`;
    elements.warningList.appendChild(div);
  }
}

elements.selectFolder.addEventListener("click", async () => {
  elements.filesInput.click();
});


elements.folderInput.addEventListener("change", async (event) => {
  const collected = await collectFilesFromInput(event.target.files || []);
  await runIngest(collected.entries, collected);
});

elements.filesInput.addEventListener("change", async (event) => {
  const collected = await collectFilesFromInput(event.target.files || []);
  await runIngest(collected.entries, collected);
});

elements.dropZone.addEventListener("dragover", (event) => {
  event.preventDefault();
  elements.dropZone.classList.add("active");
});

elements.dropZone.addEventListener("dragleave", (event) => {
  if (event.target === elements.dropZone) {
    elements.dropZone.classList.remove("active");
  }
});

elements.dropZone.addEventListener("drop", async (event) => {
  event.preventDefault();
  elements.dropZone.classList.remove("active");
  const collected = await collectFilesFromInput(event.dataTransfer.files || []);
  await runIngest(collected.entries, collected);
});

elements.fileFilter.addEventListener("change", async () => {
  await updateOutlineSymbols();
  scheduleSearch();
});

elements.runSearch.addEventListener("click", async () => {
  await runSearch();
});

if (elements.buildEmbeddings) {
  if (!embeddingsRequested) {
    elements.buildEmbeddings.disabled = true;
    elements.buildEmbeddings.title = "Embeddings are disabled. Add ?embeddings=1 to the URL to enable.";
  } else if (!globalThis.LLMX_ENABLE_WEBGPU && !forceCpu) {
    elements.buildEmbeddings.disabled = true;
    elements.buildEmbeddings.title =
      "Embeddings require WebGPU by default. Use a WebGPU-capable Chromium browser, or add ?cpu=1 to force CPU.";
  } else {
    elements.buildEmbeddings.title = "Build embeddings for semantic search.";
  }

  elements.buildEmbeddings.addEventListener("click", async () => {
    if (!state.indexLoaded) {
      setStatus("No index loaded.");
      return;
    }
    if (!embeddingsRequested) {
      setStatus("Embeddings disabled. Add ?embeddings=1 to the URL.");
      return;
    }
    if (!globalThis.LLMX_ENABLE_WEBGPU && !forceCpu) {
      setStatus("Embeddings require WebGPU. Use Chromium with WebGPU, or add ?cpu=1 to force CPU.");
      return;
    }

    // Warn about CPU embeddings being slow and potentially unstable
    if (forceCpu && !globalThis.LLMX_ENABLE_WEBGPU) {
      const chunkCount = state.files.reduce((sum, f) => sum + (f.chunks || 0), 0);
      if (chunkCount > 100) {
        const firefoxWarning = isFirefox
          ? `\n\nWARNING: Firefox has strict WASM memory limits. CPU embeddings with ${chunkCount} chunks will be VERY slow (10-20 minutes) and may still crash.\n\n`
          : '';
        const proceed = confirm(
          `CPU embeddings with ${chunkCount} chunks may take 5-10 minutes and could crash the browser.` +
          firefoxWarning +
          `For better performance, use Chrome/Edge.\n\n` +
          `Continue with CPU anyway?`
        );
        if (!proceed) {
          setStatus("Embeddings cancelled.");
          return;
        }
      }
    }

    const backendLabel = globalThis.LLMX_ENABLE_WEBGPU ? "webgpu" : "cpu";
    setStatus(`Embeddings: building (${backendLabel})...`);
    try {
      const result = await callWorker("buildEmbeddings", {});
      const meta = result?.meta;
      if (meta && typeof meta.modelId === "string") {
        setStatus(`Embeddings ready: model=${meta.modelId}, dim=${meta.dim}, count=${meta.count}`);
      } else {
        setStatus("Embeddings ready.");
      }
    } catch (error) {
      setStatus(`Embeddings failed: ${formatErrorForUi(error)}`);
    }
  });
}

elements.query.addEventListener("keydown", async (event) => {
  if (event.key === "Enter") {
    await runSearch();
  }
});

elements.query.addEventListener("input", () => {
  scheduleSearch();
});

elements.pathFilter.addEventListener("input", () => {
  scheduleSearch();
});

elements.kindFilter.addEventListener("change", () => {
  scheduleSearch();
});

elements.outlineFilter.addEventListener("change", () => {
  scheduleSearch();
});

elements.symbolFilter.addEventListener("change", () => {
  scheduleSearch();
});

elements.closeChunk.addEventListener("click", () => {
  elements.chunkView.hidden = true;
});

elements.downloadExport?.addEventListener("click", () => {
  if (!state.indexLoaded) {
    setStatus("No index to export.");
    return;
  }
  callWorker("exportZipCompact", {})
    .then(({ bytes }) => {
      const name = `${exportBaseName()}.zip`;
      downloadFile(name, bytes, "application/zip");
    })
    .catch(() => setStatus("Export failed."));
});

elements.downloadIndexJson?.addEventListener("click", () => {
  if (!state.indexLoaded) {
    setStatus("No index to export.");
    return;
  }
  callWorker("exportIndexJson", {})
    .then(({ json }) => {
      const name = `${exportBaseName()}.index.json`;
      downloadFile(name, json, "application/json");
    })
    .catch(() => setStatus("Export failed."));
});

async function runSearch() {
  if (!state.indexLoaded) {
    setStatus("No index loaded.");
    return;
  }
  const query = elements.query.value.trim();
  if (!query) {
    elements.results.replaceChildren();
    elements.chunkView.hidden = true;
    setStatus("Index ready.");
    return;
  }
  const filters = {
    path_exact: elements.fileFilter.value || null,
    path_prefix: elements.fileFilter.value ? null : selectPathPrefix(),
    kind: elements.kindFilter.value || null,
    heading_prefix: elements.outlineFilter.value || null,
    symbol_prefix: elements.symbolFilter.value || null,
  };
  const seq = ++state.searchSeq;
  elements.runSearch.disabled = true;
  setStatus("Searching...");
  try {
    const { results } = await callWorker("search", { query, filters, limit: 20 });
    if (seq !== state.searchSeq) {
      return;
    }
    renderResults(results);
    setStatus(`Found ${results.length} results.`);
  } catch (error) {
    if (seq === state.searchSeq) {
      setStatus("Search failed.");
    }
  } finally {
    if (seq === state.searchSeq) {
      elements.runSearch.disabled = false;
    }
  }
}

function renderResults(results) {
  elements.results.replaceChildren();
  if (!results.length) {
    const empty = document.createElement("div");
    empty.textContent = "No matches.";
    elements.results.appendChild(empty);
    return;
  }
  for (const result of results) {
    const item = document.createElement("div");
    item.className = "result-item";

    const title = document.createElement("strong");
    title.textContent = result.path;

    const meta = document.createElement("div");
    meta.className = "meta";
    const heading = result.heading_path.length ? ` | ${result.heading_path.join("/")}` : "";
    const ref = result.chunk_ref ? ` | ${result.chunk_ref}` : "";
    meta.textContent = `Lines ${result.start_line}-${result.end_line}${ref}${heading}`;

    const snippet = document.createElement("div");
    snippet.textContent = result.snippet;

    const button = document.createElement("button");
    button.textContent = "View chunk";
    button.addEventListener("click", async () => {
      const { chunk } = await callWorker("getChunk", { chunkId: result.chunk_id });
      if (!chunk) {
        return;
      }
      const ref = chunk.short_id ? ` | Ref: ${chunk.short_id}` : "";
      const label = chunk.slug ? ` | ${chunk.slug}` : "";
      elements.chunkTitle.textContent = `${chunk.path} (${chunk.start_line}-${chunk.end_line})${ref}${label}`;
      elements.chunkContent.textContent = chunk.content;
      elements.chunkView.hidden = false;
    });

    item.appendChild(title);
    item.appendChild(meta);
    item.appendChild(snippet);
    item.appendChild(button);
    elements.results.appendChild(item);
  }
}

function downloadFile(name, content, type) {
  let blob;
  if (content instanceof Uint8Array || Array.isArray(content)) {
    blob = new Blob([content], { type });
  } else {
    blob = new Blob([content], { type });
  }
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

function populateFileFilter() {
  elements.fileFilter.replaceChildren();
  const option = document.createElement("option");
  option.value = "";
  option.textContent = "All files";
  elements.fileFilter.appendChild(option);
  for (const file of state.files || []) {
    const item = document.createElement("option");
    item.value = file.path;
    item.textContent = file.path;
    elements.fileFilter.appendChild(item);
  }
}

async function updateOutlineSymbols() {
  elements.outlineFilter.replaceChildren();
  elements.symbolFilter.replaceChildren();
  const outlineOption = document.createElement("option");
  outlineOption.value = "";
  outlineOption.textContent = "All outlines";
  elements.outlineFilter.appendChild(outlineOption);
  const symbolOption = document.createElement("option");
  symbolOption.value = "";
  symbolOption.textContent = "All symbols";
  elements.symbolFilter.appendChild(symbolOption);
  const path = elements.fileFilter.value;
  if (!state.indexLoaded || !path) {
    return;
  }
  try {
    const { outline: outlines } = await callWorker("listOutline", { path });
    const { symbols } = await callWorker("listSymbols", { path });
    for (const outline of outlines) {
      const option = document.createElement("option");
      option.value = outline;
      option.textContent = outline;
      elements.outlineFilter.appendChild(option);
    }
    for (const symbol of symbols) {
      const option = document.createElement("option");
      option.value = symbol;
      option.textContent = symbol;
      elements.symbolFilter.appendChild(option);
    }
  } catch (error) {
    setStatus("Outline/symbol lookup failed.");
  }
}

function selectPathPrefix() {
  if (elements.fileFilter.value) {
    return elements.fileFilter.value;
  }
  const manual = elements.pathFilter.value.trim();
  return manual || null;
}

function openDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("llmx-ingestor", 1);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains("indexes")) {
        db.createObjectStore("indexes", { keyPath: "id" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

let searchTimer = null;
function scheduleSearch() {
  if (!state.indexLoaded || state.busy) {
    return;
  }
  if (searchTimer) {
    clearTimeout(searchTimer);
  }
  searchTimer = setTimeout(async () => {
    searchTimer = null;
    await runSearch();
  }, 200);
}

async function saveIndex(id, json, embeddings, embeddingsMeta) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("indexes", "readwrite");
    tx.objectStore("indexes").put({
      id,
      json,
      embeddings: embeddings || null,
      embeddings_meta: embeddingsMeta || null,
      saved_at: new Date().toISOString(),
    });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function listIndexes() {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("indexes", "readonly");
    const request = tx.objectStore("indexes").getAll();
    request.onsuccess = () => resolve(request.result || []);
    request.onerror = () => reject(request.error);
  });
}

async function deleteIndex(id) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("indexes", "readwrite");
    tx.objectStore("indexes").delete(id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function populateSavedIndexes() {
  if (!elements.savedIndexes) {
    return;
  }
  const records = await listIndexes();
  records.sort((a, b) => (b.saved_at || "").localeCompare(a.saved_at || ""));
  elements.savedIndexes.replaceChildren();
  const empty = document.createElement("option");
  empty.value = "";
  empty.textContent = records.length ? "Select saved index" : "No saved indexes";
  elements.savedIndexes.appendChild(empty);
  for (const record of records) {
    const option = document.createElement("option");
    option.value = record.id;
    option.textContent = `${record.id} (${record.saved_at || "unknown"})`;
    elements.savedIndexes.appendChild(option);
  }
}

elements.loadSavedIndex?.addEventListener("click", async () => {
  const id = elements.savedIndexes.value;
  if (!id) {
    return;
  }
  const records = await listIndexes();
  const record = records.find((r) => r.id === id);
  if (!record) {
    setStatus("Saved index not found.");
    return;
  }
  try {
    state.busy = true;
    await callWorker("loadIndexJson", { json: record.json });
    if (record.embeddings && record.embeddings_meta) {
      try {
        await callWorker(
          "setEmbeddings",
          { embeddings: record.embeddings, meta: record.embeddings_meta },
          state.backend?.kind === "worker" ? [record.embeddings] : undefined
        );
      } catch {}
    }
    const idResult = await callWorker("indexId", {});
    const statsResult = await callWorker("stats", {});
    const warningsResult = await callWorker("warnings", {});
    const filesResult = await callWorker("files", {});
    state.indexId = idResult.indexId || null;
    state.files = filesResult.files || [];
    state.indexLoaded = true;
    elements.indexId.textContent = state.indexId || "(unknown)";
    elements.chunkCount.textContent = statsResult.stats.total_chunks;
    elements.warningCount.textContent = warningsResult.warnings.length;
    renderWarnings(warningsResult.warnings);
    populateFileFilter();
    await updateOutlineSymbols();
    setStatus("Loaded saved index.");
    if (shouldAutoBuildEmbeddings) {
      void callWorker("buildEmbeddings", {}).catch(() => {});
    }
  } catch {
    setStatus("Failed to load saved index.");
  } finally {
    state.busy = false;
  }
});

elements.deleteSavedIndex?.addEventListener("click", async () => {
  const id = elements.savedIndexes.value;
  if (!id) {
    return;
  }
  try {
    await deleteIndex(id);
    await populateSavedIndexes();
    setStatus("Deleted saved index.");
  } catch {
    setStatus("Failed to delete saved index.");
  }
});

// Settings UI
function loadSettingsFromUrl() {
  if (elements.settingEmbeddings) {
    elements.settingEmbeddings.checked = embeddingsRequested;
  }
  if (elements.settingForceCpu) {
    elements.settingForceCpu.checked = forceCpu;
  }
  if (elements.settingForceWebgpu) {
    elements.settingForceWebgpu.checked = forceWebGpu;
  }
  if (elements.settingAutoEmbeddings) {
    elements.settingAutoEmbeddings.checked = autoEmbeddingsRequested;
  }
}

function applySettings() {
  const params = new URLSearchParams();

  if (elements.settingEmbeddings?.checked) {
    params.set("embeddings", "1");
  }
  if (elements.settingForceCpu?.checked) {
    params.set("cpu", "1");
  }
  if (elements.settingForceWebgpu?.checked) {
    params.set("force_webgpu", "1");
  }
  if (elements.settingAutoEmbeddings?.checked) {
    params.set("auto_embeddings", "1");
  }

  const newUrl = `${window.location.pathname}${params.toString() ? "?" + params.toString() : ""}`;
  window.location.href = newUrl;
}

function resetSettings() {
  window.location.href = window.location.pathname;
}

elements.applySettings?.addEventListener("click", applySettings);
elements.resetSettings?.addEventListener("click", resetSettings);

loadSettingsFromUrl();
configureFolderPickerUi();
initWorker().catch((error) => {
  setStatus(`Failed to start backend: ${formatErrorForUi(error)}`);
});
