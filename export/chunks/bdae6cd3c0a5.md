---
chunk_index: 1196
ref: "bdae6cd3c0a5"
id: "bdae6cd3c0a56ec124405ec6b6de75b7532cf8af67da8895f1bd3fe1315b8bef"
slug: "handler-tests-l562-683"
path: "/home/zack/dev/llmx/ingestor-core/tests/handler_tests.rs"
kind: "text"
lines: [562, 683]
token_estimate: 976
content_sha256: "f7823cf10ed43e11f28816fccb14d96c47c229ab1e3ac450f980783b2f678252"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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