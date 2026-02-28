---
chunk_index: 231
ref: "a0830ae0adae"
id: "a0830ae0adae0e616561485478c93b13eef229bd642bbb531f9ba680f8b46f27"
slug: "ingestion-spec--export-zip"
path: "/home/zack/dev/llmx/docs/INGESTION_SPEC.md"
kind: "markdown"
lines: [165, 173]
token_estimate: 115
content_sha256: "bbd46a0ae44817ad349f9f0e59c1df4fc57d1aa7d68d076422becd9053c54a0e"
compacted: false
heading_path: ["LLMX Ingestion Spec","Export Formats","export.zip"]
symbol: null
address: null
asset_path: null
---

### export.zip

- Contains `llm.md`, `index.json`, `manifest.json`, and `chunks/`.
- `index.json` is written compact (not pretty-printed) to reduce export size.
- `manifest.json` uses `format_version: 2` and is size-optimized:
  - Common values are deduplicated into top-level tables (`paths`, `kinds`).
  - Chunk records are stored as rows (arrays) with a `chunk_columns` header.
  - Chunk files are derivable as `chunks/{ref}.md` and are not stored per-row.