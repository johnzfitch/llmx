import init, { Ingestor } from "./pkg/ingestor_wasm.js";

let ready = false;
let ingestor = null;

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
        self.postMessage({ id, ok: true, data: { indexId: ingestor.indexId() } });
        return;
      }
      case "loadIndexJson": {
        ingestor = Ingestor.fromIndexJson(payload.json);
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
        const results = await ingestor.search(payload.query, payload.filters, payload.limit);
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
