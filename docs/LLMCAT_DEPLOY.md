# Deploying Semantic Embeddings to llm.cat

This repo’s production-ready static build lives in `web/`.

## What to deploy

Upload the entire `web/` directory to llm.cat, including:

- `web/index.html`
- `web/app.js`
- `web/worker.js`
- `web/pkg/` (WASM output from `wasm-pack`)
- `web/models/arctic-embed-s.bin` (model weights; cached in IndexedDB at runtime)
- `web/models/tokenizer.json` (tokenizer; cached in IndexedDB at runtime)

## Build command (produces `web/pkg/`)

From `ingestor-wasm/`:

```bash
LLMX_EMBEDDING_MODEL_URL="./models/arctic-embed-s.bin" \
wasm-pack build --target web --out-dir ../web/pkg --mode no-install --release
```

Or run:

```bash
./scripts/prepare_llmcat_web.sh
```

Important:
- `LLMX_EMBEDDING_MODEL_URL` is embedded in the WASM at build time and is visible to clients.
- Using a relative URL like `./models/arctic-embed-s.bin` forces same-origin loading and avoids CORS complexity (and works if llm.cat serves this app from a sub-path).

## Server configuration notes (important)

- Ensure `.wasm` is served with `Content-Type: application/wasm`.
- Ensure `web/models/arctic-embed-s.bin` is publicly reachable (no auth), otherwise embeddings will fall back.

## Browser expectations

- On first load, the model downloads once and is cached in IndexedDB.
- Repeat visits should load instantly from IndexedDB.
- If module workers are blocked, `/home/zack/dev/llmx/web/app.js` falls back to the local (main-thread) backend and still supports semantic ranking.
