//! MCP protocol tests - verify MCP type definitions deserialize correctly.
//!
//! Run with: cargo test --features mcp --test mcp_tests

#![cfg(feature = "mcp")]

use llmx_mcp::mcp::tools::{
    ExploreInput, IndexInput, IndexStatsOutput, IngestOptionsInput, ManageInput,
    SearchFiltersInput, SearchInput,
};
use llmx_mcp::mcp::jobs::{JobState, JobStatus, new_job_id, new_job_store};

// ============================================================================
// Index Input Deserialization
// ============================================================================

#[test]
fn test_mcp_index_input_minimal() {
    let json = r#"{"paths": ["/path/to/project"]}"#;
    let input: IndexInput = serde_json::from_str(json).expect("Should deserialize");

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
    let input: IndexInput = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.paths, vec!["/path/to/project"]);
    let options = input.options.expect("Should have options");
    assert_eq!(options.chunk_target_chars, Some(2000));
    assert_eq!(options.max_file_bytes, Some(5000000));
}

#[test]
fn test_mcp_index_input_multiple_paths() {
    let json = r#"{"paths": ["/path/one", "/path/two", "/path/three"]}"#;
    let input: IndexInput = serde_json::from_str(json).expect("Should deserialize");

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
    let input: SearchInput = serde_json::from_str(json).expect("Should deserialize");

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
    let input: SearchInput = serde_json::from_str(json).expect("Should deserialize");

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
    let filters: SearchFiltersInput = serde_json::from_str(json).expect("Should deserialize");

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
    let input: ExploreInput = serde_json::from_str(json).expect("Should deserialize");

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
    let input: ExploreInput = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.mode, "symbols");
    assert_eq!(input.path_filter, Some("src/api".to_string()));
}

#[test]
fn test_mcp_explore_all_modes() {
    for mode in &["files", "outline", "symbols"] {
        let json = format!(r#"{{"index_id": "abc123", "mode": "{}"}}"#, mode);
        let input: ExploreInput = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(input.mode, *mode);
    }
}

// ============================================================================
// Manage Input Deserialization
// ============================================================================

#[test]
fn test_mcp_manage_input_list() {
    let json = r#"{"action": "list"}"#;
    let input: ManageInput = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.action, "list");
    assert!(input.index_id.is_none());
}

#[test]
fn test_mcp_manage_input_delete() {
    let json = r#"{"action": "delete", "index_id": "abc123"}"#;
    let input: ManageInput = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.action, "delete");
    assert_eq!(input.index_id, Some("abc123".to_string()));
}

// ============================================================================
// Struct Literal Construction
// ============================================================================

#[test]
fn test_mcp_index_input_struct_construction() {
    let input = IndexInput {
        paths: vec!["/path/to/project".to_string()],
        options: Some(IngestOptionsInput {
            chunk_target_chars: Some(3000),
            max_file_bytes: Some(1_000_000),
            max_total_bytes: None,
        }),
    };

    assert_eq!(input.paths, vec!["/path/to/project"]);
    let options = input.options.expect("Should have options");
    assert_eq!(options.chunk_target_chars, Some(3000));
    assert_eq!(options.max_file_bytes, Some(1_000_000));
}

#[test]
fn test_mcp_search_input_struct_construction() {
    let input = SearchInput {
        index_id: "test-id".to_string(),
        query: "test query".to_string(),
        filters: Some(SearchFiltersInput {
            path_prefix: Some("src/".to_string()),
            kind: Some("javascript".to_string()),
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(15),
        max_tokens: Some(10_000),
        use_semantic: Some(true),
        hybrid_strategy: None,
    };

    assert_eq!(input.index_id, "test-id");
    assert_eq!(input.query, "test query");
    assert_eq!(input.limit, Some(15));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_mcp_empty_paths_array() {
    let json = r#"{"paths": []}"#;
    let result: Result<IndexInput, _> = serde_json::from_str(json);
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
    let result: Result<SearchInput, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_mcp_manage_input_job_status() {
    let json = r#"{"action": "job_status", "index_id": "a1b2c3d4e5f60001"}"#;
    let input: ManageInput = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(input.action, "job_status");
    assert_eq!(input.index_id, Some("a1b2c3d4e5f60001".to_string()));
}

// ============================================================================
// Job Status Serialization
// ============================================================================

#[test]
fn test_job_status_queued_serialization() {
    let status = JobStatus::Queued;
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["status"], "queued");
}

#[test]
fn test_job_status_running_serialization() {
    let status = JobStatus::Running;
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["status"], "running");
}

#[test]
fn test_job_status_complete_serialization() {
    let status = JobStatus::Complete {
        index_id: "abc123".to_string(),
        stats: IndexStatsOutput { total_files: 10, total_chunks: 50, avg_chunk_tokens: 200 },
        warnings: 2,
    };
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["status"], "complete");
    assert_eq!(json["index_id"], "abc123");
    assert_eq!(json["stats"]["total_files"], 10);
    assert_eq!(json["stats"]["total_chunks"], 50);
    assert_eq!(json["warnings"], 2);
}

#[test]
fn test_job_status_error_serialization() {
    let status = JobStatus::Error { message: "path not found".to_string() };
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["message"], "path not found");
}

// ============================================================================
// Job Store Lifecycle
// ============================================================================

#[test]
fn test_job_store_insert_and_lookup() {
    let store = new_job_store();
    let job_id = "test-job-1".to_string();

    store.lock().unwrap().insert(job_id.clone(), JobState::queued());
    let jobs = store.lock().unwrap();
    let state = jobs.get(&job_id).expect("Job should exist");
    assert!(matches!(state.status, JobStatus::Queued));
}

#[test]
fn test_job_store_status_transition() {
    let store = new_job_store();
    let job_id = "test-job-2".to_string();

    store.lock().unwrap().insert(job_id.clone(), JobState::queued());
    store.lock().unwrap().get_mut(&job_id).unwrap().status = JobStatus::Running;
    store.lock().unwrap().get_mut(&job_id).unwrap().status = JobStatus::Complete {
        index_id: "idx-1".to_string(),
        stats: IndexStatsOutput { total_files: 5, total_chunks: 20, avg_chunk_tokens: 150 },
        warnings: 0,
    };

    let jobs = store.lock().unwrap();
    let state = jobs.get(&job_id).unwrap();
    assert!(matches!(state.status, JobStatus::Complete { .. }));
}

#[test]
fn test_job_store_cleanup_retains_recent() {
    let store = new_job_store();
    store.lock().unwrap().insert("recent".to_string(), JobState::queued());

    // Retain jobs younger than 600s -- our fresh job should survive
    store.lock().unwrap().retain(|_, state| state.started_at.elapsed().as_secs() < 600);
    assert!(store.lock().unwrap().contains_key("recent"));
}

#[test]
fn test_new_job_id_uniqueness() {
    let ids: Vec<String> = (0..100).map(|_| new_job_id()).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    // With nanosecond precision XOR pid, collisions should be near-impossible
    assert_eq!(unique.len(), ids.len(), "Job IDs should be unique");
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
    let input: SearchInput = serde_json::from_str(json).expect("Should deserialize");

    assert!(input.filters.is_none());
    assert!(input.limit.is_none());
    assert!(input.max_tokens.is_none());
}

// ============================================================================
// Security: path traversal rejection (issue #6)
// ============================================================================

#[test]
fn test_storage_rejects_traversal_id() {
    use llmx_mcp::mcp::storage::IndexStore;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let mut store = IndexStore::new(temp.path().to_path_buf()).unwrap();

    // Path traversal attempts must be rejected before any filesystem access
    assert!(store.load("../../etc/passwd").is_err(), "traversal via load");
    assert!(store.delete("../registry").is_err(), "traversal via delete");
    assert!(store.load("/absolute/path").is_err(), "absolute path via load");
    assert!(store.delete("has/slash").is_err(), "slash in id via delete");
}

#[test]
fn test_storage_rejects_empty_and_oversized_id() {
    use llmx_mcp::mcp::storage::IndexStore;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let mut store = IndexStore::new(temp.path().to_path_buf()).unwrap();

    assert!(store.load("").is_err(), "empty id should be rejected");
    let oversized = "a".repeat(129);
    assert!(store.load(&oversized).is_err(), "129-char id should be rejected");

    // Valid IDs (alphanumeric + hyphens + underscores) should not be rejected by
    // the validator itself (they may fail with "not found", but not "invalid id")
    let err = store.load("valid-id_123").unwrap_err();
    assert!(!err.to_string().contains("Invalid index_id"), "valid id rejected: {err}");
}

// ============================================================================
// Security: search limit cap (issue #4)
// ============================================================================

#[test]
fn test_search_limit_constant_is_200() {
    // MAX_SEARCH_LIMIT is the hard cap on result count; verify the value
    assert_eq!(llmx_mcp::handlers::MAX_SEARCH_LIMIT, 200);
}
