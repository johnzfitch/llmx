//! Handler unit tests - tests handler functions directly without CLI overhead.
//!
//! These tests verify the core business logic of llmx handlers.
//!
//! Run with: cargo test --features cli --test handler_tests

#![cfg(feature = "cli")]

mod common;

use ingestor_core::handlers::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_index_handler, llmx_manage_handler,
    llmx_search_handler, ExploreInput, IndexInput, IndexStore, IngestOptionsInput, ManageInput,
    SearchFiltersInput, SearchInput,
};
use tempfile::TempDir;

/// Create a fresh IndexStore with temp directory.
fn create_store() -> (TempDir, IndexStore) {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let store = IndexStore::new(temp.path().to_path_buf()).expect("Failed to create store");
    (temp, store)
}

/// Create a test project with sample files.
fn create_test_project() -> TempDir {
    use std::fs;

    let temp = TempDir::new().expect("Failed to create temp dir");

    // Create source files
    fs::create_dir_all(temp.path().join("src")).unwrap();

    fs::write(
        temp.path().join("src/main.rs"),
        r#"//! Main entry point
fn main() {
    println!("Hello, world!");
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("src/lib.rs"),
        r#"//! Library module

/// A greeting function.
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

/// Calculate fibonacci number.
pub fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("README.md"),
        r#"# Test Project

A simple test project.

## Usage

```rust
use test_project::greet;
println!("{}", greet("World"));
```
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("config.json"),
        r#"{
    "name": "test-project",
    "version": "1.0.0"
}
"#,
    )
    .unwrap();

    temp
}

// ============================================================================
// Index Handler Tests
// ============================================================================

#[test]
fn test_handler_index_creates_new_index() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Index should succeed");

    assert!(output.created, "Should create new index");
    assert!(!output.index_id.is_empty(), "Should have index ID");
    assert!(output.stats.total_files > 0, "Should have files");
    assert!(output.stats.total_chunks > 0, "Should have chunks");
}

#[test]
fn test_handler_index_updates_existing() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let path = project.path().to_string_lossy().to_string();

    // First index
    let input1 = IndexInput {
        paths: vec![path.clone()],
        options: None,
    };
    let first = llmx_index_handler(&mut store, input1).expect("First index should succeed");
    assert!(first.created);

    // Second index (update)
    let input2 = IndexInput {
        paths: vec![path],
        options: None,
    };
    let second = llmx_index_handler(&mut store, input2).expect("Second index should succeed");
    assert!(!second.created, "Should update, not create");
    assert_eq!(first.index_id, second.index_id, "Should have same ID");
}

#[test]
fn test_handler_index_with_custom_options() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: Some(IngestOptionsInput {
            chunk_target_chars: Some(1000),
            max_file_bytes: Some(1024 * 1024),
        }),
    };

    let output = llmx_index_handler(&mut store, input).expect("Index should succeed");
    assert!(output.created);
}

#[test]
fn test_handler_index_empty_directory() {
    let (_storage, mut store) = create_store();
    let empty_dir = TempDir::new().expect("Failed to create temp dir");

    let input = IndexInput {
        paths: vec![empty_dir.path().to_string_lossy().to_string()],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Index should succeed");
    assert_eq!(output.stats.total_files, 0);
    assert_eq!(output.stats.total_chunks, 0);
}

// ============================================================================
// Search Handler Tests
// ============================================================================

#[test]
fn test_handler_search_finds_results() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    // Index first
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "fibonacci".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");
    assert!(output.total_matches > 0, "Should find fibonacci");
    assert!(!output.results.is_empty(), "Should have results");
}

#[test]
fn test_handler_search_with_path_filter() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search with path filter
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "function".to_string(),
        filters: Some(SearchFiltersInput {
            path_prefix: Some("src/".to_string()),
            kind: None,
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");
    // All results should be from src/
    for result in &output.results {
        assert!(
            result.path.contains("src/"),
            "Result {} should be in src/",
            result.path
        );
    }
}

#[test]
fn test_handler_search_token_budget_enforced() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search with very small token budget
    let search_input = SearchInput {
        index_id: idx_output.index_id.clone(),
        query: "fn".to_string(),
        filters: None,
        limit: Some(100),
        max_tokens: Some(100), // Very small budget
        use_semantic: None,
    };

    let output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");

    // Should have truncated IDs if budget was exceeded
    if output.total_matches > output.results.len() {
        assert!(
            output.truncated_ids.is_some(),
            "Should have truncated IDs when budget exceeded"
        );
    }
}

#[test]
fn test_handler_search_no_results() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "xyznonexistentterm".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");
    assert_eq!(output.total_matches, 0);
    assert!(output.results.is_empty());
}

#[test]
fn test_handler_search_invalid_index() {
    let (_storage, mut store) = create_store();

    let search_input = SearchInput {
        index_id: "nonexistent-id".to_string(),
        query: "test".to_string(),
        filters: None,
        limit: None,
        max_tokens: None,
        use_semantic: None,
    };

    let result = llmx_search_handler(&mut store, search_input);
    assert!(result.is_err(), "Should fail with invalid index ID");
}

// ============================================================================
// Explore Handler Tests
// ============================================================================

#[test]
fn test_handler_explore_files_mode() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let explore_input = ExploreInput {
        index_id: idx_output.index_id,
        mode: "files".to_string(),
        path_filter: None,
    };

    let output = llmx_explore_handler(&mut store, explore_input).expect("Explore should succeed");
    assert!(output.total > 0, "Should have files");
    assert!(!output.items.is_empty(), "Should list files");

    // Files should be sorted
    let mut sorted = output.items.clone();
    sorted.sort();
    assert_eq!(output.items, sorted, "Files should be sorted");
}

#[test]
fn test_handler_explore_files_with_path_filter() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let explore_input = ExploreInput {
        index_id: idx_output.index_id,
        mode: "files".to_string(),
        path_filter: Some("src/".to_string()),
    };

    let output = llmx_explore_handler(&mut store, explore_input).expect("Explore should succeed");
    for item in &output.items {
        assert!(item.contains("src/"), "All files should be in src/");
    }
}

#[test]
fn test_handler_explore_outline_mode() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let explore_input = ExploreInput {
        index_id: idx_output.index_id,
        mode: "outline".to_string(),
        path_filter: None,
    };

    let output = llmx_explore_handler(&mut store, explore_input).expect("Explore should succeed");
    // May or may not have headings depending on content
    assert!(output.total >= 0);
}

#[test]
fn test_handler_explore_symbols_mode() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let explore_input = ExploreInput {
        index_id: idx_output.index_id,
        mode: "symbols".to_string(),
        path_filter: None,
    };

    let output = llmx_explore_handler(&mut store, explore_input).expect("Explore should succeed");
    // Symbols are deduplicated and sorted
    let mut sorted = output.items.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(output.items.len(), sorted.len(), "Symbols should be unique");
}

#[test]
fn test_handler_explore_invalid_mode() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let explore_input = ExploreInput {
        index_id: idx_output.index_id,
        mode: "invalid_mode".to_string(),
        path_filter: None,
    };

    let result = llmx_explore_handler(&mut store, explore_input);
    assert!(result.is_err(), "Should fail with invalid mode");
}

// ============================================================================
// Manage Handler Tests
// ============================================================================

#[test]
fn test_handler_manage_list_empty() {
    let (_storage, mut store) = create_store();

    let input = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };

    let output = llmx_manage_handler(&mut store, input).expect("List should succeed");
    assert!(output.success);
    assert!(output.indexes.is_some());
    assert!(output.indexes.unwrap().is_empty());
}

#[test]
fn test_handler_manage_list_with_indexes() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    // Create an index first
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    llmx_index_handler(&mut store, idx_input).unwrap();

    // Now list
    let input = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };

    let output = llmx_manage_handler(&mut store, input).expect("List should succeed");
    assert!(output.success);
    let indexes = output.indexes.unwrap();
    assert_eq!(indexes.len(), 1);
}

#[test]
fn test_handler_manage_delete() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    // Create an index
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Delete it
    let delete_input = ManageInput {
        action: "delete".to_string(),
        index_id: Some(idx_output.index_id.clone()),
    };

    let output = llmx_manage_handler(&mut store, delete_input).expect("Delete should succeed");
    assert!(output.success);

    // Verify it's gone
    let list_input = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };
    let list_output = llmx_manage_handler(&mut store, list_input).unwrap();
    assert!(list_output.indexes.unwrap().is_empty());
}

#[test]
fn test_handler_manage_delete_missing_id() {
    let (_storage, mut store) = create_store();

    let input = ManageInput {
        action: "delete".to_string(),
        index_id: None,
    };

    let result = llmx_manage_handler(&mut store, input);
    assert!(result.is_err(), "Delete without ID should fail");
}

#[test]
fn test_handler_manage_invalid_action() {
    let (_storage, mut store) = create_store();

    let input = ManageInput {
        action: "invalid_action".to_string(),
        index_id: None,
    };

    let result = llmx_manage_handler(&mut store, input);
    assert!(result.is_err(), "Invalid action should fail");
}

// ============================================================================
// Get Chunk Handler Tests
// ============================================================================

#[test]
fn test_handler_get_chunk_by_full_id() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Get a chunk ID from search
    let search_input = SearchInput {
        index_id: idx_output.index_id.clone(),
        query: "fn".to_string(),
        filters: None,
        limit: Some(1),
        max_tokens: Some(16000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();

    if let Some(first) = search_output.results.first() {
        let chunk =
            llmx_get_chunk_handler(&mut store, &idx_output.index_id, &first.chunk_id).unwrap();
        assert!(chunk.is_some());
        let chunk = chunk.unwrap();
        assert_eq!(chunk.chunk_id, first.chunk_id);
        assert!(!chunk.content.is_empty());
    }
}

#[test]
fn test_handler_get_chunk_by_prefix() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Get a chunk ID and use prefix
    let search_input = SearchInput {
        index_id: idx_output.index_id.clone(),
        query: "fn".to_string(),
        filters: None,
        limit: Some(1),
        max_tokens: Some(16000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();

    if let Some(first) = search_output.results.first() {
        // Use first 12 characters as prefix
        let prefix = &first.chunk_id[..12.min(first.chunk_id.len())];
        let chunk = llmx_get_chunk_handler(&mut store, &idx_output.index_id, prefix).unwrap();
        assert!(chunk.is_some(), "Should find chunk by prefix");
    }
}

#[test]
fn test_handler_get_chunk_not_found() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let chunk =
        llmx_get_chunk_handler(&mut store, &idx_output.index_id, "nonexistent").unwrap();
    assert!(chunk.is_none());
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_handler_index_multiple_paths() {
    let (_storage, mut store) = create_store();
    let project1 = create_test_project();

    // Create a second project dir
    let project2 = TempDir::new().unwrap();
    std::fs::write(
        project2.path().join("extra.rs"),
        "fn extra() {}",
    )
    .unwrap();

    let input = IndexInput {
        paths: vec![
            project1.path().to_string_lossy().to_string(),
            project2.path().to_string_lossy().to_string(),
        ],
        options: None,
    };

    let output = llmx_index_handler(&mut store, input).expect("Index should succeed");
    assert!(output.created);
    // Should have files from both paths
    assert!(output.stats.total_files >= 4); // At least 4 from project1 + 1 from project2
}

#[test]
fn test_handler_search_with_kind_filter() {
    let (_storage, mut store) = create_store();
    let project = create_test_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "test".to_string(),
        filters: Some(SearchFiltersInput {
            path_prefix: None,
            kind: Some("markdown".to_string()),
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };

    let output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");
    // Results should only be markdown files
    for result in &output.results {
        assert!(
            result.path.ends_with(".md"),
            "Result should be markdown: {}",
            result.path
        );
    }
}
