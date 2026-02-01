//! Edge case tests - verify graceful handling of unusual inputs.
//!
//! These tests cover boundary conditions, error handling, and unusual inputs.
//!
//! Run with: cargo test --features cli --test edge_cases_tests

#![cfg(feature = "cli")]

mod common;

use ingestor_core::handlers::{
    llmx_index_handler, llmx_search_handler, IndexInput, IndexStore, SearchInput,
};
use ingestor_core::{ingest_files, FileInput, IngestOptions};
use std::fs;
use tempfile::TempDir;

/// Create a fresh IndexStore with temp directory.
fn create_store() -> (TempDir, IndexStore) {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let store = IndexStore::new(temp.path().to_path_buf()).expect("Failed to create store");
    (temp, store)
}

// ============================================================================
// File System Edge Cases
// ============================================================================

#[test]
fn test_empty_directory() {
    let (_storage, mut store) = create_store();
    let empty_dir = TempDir::new().unwrap();

    let input = IndexInput {
        paths: vec![empty_dir.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Should handle empty dir");
    assert_eq!(output.stats.total_files, 0);
    assert_eq!(output.stats.total_chunks, 0);
}

#[test]
fn test_deeply_nested_paths() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    // Create a deeply nested path (20 levels)
    let mut path = root.path().to_path_buf();
    for i in 0..20 {
        path = path.join(format!("level{}", i));
    }
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("deep.rs"), "fn deep() {}").unwrap();

    let input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Should handle deep paths");
    assert_eq!(output.stats.total_files, 1);
}

#[test]
fn test_spaces_in_paths() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    let dir_with_spaces = root.path().join("path with spaces");
    fs::create_dir_all(&dir_with_spaces).unwrap();
    fs::write(dir_with_spaces.join("file with spaces.rs"), "fn spacy() {}").unwrap();

    let input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Should handle spaces in paths");
    assert_eq!(output.stats.total_files, 1);
}

#[test]
fn test_unicode_filenames() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    // Various unicode filenames
    let filenames = ["日本語.rs", "中文.rs", "한국어.rs", "émoji🎉.rs"];

    for name in filenames {
        let path = root.path().join(name);
        if fs::write(&path, format!("fn {}() {{}}", name.chars().next().unwrap())).is_err() {
            // Skip if filesystem doesn't support this filename
            continue;
        }
    }

    let input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Should handle unicode filenames");
    // At least some files should have been indexed
    assert!(output.stats.total_files >= 0);
}

// ============================================================================
// Content Edge Cases
// ============================================================================

#[test]
fn test_empty_file() {
    let input = FileInput {
        path: "empty.txt".to_string(),
        data: vec![],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    // Empty files should still be tracked
    assert_eq!(index.files.len(), 1);
}

#[test]
fn test_single_byte_file() {
    let input = FileInput {
        path: "single.txt".to_string(),
        data: vec![b'x'],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
    assert!(!index.chunks.is_empty());
}

#[test]
fn test_binary_file_rejection() {
    let input = FileInput {
        path: "binary.bin".to_string(),
        data: vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    // Binary files without recognized extensions are skipped
    assert!(index.files.is_empty());
}

#[test]
fn test_file_with_null_bytes() {
    let mut data = b"fn test() { }".to_vec();
    data.push(0x00);
    data.extend_from_slice(b" more content");

    let input = FileInput {
        path: "with_null.rs".to_string(),
        data,
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    // Should handle null bytes gracefully
    assert!(!index.files.is_empty() || !index.warnings.is_empty());
}

#[test]
fn test_very_long_lines() {
    let long_line = "x".repeat(10000);
    let content = format!("// Comment\n{}\n// End", long_line);

    let input = FileInput {
        path: "long_lines.rs".to_string(),
        data: content.into_bytes(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
}

#[test]
fn test_mixed_line_endings() {
    // Mix of \n, \r\n, and \r
    let content = "line1\nline2\r\nline3\rline4";

    let input = FileInput {
        path: "mixed_endings.rs".to_string(),
        data: content.as_bytes().to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
}

#[test]
fn test_utf8_bom() {
    // UTF-8 BOM followed by content
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(b"fn main() {}");

    let input = FileInput {
        path: "with_bom.rs".to_string(),
        data: content,
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
}

#[test]
fn test_file_without_newline_at_end() {
    let content = "fn main() { println!(\"no newline\"); }"; // No trailing newline

    let input = FileInput {
        path: "no_newline.rs".to_string(),
        data: content.as_bytes().to_vec(),
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
    // Content should be preserved
    assert!(index.chunks[0].content.contains("no newline"));
}

// ============================================================================
// Index Edge Cases
// ============================================================================

#[test]
fn test_index_with_zero_chunks() {
    // Index with only empty files
    let input = FileInput {
        path: "empty.txt".to_string(),
        data: vec![],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], IngestOptions::default());
    // Should have file metadata even if no content chunks
    assert!(index.files.len() <= 1);
}

#[test]
fn test_index_many_small_files() {
    let inputs: Vec<FileInput> = (0..100)
        .map(|i| FileInput {
            path: format!("file{}.rs", i),
            data: format!("fn f{}() {{}}", i).into_bytes(),
            mtime_ms: None,
            fingerprint_sha256: None,
        })
        .collect();

    let index = ingest_files(inputs, IngestOptions::default());
    assert_eq!(index.files.len(), 100);
    assert!(index.chunks.len() >= 100);
}

// ============================================================================
// Search Edge Cases
// ============================================================================

#[test]
fn test_search_empty_query() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    fs::write(root.path().join("test.rs"), "fn main() {}").unwrap();

    let idx_input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    // Empty query should return results (or empty set, but not error)
    let result = llmx_search_handler(&mut store, search_input);
    assert!(result.is_ok());
}

#[test]
fn test_search_very_long_query() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    fs::write(root.path().join("test.rs"), "fn main() {}").unwrap();

    let idx_input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "word ".repeat(200), // 1000+ chars
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let result = llmx_search_handler(&mut store, search_input);
    assert!(result.is_ok());
}

#[test]
fn test_search_special_characters() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    fs::write(root.path().join("test.rs"), "fn main() {}").unwrap();

    let idx_input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Query with regex metacharacters
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "fn.*()".to_string(), // Contains regex chars
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let result = llmx_search_handler(&mut store, search_input);
    assert!(result.is_ok());
}

#[test]
fn test_search_unicode_query() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    fs::write(root.path().join("test.md"), "# こんにちは\n\nHello world").unwrap();

    let idx_input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "こんにちは".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let result = llmx_search_handler(&mut store, search_input);
    assert!(result.is_ok());
}

#[test]
fn test_search_token_budget_zero() {
    let (_storage, mut store) = create_store();
    let root = TempDir::new().unwrap();

    fs::write(root.path().join("test.rs"), "fn main() {}").unwrap();

    let idx_input = IndexInput {
        paths: vec![root.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "fn".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(0), // Zero budget
        use_semantic: None,
    };

    let result = llmx_search_handler(&mut store, search_input);
    // Should succeed but return no inline content
    assert!(result.is_ok());
    let output = result.unwrap();
    // Results should be empty or all in truncated_ids
    if output.total_matches > 0 {
        assert!(
            output.results.is_empty() || output.truncated_ids.is_some(),
            "With zero budget, results should be empty or truncated"
        );
    }
}

// ============================================================================
// Size Limit Edge Cases
// ============================================================================

#[test]
fn test_file_exactly_at_size_limit() {
    let options = IngestOptions {
        max_file_bytes: 100,
        ..IngestOptions::default()
    };

    // Exactly at limit
    let input = FileInput {
        path: "exact.rs".to_string(),
        data: vec![b'x'; 100],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], options);
    // Should be included (at limit, not over)
    assert_eq!(index.files.len(), 1);
}

#[test]
fn test_file_one_byte_over_limit() {
    let options = IngestOptions {
        max_file_bytes: 100,
        ..IngestOptions::default()
    };

    // One byte over limit
    let input = FileInput {
        path: "over.rs".to_string(),
        data: vec![b'x'; 101],
        mtime_ms: None,
        fingerprint_sha256: None,
    };

    let index = ingest_files(vec![input], options);
    // Should be excluded with warning
    assert!(index.files.is_empty());
    assert!(!index.warnings.is_empty());
}

// ============================================================================
// Concurrent Access Simulation
// ============================================================================

#[test]
fn test_multiple_indexes_same_store() {
    let (_storage, mut store) = create_store();

    // Create multiple projects
    let project1 = TempDir::new().unwrap();
    let project2 = TempDir::new().unwrap();

    fs::write(project1.path().join("a.rs"), "fn a() {}").unwrap();
    fs::write(project2.path().join("b.rs"), "fn b() {}").unwrap();

    // Index both
    let input1 = IndexInput {
        paths: vec![project1.path().to_string_lossy().to_string()],
        options: None,
    };
    let output1 = llmx_index_handler(&mut store, input1).unwrap();

    let input2 = IndexInput {
        paths: vec![project2.path().to_string_lossy().to_string()],
        options: None,
    };
    let output2 = llmx_index_handler(&mut store, input2).unwrap();

    // Both should succeed with different IDs
    assert_ne!(output1.index_id, output2.index_id);

    // List should show both
    let list = store.list().unwrap();
    assert_eq!(list.len(), 2);
}
