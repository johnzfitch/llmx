#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
WEB_DIR="$ROOT/web"
WASM_DIR="$ROOT/ingestor-wasm"
TOOLS_BIN="$ROOT/.cargo/bin"

MODEL_SRC="$WASM_DIR/models/arctic-embed-s.bin"
TOKENIZER_SRC="$WASM_DIR/models/tokenizer.json"

MODEL_DST="$WEB_DIR/models/arctic-embed-s.bin"
TOKENIZER_DST="$WEB_DIR/models/tokenizer.json"

MODEL_URL="./models/arctic-embed-s.bin"

mkdir -p "$WEB_DIR/models"

if [ ! -f "$MODEL_SRC" ]; then
  echo "Missing model weights: $MODEL_SRC" >&2
  echo "Build/download them first (see docs/LLMCAT_DEPLOY.md)." >&2
  exit 1
fi

if [ ! -f "$TOKENIZER_SRC" ]; then
  echo "Missing tokenizer: $TOKENIZER_SRC" >&2
  echo "Download it with:" >&2
  echo "  curl -L -o \"$TOKENIZER_SRC\" \"https://huggingface.co/Snowflake/snowflake-arctic-embed-s/resolve/main/tokenizer.json\"" >&2
  exit 1
fi

cp -f "$MODEL_SRC" "$MODEL_DST"
cp -f "$TOKENIZER_SRC" "$TOKENIZER_DST"

echo "Copied model to: $MODEL_DST"
echo "Copied tokenizer to: $TOKENIZER_DST"

cd "$WASM_DIR"
if [ -d "$TOOLS_BIN" ]; then
  PATH="$TOOLS_BIN:$PATH"
fi
export PATH

LLMX_EMBEDDING_MODEL_URL="$MODEL_URL" \
  wasm-pack build --target web --out-dir ../web/pkg --mode no-install --release \
  --features wgpu-backend,ndarray-backend

echo ""
echo "Ready to deploy: $WEB_DIR"
echo "Model URL embedded in WASM: ${MODEL_URL}"
