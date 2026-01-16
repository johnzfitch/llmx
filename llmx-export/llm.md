---
format_version: 1
package_id: "llmx-export-20260112T214421Z"
created_at: "2026-01-12T21:44:21Z"
chunk_dir: "chunks"
chunk_count: 1
hash_algorithm: "sha256"
provenance_anchor_format: "[[prov:source_id=<id>;locator=<locator>;hash=<sha256>]]"
---
# LLM Export Package

This package is a template export format for LLM ingestion. It includes a manifest and chunked Markdown files with explicit provenance anchors.

## File Layout
- llm.md: Package manifest and format specification.
- chunks/0001.md: Example chunk file (repeat with zero-padded numbering).

## Format Details
### Chunk Metadata (YAML front matter)
Required fields:
- id: Unique chunk identifier (string).
- title: Human-readable chunk title (string).
- source_id: Stable identifier for the original source (string).
- source_path: Original source path (string, relative or absolute).
- source_uri: Original source URI if applicable (string, empty when not available).
- source_sha256: SHA-256 of the full source file (hex string).
- extracted_at: UTC ISO-8601 timestamp for extraction.
- chunk_index: 1-based index of this chunk (integer).
- chunk_count: Total number of chunks for the source or package (integer).
- byte_start: UTF-8 byte offset start in the source file (integer, inclusive).
- byte_end: UTF-8 byte offset end in the source file (integer, exclusive).
- source_span_sha256: SHA-256 of the exact source span used for this chunk.
- text_sha256: SHA-256 of the chunk body as stored (excluding front matter).
- provenance_locator: Human-readable locator for the source span (string, e.g., line:10-20 or page:3).

### Provenance Anchors
Anchors are inserted inline in the chunk body to link text to source spans. Use this format:
[[prov:source_id=<id>;locator=<locator>;hash=<sha256>]]
- source_id matches the YAML field.
- locator matches provenance_locator.
- hash is source_span_sha256 for the referenced span.

### Chunk Body
The chunk body is Markdown content following the front matter. Anchors may be placed on their own line before the associated text.

## Chunk Index
- chunks/0001.md: Example chunk template for the export format.
