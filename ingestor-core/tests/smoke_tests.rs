//! Smoke tests - quick validation of core functionality.
//!
//! These tests verify the basic happy paths work correctly.
//! They should complete quickly (<5 seconds total) and catch obvious regressions.
//!
//! Run with: cargo test --features cli --test smoke_tests

#![cfg(feature = "cli")]

mod common;

use ingestor_core::handlers::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_index_handler, llmx_manage_handler,
    llmx_search_handler, ExploreInput, IndexInput, IndexStore, ManageInput, SearchInput,
};
use ingestor_core::{export_llm, export_zip};
use tempfile::TempDir;
use std::fs;

/// Create a minimal test project for smoke tests.
fn create_minimal_project() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");

    fs::write(
        temp.path().join("main.rs"),
        "fn main() { println!(\"Hello\"); }",
    )
    .unwrap();

    fs::write(
        temp.path().join("lib.rs"),
        "pub fn greet(name: &str) -> String { format!(\"Hello, {}\", name) }",
    )
    .unwrap();

    fs::write(
        temp.path().join("README.md"),
        "# Test\n\nA test project.\n\n## Usage\n\nRun `cargo run`.",
    )
    .unwrap();

    temp
}

#[test]
fn smoke_index_search_export() {
    // 1. Create temp storage and project
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();
    let project = create_minimal_project();

    // 2. Index the project
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).expect("Index should succeed");

    assert!(idx_output.created);
    assert!(idx_output.stats.total_files >= 3);
    assert!(idx_output.stats.total_chunks >= 3);

    // 3. Search the index
    let search_input = SearchInput {
        index_id: idx_output.index_id.clone(),
        query: "greet".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(16000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).expect("Search should succeed");

    assert!(search_output.total_matches > 0);
    assert!(!search_output.results.is_empty());

    // 4. Export to llm.md format
    let index = store.load(&idx_output.index_id).unwrap();
    let llm_md = export_llm(index);

    assert!(!llm_md.is_empty());
    assert!(llm_md.contains("# "));
}

#[test]
fn smoke_full_workflow() {
    // Setup
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();
    let project = create_minimal_project();

    // 1. Index
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).expect("1. Index should succeed");
    let index_id = idx_output.index_id.clone();

    // 2. List indexes
    let list_input = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };
    let list_output = llmx_manage_handler(&mut store, list_input).expect("2. List should succeed");
    assert!(list_output.success);
    assert_eq!(list_output.indexes.as_ref().unwrap().len(), 1);

    // 3. Search
    let search_input = SearchInput {
        index_id: index_id.clone(),
        query: "fn".to_string(),
        filters: None,
        limit: Some(5),
        max_tokens: Some(8000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).expect("3. Search should succeed");
    assert!(search_output.total_matches > 0);

    // 4. Explore files
    let explore_files = ExploreInput {
        index_id: index_id.clone(),
        mode: "files".to_string(),
        path_filter: None,
    };
    let files_output = llmx_explore_handler(&mut store, explore_files).expect("4. Explore files should succeed");
    assert!(files_output.total >= 3);

    // 5. Explore symbols
    let explore_symbols = ExploreInput {
        index_id: index_id.clone(),
        mode: "symbols".to_string(),
        path_filter: None,
    };
    let _symbols_output = llmx_explore_handler(&mut store, explore_symbols).expect("5. Explore symbols should succeed");

    // 6. Get chunk
    if let Some(first_result) = search_output.results.first() {
        let chunk = llmx_get_chunk_handler(&mut store, &index_id, &first_result.chunk_id)
            .expect("6. Get chunk should succeed");
        assert!(chunk.is_some());
    }

    // 7. Export zip
    let index = store.load(&index_id).unwrap();
    let zip_data = export_zip(index);
    assert!(!zip_data.is_empty());

    // 8. Delete
    let delete_input = ManageInput {
        action: "delete".to_string(),
        index_id: Some(index_id.clone()),
    };
    let delete_output = llmx_manage_handler(&mut store, delete_input).expect("8. Delete should succeed");
    assert!(delete_output.success);

    // 9. Verify empty
    let list_after = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };
    let list_after_output = llmx_manage_handler(&mut store, list_after).expect("9. List should succeed");
    assert!(list_after_output.indexes.unwrap().is_empty());
}

#[test]
fn smoke_explore_all_modes() {
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();
    let project = create_minimal_project();

    // Index
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Test all explore modes
    for mode in &["files", "outline", "symbols"] {
        let input = ExploreInput {
            index_id: idx_output.index_id.clone(),
            mode: mode.to_string(),
            path_filter: None,
        };
        let output = llmx_explore_handler(&mut store, input)
            .unwrap_or_else(|e| panic!("Explore {} mode failed: {}", mode, e));

        // All modes should return valid output
        assert!(output.total >= 0, "Mode {} should have valid total", mode);
    }
}

#[test]
fn smoke_search_with_filters() {
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();
    let project = create_minimal_project();

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search with various filter combinations
    let filter_tests = vec![
        (Some("main"), None, "path filter"),
        (None, Some("markdown"), "kind filter"),
    ];

    for (path_prefix, kind, desc) in filter_tests {
        let input = SearchInput {
            index_id: idx_output.index_id.clone(),
            query: "test".to_string(),
            filters: Some(ingestor_core::handlers::SearchFiltersInput {
                path_prefix: path_prefix.map(String::from),
                kind: kind.map(String::from),
                symbol_prefix: None,
                heading_prefix: None,
            }),
            limit: Some(10),
            max_tokens: Some(8000),
            use_semantic: None,
        };

        let _output = llmx_search_handler(&mut store, input)
            .unwrap_or_else(|e| panic!("Search with {} failed: {}", desc, e));
    }
}

#[test]
fn smoke_index_multiple_file_types() {
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();
    let project = TempDir::new().unwrap();

    // Create files of different types
    let files = [
        ("code.rs", "fn main() {}"),
        ("script.js", "function hello() {}"),
        ("style.css", "body { color: red; }"),
        ("data.json", r#"{"key": "value"}"#),
        ("config.toml", "[section]\nkey = \"value\""),
        ("readme.md", "# Title\n\nContent"),
        ("notes.txt", "Plain text content"),
    ];

    for (name, content) in files {
        fs::write(project.path().join(name), content).unwrap();
    }

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).expect("Index should succeed");

    assert_eq!(idx_output.stats.total_files, 7, "Should index all 7 files");
    assert!(idx_output.warnings.is_empty(), "Should have no warnings");
}

#[test]
fn smoke_token_budget_respected() {
    let storage_temp = TempDir::new().unwrap();
    let mut store = IndexStore::new(storage_temp.path().to_path_buf()).unwrap();

    // Create a project with multiple large files
    let project = TempDir::new().unwrap();
    for i in 0..10 {
        let content = format!(
            "// File {}\n{}",
            i,
            "fn function() { /* lots of code */ }\n".repeat(100)
        );
        fs::write(project.path().join(format!("file{}.rs", i)), content).unwrap();
    }

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search with small token budget
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "function".to_string(),
        filters: None,
        limit: Some(100),
        max_tokens: Some(500), // Very small
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();

    // Should have results and possibly truncated IDs
    assert!(search_output.total_matches > 0);

    // Calculate total tokens in results
    let total_content_len: usize = search_output
        .results
        .iter()
        .map(|r| r.content.len())
        .sum();

    // Should be roughly within budget (with some overhead for formatting)
    // 500 tokens * ~4 chars/token = ~2000 chars
    assert!(
        total_content_len < 3000,
        "Content should be within token budget"
    );
}
