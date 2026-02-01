//! CLI integration tests - end-to-end testing of llmx commands.
//!
//! These tests spawn the actual llmx binary and verify its output.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get a Command for the llmx binary.
fn llmx() -> Command {
    Command::cargo_bin("llmx").expect("Failed to find llmx binary")
}

/// Create a test project with sample files.
fn create_test_project() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");

    fs::create_dir_all(temp.path().join("src")).unwrap();

    fs::write(
        temp.path().join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
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

    // Note: llmx index succeeds with nonexistent paths but creates empty index
    // This is the current behavior - it doesn't fail on missing paths
    llmx()
        .args([
            "index",
            "/nonexistent/path/that/does/not/exist",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 files"));
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
        .failure()
        .stderr(predicate::str::contains("No index found"));
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
        .stdout(predicate::str::contains("No indexes found"));
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
        .stdout(predicate::str::contains("Indexes:"))
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
        .stdout(predicate::str::contains("No indexes found"));
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
    for subcommand in &["index", "search", "explore", "list", "delete", "export", "get"] {
        llmx()
            .args([subcommand, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage"));
    }
}
