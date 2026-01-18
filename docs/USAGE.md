# Prototype Usage

## Build WASM

From repo root:

```bash
cd ingestor-wasm
wasm-pack build --target web --out-dir ../web/pkg
```

## Verification

From repo root:

```bash
# Build the WASM package (skip wasm-bindgen/wasm-opt auto-installs)
cd ingestor-wasm
LLMX_EMBEDDING_MODEL_URL="./models/arctic-embed-s.bin" \
  wasm-pack build --target web --out-dir ../web/pkg --mode no-install --release

# Run Rust unit tests (native)
cargo test -p ingestor-wasm

# Optional: validate INT8 quantization quality (requires local safetensors)
LLMX_VALIDATE_QUANT=1 cargo test -p ingestor-wasm

# Optional: tighten bin-vs-quantized threshold (default 1e-6)
LLMX_VALIDATE_QUANT=1 LLMX_BIN_MSE_MAX=1e-6 cargo test -p ingestor-wasm

# Optional: backend parity (requires working WebGPU on host)
LLMX_RUN_WGPU_TESTS=1 cargo test -p ingestor-wasm
```

For a deployment-style build (model + tokenizer staged under `web/models/`), run:

```bash
./scripts/prepare_llmcat_web.sh
```

## Run UI

From repo root (choose any high port; avoid `80`):

```bash
python3 -m http.server 8001 --bind 127.0.0.1 --directory web
```

Open one of these in a browser:

- `http://127.0.0.1:8001/` (explicit IPv4 loopback)
- `http://localhost:8001/` (may resolve to IPv4 or IPv6 depending on your system)
- `http://[::1]:8001/` (explicit IPv6 loopback; requires `--bind ::1`)

Notes:
- Binding to `127.0.0.1` keeps the dev server local-only (recommended). Use `--bind ::1` if you need IPv6 loopback.
 - If you see `OSError: [Errno 98] Address already in use`, pick a different port (e.g. `8002`).

## Browser compatibility notes

- Folder picking is browser-dependent:
  - Chromium (Chrome/Edge): `showDirectoryPicker` (best experience; preserves paths).
  - WebKit (Safari): folder input via `webkitdirectory` (preserves paths).
  - Firefox/Floorp: folder picking is not supported; use `Select files` or drag-and-drop.
- In Firefox/Floorp, the app may fall back to running WASM on the main thread if module workers fail; ingestion will still work but the UI can stutter during heavy ingest/search.
- If the tab crashes during ingest, reduce the total input size (defaults: 10 MB per file, 50 MB total) or ingest a smaller subset of the repo.

## Notes

- The UI loads `web/pkg/ingestor_wasm.js` produced by `wasm-pack`.
- Ingestion runs entirely in the browser; no network calls are required.
- Heavy work (ingest/search) runs in a Web Worker to reduce UI freezes.
- If `mise` complains the config is untrusted: `mise trust ~/dev/llmx/.mise.toml`.
- Images: Phase 1 ingests common screenshot formats and includes them in `export.zip` under `images/`. No OCR is performed yet; search matches mainly via filename/path.

## Quick QA (manual)

Search behavior:

- Type in the search box and confirm results update automatically (debounced).
- Change `All files` → a specific file and confirm `Outline` and `Symbols` populate.
- Select an outline or symbol and confirm results narrow without needing to press Search.

Filtering:

- `All files` uses `Path prefix` (prefix match).
- Selecting a file filters by exact path.
