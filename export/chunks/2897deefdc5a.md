---
chunk_index: 1205
ref: "2897deefdc5a"
id: "2897deefdc5a89770ff3bd8152c5e7b37ab2c9669aa6e84379658bff97807836"
slug: "smoke-tests-l130-249"
path: "/home/zack/dev/llmx/ingestor-core/tests/smoke_tests.rs"
kind: "text"
lines: [130, 249]
token_estimate: 1034
content_sha256: "ae53017bb0a68809593bb19abe41c0883965b750e42e445bf3819293b43d4574"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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