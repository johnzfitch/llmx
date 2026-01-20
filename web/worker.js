import init, { Embedder, Ingestor } from "./pkg/ingestor_wasm.js";

const urlParams = (() => {
  try {
    return new URL(self.location.href).searchParams;
  } catch {
    return new URLSearchParams();
  }
})();
const embeddingsRequested = urlParams.get("embeddings") === "1";
const forceCpu = urlParams.get("cpu") === "1";
const webGpuParam = urlParams.get("webgpu");
const webGpuRequested = webGpuParam === "1" || (embeddingsRequested && webGpuParam !== "0" && !forceCpu);
const forceWebGpu = urlParams.get("force_webgpu") === "1";
const isFirefox = (() => {
  const ua = (self.navigator && self.navigator.userAgent) || "";
  return ua.includes("Firefox/") && !ua.includes("Seamonkey/");
})();
const isFirefoxNightly = (() => {
  const ua = (self.navigator && self.navigator.userAgent) || "";
  // Nightly user agents usually look like: "Firefox/123.0a1"
  return /Firefox\/[0-9]+(\.[0-9]+)*a1\b/.test(ua);
})();
const webGpuAvailable = Boolean(self.navigator && self.navigator.gpu);
self.LLMX_ENABLE_WEBGPU = webGpuRequested && webGpuAvailable;
self.LLMX_ENABLE_EMBEDDINGS = embeddingsRequested;
if (webGpuRequested && !webGpuAvailable) {
  console.warn(
    "WebGPU unavailable (navigator.gpu missing). To use embeddings, either use a WebGPU-capable Chromium browser or add ?cpu=1 to allow slow CPU embeddings."
  );
}
if (webGpuRequested && isFirefox && !isFirefoxNightly && !forceWebGpu) {
  self.LLMX_ENABLE_WEBGPU = false;
  console.warn(
    "WebGPU requested on Firefox, but is disabled by default due to stability issues. Use Chromium, use Firefox Nightly, or add ?force_webgpu=1 to override."
  );
}
if (!embeddingsRequested) {
  console.log("Embeddings disabled (add ?embeddings=1 to enable).");
}

let ready = false;
let ingestor = null;
let embedder = null;
let embeddings = null; // Float32Array
let embeddingsMeta = null; // { dim, count, modelId }
let chunkMeta = null; // Array<{ id, ref, path, kind, start_line, end_line, heading_path, heading_joined, symbol, snippet }>
let buildEmbeddingsPromise = null;

let readyPromise = null;

function toError(error) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  try {
    return JSON.stringify(error);
  } catch {
    return "Unknown error";
  }
}

function ensureInitStarted() {
  if (readyPromise) {
    return readyPromise;
  }

  readyPromise = (async () => {
    try {
      // Be explicit about the WASM path. Some browsers/extensions/tooling can
      // cause wasm-bindgen's default resolution to fall back to the document base.
      const wasmUrl = new URL("./pkg/ingestor_wasm_bg.wasm", self.location.href);
      await init({ module_or_path: wasmUrl });
      ready = true;
    } catch (error) {
      ready = false;
      throw error;
    }
  })();

  return readyPromise;
}

function attachGlobalErrorLogging() {
  self.addEventListener("unhandledrejection", (event) => {
    const reason = event?.reason;
    const message = `Worker unhandledrejection: ${toError(reason)}`;
    console.error(message, reason);
  });

  self.addEventListener("error", (event) => {
    const message = event?.message ? `Worker error: ${event.message}` : "Worker error";
    console.error(message, event?.error);
  });
}

attachGlobalErrorLogging();

async function ensureReady() {
  await ensureInitStarted();
  if (!ready) {
    throw new Error("WASM not initialized");
  }
}

async function ensureEmbedder() {
  if (!self.LLMX_ENABLE_EMBEDDINGS) {
    throw new Error("Embeddings disabled (add ?embeddings=1).");
  }

  if (!embedder) {
    const requestedBackend = self.LLMX_ENABLE_WEBGPU ? "WebGPU" : "CPU";
    console.log(`Embedder init: starting (backend=${requestedBackend})`);

    try {
      embedder = await Embedder.create();
      console.log(`Embedder init: ready (backend=${requestedBackend})`);
    } catch (error) {
      // If WebGPU was requested but failed, try falling back to CPU if allowed
      if (self.LLMX_ENABLE_WEBGPU && forceCpu) {
        console.warn(`WebGPU embedder creation failed, falling back to CPU: ${toError(error)}`);
        self.LLMX_ENABLE_WEBGPU = false;
        embedder = await Embedder.create();
        console.log("Embedder init: ready (backend=CPU, fallback)");
      } else {
        throw error;
      }
    }
  }
  return embedder;
}

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
  if (!self.LLMX_ENABLE_EMBEDDINGS) return false;
  if (!embeddings || !embeddingsMeta || !chunkMeta) return false;
  if (!embedder) return false;
  if (embeddingsMeta.modelId !== embedder.modelId()) return false;
  if (embeddingsMeta.count !== chunkMeta.length) return false;
  return true;
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

  const merged = Array.from(scores.entries())
    .map(([chunkId, score]) => ({ chunkId, score }))
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);

  return merged;
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

function dotProduct(a, b, bOffset, dim) {
  let sum = 0;
  for (let i = 0; i < dim; i += 1) {
    sum += a[i] * b[bOffset + i];
  }
  return sum;
}

async function buildEmbeddingsIndex() {
  if (!self.LLMX_ENABLE_EMBEDDINGS) {
    throw new Error("Embeddings disabled (add ?embeddings=1).");
  }
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

  const count = chunks.length;
  const estimatedMb = (count * dim * 4) / 1024 / 1024;
  console.log(
    `Embeddings: building (model=${modelId}, chunks=${count}, dim=${dim}, approx=${estimatedMb.toFixed(1)}MB)`
  );

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

  if (index.embeddings && index.embedding_model === modelId) {
    const fromJson = index.embeddings;
    const count = chunkMeta.length;
    if (!Array.isArray(fromJson) || fromJson.length !== count) {
      throw new Error("Saved embeddings shape mismatch");
    }
    const view = new Float32Array(count * dim);
    for (let i = 0; i < count; i += 1) {
      const row = fromJson[i];
      if (!Array.isArray(row) || row.length !== dim) {
        throw new Error("Saved embeddings dimension mismatch");
      }
      view.set(row, i * dim);
    }
    embeddings = view;
    embeddingsMeta = { dim, count, modelId };
    for (const meta of chunkMeta) {
      delete meta.content;
    }
    return embeddingsMeta;
  }

  const view = new Float32Array(count * dim);
  // Detect Firefox for extra-conservative batching (stricter WASM memory limits)
  const isFirefox = typeof navigator !== 'undefined' && /Firefox/.test(navigator.userAgent);
  // Reduce batch size for CPU to prevent browser crashes
  // Firefox needs batch size 1 due to stricter WASM memory limits
  const batchSize = isFirefox ? 1 : 2;
  const totalBatches = Math.ceil(count / batchSize);

  for (let offset = 0; offset < count; offset += batchSize) {
    const batchNum = Math.floor(offset / batchSize) + 1;
    if (batchNum % 20 === 0 || batchNum === 1) {
      console.log(`Embeddings: processing batch ${batchNum}/${totalBatches} (${offset}/${count} chunks)`);
    }
    const batch = chunkMeta.slice(offset, offset + batchSize);
    const texts = batch.map((item) => item.content);

    // Add yield point to prevent blocking and allow GC
    if (batchNum % 5 === 0) {
      await new Promise(resolve => setTimeout(resolve, 0));
    }

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
  }

  embeddings = view;
  embeddingsMeta = { dim, count, modelId };

  for (const meta of chunkMeta) {
    delete meta.content;
  }

  // Store embeddings in the index for persistence via exportIndexJson
  try {
    ingestor.setEmbeddings(embeddings, modelId, dim);
    console.log("Embeddings stored in index for persistence");
  } catch (error) {
    console.warn("Failed to store embeddings in index:", error);
    // Non-fatal: embeddings are still available in memory
  }

  return embeddingsMeta;
}

self.onmessage = async (event) => {
  const { id, op, payload } = event.data || {};
  if (!id) {
    return;
  }

  try {
    await ensureReady();

    switch (op) {
      case "ping": {
        self.postMessage({ id, ok: true, data: { ready: true } });
        return;
      }
      case "getCapabilities": {
        self.postMessage({
          id,
          ok: true,
          data: {
            webgpu: self.LLMX_ENABLE_WEBGPU,
            embeddings: self.LLMX_ENABLE_EMBEDDINGS,
            forceCpu,
          },
        });
        return;
      }
      case "ingest": {
        const files = (payload.files || []).map((file) => ({
          path: file.path,
          data: new Uint8Array(file.data),
          mtime_ms: file.mtime_ms ?? null,
          fingerprint_sha256: file.fingerprint_sha256 ?? null,
        }));
        ingestor = Ingestor.ingest(files, null);
        embeddings = null;
        embeddingsMeta = null;
        chunkMeta = null;
        buildEmbeddingsPromise = null;
        self.postMessage({ id, ok: true, data: { indexId: ingestor.indexId() } });
        return;
      }
      case "initEmbedder": {
        if (!self.LLMX_ENABLE_EMBEDDINGS) {
          throw new Error("Embeddings disabled (add ?embeddings=1).");
        }

        await ensureEmbedder();
        self.postMessage({
          id,
          ok: true,
          data: { modelId: embedder.modelId(), dimension: embedder.dimension() },
        });
        return;
      }
      case "buildEmbeddings": {
        if (!buildEmbeddingsPromise) {
          buildEmbeddingsPromise = buildEmbeddingsIndex().finally(() => {
            buildEmbeddingsPromise = null;
          });
        }
        const meta = await buildEmbeddingsPromise;
        self.postMessage({ id, ok: true, data: { meta } });
        return;
      }
      case "getEmbeddings": {
        if (!embeddings || !embeddingsMeta) {
          self.postMessage({ id, ok: true, data: { embeddings: null } });
          return;
        }
        const buffer = embeddings.buffer.slice(0);
        self.postMessage(
          { id, ok: true, data: { embeddings: buffer, meta: embeddingsMeta } },
          [buffer]
        );
        return;
      }
      case "setEmbeddings": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
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
        self.postMessage({ id, ok: true, data: { loaded: true } });
        return;
      }
      case "loadIndexJson": {
        ingestor = Ingestor.fromIndexJson(payload.json);
        embeddings = null;
        embeddingsMeta = null;
        chunkMeta = null;
        buildEmbeddingsPromise = null;
        self.postMessage({ id, ok: true, data: { loaded: true } });
        return;
      }
      case "updateSelective": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const files = (payload.files || []).map((file) => ({
          path: file.path,
          data: new Uint8Array(file.data),
          mtime_ms: file.mtime_ms ?? null,
          fingerprint_sha256: file.fingerprint_sha256 ?? null,
        }));
        await ingestor.updateSelective(files, payload.keepPaths || [], null);

        // Clear embeddings after selective update (index has changed)
        embeddings = null;
        embeddingsMeta = null;
        chunkMeta = null;

        self.postMessage({ id, ok: true, data: { updated: true } });
        return;
      }
      case "exportIndexJson": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        self.postMessage({ id, ok: true, data: { json: ingestor.exportIndexJson() } });
        return;
      }
      case "stats": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const stats = await ingestor.stats();
        self.postMessage({ id, ok: true, data: { stats } });
        return;
      }
      case "files": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const files = await ingestor.files();
        self.postMessage({ id, ok: true, data: { files } });
        return;
      }
      case "indexId": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        self.postMessage({ id, ok: true, data: { indexId: ingestor.indexId() } });
        return;
      }
      case "warnings": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const warnings = await ingestor.warnings();
        self.postMessage({ id, ok: true, data: { warnings } });
        return;
      }
      case "search": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const query = payload.query || "";
        const filters = payload.filters || null;
        const limit = payload.limit || 20;

        const bm25Results = await ingestor.search(query, filters, limit * 2);

        if (!embeddings || !embeddingsMeta || !chunkMeta) {
          self.postMessage({ id, ok: true, data: { results: bm25Results } });
          return;
        }

        await ensureEmbedder();

        if (!shouldUseEmbeddings()) {
          self.postMessage({ id, ok: true, data: { results: bm25Results } });
          return;
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
          return { chunk_id: chunkId, chunk_ref: "", score, path: "", start_line: 0, end_line: 0, snippet: "", heading_path: [] };
        });

        self.postMessage({ id, ok: true, data: { results } });
        return;
      }
      case "getChunk": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const chunk = await ingestor.getChunk(payload.chunkId);
        self.postMessage({ id, ok: true, data: { chunk } });
        return;
      }
      case "listOutline": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const outline = await ingestor.listOutline(payload.path);
        self.postMessage({ id, ok: true, data: { outline } });
        return;
      }
      case "listSymbols": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const symbols = await ingestor.listSymbols(payload.path);
        self.postMessage({ id, ok: true, data: { symbols } });
        return;
      }
      case "exportLlm": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const content = ingestor.exportLlm();
        self.postMessage({ id, ok: true, data: { content } });
        return;
      }
      case "exportLlmPointer": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const content = ingestor.exportLlmPointer();
        self.postMessage({ id, ok: true, data: { content } });
        return;
      }
      case "exportManifestLlmTsv": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const content = ingestor.exportManifestLlmTsv();
        self.postMessage({ id, ok: true, data: { content } });
        return;
      }
      case "exportCatalogLlmMd": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const content = ingestor.exportCatalogLlmMd();
        self.postMessage({ id, ok: true, data: { content } });
        return;
      }
      case "exportOutline": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const content = ingestor.exportLlm();
        self.postMessage({ id, ok: true, data: { content } });
        return;
      }
      case "exportZip": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const bytes = ingestor.exportZip();
        self.postMessage({ id, ok: true, data: { bytes } }, [bytes.buffer]);
        return;
      }
      case "exportZipCompact": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const bytes = ingestor.exportZipCompact();
        self.postMessage({ id, ok: true, data: { bytes } }, [bytes.buffer]);
        return;
      }
      default: {
        throw new Error(`Unknown op: ${op}`);
      }
    }
  } catch (error) {
    self.postMessage({ id, ok: false, error: toError(error) });
  }
};
