---
chunk_index: 319
ref: "3dba1ef57eb4"
id: "3dba1ef57eb4b233de1f55ee9c953845b07b2030a90b70179addfc82d97f8240"
slug: "p6-directions--export-variants-with-embeddings"
path: "/home/zack/dev/llmx/docs/P6_DIRECTIONS.md"
kind: "markdown"
lines: [499, 523]
token_estimate: 162
content_sha256: "7baaaed419c6c27cf0b453c9ed0227700c0e230f65eb669adc805024cdbb0876"
compacted: false
heading_path: ["Phase 6: Burn + WebGPU Embeddings & Advanced Hybrid Search","10. llm.cat Integration","Export Variants with Embeddings"]
symbol: null
address: null
asset_path: null
---

### Export Variants with Embeddings

Add to existing export options:

```typescript
interface ExportOptions {
  mode: 'standard' | 'quality' | 'minimal';
  includeEmbeddings: boolean;  // NEW: bundle embeddings for offline search
}

function exportWithEmbeddings(chunks: Chunk[], options: ExportOptions) {
  const output = {
    chunks: options.mode === 'standard' ? chunks : filterByQuality(chunks),
    embeddings: options.includeEmbeddings ? chunks.map(c => c.embedding) : null,
    embeddingModel: 'bge-small-en-v1.5',
    version: 2
  };
  return JSON.stringify(output);
}
```

Consumers can then do semantic search without re-embedding.

---