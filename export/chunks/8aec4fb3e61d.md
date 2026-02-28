---
chunk_index: 1208
ref: "8aec4fb3e61d"
id: "8aec4fb3e61dd0e344de2abd06d3aa400262cb55163521b56cc6c379e6fe3ccb"
slug: "token-savings-tests-l133-280"
path: "/home/zack/dev/llmx/ingestor-core/tests/token_savings_tests.rs"
kind: "text"
lines: [133, 280]
token_estimate: 1101
content_sha256: "601fd96f32ae5f0889a9d835a618b1d899006c0449689c96d49cb4dcebae6bc9"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

// ============================================================================
// Medium Codebase Tests (50 files, mixed sizes)
// ============================================================================

#[test]
fn test_token_savings_medium_codebase() {
    let (_storage, mut store) = create_store();
    let project = TempDir::new().unwrap();

    // Create 50 files of varying sizes
    for i in 0..50 {
        let size = (i % 5 + 1) * 20; // 20, 40, 60, 80, or 100 lines
        let content = format!(
            "// Module {}\n\n{}\n",
            i,
            "pub fn example() { todo!() }\n".repeat(size)
        );
        let subdir = match i % 3 {
            0 => "src",
            1 => "lib",
            _ => "tests",
        };
        fs::create_dir_all(project.path().join(subdir)).ok();
        fs::write(
            project.path().join(subdir).join(format!("mod{}.rs", i)),
            &content,
        )
        .unwrap();
    }

    let raw_tokens = calculate_raw_tokens(project.path());

    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    // Verify indexing succeeded
    assert_eq!(idx_output.stats.total_files, 50);

    // Search with focused query
    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "example".to_string(),
        filters: None,
        limit: Some(10),
        max_tokens: Some(8000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();
    let search_tokens: usize = search_output
        .results
        .iter()
        .map(|r| estimate_tokens(&r.content))
        .sum();

    let savings_pct = if raw_tokens > 0 {
        ((raw_tokens - search_tokens) as f64 / raw_tokens as f64) * 100.0
    } else {
        0.0
    };

    println!("Medium codebase search savings: {:.1}%", savings_pct);

    // Search should provide meaningful savings
    // Note: Actual savings depend on query and content distribution
    assert!(
        savings_pct >= 60.0,
        "Medium codebase search should save at least 60%, got {:.1}%",
        savings_pct
    );
}

// ============================================================================
// Search vs Grep Comparison
// ============================================================================

#[test]
fn test_token_savings_search_vs_grep() {
    let (_storage, mut store) = create_store();
    let project = TempDir::new().unwrap();

    // Create files with specific patterns
    let content = r#"
fn process_data(input: &str) -> Result<String, Error> {
    let parsed = parse_input(input)?;
    let validated = validate(parsed)?;
    let transformed = transform(validated)?;
    Ok(format_output(transformed))
}

fn parse_input(s: &str) -> Result<Data, Error> {
    serde_json::from_str(s).map_err(Error::Parse)
}

fn validate(data: Data) -> Result<Data, Error> {
    if data.is_valid() { Ok(data) } else { Err(Error::Invalid) }
}

fn transform(data: Data) -> Data {
    Data { value: data.value * 2, ..data }
}

fn format_output(data: Data) -> String {
    format!("Result: {}", data.value)
}
"#;

    for i in 0..20 {
        fs::write(
            project.path().join(format!("processor{}.rs", i)),
            content,
        )
        .unwrap();
    }

    // Simulate grep output (all matching lines from all files)
    let grep_output = content
        .lines()
        .filter(|l| l.contains("fn"))
        .collect::<Vec<_>>()
        .join("\n")
        .repeat(20);
    let grep_tokens = estimate_tokens(&grep_output);

    // llmx search (with context and deduplication)
    let idx_input = IndexInput {
        paths: vec![project.path().to_string_lossy().to_string()],
        options: None,
    };
    let idx_output = llmx_index_handler(&mut store, idx_input).unwrap();

    let search_input = SearchInput {
        index_id: idx_output.index_id,
        query: "fn".to_string(),
        filters: None,
        limit: Some(5),
        max_tokens: Some(4000),
        use_semantic: None,
    };
    let search_output = llmx_search_handler(&mut store, search_input).unwrap();
    let search_tokens: usize = search_output
        .results
        .iter()
        .map(|r| estimate_tokens(&r.content))
        .sum();