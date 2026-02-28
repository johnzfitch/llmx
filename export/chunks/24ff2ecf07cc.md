---
chunk_index: 1333
ref: "24ff2ecf07cc"
id: "24ff2ecf07cc25f8de7c0813a5e3cb3645e46854fd20fa2bd31522066945febd"
slug: "app--initworker"
path: "/home/zack/dev/llmx/web/app.js"
kind: "java_script"
lines: [225, 256]
token_estimate: 219
content_sha256: "ea5fbe16b36951173d086fa261e790039e64e535f9a3e053fb8c7bd732b63a81"
compacted: false
heading_path: []
symbol: "initWorker"
address: null
asset_path: null
---

async function initWorker() {
  let initError = null;

  try {
    state.backend = createWorkerBackend();
    const result = await callWorker("ping", {});
    if (!result.ready) {
      throw new Error("Worker did not initialize");
    }
    state.workerReady = true;
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
  setStatus(`Ready for ingestion (worker disabled)${reason}.`);
  await populateSavedIndexes();
}