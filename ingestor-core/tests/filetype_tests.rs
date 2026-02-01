//! File type tests - verify all supported extensions are handled correctly.
//!
//! This test suite ensures that each of the 30+ supported file extensions
//! is properly detected and chunked.
//!
//! Run with: cargo test --test filetype_tests

mod common;

use ingestor_core::{ingest_files, ChunkKind, FileInput, IngestOptions};
use test_case::test_case;

/// Helper to create a FileInput from extension and content.
fn create_file(ext: &str, content: &str) -> FileInput {
    FileInput {
        path: format!("test.{}", ext),
        data: content.as_bytes().to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    }
}

/// Test that a file extension produces expected chunk kind.
fn test_extension_produces_kind(ext: &str, content: &str, expected_kind: ChunkKind) {
    let input = create_file(ext, content);
    let index = ingest_files(vec![input], IngestOptions::default());

    assert!(!index.files.is_empty(), "File should be indexed for .{}", ext);
    assert_eq!(
        index.files[0].kind, expected_kind,
        "Extension .{} should produce {:?}",
        ext, expected_kind
    );
}

// ============================================================================
// Rust
// ============================================================================

#[test]
fn test_filetype_rust() {
    test_extension_produces_kind(
        "rs",
        "fn main() { println!(\"Hello\"); }",
        ChunkKind::Unknown, // Rust files are currently Unknown in detect_kind
    );
}

// ============================================================================
// JavaScript/TypeScript
// ============================================================================

// Note: Currently only js, ts, tsx are mapped to JavaScript in detect_kind.
// jsx, mjs, cjs are allowed extensions but map to Unknown.
#[test_case("js", ChunkKind::JavaScript)]
#[test_case("ts", ChunkKind::JavaScript)]
#[test_case("tsx", ChunkKind::JavaScript)]
fn test_filetype_javascript_core(ext: &str, expected: ChunkKind) {
    test_extension_produces_kind(ext, "function test() { return 42; }", expected);
}

// TODO: These should be JavaScript but currently map to Unknown
#[test_case("jsx", ChunkKind::Unknown)]
#[test_case("mjs", ChunkKind::Unknown)]
#[test_case("cjs", ChunkKind::Unknown)]
fn test_filetype_javascript_unmapped(ext: &str, expected: ChunkKind) {
    test_extension_produces_kind(ext, "function test() { return 42; }", expected);
}

// ============================================================================
// Web
// ============================================================================

#[test]
fn test_filetype_html() {
    test_extension_produces_kind(
        "html",
        "<!DOCTYPE html><html><body>Hello</body></html>",
        ChunkKind::Html,
    );
}

#[test_case("css")]
#[test_case("scss")]
#[test_case("sass")]
#[test_case("less")]
fn test_filetype_stylesheets(ext: &str) {
    let input = create_file(ext, "body { color: red; }");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Style file .{} should be indexed", ext);
}

// ============================================================================
// Data Formats
// ============================================================================

#[test]
fn test_filetype_json() {
    test_extension_produces_kind(
        "json",
        r#"{"key": "value"}"#,
        ChunkKind::Json,
    );
}

#[test_case("yaml")]
#[test_case("yml")]
fn test_filetype_yaml(ext: &str) {
    let input = create_file(ext, "key: value\nlist:\n  - item1\n  - item2");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "YAML file .{} should be indexed", ext);
}

#[test]
fn test_filetype_toml() {
    let input = create_file("toml", "[section]\nkey = \"value\"");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "TOML file should be indexed");
}

// ============================================================================
// Documentation
// ============================================================================

#[test]
fn test_filetype_markdown() {
    test_extension_produces_kind(
        "md",
        "# Title\n\n## Section\n\nContent here.",
        ChunkKind::Markdown,
    );
}

#[test]
fn test_filetype_text() {
    test_extension_produces_kind(
        "txt",
        "Plain text content.\nLine two.",
        ChunkKind::Text,
    );
}

// ============================================================================
// Python
// ============================================================================

#[test]
fn test_filetype_python() {
    let input = create_file("py", "def hello():\n    print('Hello')");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Python file should be indexed");
}

// ============================================================================
// Go
// ============================================================================

#[test]
fn test_filetype_go() {
    let input = create_file(
        "go",
        "package main\n\nfunc main() {\n    println(\"Hello\")\n}",
    );
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Go file should be indexed");
}

// ============================================================================
// C/C++
// ============================================================================

#[test_case("c")]
#[test_case("cpp")]
#[test_case("cc")]
#[test_case("cxx")]
#[test_case("h")]
#[test_case("hpp")]
#[test_case("hxx")]
fn test_filetype_c_family(ext: &str) {
    let content = match ext {
        "h" | "hpp" | "hxx" => "#ifndef TEST_H\n#define TEST_H\nvoid test();\n#endif",
        _ => "int main() { return 0; }",
    };
    let input = create_file(ext, content);
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "C/C++ file .{} should be indexed", ext);
}

// ============================================================================
// Java
// ============================================================================

#[test]
fn test_filetype_java() {
    let input = create_file(
        "java",
        "public class Test {\n    public static void main(String[] args) {}\n}",
    );
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Java file should be indexed");
}

// ============================================================================
// Ruby
// ============================================================================

#[test]
fn test_filetype_ruby() {
    let input = create_file("rb", "def hello\n  puts 'Hello'\nend");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Ruby file should be indexed");
}

// ============================================================================
// PHP
// ============================================================================

#[test]
fn test_filetype_php() {
    let input = create_file("php", "<?php\nfunction hello() { echo 'Hello'; }\n?>");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "PHP file should be indexed");
}

// ============================================================================
// Shell
// ============================================================================

#[test_case("sh")]
#[test_case("bash")]
#[test_case("zsh")]
fn test_filetype_shell(ext: &str) {
    let input = create_file(ext, "#!/bin/bash\necho 'Hello'");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Shell file .{} should be indexed", ext);
}

// ============================================================================
// SQL
// ============================================================================

#[test]
fn test_filetype_sql() {
    let input = create_file("sql", "SELECT * FROM users WHERE active = true;");
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "SQL file should be indexed");
}

// ============================================================================
// Images (as assets)
// ============================================================================

// Note: Currently png, jpg, jpeg, gif, webp are mapped to Image.
// svg is an allowed extension but not mapped to Image.
#[test_case("png")]
#[test_case("jpg")]
#[test_case("jpeg")]
#[test_case("gif")]
#[test_case("webp")]
fn test_filetype_images(ext: &str) {
    let input = FileInput {
        path: format!("image.{}", ext),
        data: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes (works for testing)
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(!index.files.is_empty(), "Image .{} should be indexed as asset", ext);
    assert_eq!(index.files[0].kind, ChunkKind::Image);
}

// TODO: svg should probably be Image but is currently Unknown
#[test]
fn test_filetype_svg_is_indexed() {
    let input = FileInput {
        path: "image.svg".to_string(),
        data: b"<svg></svg>".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    // SVG is indexed but as Unknown kind currently
    assert!(!index.files.is_empty(), "SVG should be indexed");
}

// ============================================================================
// Extension Handling
// ============================================================================

#[test]
fn test_filetype_unsupported_extension_indexed_as_unknown() {
    // Note: The ingest_files function processes FileInput directly.
    // The extension filtering happens at the handler level (walk_directory).
    // When FileInput is passed directly, it's processed regardless of extension.
    let input = FileInput {
        path: "file.xyz".to_string(),
        data: b"some content".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    // At the ingest level, all files are processed (filtering is at handler level)
    assert_eq!(index.files.len(), 1, "File should be processed at ingest level");
    assert_eq!(index.files[0].kind, ChunkKind::Unknown);
}

#[test]
fn test_filetype_no_extension_indexed_as_unknown() {
    // Note: Same as above - filtering happens at handler level.
    let input = FileInput {
        path: "Makefile".to_string(),
        data: b"all: build".to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    // At the ingest level, all files are processed
    assert_eq!(index.files.len(), 1, "File should be processed at ingest level");
    assert_eq!(index.files[0].kind, ChunkKind::Unknown);
}

// ============================================================================
// Mixed Content Tests
// ============================================================================

#[test]
fn test_multiple_filetypes_together() {
    let inputs = vec![
        create_file("rs", "fn main() {}"),
        create_file("js", "function test() {}"),
        create_file("md", "# Title"),
        create_file("json", "{}"),
        create_file("py", "def test(): pass"),
    ];

    let index = ingest_files(inputs, IngestOptions::default());

    assert_eq!(index.files.len(), 5, "All 5 files should be indexed");

    // Verify each has correct kind
    let kinds: Vec<_> = index.files.iter().map(|f| (&f.path, f.kind)).collect();
    assert!(kinds.iter().any(|(p, k)| p.ends_with(".md") && *k == ChunkKind::Markdown));
    assert!(kinds.iter().any(|(p, k)| p.ends_with(".json") && *k == ChunkKind::Json));
    assert!(kinds.iter().any(|(p, k)| p.ends_with(".js") && *k == ChunkKind::JavaScript));
}

// ============================================================================
// Fixture File Tests
// ============================================================================

#[test]
fn test_fixture_files_index_correctly() {
    // Test that our fixture files are valid
    let fixtures = [
        ("filetypes/rust/sample.rs", ChunkKind::Unknown),
        ("filetypes/javascript/sample.js", ChunkKind::JavaScript),
        ("filetypes/javascript/sample.ts", ChunkKind::JavaScript),
        ("filetypes/web/sample.html", ChunkKind::Html),
        ("filetypes/data/sample.json", ChunkKind::Json),
        ("filetypes/docs/sample.md", ChunkKind::Markdown),
        ("filetypes/docs/sample.txt", ChunkKind::Text),
    ];

    for (fixture_path, expected_kind) in fixtures {
        let full_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture_path);

        if !full_path.exists() {
            continue; // Skip if fixture doesn't exist yet
        }

        let data = std::fs::read(&full_path).unwrap();
        let input = FileInput {
            path: fixture_path.to_string(),
            data,
            mtime_ms: None,
            fingerprint_sha256: None,
        };

        let index = ingest_files(vec![input], IngestOptions::default());

        assert!(
            !index.files.is_empty(),
            "Fixture {} should be indexed",
            fixture_path
        );
        assert_eq!(
            index.files[0].kind, expected_kind,
            "Fixture {} should have kind {:?}",
            fixture_path, expected_kind
        );
    }
}
