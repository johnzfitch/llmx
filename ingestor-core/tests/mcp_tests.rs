//! MCP protocol tests - verify MCP type definitions and conversions.
//!
//! These tests ensure MCP input types deserialize correctly and convert
//! properly to handler types.
//!
//! Run with: cargo test --features mcp --test mcp_tests

#![cfg(feature = "mcp")]

use ingestor_core::mcp::tools::{
    ExploreInputMcp, IndexInputMcp, IngestOptionsInputMcp, ManageInputMcp,
    SearchFiltersInputMcp, SearchInputMcp,
};

// ============================================================================
// Index Input Deserialization
// ============================================================================

#[test]
fn test_mcp_index_input_minimal() {
    let json = r#"{"paths": ["/path/to/project"]}"#;
    let input: IndexInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.paths, vec!["/path/to/project"]);
    assert!(input.options.is_none());
}

#[test]
fn test_mcp_index_input_with_options() {
    let json = r#"{
        "paths": ["/path/to/project"],
        "options": {
            "chunk_target_chars": 2000,
            "max_file_bytes": 5000000
        }
    }"#;
    let input: IndexInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.paths, vec!["/path/to/project"]);
    let options = input.options.expect("Should have options");
    assert_eq!(options.chunk_target_chars, Some(2000));
    assert_eq!(options.max_file_bytes, Some(5000000));
}

#[test]
fn test_mcp_index_input_multiple_paths() {
    let json = r#"{"paths": ["/path/one", "/path/two", "/path/three"]}"#;
    let input: IndexInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.paths.len(), 3);
}

// ============================================================================
// Search Input Deserialization
// ============================================================================

#[test]
fn test_mcp_search_input_minimal() {
    let json = r#"{
        "index_id": "abc123",
        "query": "test query"
    }"#;
    let input: SearchInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.index_id, "abc123");
    assert_eq!(input.query, "test query");
    assert!(input.filters.is_none());
    assert!(input.limit.is_none());
    assert!(input.max_tokens.is_none());
}

#[test]
fn test_mcp_search_input_with_all_options() {
    let json = r#"{
        "index_id": "abc123",
        "query": "function",
        "filters": {
            "path_prefix": "src/",
            "kind": "javascript"
        },
        "limit": 20,
        "max_tokens": 8000,
        "use_semantic": true
    }"#;
    let input: SearchInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.index_id, "abc123");
    assert_eq!(input.query, "function");
    assert_eq!(input.limit, Some(20));
    assert_eq!(input.max_tokens, Some(8000));
    assert_eq!(input.use_semantic, Some(true));

    let filters = input.filters.expect("Should have filters");
    assert_eq!(filters.path_prefix, Some("src/".to_string()));
    assert_eq!(filters.kind, Some("javascript".to_string()));
}

#[test]
fn test_mcp_search_filters_all_fields() {
    let json = r###"{
        "path_prefix": "src/api",
        "kind": "markdown",
        "symbol_prefix": "User",
        "heading_prefix": "## API"
    }"###;
    let filters: SearchFiltersInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(filters.path_prefix, Some("src/api".to_string()));
    assert_eq!(filters.kind, Some("markdown".to_string()));
    assert_eq!(filters.symbol_prefix, Some("User".to_string()));
    assert_eq!(filters.heading_prefix, Some("## API".to_string()));
}

// ============================================================================
// Explore Input Deserialization
// ============================================================================

#[test]
fn test_mcp_explore_input_files_mode() {
    let json = r#"{
        "index_id": "abc123",
        "mode": "files"
    }"#;
    let input: ExploreInputMcp = serde_json::from_str(json).expect("Should deserialize");

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

    let handler_input: ingestor_core::handlers::ManageInput = mcp_input.into();

    assert_eq!(handler_input.action, "delete");
    assert_eq!(handler_input.index_id, Some("test-id".to_string()));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_mcp_empty_paths_array() {
    let json = r#"{"paths": []}"#;
    let result: Result<IndexInputMcp, _> = serde_json::from_str(json);
    // Empty paths should deserialize (handler will validate)
    assert!(result.is_ok());
}

#[test]
fn test_mcp_unknown_fields_ignored() {
    let json = r#"{
        "index_id": "abc123",
        "query": "test",
        "unknown_field": "ignored",
        "another_unknown": 42
    }"#;
    // Should deserialize successfully, ignoring unknown fields
    let result: Result<SearchInputMcp, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_mcp_null_optional_fields() {
    let json = r#"{
        "index_id": "abc123",
        "query": "test",
        "filters": null,
        "limit": null,
        "max_tokens": null
    }"#;
    let input: SearchInputMcp = serde_json::from_str(json).expect("Should deserialize");

    assert!(input.filters.is_none());
    assert!(input.limit.is_none());
    assert!(input.max_tokens.is_none());
}
