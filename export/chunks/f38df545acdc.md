---
chunk_index: 1202
ref: "f38df545acdc"
id: "f38df545acdc25e2a719a8b7a0b1eaeb71e1992acdb7bc47f95c152f24c6eee3"
slug: "mcp-tests-l126-247"
path: "/home/zack/dev/llmx/ingestor-core/tests/mcp_tests.rs"
kind: "text"
lines: [126, 247]
token_estimate: 1015
content_sha256: "23acb66e6d436f704264ca21178ac387ae9746005c405a201a632196becacd03"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

assert_eq!(input.index_id, "abc123");
    assert_eq!(input.mode, "files");
    assert!(input.path_filter.is_none());
}

#[test]
fn test_mcp_explore_input_with_path_filter() {
    let json = r#"{
        "index_id": "abc123",
        "mode": "symbols",
        "path_filter": "src/api"
    }"#;
    let input: ExploreInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.mode, "symbols");
    assert_eq!(input.path_filter, Some("src/api".to_string()));
}

#[test]
fn test_mcp_explore_all_modes() {
    for mode in &["files", "outline", "symbols"] {
        let json = format!(r#"{{"index_id": "abc123", "mode": "{}"}}"#, mode);
        let input: ExploreInputMcp = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(input.mode, *mode);
    }
}

// ============================================================================
// Manage Input Deserialization
// ============================================================================

#[test]
fn test_mcp_manage_input_list() {
    let json = r#"{"action": "list"}"#;
    let input: ManageInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.action, "list");
    assert!(input.index_id.is_none());
}

#[test]
fn test_mcp_manage_input_delete() {
    let json = r#"{"action": "delete", "index_id": "abc123"}"#;
    let input: ManageInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.action, "delete");
    assert_eq!(input.index_id, Some("abc123".to_string()));
}

// ============================================================================
// Type Conversion Tests
// ============================================================================

#[test]
fn test_mcp_to_handler_index_conversion() {
    let mcp_input = IndexInputMcp {
        paths: vec!["/path/to/project".to_string()],
        options: Some(IngestOptionsInputMcp {
            chunk_target_chars: Some(3000),
            max_file_bytes: Some(1000000),
        }),
    };

    let handler_input: ingestor_core::handlers::IndexInput = mcp_input.into();

    assert_eq!(handler_input.paths, vec!["/path/to/project"]);
    let options = handler_input.options.expect("Should have options");
    assert_eq!(options.chunk_target_chars, Some(3000));
    assert_eq!(options.max_file_bytes, Some(1000000));
}

#[test]
fn test_mcp_to_handler_search_conversion() {
    let mcp_input = SearchInputMcp {
        index_id: "test-id".to_string(),
        query: "test query".to_string(),
        filters: Some(SearchFiltersInputMcp {
            path_prefix: Some("src/".to_string()),
            kind: Some("javascript".to_string()),
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(15),
        max_tokens: Some(10000),
        use_semantic: Some(true),
    };

    let handler_input: ingestor_core::handlers::SearchInput = mcp_input.into();

    assert_eq!(handler_input.index_id, "test-id");
    assert_eq!(handler_input.query, "test query");
    assert_eq!(handler_input.limit, Some(15));
    assert_eq!(handler_input.max_tokens, Some(10000));
    assert_eq!(handler_input.use_semantic, Some(true));

    let filters = handler_input.filters.expect("Should have filters");
    assert_eq!(filters.path_prefix, Some("src/".to_string()));
    assert_eq!(filters.kind, Some("javascript".to_string()));
}

#[test]
fn test_mcp_to_handler_explore_conversion() {
    let mcp_input = ExploreInputMcp {
        index_id: "test-id".to_string(),
        mode: "files".to_string(),
        path_filter: Some("tests/".to_string()),
    };

    let handler_input: ingestor_core::handlers::ExploreInput = mcp_input.into();

    assert_eq!(handler_input.index_id, "test-id");
    assert_eq!(handler_input.mode, "files");
    assert_eq!(handler_input.path_filter, Some("tests/".to_string()));
}

#[test]
fn test_mcp_to_handler_manage_conversion() {
    let mcp_input = ManageInputMcp {
        action: "delete".to_string(),
        index_id: Some("test-id".to_string()),
    };