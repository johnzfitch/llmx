import {
  buildManageStats,
  linearFuse,
  listRichSymbols,
  lookupSymbols,
  rrfFuse,
  traceRefs,
} from "./index-insights.js";

const state = {
  backend: null,
  workerReady: false,
  indexLoaded: false,
  busy: false,
  buildingEmbeddings: false,
  indexId: null,
  sourceLabel: null,
  files: [],
  chunkCount: 0,
  searchSeq: 0,
  activeTool: "search",
};

// Progressive disclosure: update body state for CSS
function updateUIState() {
  const body = document.body;
  if (state.indexLoaded) {
    body.dataset.state = "indexed";
  } else if (state.busy) {
    body.dataset.state = "indexing";
  } else {
    body.dataset.state = "empty";
  }
}

const elements = {
  status: document.getElementById("ingest-status"),
  selectFolder: document.getElementById("select-folder"),
  loadIndexJson: document.getElementById("load-index-json"),
  replaceIndex: document.getElementById("replace-index"),
  folderInput: document.getElementById("folder-input"),
  filesInput: document.getElementById("files-input"),
  indexJsonInput: document.getElementById("index-json-input"),
  dropZone: document.getElementById("drop-zone"),
  toolTabs: document.getElementById("tool-tabs"),
  query: document.getElementById("query"),
  searchStrategy: document.getElementById("search-strategy"),
  hybridStrategy: document.getElementById("hybrid-strategy"),
  searchIntent: document.getElementById("search-intent"),
  searchExplain: document.getElementById("search-explain"),
  pathFilter: document.getElementById("path-filter"),
  fileFilter: document.getElementById("file-filter"),
  outlineFilter: document.getElementById("outline-filter"),
  symbolFilter: document.getElementById("symbol-filter"),
  kindFilter: document.getElementById("kind-filter"),
  runSearch: document.getElementById("run-search"),
  runSymbols: document.getElementById("run-symbols"),
  symbolsPattern: document.getElementById("symbols-pattern"),
  symbolsKind: document.getElementById("symbols-kind"),
  symbolsPath: document.getElementById("symbols-path"),
  symbolsLimit: document.getElementById("symbols-limit"),
  symbolsResults: document.getElementById("symbols-results"),
  runLookup: document.getElementById("run-lookup"),
  lookupSymbol: document.getElementById("lookup-symbol"),
  lookupKind: document.getElementById("lookup-kind"),
  lookupPath: document.getElementById("lookup-path"),
  lookupLimit: document.getElementById("lookup-limit"),
  lookupResults: document.getElementById("lookup-results"),
  runRefs: document.getElementById("run-refs"),
  refsSymbol: document.getElementById("refs-symbol"),
  refsDirection: document.getElementById("refs-direction"),
  refsDepth: document.getElementById("refs-depth"),
  refsLimit: document.getElementById("refs-limit"),
  refsResults: document.getElementById("refs-results"),
  statsSummary: document.getElementById("stats-summary"),
  statsBreakdowns: document.getElementById("stats-breakdowns"),
  buildEmbeddings: document.getElementById("build-embeddings"),
  embeddingsStatus: document.getElementById("embeddings-status"),
  results: document.getElementById("results"),
  chunkView: document.getElementById("chunk-view"),
  chunkTitle: document.getElementById("chunk-title"),
  chunkContent: document.getElementById("chunk-content"),
  closeChunk: document.getElementById("close-chunk"),
  downloadExport: document.getElementById("download-export"),
  downloadCompactExport: document.getElementById("download-compact-export"),
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
const embeddingsRequested = urlParams.get("embeddings") !== "0";
const forceCpu = urlParams.get("cpu") === "1";
const webGpuParam = urlParams.get("webgpu");
const forceWebGpu = urlParams.get("force_webgpu") === "1";
const autoEmbeddingsRequested = urlParams.get("auto_embeddings") === "1";
const isFirefox = (() => {
  const ua = window.navigator?.userAgent || "";
  return ua.includes("Firefox/") && !ua.includes("Seamonkey/");
})();
const isFirefoxNightly = (() => {
  const ua = window.navigator?.userAgent || "";
  return /Firefox\/[0-9]+(\.[0-9]+)*a1\b/.test(ua);
})();
const webGpuRequested = webGpuParam === "1" || (webGpuParam !== "0" && !forceCpu);
const webGpuAvailable = Boolean(window.navigator && window.navigator.gpu);
globalThis.LLMX_ENABLE_WEBGPU = webGpuRequested && webGpuAvailable;
globalThis.LLMX_ENABLE_EMBEDDINGS = embeddingsRequested;
if (webGpuRequested && !webGpuAvailable) {
  console.warn(
    "WebGPU unavailable (navigator.gpu missing). To use embeddings, either use a WebGPU-capable Chromium browser or add ?cpu=1 to allow slow CPU embeddings."
  );
}
if (globalThis.LLMX_ENABLE_WEBGPU && isFirefox && !isFirefoxNightly && !forceWebGpu) {
  globalThis.LLMX_ENABLE_WEBGPU = false;
  console.warn(
    "WebGPU requested on Firefox, but is disabled by default due to Burn/WGPU instability. Use Firefox Nightly, Chromium, or add ?force_webgpu=1 to override."
  );
}
if (embeddingsRequested && !globalThis.LLMX_ENABLE_WEBGPU && !forceCpu) {
  console.warn(
    "Embeddings are enabled with CPU/WebGPU auto mode. WebGPU is unavailable or disabled, so the browser will fall back to CPU when semantic search is needed."
  );
}
const shouldAutoBuildEmbeddings = embeddingsRequested && autoEmbeddingsRequested;
const SEARCH_LIMIT = 20;
const EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS = 240;
const MAX_FILE_GROUPS_PER_SEARCH = 2;

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
const LLMX_EXPORT_DIR_PATTERN = /\.llmx-[a-f0-9]{8,}(?:\.[a-z0-9_-]+)?$/i;
const DEFAULT_LIMITS = {
  maxFileBytes: 32 * 1024 * 1024,    // 32MB per file
  maxTotalBytes: 200 * 1024 * 1024,  // 200MB total
  maxFileCount: null,                 // No file-count cap; enforce byte limits instead
  warnFileBytes: 1 * 1024 * 1024,    // Warn at 1MB per file
  warnTotalBytes: 100 * 1024 * 1024, // Warn at 100MB total
};

function setStatus(message) {
  elements.status.textContent = message;
  if (message) {
    elements.status.style.display = "block";
  } else {
    elements.status.style.display = "none";
  }
}

function updateBackendInfo(backendType, capabilities) {
  let info = backendType;
  if (capabilities) {
    const parts = [];
    if (capabilities.embeddings) {
      if (capabilities.webgpu) {
        parts.push("WebGPU");
      } else {
        parts.push("CPU");
      }
    }
    if (parts.length > 0) {
      info += ` (${parts.join(", ")})`;
    }
  }
  elements.backendInfo.textContent = info;
  elements.backendInfo.classList.add("ready");
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

function refreshEmbeddingsStatus(meta = null) {
  if (!elements.embeddingsStatus || state.buildingEmbeddings) {
    return;
  }
  elements.embeddingsStatus.classList.remove("building");
  if (!embeddingsRequested) {
    elements.embeddingsStatus.textContent = "Disabled";
    return;
  }
  if (meta && typeof meta.count === "number") {
    elements.embeddingsStatus.textContent = `Ready (${meta.count} chunks)`;
    return;
  }
  if (shouldAutoBuildEmbeddings && state.chunkCount > 0 && state.chunkCount <= EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS) {
    elements.embeddingsStatus.textContent = "Auto";
    return;
  }
  if (state.chunkCount > EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS) {
    elements.embeddingsStatus.textContent = `Lazy (> ${EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS} chunks)`;
    return;
  }
  elements.embeddingsStatus.textContent = "Not built";
}

async function maybeAutoBuildEmbeddings(reason) {
  refreshEmbeddingsStatus();
  if (!shouldAutoBuildEmbeddings) {
    return null;
  }
  if (state.chunkCount <= 0 || state.chunkCount > EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS) {
    return null;
  }
  try {
    return await buildEmbeddingsForSearch({ reason, silent: true });
  } catch (error) {
    console.warn("Auto embeddings build failed:", error);
    return null;
  }
}

function shouldUseSemanticIntent(intent) {
  return intent === "semantic" || intent === "keyword";
}

function resultHeadingKey(result) {
  return Array.isArray(result.heading_path) && result.heading_path.length
    ? result.heading_path.join(" / ")
    : "";
}

function mergeMatchedEngines(results) {
  const seen = new Set();
  const merged = [];
  for (const result of results) {
    for (const engine of result.matched_engines || []) {
      if (!seen.has(engine)) {
        seen.add(engine);
        merged.push(engine);
      }
    }
  }
  return merged;
}

function mergeReasons(results) {
  const seen = new Set();
  const merged = [];
  for (const result of results) {
    const reason = String(result.match_reason || "").trim();
    if (reason && !seen.has(reason)) {
      seen.add(reason);
      merged.push(reason);
    }
  }
  return merged;
}

function shapeSearchResults(results, limit = SEARCH_LIMIT) {
  const groups = [];
  const byKey = new Map();

  for (const result of results) {
    const sectionKey = resultHeadingKey(result);
    const key = `${result.path}::${sectionKey}`;
    const groupList = byKey.get(key) || [];
    let compatible = null;
    for (let i = groupList.length - 1; i >= 0; i -= 1) {
      if (result.start_line <= groupList[i].end_line + 20) {
        compatible = groupList[i];
        break;
      }
    }
    if (compatible) {
      compatible.results.push(result);
      compatible.score += result.score * 0.2;
      compatible.start_line = Math.min(compatible.start_line, result.start_line);
      compatible.end_line = Math.max(compatible.end_line, result.end_line);
      compatible.chunk_ids.push(result.chunk_id);
      continue;
    }

    const group = {
      key,
      path: result.path,
      heading_path: result.heading_path || [],
      results: [result],
      start_line: result.start_line,
      end_line: result.end_line,
      score: result.score,
      chunk_ids: [result.chunk_id],
    };
    groups.push(group);
    groupList.push(group);
    byKey.set(key, groupList);
  }

  groups.sort((a, b) => b.score - a.score);
  const selected = [];
  const perFile = new Map();

  for (const group of groups) {
    const count = perFile.get(group.path) || 0;
    if (count >= MAX_FILE_GROUPS_PER_SEARCH && selected.length < limit) {
      continue;
    }
    perFile.set(group.path, count + 1);
    selected.push(group);
    if (selected.length >= limit) {
      break;
    }
  }

  if (selected.length < limit) {
    for (const group of groups) {
      if (selected.includes(group)) {
        continue;
      }
      selected.push(group);
      if (selected.length >= limit) {
        break;
      }
    }
  }

  return selected.map((group) => {
    const primary = group.results[0];
    const heading = resultHeadingKey(primary);
    const reasons = mergeReasons(group.results);
    const snippets = [];
    for (const result of group.results) {
      const snippet = String(result.snippet || "").trim();
      if (snippet && !snippets.includes(snippet)) {
        snippets.push(snippet);
      }
      if (snippets.length >= 2) {
        break;
      }
    }
    return {
      ...primary,
      path: group.path,
      heading_path: group.heading_path,
      start_line: group.start_line,
      end_line: group.end_line,
      score: group.score,
      chunk_ids: group.chunk_ids,
      title: heading || group.path,
      subtitle: heading ? group.path : "",
      snippet: snippets.join("\n\n"),
      match_reason: reasons.length > 1 ? `${reasons[0]} | ${reasons[1]}` : reasons[0] || primary.match_reason || null,
      matched_engines: mergeMatchedEngines(group.results),
    };
  });
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

  function buildSearchResult(meta, score, extras = {}) {
    return {
      chunk_id: meta.id,
      chunk_ref: meta.ref,
      score,
      path: meta.path,
      start_line: meta.start_line,
      end_line: meta.end_line,
      snippet: meta.snippet,
      heading_path: meta.heading_path,
      match_reason: extras.match_reason || null,
      matched_engines: extras.matched_engines || [],
    };
  }

  async function ensureEmbedder() {
    if (!embedder) {
      try {
        embedder = await WasmEmbedder.create();
      } catch (error) {
        if (globalThis.LLMX_ENABLE_WEBGPU) {
          console.warn(`WebGPU embedder creation failed, falling back to CPU: ${formatErrorForUi(error)}`);
          globalThis.LLMX_ENABLE_WEBGPU = false;
          embedder = await WasmEmbedder.create();
        } else {
          throw error;
        }
      }
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

  function getCurrentIndex() {
    if (!ingestor) {
      throw new Error("No index loaded");
    }
    return JSON.parse(ingestor.exportIndexJson());
  }

  async function maybePrepareEmbeddings(filters, query, limit, notices) {
    if (!embeddingsRequested || !query) {
      return null;
    }
    if (!embeddings || !embeddingsMeta || !chunkMeta) {
      const index = getCurrentIndex();
      const totalChunks = Array.isArray(index.chunks) ? index.chunks.length : 0;
      if (totalChunks > 0 && totalChunks <= EMBEDDINGS_AUTO_BUILD_MAX_CHUNKS) {
        try {
          await buildEmbeddingsIndex();
          notices.push(`Built embeddings lazily for ${totalChunks} chunks.`);
        } catch (error) {
          console.warn("Lazy embeddings build failed; continuing without semantic search.", error);
          notices.push(`Semantic search unavailable: ${formatErrorForUi(error)}`);
          return null;
        }
      }
    }
    await ensureEmbedder();
    if (!shouldUseEmbeddings()) {
      notices.push("Semantic search unavailable; showing lexical results.");
      return null;
    }
    return buildSemanticResults(query, filters, limit, notices);
  }

  function buildSemanticResults(query, filters, limit, notices) {
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
    if (!semantic.length) {
      notices.push("Semantic search found no matching chunks after filters.");
      return [];
    }

    return semantic.slice(0, limit * 2).map(({ idx, score }) => {
      const meta = chunkMeta[idx];
      return buildSearchResult(meta, score, {
        match_reason: `Semantic similarity for "${query}"`,
        matched_engines: ["dense"],
      });
    });
  }

  function mergeSearchResults(baseResults, semanticResults, strategy, hybridStrategy, limit) {
    const useHybrid = strategy === "hybrid" || (strategy === "auto" && semanticResults.length && baseResults.length);
    const useSemanticOnly = strategy === "semantic";

    if (useSemanticOnly) {
      return semanticResults.slice(0, limit);
    }
    if (!semanticResults.length) {
      return baseResults.slice(0, limit);
    }
    if (!baseResults.length) {
      return semanticResults.slice(0, limit);
    }

    const mergedIds = hybridStrategy === "linear"
      ? linearFuse(baseResults, semanticResults, limit * 2)
      : rrfFuse(baseResults, semanticResults, limit * 2);

    if (!useHybrid && strategy !== "auto") {
      return baseResults.slice(0, limit);
    }

    const baseById = new Map(baseResults.map((result) => [result.chunk_id, result]));
    const semanticById = new Map(semanticResults.map((result) => [result.chunk_id, result]));

    return mergedIds.map(({ chunkId, score }) => {
      const lexical = baseById.get(chunkId);
      const dense = semanticById.get(chunkId);
      if (lexical && dense) {
        return {
          ...lexical,
          score,
          matched_engines: Array.from(new Set([...(lexical.matched_engines || []), "dense"])),
        };
      }
      if (lexical) {
        return { ...lexical, score };
      }
      if (dense) {
        return { ...dense, score };
      }
      return null;
    }).filter(Boolean).slice(0, limit);
  }

  async function runAdvancedSearch(query, filters, limit, options = {}) {
    return ingestor.searchAdvanced(query, filters, limit * 2, {
      explain: options.explain !== false,
      intent: options.intent || "auto",
      use_semantic: false,
    });
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
          return { meta, backend: globalThis.LLMX_ENABLE_WEBGPU ? "webgpu" : "cpu" };
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
          return { stats: buildManageStats(getCurrentIndex()) };
        case "warnings":
          if (!ingestor) throw new Error("No index loaded");
          return { warnings: await ingestor.warnings() };
        case "search":
          if (!ingestor) throw new Error("No index loaded");
          {
            const query = payload.query || "";
            const filters = payload.filters || null;
            const limit = payload.limit || 20;
            const strategy = String(payload.strategy || "auto").toLowerCase();
            const hybridStrategy = String(payload.hybridStrategy || "rrf").toLowerCase();
            const explain = payload.explain !== false;
            const requestedIntent = String(payload.intent || "auto").toLowerCase();
            const notices = [];

            const base = await runAdvancedSearch(query, filters, limit, {
              explain,
              intent: requestedIntent,
            });
            const baseResults = base?.results || [];
            const resolvedIntent = base?.resolved_intent || "keyword";

            if (strategy === "bm25") {
              return { results: baseResults, resolvedIntent, usedSemantic: false, notices };
            }
            if (!embeddingsRequested && (strategy === "semantic" || strategy === "hybrid")) {
              notices.push("Embeddings disabled; falling back to lexical search.");
              return { results: baseResults, resolvedIntent, usedSemantic: false, notices };
            }
            if (!shouldUseSemanticIntent(resolvedIntent) && strategy === "auto") {
              return { results: baseResults, resolvedIntent, usedSemantic: false, notices };
            }
            if (strategy === "semantic" && !embeddingsRequested) {
              notices.push("Semantic search requested, but embeddings are disabled.");
              return { results: baseResults, resolvedIntent, usedSemantic: false, notices };
            }

            const semanticResults = await maybePrepareEmbeddings(filters, query, limit, notices) || [];
            const results = mergeSearchResults(baseResults, semanticResults, strategy, hybridStrategy, limit);
            return {
              results,
              resolvedIntent,
              usedSemantic: semanticResults.length > 0,
              notices,
            };
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
        case "symbolsRich":
          if (!ingestor) throw new Error("No index loaded");
          return listRichSymbols(getCurrentIndex(), payload || {});
        case "lookupSymbol":
          if (!ingestor) throw new Error("No index loaded");
          return lookupSymbols(getCurrentIndex(), payload || {});
        case "refsForSymbol":
          if (!ingestor) throw new Error("No index loaded");
          return traceRefs(getCurrentIndex(), payload || {});
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

function hasMaxFileCountLimit() {
  return Number.isFinite(DEFAULT_LIMITS.maxFileCount) && DEFAULT_LIMITS.maxFileCount >= 0;
}

function findLlmxExportDir(path) {
  return String(path)
    .split("/")
    .find((segment) => LLMX_EXPORT_DIR_PATTERN.test(segment)) || null;
}

async function collectFilesFromInput(fileList) {
  const entries = [];
  let totalBytes = 0;
  let skippedLarge = 0;
  let skippedTotal = 0;
  let skippedCount = 0;
  const skippedExports = new Set();
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
    const exportDir = findLlmxExportDir(path);
    if (exportDir) {
      skippedExports.add(exportDir);
      continue;
    }
    if (hasMaxFileCountLimit() && entries.length >= DEFAULT_LIMITS.maxFileCount) {
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
  return {
    entries,
    skippedLarge,
    skippedTotal,
    skippedCount,
    skippedExports: skippedExports.size,
    totalBytes,
    rootName,
  };
}

async function collectFilesFromHandle(handle, basePath = "", budget = null, rootName = null) {
  const entries = [];
  const shared = budget || {
    totalBytes: 0,
    fileCount: 0,
    skippedLarge: 0,
    skippedTotal: 0,
    skippedCount: 0,
    skippedExports: 0,
  };
  for await (const [name, entry] of handle.entries()) {
    if (entry.kind === "directory") {
      if (SKIP_DIRS.includes(name)) {
        continue;
      }
      if (LLMX_EXPORT_DIR_PATTERN.test(name)) {
        shared.skippedExports += 1;
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
    if (hasMaxFileCountLimit() && shared.fileCount >= DEFAULT_LIMITS.maxFileCount) {
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
    skippedExports: shared.skippedExports,
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

function inferSourceLabelFromPaths(paths) {
  const counts = new Map();
  for (const path of paths || []) {
    const value = String(path || "");
    const first = value.includes("/") ? value.split("/")[0] : "";
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
  const total = (paths || []).length || 1;
  if (best && bestCount / total >= 0.6) {
    return best;
  }
  return null;
}

function inferSourceLabel(entries, collectedMeta) {
  const direct = collectedMeta?.rootName;
  if (direct && String(direct).trim()) {
    return String(direct).trim();
  }
  return inferSourceLabelFromPaths((entries || []).map((entry) => entry?.path || ""));
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
    elements.downloadExport.textContent = `Download ${base}.searchable.zip`;
    elements.downloadExport.title = `Download searchable export bundle: ${base}.searchable.zip`;
  }
  if (elements.downloadCompactExport) {
    elements.downloadCompactExport.textContent = `Download ${base}.compact.zip`;
    elements.downloadCompactExport.title = `Download compact agent bundle: ${base}.compact.zip`;
  }
  if (elements.downloadIndexJson) {
    elements.downloadIndexJson.textContent = `Download ${base}.index.json`;
    elements.downloadIndexJson.title = `Download index file: ${base}.index.json`;
  }
}

async function buildEmbeddingsForSearch({ reason, silent = false } = {}) {
  if (!state.indexLoaded) {
    return null;
  }
  if (!embeddingsRequested) {
    return null;
  }
  const backendLabel = globalThis.LLMX_ENABLE_WEBGPU ? "webgpu" : "cpu";
  state.buildingEmbeddings = true;
  if (elements.buildEmbeddings) elements.buildEmbeddings.disabled = true;
  if (elements.downloadExport) elements.downloadExport.disabled = true;
  if (elements.downloadIndexJson) elements.downloadIndexJson.disabled = true;
  if (elements.embeddingsStatus) {
    elements.embeddingsStatus.textContent = `Building (${backendLabel})...`;
    elements.embeddingsStatus.classList.add("building");
  }
  if (!silent) {
    setStatus(`${reason || "Embeddings"}: building (${backendLabel})...`);
  }
  try {
    const result = await callWorker("buildEmbeddings", {});
    const meta = result?.meta || null;
    if (result?.backend === "cpu") {
      globalThis.LLMX_ENABLE_WEBGPU = false;
    }
    if (elements.embeddingsStatus) {
      elements.embeddingsStatus.textContent = meta ? `Ready (${meta.count} chunks)` : "Ready";
      elements.embeddingsStatus.classList.remove("building");
    }
    if (!silent) {
      if (meta && typeof meta.modelId === "string") {
        setStatus(`Embeddings ready: model=${meta.modelId}, dim=${meta.dim}, count=${meta.count}`);
      } else {
        setStatus("Embeddings ready.");
      }
    }
    updateBackendInfo(state.backend?.kind === "worker" ? "Worker" : "Local", {
      embeddings: embeddingsRequested,
      webgpu: globalThis.LLMX_ENABLE_WEBGPU,
      forceCpu,
    });
    return meta;
  } catch (error) {
    if (elements.embeddingsStatus) {
      elements.embeddingsStatus.textContent = "Failed";
      elements.embeddingsStatus.classList.remove("building");
    }
    if (!silent) {
      setStatus(`Embeddings failed: ${formatErrorForUi(error)}`);
    }
    throw error;
  } finally {
    state.buildingEmbeddings = false;
    if (elements.buildEmbeddings) elements.buildEmbeddings.disabled = false;
    if (elements.downloadExport) elements.downloadExport.disabled = false;
    if (elements.downloadIndexJson) elements.downloadIndexJson.disabled = false;
    if (!silent) {
      refreshEmbeddingsStatus();
    }
  }
}

function clearSearchUi() {
  elements.results.replaceChildren();
  elements.symbolsResults?.replaceChildren();
  elements.lookupResults?.replaceChildren();
  elements.refsResults?.replaceChildren();
  elements.statsSummary?.replaceChildren();
  elements.statsBreakdowns?.replaceChildren();
  elements.chunkView.hidden = true;
}

function renderEmptyState(container, message) {
  if (!container) return;
  container.replaceChildren();
  const empty = document.createElement("div");
  empty.textContent = message;
  container.appendChild(empty);
}

function parseBoundedInt(value, fallback, min, max) {
  const parsed = Number.parseInt(String(value || ""), 10);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.max(min, Math.min(parsed, max));
}

function formatNotices(notices) {
  const values = (notices || []).filter(Boolean);
  if (!values.length) {
    return "";
  }
  return ` ${values.join(" ")}`;
}

function buildSearchFilters() {
  return {
    path_exact: elements.fileFilter.value || null,
    path_prefix: elements.fileFilter.value ? null : selectPathPrefix(),
    kind: elements.kindFilter.value || null,
    heading_prefix: elements.outlineFilter.value || null,
    symbol_prefix: elements.symbolFilter.value || null,
  };
}

function breakdownEntries(breakdown) {
  return Object.entries(breakdown || {}).sort((a, b) => {
    return Number(b[1] || 0) - Number(a[1] || 0) || String(a[0]).localeCompare(String(b[0]));
  });
}

function renderStats(stats) {
  if (!elements.statsSummary || !elements.statsBreakdowns) {
    return;
  }
  elements.statsSummary.replaceChildren();
  elements.statsBreakdowns.replaceChildren();

  const summaryItems = [
    ["Files", stats.total_files],
    ["Chunks", stats.total_chunks],
    ["Avg Tokens", Math.round(Number(stats.avg_chunk_tokens || 0))],
    ["Symbols", stats.symbol_count],
    ["Edges", stats.edge_count],
    ["Languages", stats.language_count],
  ];

  for (const [label, value] of summaryItems) {
    const card = document.createElement("div");
    card.className = "stat-card";
    const key = document.createElement("div");
    key.className = "meta";
    key.textContent = label;
    const val = document.createElement("strong");
    val.textContent = String(value ?? 0);
    card.appendChild(key);
    card.appendChild(val);
    elements.statsSummary.appendChild(card);
  }

  const sections = [
    ["File Kinds", stats.file_kind_breakdown],
    ["Extensions", stats.extension_breakdown],
    ["AST Kinds", stats.ast_kind_breakdown],
    ["Edge Kinds", stats.edge_kind_breakdown],
  ];

  for (const [title, breakdown] of sections) {
    const entries = breakdownEntries(breakdown);
    if (!entries.length) {
      continue;
    }
    const card = document.createElement("section");
    card.className = "breakdown-card";
    const heading = document.createElement("strong");
    heading.textContent = title;
    const list = document.createElement("div");
    list.className = "breakdown-list";
    for (const [name, count] of entries) {
      const row = document.createElement("div");
      row.className = "breakdown-row";
      const label = document.createElement("span");
      label.textContent = name;
      const value = document.createElement("strong");
      value.textContent = String(count);
      row.appendChild(label);
      row.appendChild(value);
      list.appendChild(row);
    }
    card.appendChild(heading);
    card.appendChild(list);
    elements.statsBreakdowns.appendChild(card);
  }
}

function renderSymbolResults(container, entries, emptyMessage) {
  if (!container) {
    return;
  }
  container.replaceChildren();
  if (!entries.length) {
    renderEmptyState(container, emptyMessage);
    return;
  }
  for (const entry of entries) {
    const item = document.createElement("div");
    item.className = "result-item";

    const title = document.createElement("strong");
    title.className = "title";
    title.textContent = entry.qualified_name || entry.path || "(unknown)";

    const meta = document.createElement("div");
    meta.className = "meta";
    const exported = entry.exported ? " | exported" : "";
    meta.textContent = `${entry.ast_kind || "other"} | ${entry.path || ""} | Lines ${entry.start_line || 0}-${entry.end_line || 0}${exported}`;

    item.appendChild(title);
    item.appendChild(meta);

    if (entry.signature) {
      const signature = document.createElement("div");
      signature.className = "snippet";
      signature.textContent = entry.signature;
      item.appendChild(signature);
    }

    if (entry.doc_summary) {
      const docs = document.createElement("div");
      docs.className = "meta";
      docs.textContent = entry.doc_summary;
      item.appendChild(docs);
    }

    if (entry.chunk_id) {
      const actions = document.createElement("div");
      actions.className = "actions";
      const button = document.createElement("button");
      button.textContent = "View section";
      button.addEventListener("click", async () => {
        await openResultSection(entry);
      });
      actions.appendChild(button);
      item.appendChild(actions);
    }

    container.appendChild(item);
  }
}

function renderRefResults(container, refs) {
  if (!container) {
    return;
  }
  container.replaceChildren();
  if (!refs.length) {
    renderEmptyState(container, "No references.");
    return;
  }
  for (const ref of refs) {
    const item = document.createElement("div");
    item.className = "result-item";

    const title = document.createElement("strong");
    title.className = "title";
    title.textContent = `${ref.source_symbol || "(unknown)"} -> ${ref.target_symbol || "(unknown)"}`;

    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = `${ref.ast_kind || "other"} | ${ref.path || ""} | Lines ${ref.start_line || 0}-${ref.end_line || 0}`;

    const snippet = document.createElement("div");
    snippet.className = "snippet";
    snippet.textContent = ref.context || "";

    item.appendChild(title);
    item.appendChild(meta);
    item.appendChild(snippet);

    if (ref.chunk_id) {
      const actions = document.createElement("div");
      actions.className = "actions";
      const button = document.createElement("button");
      button.textContent = "View section";
      button.addEventListener("click", async () => {
        await openResultSection(ref);
      });
      actions.appendChild(button);
      item.appendChild(actions);
    }

    container.appendChild(item);
  }
}

async function loadStats(options = {}) {
  if (!state.indexLoaded) {
    setStatus("No index loaded.");
    return;
  }
  try {
    const { stats } = await callWorker("stats", {});
    renderStats(stats || {});
    if (!options.silent) {
      setStatus(`Index stats loaded for ${stats?.total_files || 0} files and ${stats?.total_chunks || 0} chunks.`);
    }
  } catch (error) {
    setStatus(`Stats failed: ${formatErrorForUi(error)}`);
  }
}

function setActiveTool(tool, options = {}) {
  state.activeTool = tool;
  const tabButtons = elements.toolTabs?.querySelectorAll("[data-tool]") || [];
  for (const button of tabButtons) {
    button.classList.toggle("is-active", button.dataset.tool === tool);
  }
  const views = document.querySelectorAll("[data-tool-view]");
  for (const view of views) {
    view.classList.toggle("is-active", view.dataset.toolView === tool);
  }
  if (state.indexLoaded && options.run !== false) {
    if (tool === "stats") {
      void loadStats({ silent: true });
    } else if (tool === "search" && elements.query?.value?.trim()) {
      void runSearch();
    }
  }
}

function formatSkipSummary({ skippedLarge, skippedTotal, skippedCount, skippedExports }) {
  if (!skippedLarge && !skippedTotal && !skippedCount && !skippedExports) {
    return "";
  }
  return ` (skipped: ${skippedLarge} too large, ${skippedTotal} over total limit, ${skippedCount} too many files, ${skippedExports} llmx exports)`;
}

function buildReadyStatus({ entryCount, replaced, previousLabel, nextLabel, skippedExports }) {
  const action = replaced
    ? `Replaced ${previousLabel || "current index"} with ${nextLabel || "new folder"}`
    : `Indexed ${entryCount} files`;
  const exportNote = skippedExports
    ? ` Ignored ${skippedExports} nested .llmx export bundle${skippedExports === 1 ? "" : "s"}.`
    : "";
  return `${action}.${exportNote}`;
}

async function runIngest(entries, collectedMeta) {
  if (!state.workerReady) {
    setStatus("Backend not ready.");
    return;
  }
  const skippedLarge = collectedMeta?.skippedLarge || 0;
  const skippedTotal = collectedMeta?.skippedTotal || 0;
  const skippedCount = collectedMeta?.skippedCount || 0;
  const skippedExports = collectedMeta?.skippedExports || 0;
  const totalBytes = collectedMeta?.totalBytes || 0;
  const skippedNote = formatSkipSummary({ skippedLarge, skippedTotal, skippedCount, skippedExports });
  if (!entries.length) {
    if (skippedExports) {
      setStatus(
        `Ignored ${skippedExports} .llmx export bundle${skippedExports === 1 ? "" : "s"}. Select the original source folder or load an exported index JSON instead.`
      );
    } else {
      setStatus(`No supported files found. Accepted: ${ALLOWED_EXTENSIONS.join(", ")}`);
    }
    return;
  }

  // Warn if approaching limits
  if (totalBytes > DEFAULT_LIMITS.warnTotalBytes) {
    console.warn(`Large upload: ${(totalBytes / 1024 / 1024).toFixed(1)}MB. Browser may slow down.`);
  }
  if (hasMaxFileCountLimit() && entries.length > DEFAULT_LIMITS.maxFileCount * 0.8) {
    console.warn(`Many files: ${entries.length}. Processing may take time.`);
  }

  const incomingSourceLabel = inferSourceLabel(entries, collectedMeta);
  const previousSourceLabel = state.sourceLabel;
  const replacingIndex =
    state.indexLoaded &&
    Boolean(previousSourceLabel) &&
    Boolean(incomingSourceLabel) &&
    previousSourceLabel !== incomingSourceLabel;
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
    updateUIState();
    state.sourceLabel = incomingSourceLabel;
    clearSearchUi();

    if (!state.indexLoaded || replacingIndex) {
      const actionLabel = replacingIndex
        ? `Replacing ${previousSourceLabel || "current index"} with ${incomingSourceLabel || "new folder"}`
        : `Ingesting ${entries.length} files`;
      setStatus(`${actionLabel}${skippedNote}...`);
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
          const progressLabel = replacingIndex
            ? `Replacing index (${i}/${entries.length})`
            : `Ingesting ${entries.length} files (${i}/${entries.length})`;
          setStatus(`${progressLabel}${skippedNote}...`);
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
    state.sourceLabel = inferSourceLabelFromPaths(state.files.map((file) => file.path)) || incomingSourceLabel;
    updateExportUiLabels();

    elements.indexId.textContent = state.indexId || "(unknown)";
    const chunks = statsResult.stats.total_chunks;
    state.chunkCount = chunks;
    elements.chunkCount.textContent = `${chunks} chunks`;
    elements.chunkCount.hidden = false;
    elements.warningCount.textContent = warningsResult.warnings.length;
    renderWarnings(warningsResult.warnings);
    populateFileFilter();
    await updateOutlineSymbols();
    await populateSavedIndexes();
    state.indexLoaded = true;
    updateUIState();
    refreshEmbeddingsStatus();
    await maybeAutoBuildEmbeddings("Auto embeddings");
    if (state.activeTool === "stats") {
      await loadStats({ silent: true });
    }
    if (state.activeTool === "search" && elements.query.value.trim()) {
      await runSearch();
    } else {
      setStatus(
        buildReadyStatus({
          entryCount: entries.length,
          replaced: replacingIndex,
          previousLabel: previousSourceLabel,
          nextLabel: state.sourceLabel,
          skippedExports,
        })
      );
    }
  } catch (error) {
    state.sourceLabel = previousSourceLabel;
    setStatus(`Ingestion failed: ${formatErrorForUi(error)}`);
  } finally {
    state.busy = false;
    updateUIState();
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

async function activateLoadedIndex(statusMessage, restoredEmbeddingsMeta = null) {
  const idResult = await callWorker("indexId", {});
  const statsResult = await callWorker("stats", {});
  const warningsResult = await callWorker("warnings", {});
  const filesResult = await callWorker("files", {});
  state.indexId = idResult.indexId || null;
  state.files = filesResult.files || [];
  state.sourceLabel = inferSourceLabelFromPaths(state.files.map((file) => file.path));
  state.indexLoaded = true;
  updateUIState();
  updateExportUiLabels();
  clearSearchUi();
  elements.indexId.textContent = state.indexId || "(unknown)";
  const chunks = statsResult.stats.total_chunks;
  state.chunkCount = chunks;
  elements.chunkCount.textContent = `${chunks} chunks`;
  elements.chunkCount.hidden = false;
  elements.warningCount.textContent = warningsResult.warnings.length;
  renderWarnings(warningsResult.warnings);
  populateFileFilter();
  await updateOutlineSymbols();
  await populateSavedIndexes();
  if (restoredEmbeddingsMeta) {
    refreshEmbeddingsStatus(restoredEmbeddingsMeta);
  } else {
    refreshEmbeddingsStatus();
    await maybeAutoBuildEmbeddings("Auto embeddings");
  }
  if (state.activeTool === "stats") {
    await loadStats({ silent: true });
  }
  if (state.activeTool === "search" && elements.query.value.trim()) {
    await runSearch();
  } else {
    setStatus(statusMessage);
  }
}

async function loadIndexJsonFile(file) {
  if (!file) {
    return;
  }
  try {
    state.busy = true;
    updateUIState();
    setStatus(`Loading ${file.name}...`);
    const json = await file.text();
    await callWorker("loadIndexJson", { json });
    await activateLoadedIndex(`Loaded ${file.name}.`);
  } catch (error) {
    setStatus(`Failed to load index JSON: ${formatErrorForUi(error)}`);
  } finally {
    state.busy = false;
    updateUIState();
  }
}

elements.selectFolder.addEventListener("click", async () => {
  elements.folderInput.click();
});

elements.loadIndexJson?.addEventListener("click", async () => {
  elements.indexJsonInput?.click();
});

elements.replaceIndex?.addEventListener("click", async () => {
  elements.folderInput.click();
});

elements.indexJsonInput?.addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  await loadIndexJsonFile(file);
  event.target.value = "";
});


elements.folderInput.addEventListener("change", async (event) => {
  const collected = await collectFilesFromInput(event.target.files || []);
  await runIngest(collected.entries, collected);
  event.target.value = "";
});

elements.filesInput.addEventListener("change", async (event) => {
  const collected = await collectFilesFromInput(event.target.files || []);
  await runIngest(collected.entries, collected);
  event.target.value = "";
});

elements.dropZone.addEventListener("click", (event) => {
  if (event.target.closest("button, input, select, a, label")) {
    return;
  }
  elements.folderInput.click();
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

elements.toolTabs?.addEventListener("click", (event) => {
  const button = event.target.closest("[data-tool]");
  if (!button) {
    return;
  }
  setActiveTool(button.dataset.tool);
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
    elements.buildEmbeddings.title = "Embeddings are disabled. Remove ?embeddings=0 to enable them.";
  } else {
    elements.buildEmbeddings.title = globalThis.LLMX_ENABLE_WEBGPU
      ? "Build embeddings for semantic search."
      : "Build embeddings for semantic search using the CPU fallback.";
  }

  elements.buildEmbeddings.addEventListener("click", async () => {
    if (!state.indexLoaded) {
      setStatus("No index loaded.");
      return;
    }
    if (!embeddingsRequested) {
      setStatus("Embeddings disabled (?embeddings=0).");
      return;
    }

    // Warn about CPU embeddings being slow and potentially unstable
    if (forceCpu && !globalThis.LLMX_ENABLE_WEBGPU) {
      const chunkCount = state.chunkCount;
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
    try {
      await buildEmbeddingsForSearch({ reason: "Embeddings" });
    } catch (error) {
      console.warn("Manual embeddings build failed:", error);
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

elements.searchStrategy?.addEventListener("change", () => {
  scheduleSearch();
});

elements.hybridStrategy?.addEventListener("change", () => {
  scheduleSearch();
});

elements.searchIntent?.addEventListener("change", () => {
  scheduleSearch();
});

elements.searchExplain?.addEventListener("change", () => {
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

elements.runSymbols?.addEventListener("click", async () => {
  await runSymbols();
});

elements.runLookup?.addEventListener("click", async () => {
  await runLookup();
});

elements.runRefs?.addEventListener("click", async () => {
  await runRefs();
});

for (const input of [
  elements.symbolsPattern,
  elements.symbolsPath,
  elements.symbolsLimit,
  elements.lookupSymbol,
  elements.lookupPath,
  elements.lookupLimit,
  elements.refsSymbol,
  elements.refsDepth,
  elements.refsLimit,
]) {
  input?.addEventListener("keydown", async (event) => {
    if (event.key !== "Enter") {
      return;
    }
    if (input === elements.lookupSymbol || input === elements.lookupPath || input === elements.lookupLimit) {
      await runLookup();
      return;
    }
    if (input === elements.refsSymbol || input === elements.refsDepth || input === elements.refsLimit) {
      await runRefs();
      return;
    }
    await runSymbols();
  });
}

elements.symbolsKind?.addEventListener("change", () => {
  void runSymbols();
});

elements.lookupKind?.addEventListener("change", () => {
  void runLookup();
});

elements.refsDirection?.addEventListener("change", () => {
  void runRefs();
});

elements.closeChunk.addEventListener("click", () => {
  elements.chunkView.hidden = true;
});

elements.downloadExport?.addEventListener("click", () => {
  if (!state.indexLoaded) {
    setStatus("No index to export.");
    return;
  }
  callWorker("exportZip", {})
    .then(({ bytes }) => {
      const name = `${exportBaseName()}.searchable.zip`;
      downloadFile(name, bytes, "application/zip");
    })
    .catch(() => setStatus("Export failed."));
});

elements.downloadCompactExport?.addEventListener("click", () => {
  if (!state.indexLoaded) {
    setStatus("No index to export.");
    return;
  }
  callWorker("exportZipCompact", {})
    .then(({ bytes }) => {
      const name = `${exportBaseName()}.compact.zip`;
      downloadFile(name, bytes, "application/zip");
    })
    .catch(() => setStatus("Compact export failed."));
});

elements.downloadIndexJson?.addEventListener("click", async () => {
  if (!state.indexLoaded) {
    setStatus("No index to export.");
    return;
  }
  try {
    const { json } = await callWorker("exportIndexJson", {});
    const index = JSON.parse(json);

    // Try to get embeddings and include them if available
    try {
      const embResult = await callWorker("getEmbeddings", {});
      if (embResult.embeddings && embResult.meta) {
        const floatArray = new Float32Array(embResult.embeddings);
        const { dim, count, modelId } = embResult.meta;

        index.embeddings_meta = { dim, count, modelId };
        setStatus("Exporting index metadata...");
      }
    } catch (embErr) {
      // No embeddings available, continue without them
      console.log("No embeddings to export:", embErr);
    }

    const name = `${exportBaseName()}.index.json`;
    const updatedJson = JSON.stringify(index, null, 2);
    downloadFile(name, updatedJson, "application/json");
    setStatus("Index exported.");
  } catch (error) {
    setStatus("Export failed.");
  }
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
  const filters = buildSearchFilters();
  const seq = ++state.searchSeq;
  elements.runSearch.disabled = true;
  setStatus("Searching...");
  try {
    const response = await callWorker("search", {
      query,
      filters,
      limit: SEARCH_LIMIT,
      strategy: elements.searchStrategy?.value || "auto",
      hybridStrategy: elements.hybridStrategy?.value || "rrf",
      intent: elements.searchIntent?.value || "auto",
      explain: elements.searchExplain?.checked !== false,
    });
    if (seq !== state.searchSeq) {
      return;
    }
    const rawResults = response?.results || [];
    const groupedResults = shapeSearchResults(rawResults, SEARCH_LIMIT);
    renderResults(groupedResults);
    const semanticNote = response?.usedSemantic ? " using semantic reranking" : "";
    const noticeText = formatNotices(response?.notices);
    setStatus(`Found ${groupedResults.length} sections from ${rawResults.length} hits${semanticNote}.${noticeText}`);
  } catch (error) {
    if (seq === state.searchSeq) {
      setStatus(`Search failed: ${formatErrorForUi(error)}`);
    }
  } finally {
    if (seq === state.searchSeq) {
      elements.runSearch.disabled = false;
    }
  }
}

async function runSymbols() {
  if (!state.indexLoaded) {
    setStatus("No index loaded.");
    return;
  }
  const limit = parseBoundedInt(elements.symbolsLimit?.value, 50, 1, 500);
  try {
    const response = await callWorker("symbolsRich", {
      pattern: elements.symbolsPattern?.value?.trim() || null,
      ast_kind: elements.symbolsKind?.value || null,
      path_prefix: elements.symbolsPath?.value?.trim() || null,
      limit,
    });
    renderSymbolResults(elements.symbolsResults, response?.symbols || [], "No symbols.");
    setStatus(`Listed ${Math.min(response?.symbols?.length || 0, response?.total || 0)} of ${response?.total || 0} symbols.`);
  } catch (error) {
    setStatus(`Symbols failed: ${formatErrorForUi(error)}`);
  }
}

async function runLookup() {
  if (!state.indexLoaded) {
    setStatus("No index loaded.");
    return;
  }
  const symbol = elements.lookupSymbol?.value?.trim() || "";
  if (!symbol) {
    renderEmptyState(elements.lookupResults, "Enter a symbol to look up.");
    setStatus("Enter a symbol to look up.");
    return;
  }
  const limit = parseBoundedInt(elements.lookupLimit?.value, 20, 1, 200);
  try {
    const response = await callWorker("lookupSymbol", {
      symbol,
      kind: elements.lookupKind?.value || null,
      path_prefix: elements.lookupPath?.value?.trim() || null,
      limit,
    });
    renderSymbolResults(elements.lookupResults, response?.matches || [], "No symbol matches.");
    setStatus(`Found ${response?.total || 0} symbol matches.`);
  } catch (error) {
    setStatus(`Lookup failed: ${formatErrorForUi(error)}`);
  }
}

async function runRefs() {
  if (!state.indexLoaded) {
    setStatus("No index loaded.");
    return;
  }
  const symbol = elements.refsSymbol?.value?.trim() || "";
  if (!symbol) {
    renderEmptyState(elements.refsResults, "Enter a symbol to trace.");
    setStatus("Enter a symbol to trace.");
    return;
  }
  try {
    const response = await callWorker("refsForSymbol", {
      symbol,
      direction: elements.refsDirection?.value || "callers",
      depth: parseBoundedInt(elements.refsDepth?.value, 1, 1, 8),
      limit: parseBoundedInt(elements.refsLimit?.value, 20, 1, 200),
    });
    renderRefResults(elements.refsResults, response?.refs || []);
    setStatus(`Found ${response?.total || 0} references.`);
  } catch (error) {
    setStatus(`Refs failed: ${formatErrorForUi(error)}`);
  }
}

async function openResultSection(result) {
  const chunkIds = Array.from(new Set(result.chunk_ids || [result.chunk_id]));
  const chunks = [];
  for (const chunkId of chunkIds) {
    const { chunk } = await callWorker("getChunk", { chunkId });
    if (chunk) {
      chunks.push(chunk);
    }
  }
  if (!chunks.length) {
    return;
  }

  chunks.sort((a, b) => a.start_line - b.start_line);
  const first = chunks[0];
  const last = chunks[chunks.length - 1];
  const ref = result.chunk_ref ? ` | Ref: ${result.chunk_ref}` : "";
  const heading = Array.isArray(result.heading_path) && result.heading_path.length
    ? ` | ${result.heading_path.join(" / ")}`
    : "";
  elements.chunkTitle.textContent = `${result.path} (${first.start_line}-${last.end_line})${ref}${heading}`;
  elements.chunkContent.textContent = chunks
    .map((chunk) => {
      if (chunks.length === 1) {
        return chunk.content;
      }
      return `[${chunk.start_line}-${chunk.end_line}]\n${chunk.content}`;
    })
    .join("\n\n");
  elements.chunkView.hidden = false;
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
    title.className = "title";
    title.textContent = result.title || result.path;

    const meta = document.createElement("div");
    meta.className = "meta";
    const subtitle = result.subtitle ? ` | ${result.subtitle}` : "";
    const ref = result.chunk_ref ? ` | ${result.chunk_ref}` : "";
    const engines = (result.matched_engines || []).length
      ? ` | ${result.matched_engines.join(" + ")}`
      : "";
    meta.textContent = `Lines ${result.start_line}-${result.end_line}${ref}${subtitle}${engines}`;

    const snippet = document.createElement("div");
    snippet.className = "snippet";
    snippet.textContent = result.snippet;

    const reason = document.createElement("div");
    reason.className = "meta";
    reason.textContent = result.match_reason || "Matched by lexical ranking.";

    const button = document.createElement("button");
    button.textContent = "View section";
    button.addEventListener("click", async () => {
      await openResultSection(result);
    });

    item.appendChild(title);
    item.appendChild(meta);
    item.appendChild(snippet);
    item.appendChild(reason);
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
  elements.kindFilter.replaceChildren();
  const option = document.createElement("option");
  option.value = "";
  option.textContent = "All files";
  elements.fileFilter.appendChild(option);
  const kindOption = document.createElement("option");
  kindOption.value = "";
  kindOption.textContent = "All kinds";
  elements.kindFilter.appendChild(kindOption);
  const kinds = new Set();
  for (const file of state.files || []) {
    const item = document.createElement("option");
    item.value = file.path;
    item.textContent = file.path;
    elements.fileFilter.appendChild(item);
    if (file.kind) {
      kinds.add(String(file.kind));
    }
  }
  for (const kind of Array.from(kinds).sort()) {
    const item = document.createElement("option");
    item.value = kind;
    item.textContent = kind;
    elements.kindFilter.appendChild(item);
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
  if (!state.indexLoaded || state.busy || state.activeTool !== "search") {
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
    updateUIState();
    await callWorker("loadIndexJson", { json: record.json });
    let restoredEmbeddingsMeta = null;
    if (record.embeddings && record.embeddings_meta) {
      try {
        await callWorker(
          "setEmbeddings",
          { embeddings: record.embeddings, meta: record.embeddings_meta },
          state.backend?.kind === "worker" ? [record.embeddings] : undefined
        );
        restoredEmbeddingsMeta = record.embeddings_meta;
      } catch {}
    }
    await activateLoadedIndex("Loaded saved index.", restoredEmbeddingsMeta);
  } catch (error) {
    setStatus(`Failed to load saved index: ${formatErrorForUi(error)}`);
  } finally {
    state.busy = false;
    updateUIState();
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
  const currentUrl = `${window.location.pathname}${window.location.search}`;

  // Only reload if settings actually changed
  if (newUrl === currentUrl) {
    setStatus("Settings unchanged.");
    return;
  }

  window.location.href = newUrl;
}

function resetSettings() {
  window.location.href = window.location.pathname;
}

elements.applySettings?.addEventListener("click", applySettings);
elements.resetSettings?.addEventListener("click", resetSettings);

loadSettingsFromUrl();
configureFolderPickerUi();
setActiveTool(state.activeTool, { run: false });
initWorker().catch((error) => {
  setStatus(`Failed to start backend: ${formatErrorForUi(error)}`);
});
