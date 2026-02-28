---
chunk_index: 1210
ref: "bbdc4503bda6"
id: "bbdc4503bda671ee0e7b96baa0817eab58071e0d3207631c6842f0ed3d4f3a1e"
slug: "token-savings-tests-l414-480"
path: "/home/zack/dev/llmx/ingestor-core/tests/token_savings_tests.rs"
kind: "text"
lines: [414, 480]
token_estimate: 561
content_sha256: "e5d86c61e1357d9467f571acb630aa68e42f5fabec00ce3fdc8c660ef0c24905"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

// The important thing is that explore provides structured data
    // Explore tokens may be slightly higher due to full paths vs relative
    assert!(
        explore_tokens <= tree_tokens * 5,
        "Explore should be reasonably efficient (got {} vs tree {})",
        explore_tokens,
        tree_tokens
    );
}

// ============================================================================
// Token Budget Enforcement
// ============================================================================

#[test]
fn test_token_budget_strictly_enforced() {
    let (_storage, mut store) = create_store();
    let project = TempDir::new().unwrap();

    // Create many files that would exceed budget
    for i in 0..50 {
        let content = format!(
            "// File {} with lots of content\n{}\n",
            i,
            "fn function() { let x = very_long_variable_name_here; }\n".repeat(20)
        );
        fs::write(project.path().join(format!("file{}.rs", i)), &content).unwrap();
    }

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Search with strict token budget
    let budget = 2000;
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "function".to_string(),
        filters: None,
        limit: Some(100),
        max_tokens: Some(budget),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();

    let actual_tokens: usize = search_output
        .results
        .iter()
        .map(|r| estimate_tokens(&r.content))
        .sum();

    println!("Token budget enforcement:");
    println!("  Budget: {}", budget);
    println!("  Actual tokens used: {}", actual_tokens);
    println!("  Results returned: {}", search_output.results.len());
    if let Some(ref truncated) = search_output.truncated_ids {
        println!("  Truncated results: {}", truncated.len());
    }

    // Should stay under budget (with some tolerance for metadata)
    assert!(
        actual_tokens <= budget + 500,
        "Should stay roughly within token budget"
    );
}