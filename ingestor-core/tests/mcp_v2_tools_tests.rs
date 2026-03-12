#![cfg(feature = "mcp")]

use llmx_mcp::mcp::{
    llmx_get_chunk_handler, llmx_index_handler, llmx_lookup_handler, llmx_refs_handler,
    GetChunkInput, IndexInput, IndexStore, LookupInput, RefsInput,
};
use tempfile::tempdir;

fn build_store(files: &[(&str, &str)]) -> (tempfile::TempDir, IndexStore, String) {
    let project = tempdir().expect("should create temp project");
    for (path, content) in files {
        let file_path = project.path().join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("should create parent dirs");
        }
        std::fs::write(&file_path, content).expect("should write source file");
    }

    let storage_dir = tempdir().expect("should create temp storage");
    let mut store = IndexStore::new(storage_dir.path().to_path_buf()).expect("should create store");
    let index_output = llmx_index_handler(
        &mut store,
        IndexInput {
            paths: vec![project.path().to_string_lossy().to_string()],
            options: None,
        },
    )
    .expect("index should succeed");

    (project, store, index_output.index_id)
}

#[test]
fn lookup_returns_exact_symbol_match() {
    let (_project, mut store, index_id) = build_store(&[(
        "auth.ts",
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
    )]);

    let output = llmx_lookup_handler(
        &mut store,
        LookupInput {
            index_id,
            symbol: "verifyToken".to_string(),
            kind: Some("function".to_string()),
            path_prefix: None,
            limit: Some(10),
        },
    )
    .expect("lookup should succeed");

    assert_eq!(output.total, 1, "expected one exact symbol match");
    assert_eq!(output.matches[0].qualified_name, "verifyToken");
    assert_eq!(output.matches[0].ast_kind, "function");
    assert!(output.matches[0].path.ends_with("auth.ts"));
}

#[test]
fn lookup_supports_prefix_queries() {
    let (_project, mut store, index_id) = build_store(&[(
        "auth.ts",
        r#"
export function verifyToken(token: string): boolean {
  return token.length > 0;
}

export function verifySession(token: string): boolean {
  return token.length > 1;
}
"#,
    )]);

    let output = llmx_lookup_handler(
        &mut store,
        LookupInput {
            index_id,
            symbol: "verify*".to_string(),
            kind: Some("function".to_string()),
            path_prefix: None,
            limit: Some(10),
        },
    )
    .expect("prefix lookup should succeed");

    assert_eq!(output.total, 2, "expected both verify* functions to match");
    assert!(
        output.matches.iter().any(|entry| entry.qualified_name == "verifyToken"),
        "expected verifyToken in prefix results, got {:?}",
        output.matches.iter().map(|entry| &entry.qualified_name).collect::<Vec<_>>()
    );
    assert!(
        output.matches.iter().any(|entry| entry.qualified_name == "verifySession"),
        "expected verifySession in prefix results, got {:?}",
        output.matches.iter().map(|entry| &entry.qualified_name).collect::<Vec<_>>()
    );
}

#[test]
fn refs_returns_callers_for_symbol() {
    let (_project, mut store, index_id) = build_store(&[(
        "auth.ts",
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
    )]);

    let output = llmx_refs_handler(
        &mut store,
        RefsInput {
            index_id,
            symbol: "verifyToken".to_string(),
            direction: "callers".to_string(),
            depth: Some(1),
            limit: Some(10),
        },
    )
    .expect("refs should succeed");

    assert!(
        output
            .refs
            .iter()
            .any(|reference| reference.source_symbol.contains("login") && reference.target_symbol == "verifyToken"),
        "expected AuthService.login to call verifyToken, got {:?}",
        output.refs
    );
}

#[test]
fn refs_callees_use_target_chunk_context_when_resolved() {
    let (_project, mut store, index_id) = build_store(&[
        (
            "auth.ts",
            r#"
export function verifyToken(token: string): boolean {
  return token.length > 0;
}
"#,
        ),
        (
            "config.ts",
            r#"
import { verifyToken } from "./auth";

export function parseConfig(token: string): boolean {
  return verifyToken(token);
}
"#,
        ),
    ]);

    let output = llmx_refs_handler(
        &mut store,
        RefsInput {
            index_id,
            symbol: "parseConfig".to_string(),
            direction: "callees".to_string(),
            depth: Some(1),
            limit: Some(10),
        },
    )
    .expect("callee refs should succeed");

    assert_eq!(output.refs.len(), 1, "expected one callee edge for parseConfig");
    let reference = &output.refs[0];
    assert_eq!(reference.target_symbol, "verifyToken");
    assert!(
        reference.path.ends_with("auth.ts"),
        "expected callee context to point at auth.ts, got {}",
        reference.path
    );
    assert!(
        reference.context.contains("export function verifyToken"),
        "expected target chunk context, got {}",
        reference.context
    );
    assert!(
        !reference.context.contains("parseConfig"),
        "expected source snippet to be excluded from forward-ref context, got {}",
        reference.context
    );
}

#[test]
fn refs_qualified_identity_avoids_duplicate_name_collapse() {
    let (_project, mut store, index_id) = build_store(&[
        (
            "a/auth.ts",
            r#"
export function login(): boolean {
  return true;
}

export function startA(): boolean {
  return login();
}
"#,
        ),
        (
            "b/auth.ts",
            r#"
export function login(): boolean {
  return false;
}

export function startB(): boolean {
  return login();
}
"#,
        ),
    ]);

    let output = llmx_refs_handler(
        &mut store,
        RefsInput {
            index_id: index_id.clone(),
            symbol: "startA".to_string(),
            direction: "callees".to_string(),
            depth: Some(1),
            limit: Some(10),
        },
    )
    .expect("refs should succeed");

    assert_eq!(output.refs.len(), 1, "expected one callee edge for startA");
    let target_chunk_id = output.refs[0]
        .target_chunk_id
        .clone()
        .expect("callee should resolve to a specific chunk");

    let target_chunk = llmx_get_chunk_handler(
        &mut store,
        GetChunkInput {
            index_id,
            chunk_id: target_chunk_id,
        },
    )
    .expect("get chunk should succeed")
    .expect("target chunk should exist");

    assert!(
        target_chunk.path.contains("/a/") || target_chunk.path.ends_with("a/auth.ts"),
        "expected startA to resolve to a/auth.ts login, got {}",
        target_chunk.path
    );
}
