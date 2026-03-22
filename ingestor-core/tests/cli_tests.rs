//! CLI integration tests - end-to-end testing of llmx commands.
//!
//! These tests spawn the actual llmx binary and verify its output.

#![cfg(feature = "cli")]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get a Command for the llmx binary.
fn llmx() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("llmx").expect("Failed to find llmx binary")
}

#[cfg(feature = "embeddings")]
use std::path::Path;

#[cfg(feature = "embeddings")]
fn rewrite_embedding_model(storage_dir: &Path, index_id: &str, embedding_model: &str) {
    let index_path = storage_dir.join(format!("{index_id}.json"));
    let mut index_json: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).expect("Failed to read stored index"))
            .expect("Failed to parse stored index");
    index_json["embedding_model"] = serde_json::Value::String(embedding_model.to_string());
    fs::write(
        &index_path,
        serde_json::to_vec(&index_json).expect("Failed to serialize stored index"),
    )
    .expect("Failed to rewrite stored index");
}

/// Create a test project with sample files.
fn create_test_project() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");

    fs::create_dir_all(temp.path().join("src")).unwrap();

    fs::write(
        temp.path().join("src/main.rs"),
        r#"fn main() {
    println!("{}", greet("world"));
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("src/lib.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("README.md"),
        "# Test Project\n\nA simple test.\n",
    )
    .unwrap();

    temp
}

fn create_structural_project() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");

    fs::write(
        temp.path().join("auth.ts"),
        r#"
export function verifyToken(token: string): boolean {
  return token.length > 0;
}

export class AuthService {
  login(token: string): boolean {
    return verifyToken(token);
  }
}
"#,
    )
    .unwrap();

    temp
}

// ============================================================================
// Index Command Tests
// ============================================================================

#[test]
fn test_cli_index_directory() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created new index"))
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("chunks"));
}

#[test]
fn test_cli_index_json_output() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"index_id\""))
        .stdout(predicate::str::contains("\"created\": true"))  // JSON with spaces
        .stdout(predicate::str::contains("\"total_files\""));
}

#[test]
fn test_cli_index_nonexistent_path() {
    let storage = TempDir::new().unwrap();

    llmx()
        .args([
            "index",
            "/nonexistent/path/that/does/not/exist",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid path"));
}

#[test]
fn test_cli_index_empty_directory() {
    let storage = TempDir::new().unwrap();
    let empty_dir = TempDir::new().unwrap();

    llmx()
        .args([
            "index",
            empty_dir.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 files"));
}

#[test]
fn test_cli_index_with_options() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--chunk-size",
            "2000",
            "--max-file",
            "5000000",
        ])
        .assert()
        .success();
}

#[test]
fn test_cli_index_help_reports_default_file_cap() {
    llmx()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default: 256MB"))
        .stdout(predicate::str::contains("268435456"));
}

#[test]
fn test_cli_search_help_reports_v2_options() {
    llmx()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--strategy"))
        .stdout(predicate::str::contains("--hybrid-strategy"))
        .stdout(predicate::str::contains("--intent"))
        .stdout(predicate::str::contains("--explain"))
        .stdout(predicate::str::contains("8000"));
}

#[test]
fn test_cli_parse_errors_offer_recovery_examples() {
    llmx()
        .args(["search", "auth", "src/"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Use `--path <dir>`"))
        .stderr(predicate::str::contains("llmx search \"auth\" --path ./src"));
}

// ============================================================================
// Search Command Tests
// ============================================================================

#[test]
fn test_cli_search_basic() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index first
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Search
    llmx()
        .args([
            "search",
            "greet",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found"))
        .stdout(predicate::str::contains("results"));
}

#[test]
fn test_cli_search_json_output() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "search",
            "fn",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"results\""))
        .stdout(predicate::str::contains("\"total_matches\""));
}

#[cfg(feature = "embeddings")]
#[test]
fn test_cli_search_semantic_reports_reindex_required_on_model_mismatch() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    let output = llmx()
        .env("LLMX_FORCE_CPU", "1")
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let index_id = json["index_id"].as_str().unwrap();

    rewrite_embedding_model(storage.path(), index_id, env!("LLMX_MODEL_ID_F32"));

    llmx()
        .env("LLMX_FORCE_CPU", "1")
        .args([
            "search",
            "greet",
            "--index-id",
            index_id,
            "--semantic",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Re-index before using semantic search."));
}

#[test]
fn test_cli_search_with_limit() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "search",
            "fn",
            "--limit",
            "1",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success();
}

#[test]
fn test_cli_search_with_path_filter() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "search",
            "fn",
            "--path",
            "src/",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success();
}

#[test]
fn test_cli_search_no_index() {
    let storage = TempDir::new().unwrap();
    let empty_dir = TempDir::new().unwrap();

    llmx()
        .args([
            "search",
            "test",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(empty_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[dynamic]"))
        .stdout(predicate::str::contains("Found 0 results"));
}

#[test]
fn test_cli_search_defaults_to_current_directory_not_parent_project_root() {
    let storage = TempDir::new().unwrap();
    let parent = TempDir::new().unwrap();

    fs::write(parent.path().join("Cargo.toml"), "[package]\nname = \"parent\"\nversion = \"0.1.0\"\n").unwrap();
    fs::write(parent.path().join("parent_only.rs"), "fn parent_only() -> bool { true }\n").unwrap();
    fs::create_dir_all(parent.path().join("child")).unwrap();
    fs::write(parent.path().join("child/child_only.rs"), "fn child_only() -> bool { true }\n").unwrap();

    llmx()
        .args([
            "index",
            parent.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "index",
            parent.path().join("child").to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "search",
            "parent_only",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(parent.path().join("child"))
        .assert()
        .success()
        .stdout(predicate::str::contains("[persistent]"))
        .stdout(predicate::str::contains("parent_only.rs").not())
        .stdout(predicate::str::contains("child_only.rs"));
}

// ============================================================================
// Explore Command Tests
// ============================================================================

#[test]
fn test_cli_explore_files() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "explore",
            "files",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("total"));
}

#[test]
fn test_cli_explore_symbols() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "explore",
            "symbols",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("symbols"));
}

#[test]
fn test_cli_explore_outline() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "explore",
            "outline",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("outline"));
}

#[test]
fn test_cli_explore_invalid_mode() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "explore",
            "invalid_mode",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid mode"));
}

#[test]
fn test_cli_symbols_json_output() {
    let storage = TempDir::new().unwrap();
    let project = create_structural_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "symbols",
            "--pattern",
            "verify*",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"symbols\""))
        .stdout(predicate::str::contains("\"qualified_name\": \"verifyToken\""))
        .stdout(predicate::str::contains("\"ast_kind\": \"function\""));
}

#[test]
fn test_cli_lookup_json_output() {
    let storage = TempDir::new().unwrap();
    let project = create_structural_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "lookup",
            "verifyToken",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"matches\""))
        .stdout(predicate::str::contains("\"qualified_name\": \"verifyToken\""));
}

#[test]
fn test_cli_refs_json_output() {
    let storage = TempDir::new().unwrap();
    let project = create_structural_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "refs",
            "verifyToken",
            "--direction",
            "callers",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"refs\""))
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("\"target_symbol\": \"verifyToken\""));
}

// ============================================================================
// List Command Tests
// ============================================================================

#[test]
fn test_cli_list_empty() {
    let storage = TempDir::new().unwrap();

    llmx()
        .args(["list", "--storage-dir", storage.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No persistent indexes found"));
}

#[test]
fn test_cli_list_with_indexes() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args(["list", "--storage-dir", storage.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Persistent indexes:"))
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("chunks"));
}

#[test]
fn test_cli_list_json() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "list",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("["))
        .stdout(predicate::str::contains("\"id\""));
}

#[test]
fn test_cli_stats_json() {
    let storage = TempDir::new().unwrap();
    let project = create_structural_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args(["stats", "--storage-dir", storage.path().to_str().unwrap(), "--json"])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"stats\""))
        .stdout(predicate::str::contains("\"symbol_count\""))
        .stdout(predicate::str::contains("\"edge_count\""))
        .stdout(predicate::str::contains("\"file_kind_breakdown\""))
        .stdout(predicate::str::contains("\"extension_breakdown\""))
        .stdout(predicate::str::contains("\"edge_kind_breakdown\""));
}

// ============================================================================
// Delete Command Tests
// ============================================================================

#[test]
fn test_cli_delete() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index
    let output = llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let index_id = json["index_id"].as_str().unwrap();

    // Delete
    llmx()
        .args([
            "delete",
            index_id,
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted"));

    // Verify it's gone
    llmx()
        .args(["list", "--storage-dir", storage.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No persistent indexes found"));
}

// ============================================================================
// Export Command Tests
// ============================================================================

#[test]
fn test_cli_export_llm_md() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "export",
            "--format",
            "llm.md",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("#"));
}

#[test]
fn test_cli_export_zip() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();
    let output_dir = TempDir::new().unwrap();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let zip_path = output_dir.path().join("export.zip");
    llmx()
        .args([
            "export",
            "--format",
            "zip",
            "--output",
            zip_path.to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported zip"));

    assert!(zip_path.exists(), "Zip file should exist");
    assert!(
        fs::metadata(&zip_path).unwrap().len() > 0,
        "Zip file should not be empty"
    );
}

#[test]
fn test_cli_export_json() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "export",
            "--format",
            "json",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("{"))
        .stdout(predicate::str::contains("\"index_id\""));
}

// ============================================================================
// Get Command Tests
// ============================================================================

#[test]
fn test_cli_get_chunk() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Get chunk ID from search
    let output = llmx()
        .args([
            "search",
            "fn",
            "--json",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    if let Some(first_result) = json["results"].as_array().and_then(|a| a.first()) {
        let chunk_id = first_result["chunk_id"].as_str().unwrap();

        llmx()
            .args([
                "get",
                chunk_id,
                "--storage-dir",
                storage.path().to_str().unwrap(),
            ])
            .current_dir(project.path())
            .assert()
            .success()
            .stdout(predicate::str::contains(".rs:"));
    }
}

#[test]
fn test_cli_get_chunk_not_found() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    llmx()
        .args([
            "get",
            "nonexistent-chunk-id",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success() // Command succeeds but outputs "Chunk not found"
        .stderr(predicate::str::contains("Chunk not found"));
}

// ============================================================================
// Global Flags Tests
// ============================================================================

#[test]
fn test_cli_version() {
    llmx()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("llmx"));
}

#[test]
fn test_cli_help() {
    llmx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("explore"));
}

#[test]
fn test_cli_subcommand_help() {
    for subcommand in &[
        "index", "search", "explore", "symbols", "lookup", "refs", "list", "stats", "delete", "export",
        "get",
    ] {
        llmx()
            .args([subcommand, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage"));
    }
}
