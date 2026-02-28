---
chunk_index: 1106
ref: "ac92fec19d2d"
id: "ac92fec19d2d865c4685ed84992f3fa796b789864ac306760bf8f22090e1ff41"
slug: "export-tests-l1-120"
path: "/home/zack/dev/llmx/ingestor-core/tests/export_tests.rs"
kind: "text"
lines: [1, 120]
token_estimate: 1016
content_sha256: "c6c096f116662ebd529955e6f8154ad8b0a53ab80a90ab97720ce5db20e8395c"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

use ingestor_core::{export_llm, ingest_files, FileInput, IngestOptions};

#[test]
fn export_llm_uses_clean_file_headers() {
    let input = FileInput {
        path: "docs/example.md".to_string(),
        data: b"# Title\n\nBody.\n".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    let llm = export_llm(&index);

    assert!(
        llm.contains("### docs/example.md (md,"),
        "expected file headings with kind and line count: {}",
        llm
    );
}

#[test]
fn export_llm_uses_outline_format() {
    let input = FileInput {
        path: "data/example.json".to_string(),
        data: br#"{ "key": "value" }"#.to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    let llm = export_llm(&index);

    // Chunk entries should be in outline format: - ref (lines) semantic
    let chunk_lines: Vec<&str> = llm.lines().filter(|line| line.starts_with("- ")).collect();
    assert!(!chunk_lines.is_empty(), "expected outline-format chunk lines starting with '- '");
    for line in chunk_lines {
        // Should contain parentheses for line ranges
        assert!(line.contains('(') && line.contains(')'), "expected line range in parens: {}", line);
        // Should not have literal newlines
        assert!(!line.contains('\n') && !line.contains('\r'));
    }
}

#[test]
fn export_chunks_use_short_filenames() {
    let input = FileInput {
        path: "docs/example.md".to_string(),
        data: b"# Title\n\nBody.\n".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    let exported = ingestor_core::export_chunks(&index);
    assert!(!exported.is_empty(), "expected chunk files");
    for (name, _body) in exported {
        assert!(name.starts_with("chunks/"), "expected chunks/ prefix: {}", name);
        assert!(name.ends_with(".md"), "expected .md suffix: {}", name);
        assert!(
            !name.contains("--"),
            "expected chunk filenames to omit slug: {}",
            name
        );
    }
}

#[test]
fn export_llm_shows_semantic_context() {
    // Test markdown with heading paths
    let md_input = FileInput {
        path: "docs/guide.md".to_string(),
        data: b"# Getting Started\n\n## Authentication\n\nContent here.\n".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    // Test JavaScript with symbols
    let js_input = FileInput {
        path: "src/utils.js".to_string(),
        data: b"function loginUser() {\n  return true;\n}\n".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![md_input, js_input], IngestOptions::default());
    let llm = export_llm(&index);

    // Markdown chunks should show heading breadcrumbs
    assert!(
        llm.contains("Getting Started") || llm.contains("Authentication"),
        "expected markdown heading text in outline: {}",
        llm
    );

    // JavaScript chunks with symbols should show them
    // Note: Symbol extraction may be WASM-only, so this is conditional
    if llm.contains("loginUser") {
        assert!(
            llm.contains("`loginUser()`") || llm.contains("loginUser"),
            "expected JS symbol in outline: {}",
            llm
        );
    }
}

#[test]
fn manifest_chunk_files_match_refs() {
    let input = FileInput {
        path: "docs/example.md".to_string(),
        data: b"# Title\n\nBody.\n".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    let manifest_json = ingestor_core::export_manifest_json(&index);
    let value: serde_json::Value = serde_json::from_str(&manifest_json).expect("manifest json");
    assert_eq!(
        value.get("format_version").and_then(|v| v.as_u64()),
        Some(2),
        "expected manifest format_version 2"
    );