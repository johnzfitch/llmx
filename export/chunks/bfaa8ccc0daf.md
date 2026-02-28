---
chunk_index: 1061
ref: "bfaa8ccc0daf"
id: "bfaa8ccc0daf5ff6cc9a40222af9e70e135174245a80e45aa431733c5050d589"
slug: "llmx-l462-564"
path: "/home/zack/dev/llmx/ingestor-core/src/bin/llmx.rs"
kind: "text"
lines: [462, 564]
token_estimate: 865
content_sha256: "6a80c2e8f8cb0ef3eefa662e281841a52cc867748e40f1c86c5de9752ac46300"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

Ok(())
}

fn cmd_get(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    chunk_id: String,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;

    let chunk = llmx_get_chunk_handler(store, &index_id, &chunk_id)?;

    if let Some(chunk) = chunk {
        if json_output {
            println!("{}", serde_json::to_string_pretty(&chunk)?);
        } else {
            println!("{}:{}-{}", chunk.path, chunk.start_line, chunk.end_line);
            if let Some(ref sym) = chunk.symbol {
                println!("Symbol: {}", sym);
            }
            if !chunk.heading_path.is_empty() {
                println!("Heading: {}", chunk.heading_path.join(" > "));
            }
            println!("Tokens: ~{}", chunk.token_estimate);
            println!("───────────────────────────────────");
            println!("{}", chunk.content);
            println!("───────────────────────────────────");
        }
    } else {
        if json_output {
            println!("null");
        } else {
            eprintln!("Chunk not found: {}", chunk_id);
        }
    }

    Ok(())
}

/// Resolve index ID from --index-id flag or auto-detect from cwd.
/// If no index exists, auto-creates one from the project root.
fn resolve_index_id(store: &mut IndexStore, override_id: &Option<String>) -> Result<String> {
    if let Some(id) = override_id {
        return Ok(id.clone());
    }

    // Try to auto-detect from current directory
    let cwd = std::env::current_dir().context("Could not get current directory")?;

    // Walk up to find project root (by .git, Cargo.toml, package.json, etc.)
    let mut project_root: Option<&std::path::Path> = None;
    let mut dir = cwd.as_path();
    loop {
        // Check for common project markers
        let markers = [".git", "Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
        for marker in markers {
            let marker_path = dir.join(marker);
            if marker_path.exists() {
                // Found project root, check if it's indexed
                if let Some(meta) = store.find_metadata_by_path(dir) {
                    return Ok(meta.id.clone());
                }
                // Remember as potential root for auto-indexing
                if project_root.is_none() {
                    project_root = Some(dir);
                }
            }
        }

        // Also check if this exact path is indexed
        if let Some(meta) = store.find_metadata_by_path(dir) {
            return Ok(meta.id.clone());
        }

        // Go up one directory
        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            break;
        }
    }

    // Auto-index if we found a project root
    if let Some(root) = project_root {
        eprintln!("No index found. Auto-indexing {}...", root.display());
        let input = IndexInput {
            paths: vec![root.to_string_lossy().to_string()],
            options: None,
        };
        let output = llmx_index_handler(store, input)?;
        eprintln!(
            "Created index: {} ({} files, {} chunks)",
            output.index_id, output.stats.total_files, output.stats.total_chunks
        );
        return Ok(output.index_id);
    }

    anyhow::bail!(
        "No index found and no project root detected.\n\
         Run `llmx index <path>` to create one, or use --index-id to specify."
    )
}