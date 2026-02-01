//! Shared test utilities for llmx test suite.
//!
//! This module provides common helpers for testing handlers, CLI commands,
//! and measuring token savings.

use ingestor_core::{ingest_files, FileInput, IndexFile, IngestOptions};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a temporary project directory with test files.
pub fn create_test_project(files: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for (path, content) in files {
        let file_path = temp_dir.path().join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        fs::write(&file_path, content).expect("Failed to write test file");
    }

    temp_dir
}

/// Load fixture file content from tests/fixtures directory.
#[allow(dead_code)]
pub fn load_fixture(relative_path: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(relative_path);
    fs::read(&path).unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", relative_path, e))
}

/// Load fixture as string.
#[allow(dead_code)]
pub fn load_fixture_str(relative_path: &str) -> String {
    String::from_utf8(load_fixture(relative_path)).expect("Fixture is not valid UTF-8")
}

/// Estimate token count (same algorithm as util.rs).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Calculate raw token count for all files in a directory.
#[allow(dead_code)]
pub fn calculate_raw_tokens(dir: &Path) -> usize {
    fn walk_dir(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(walk_dir(&path));
                } else if path.is_file() {
                    files.push(path);
                }
            }
        }
        files
    }

    let mut total = 0;
    for path in walk_dir(dir) {
        if let Ok(content) = fs::read_to_string(&path) {
            total += estimate_tokens(&content);
        }
    }
    total
}

/// Index files and return the index without storage.
#[allow(dead_code)]
pub fn index_files_direct(files: &[(&str, &str)]) -> IndexFile {
    let inputs: Vec<FileInput> = files
        .iter()
        .map(|(path, content)| FileInput {
            path: (*path).to_string(),
            data: content.as_bytes().to_vec(),
            mtime_ms: None,
            fingerprint_sha256: None,
        })
        .collect();

    ingest_files(inputs, IngestOptions::default())
}

/// Token savings report.
#[derive(Debug)]
#[allow(dead_code)]
pub struct TokenReport {
    pub raw_tokens: usize,
    pub index_tokens: usize,
    pub search_tokens: usize,
    pub savings_percentage: f64,
}

impl TokenReport {
    #[allow(dead_code)]
    pub fn calculate(raw: usize, output: usize) -> Self {
        let savings = if raw > 0 {
            ((raw - output) as f64 / raw as f64) * 100.0
        } else {
            0.0
        };
        TokenReport {
            raw_tokens: raw,
            index_tokens: output,
            search_tokens: 0,
            savings_percentage: savings,
        }
    }
}

/// Simple test project templates.
#[allow(dead_code)]
pub mod templates {
    pub const RUST_HELLO: &str = r#"fn main() {
    println!("Hello, world!");
}
"#;

    pub const JS_FUNCTION: &str = r#"function greet(name) {
    return `Hello, ${name}!`;
}

module.exports = { greet };
"#;

    pub const MARKDOWN_DOC: &str = r#"# Title

## Introduction

This is a test document.

## Usage

```rust
fn example() {}
```
"#;

    pub const JSON_CONFIG: &str = r#"{
    "name": "test-project",
    "version": "1.0.0",
    "dependencies": {}
}
"#;

    pub const TOML_CONFIG: &str = r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1"
"#;
}
