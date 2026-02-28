---
chunk_index: 395
ref: "c8923cddaef1"
id: "c8923cddaef14aff757349bcc7c7e76ea708559572dceec087e688d19f4a8a13"
slug: "phase6-implementation--creating-index-with-embeddings"
path: "/home/zack/dev/llmx/docs/PHASE6_IMPLEMENTATION.md"
kind: "markdown"
lines: [341, 355]
token_estimate: 96
content_sha256: "373f7ffb681013631b93784c2c6d321ebb0c0c2437f5b1c5b4634e6c092c09e1"
compacted: false
heading_path: ["Phase 6 Implementation: Burn + WebGPU Embeddings","Usage Examples","Creating Index with Embeddings"]
symbol: null
address: null
asset_path: null
---

### Creating Index with Embeddings

```javascript
// Ingest files
const ingestor = Ingestor.ingest(files, options);

// Generate embeddings (Phase 6 - to be implemented)
const embedder = await new Embedder();
const chunks = /* get chunks from ingestor */;
const embeddings = embedder.embedBatch(chunks.map(c => c.content));

// Store embeddings in index
// (API to be finalized)
```