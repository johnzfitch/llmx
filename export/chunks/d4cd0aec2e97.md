---
chunk_index: 337
ref: "d4cd0aec2e97"
id: "d4cd0aec2e97a4fd5f9b3ed9d5d5a4462a1d6f37b7f6564f84b9fa45066fd7bf"
slug: "phase6-blocker-fixes--verification-checklist"
path: "/home/zack/dev/llmx/docs/PHASE6_BLOCKER_FIXES.md"
kind: "markdown"
lines: [269, 293]
token_estimate: 154
content_sha256: "06faff8589346fd914edebb660326833241e6dae0d354a8d3886f2255f638ebb"
compacted: false
heading_path: ["Phase 6 Blocker Resolution Guide","Verification Checklist"]
symbol: null
address: null
asset_path: null
---

## Verification Checklist

After applying fixes:

```bash
# 1. Verify ONNX opset
python -c "import onnx; m = onnx.load('models/bge-small-en-v1.5-opset13.onnx'); print(m.opset_import[0].version)"
# Expected: 13

# 2. Test burn-import in build
cd ingestor-wasm
cargo build 2>&1 | head -50
# Expected: No opset errors, model code generated in src/model/

# 3. Test WASM compilation
wasm-pack build --target web --dev 2>&1 | tail -20
# Expected: Successful build

# 4. Test tokenizer in WASM
# Create a simple test that loads tokenizer and encodes text
cargo test --target wasm32-unknown-unknown tokenizer_works
```

---