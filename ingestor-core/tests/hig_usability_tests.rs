//! HIG Usability Evaluation Tests
//!
//! These tests apply Apple Human Interface Guidelines principles to evaluate
//! the CLI user experience. Based on the Macintosh Human Interface Guidelines.
//!
//! Run with: cargo test --features cli --test hig_usability_tests

#![cfg(feature = "cli")]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get a Command for the llmx binary.
fn llmx() -> Command {
    Command::cargo_bin("llmx").expect("Failed to find llmx binary")
}

/// Create a test project.
fn create_test_project() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    fs::write(
        temp.path().join("main.rs"),
        "fn main() { println!(\"Hello\"); }",
    )
    .unwrap();
    fs::write(temp.path().join("lib.rs"), "pub fn greet() {}").unwrap();
    fs::write(temp.path().join("README.md"), "# Project\n\nDescription").unwrap();
    temp
}

// ============================================================================
// HIG Principle: Feedback
// "Keep users informed about what's happening"
// ============================================================================

#[test]
fn test_hig_feedback_index_reports_progress() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index should report what it did
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("chunks"));
}

#[test]
fn test_hig_feedback_search_reports_count() {
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

    // Search should report result count
    llmx()
        .args([
            "search",
            "fn",
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
fn test_hig_feedback_delete_confirms_action() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index first
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

    // Delete should confirm what was deleted
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
}

// ============================================================================
// HIG Principle: Consistency
// "Use consistent terminology and interactions"
// ============================================================================

#[test]
fn test_hig_consistency_json_flag_everywhere() {
    // All commands should support --json consistently
    let commands_with_output = ["list", "index", "search", "explore", "export"];

    for cmd in commands_with_output {
        llmx()
            .args([cmd, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--json"));
    }
}

#[test]
fn test_hig_consistency_storage_dir_flag() {
    // --storage-dir should be consistent global flag
    llmx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--storage-dir"));
}

#[test]
fn test_hig_consistency_subcommand_naming() {
    // Commands should use consistent verb naming
    let expected_commands = ["index", "search", "explore", "list", "delete", "export", "get"];

    let output = llmx().arg("--help").assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    for cmd in expected_commands {
        assert!(
            stdout.contains(cmd),
            "Help should list '{}' command",
            cmd
        );
    }
}

// ============================================================================
// HIG Principle: Clarity
// "Communicate clearly and avoid jargon"
// ============================================================================

#[test]
fn test_hig_clarity_error_no_index() {
    let storage = TempDir::new().unwrap();
    let empty_dir = TempDir::new().unwrap();

    // Error message should be clear and actionable
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
        .stderr(predicate::str::contains("No index found"))
        .stderr(predicate::str::contains("llmx index")); // Suggests how to fix
}

#[test]
fn test_hig_clarity_error_invalid_mode() {
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

    // Error should explain valid options
    llmx()
        .args([
            "explore",
            "invalid",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid mode"))
        .stderr(predicate::str::contains("files").or(predicate::str::contains("outline")));
}

#[test]
fn test_hig_clarity_help_is_helpful() {
    // Help should explain what the tool does
    llmx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("search"));

    // Subcommand help should explain arguments
    llmx()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("path"));
}

// ============================================================================
// HIG Principle: Forgiveness
// "Allow users to undo or recover from actions"
// ============================================================================

#[test]
fn test_hig_forgiveness_can_reindex() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index once
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    // Re-index (update) should work
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));
}

#[test]
fn test_hig_forgiveness_delete_nonexistent_is_graceful() {
    let storage = TempDir::new().unwrap();

    // Note: Current behavior is that delete succeeds even for nonexistent IDs
    // This is arguably more forgiving - no error if already gone
    llmx()
        .args([
            "delete",
            "nonexistent-id",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();
}

// ============================================================================
// HIG Principle: Modelessness
// "Commands should work independently"
// ============================================================================

#[test]
fn test_hig_modelessness_commands_independent() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Each command should work without prior state
    llmx()
        .args(["list", "--storage-dir", storage.path().to_str().unwrap()])
        .assert()
        .success();

    // Index should work fresh
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Search should work immediately after index
    llmx()
        .args([
            "search",
            "fn",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success();
}

// ============================================================================
// HIG Language Guidelines
// "Use clear, consistent, helpful language"
// ============================================================================

#[test]
fn test_hig_language_no_internal_jargon() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    let output = llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Output should use user-friendly terms
    // Should NOT contain internal implementation details
    assert!(
        !stdout.contains("BTreeMap"),
        "Should not expose internal types"
    );
    assert!(
        !stdout.contains("inverted_index"),
        "Should not expose internal structures"
    );
    assert!(
        !stdout.contains("chunk_refs"),
        "Should not expose internal names"
    );
}

#[test]
fn test_hig_language_consistent_verbs() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // "Created" for new index
    llmx()
        .args([
            "index",
            project.path().to_str().unwrap(),
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created").or(predicate::str::contains("Updated")));

    // "deleted" for removal
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
}

// ============================================================================
// User Control
// "Users should feel in control"
// ============================================================================

#[test]
fn test_hig_control_explicit_options() {
    // User should be able to control chunking behavior
    llmx()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--chunk-size"))
        .stdout(predicate::str::contains("--max-file"));
}

#[test]
fn test_hig_control_limit_results() {
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

    // User can control result count
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
fn test_hig_control_token_budget() {
    // User can control token budget
    llmx()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-tokens"));
}

// ============================================================================
// Output Quality
// "Output should be scannable and useful"
// ============================================================================

#[test]
fn test_output_structured_for_scanning() {
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

    // Search output should be structured
    let output = llmx()
        .args([
            "search",
            "fn",
            "--storage-dir",
            storage.path().to_str().unwrap(),
        ])
        .current_dir(project.path())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Should have clear section markers
    assert!(
        stdout.contains("Found"),
        "Should indicate result count"
    );
}

#[test]
fn test_json_output_well_formed() {
    let storage = TempDir::new().unwrap();
    let project = create_test_project();

    // Index JSON output
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
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Index JSON output should be valid JSON");

    // Search JSON output
    let output = llmx()
        .args([
            "search",
            "fn",
            "--storage-dir",
            storage.path().to_str().unwrap(),
            "--json",
        ])
        .current_dir(project.path())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Search JSON output should be valid JSON");
}
