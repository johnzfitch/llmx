---
chunk_index: 470
ref: "ddf97e0f67ca"
id: "ddf97e0f67ca439cbf65a676db01cd87bb55b13cac6a4a7e14002583ad45af8c"
slug: "phase6-success--model-structure"
path: "/home/zack/dev/llmx/docs/PHASE6_SUCCESS.md"
kind: "markdown"
lines: [179, 195]
token_estimate: 133
content_sha256: "b2abe3bcce1d207b21777e269d25bc9bd10de5ebbd15d3748670f4003eb41751"
compacted: false
heading_path: ["Phase 6 Blockers - FULLY RESOLVED ✅","Verification","Model Structure"]
symbol: null
address: null
asset_path: null
---

### Model Structure
```bash
$ python -c "import onnx; m = onnx.load('models/bge-small-en-v1.5-opset13.onnx'); \
  print('Inputs:', [(i.name, i.type.tensor_type.shape.dim) for i in m.graph.input]); \
  print('Outputs:', [(o.name, o.type.tensor_type.shape.dim) for o in m.graph.output])"

Inputs: [
  ('input_ids', [batch_size, sequence_length]),
  ('attention_mask', [batch_size, sequence_length]),
  ('token_type_ids', [batch_size, sequence_length])
]
Outputs: [
  ('last_hidden_state', [batch_size, sequence_length, 384])
]
✓
```