import init, { Embedder, Ingestor } from "./pkg/ingestor_wasm.js";

let ready = false;
let ingestor = null;
let embedder = null;
let embeddings = null; // Float32Array
let embeddingsMeta = null; // { dim, count, modelId }
let chunkMeta = null; // Array<{ id, ref, path, kind, start_line, end_line, heading_path, heading_joined, symbol, snippet }>
let buildEmbeddingsPromise = null;

const readyPromise = init().then(() => {
  ready = true;
});

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

async function ensureReady() {
  await readyPromise;
  if (!ready) {
    throw new Error("WASM not initialized");
  }
}

async function ensureEmbedder() {
  if (!embedder) {
    embedder = await Embedder.create();
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
  }

  embeddings = view;
  embeddingsMeta = { dim, count, modelId };

  for (const meta of chunkMeta) {
    delete meta.content;
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
      case "exportZip": {
        if (!ingestor) {
          throw new Error("No index loaded");
        }
        const bytes = ingestor.exportZip();
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
