# Phase 1-5 Verification Checklist

Date: 2026-01-17

## Phase 1: Ingestion, chunking, exports, offline UI

- Deterministic chunk IDs and stable refs. Status: PASS. Evidence: ingestor-core/src/chunk.rs, ingestor-core/src/util.rs.
- File-type chunking rules (markdown/json/js/html/text/image). Status: PASS. Evidence: ingestor-core/src/chunk.rs.
- Image handling without UTF-8 decode and asset_path set. Status: PASS. Evidence: ingestor-core/src/lib.rs, ingestor-core/src/chunk.rs.
- Export formats (llm.md, manifest.json v2, chunks). Status: PASS. Evidence: ingestor-core/src/export.rs.
- Offline UI ingestion, worker fallback, IndexedDB persistence. Status: PASS. Evidence: web/app.js, web/worker.js, web/index.html.

## Phase 2: Indexing, BM25 search, filters

- Inverted index build and BM25 search. Status: PASS. Evidence: ingestor-core/src/index.rs.
- Search filters (path prefix, kind, heading, symbol). Status: PASS. Evidence: ingestor-core/src/index.rs, ingestor-core/src/mcp/tools.rs.
- Token budgeted inline content in MCP search. Status: PASS. Evidence: ingestor-core/src/mcp/tools.rs.

## Phase 3: Export ergonomics and manifest structure

- llm.md semantic outline entries. Status: PASS. Evidence: ingestor-core/src/export.rs.
- Chunk YAML front matter fields. Status: PASS. Evidence: ingestor-core/src/export.rs.
- Manifest v2 with paths/kinds tables. Status: PASS. Evidence: ingestor-core/src/export.rs.

## Phase 4: MCP server and storage

- Four MCP tools (index/search/explore/manage). Status: PASS. Evidence: ingestor-core/src/bin/mcp_server.rs, ingestor-core/src/mcp/tools.rs.
- Disk-backed IndexStore with cache and rebuild on load. Status: PASS. Evidence: ingestor-core/src/mcp/storage.rs.
- Manual MCP verification completed. Status: NEEDS MANUAL TEST. Evidence: docs/PHASE_4_MCP_VERIFICATION.md.

## Phase 5: Semantic search (hash embeddings) and hybrid ranking

- Embeddings stored in index schema. Status: PASS. Evidence: ingestor-core/src/model.rs.
- Hash-based embeddings and cosine similarity. Status: PASS. Evidence: ingestor-core/src/embeddings.rs.
- Vector search and hybrid search. Status: PASS. Evidence: ingestor-core/src/index.rs.
- use_semantic flag wired in MCP search. Status: PASS. Evidence: ingestor-core/src/mcp/tools.rs.
- Hybrid strategy selection (rrf vs linear). Status: PASS. Evidence: ingestor-core/src/model.rs, ingestor-core/src/index.rs, ingestor-core/src/mcp/tools.rs.

## Known limitations and follow-ups

- HTML parsing is regex-based and can miss edge cases. Status: ACCEPTED LIMITATION. Evidence: docs/REVIEW_REPORT.md.
- JSON provenance line ranges are best-effort and often coarse. Status: ACCEPTED LIMITATION. Evidence: docs/REVIEW_REPORT.md.
- Zip export ignores write errors. Status: OPEN. Evidence: ingestor-core/src/export.rs, ingestor-wasm/src/lib.rs, docs/REVIEW_REPORT.md.

## Re-verify (quick pointers)

- WASM build + UI smoke steps: docs/USAGE.md
- MCP server verification steps: docs/PHASE_4_MCP_VERIFICATION.md
