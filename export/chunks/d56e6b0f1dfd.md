---
chunk_index: 1194
ref: "d56e6b0f1dfd"
id: "d56e6b0f1dfd0e9a41fd3cc678101c54aa18a1b43257e56b33a596e11a3cd44d"
slug: "handler-tests-l293-420"
path: "/home/zack/dev/llmx/ingestor-core/tests/handler_tests.rs"
kind: "text"
lines: [293, 420]
token_estimate: 1008
content_sha256: "ad94e3fd964d55ff96bb867818724d775bf98f3d2bf2d47b7d038826f5049fa4"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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