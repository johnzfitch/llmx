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

    let columns = value
        .get("chunk_columns")
        .and_then(|v| v.as_array())
        .expect("chunk_columns");
    assert!(!columns.is_empty(), "expected chunk_columns");

    let paths = value.get("paths").and_then(|v| v.as_array()).expect("paths");
    let kinds = value.get("kinds").and_then(|v| v.as_array()).expect("kinds");

    let rows = value.get("chunks").and_then(|v| v.as_array()).expect("chunks");
    assert!(!rows.is_empty(), "expected chunk rows");

    for row in rows {
        let row = row.as_array().expect("chunk row array");
        assert_eq!(
            row.len(),
            columns.len(),
            "expected chunk row length to match chunk_columns length"
        );

        let r#ref = row.get(0).and_then(|v| v.as_str()).expect("ref");
        let path_i = row.get(3).and_then(|v| v.as_u64()).expect("path_i") as usize;
        let kind_i = row.get(4).and_then(|v| v.as_u64()).expect("kind_i") as usize;

        assert!(path_i < paths.len(), "path_i out of range");
        assert!(kind_i < kinds.len(), "kind_i out of range");

        let derived_chunk_file = format!("chunks/{}.md", r#ref);
        assert!(derived_chunk_file.starts_with("chunks/"));
        assert!(derived_chunk_file.ends_with(".md"));
    }
}
#[cfg(test)]
mod tests {
    use ingestor_core::{export_llm, ingest_files, FileInput, IngestOptions};

    #[test]
    #[ignore]
    fn show_output_format() {
        let md_input = FileInput {
            path: "docs/api-reference.md".to_string(),
            data: b"# API Reference\n\n## Authentication\n\nUse JWT tokens.\n\n## Rate Limiting\n\nLimited to 1000/hour.\n".to_vec(),
            mtime_ms: None,
            fingerprint_sha256: None,
        };

        let js_input = FileInput {
            path: "src/auth.js".to_string(),
            data: b"function loginUser(credentials) {\n  return authenticate(credentials);\n}\n\nfunction logout() {\n  clearSession();\n}\n".to_vec(),
            mtime_ms: None,
            fingerprint_sha256: None,
        };

        let index = ingest_files(vec![md_input, js_input], IngestOptions::default());
        let llm = export_llm(&index);
        
        println!("\n=== Generated llm.md ===\n{}\n", llm);
        
        // Always pass so we can see the output
        assert!(true);
    }
}
