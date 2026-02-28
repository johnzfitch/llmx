---
chunk_index: 1060
ref: "c6cfccab838a"
id: "c6cfccab838a5e76a90597869090154f19dca342b374e681a38ab91689ef7119"
slug: "llmx-l297-461"
path: "/home/zack/dev/llmx/ingestor-core/src/bin/llmx.rs"
kind: "text"
lines: [297, 461]
token_estimate: 1166
content_sha256: "eef5503d9d0d4be4f8a5e87c0e15b5b90a79df599e647e0ce0183554d4fbf008"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

if let Some(ref truncated) = output.truncated_ids {
            println!(
                "Note: {} more results available (token budget exceeded)",
                truncated.len()
            );
        }
    }

    Ok(())
}

fn cmd_explore(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    mode: String,
    path: Option<String>,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;

    let input = ExploreInput {
        index_id,
        mode: mode.clone(),
        path_filter: path,
    };

    let output = llmx_explore_handler(store, input)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{} (total: {})\n", mode, output.total);
        for item in &output.items {
            println!("{}", item);
        }
    }

    Ok(())
}

fn cmd_list(store: &mut IndexStore, json_output: bool) -> Result<()> {
    let input = ManageInput {
        action: "list".to_string(),
        index_id: None,
    };

    let output = llmx_manage_handler(store, input)?;

    if json_output {
        if let Some(indexes) = &output.indexes {
            println!("{}", serde_json::to_string_pretty(indexes)?);
        }
    } else if let Some(indexes) = &output.indexes {
        if indexes.is_empty() {
            println!("No indexes found. Run `llmx index <path>` to create one.");
        } else {
            println!("Indexes:\n");
            for idx in indexes {
                println!("  {} ({})", idx.id, idx.root_path);
                println!(
                    "    {} files, {} chunks",
                    idx.file_count, idx.chunk_count
                );
                println!();
            }
        }
    }

    Ok(())
}

fn cmd_delete(store: &mut IndexStore, id: String, json_output: bool) -> Result<()> {
    let input = ManageInput {
        action: "delete".to_string(),
        index_id: Some(id.clone()),
    };

    let output = llmx_manage_handler(store, input)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if let Some(msg) = &output.message {
        println!("{}", msg);
    }

    Ok(())
}

fn cmd_export(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    explicit_id: Option<String>,
    format: String,
    output_path: Option<PathBuf>,
    json_output: bool,
) -> Result<()> {
    let index_id = explicit_id
        .or_else(|| index_id_override.clone())
        .map(Ok)
        .unwrap_or_else(|| resolve_index_id(store, &None))?;

    let index = store.load(&index_id)?;

    match format.as_str() {
        "llm.md" | "llm" => {
            let content = export_llm(index);
            if let Some(path) = output_path {
                fs::write(&path, &content)?;
                if !json_output {
                    println!("Exported llm.md to {}", path.display());
                }
            } else {
                print!("{}", content);
            }
        }
        "manifest" | "manifest.json" => {
            let content = export_manifest_json(index);
            if let Some(path) = output_path {
                fs::write(&path, &content)?;
                if !json_output {
                    println!("Exported manifest.json to {}", path.display());
                }
            } else {
                println!("{}", content);
            }
        }
        "json" | "index.json" => {
            let content = serde_json::to_string_pretty(index)?;
            if let Some(path) = output_path {
                fs::write(&path, &content)?;
                if !json_output {
                    println!("Exported index.json to {}", path.display());
                }
            } else {
                println!("{}", content);
            }
        }
        "zip" => {
            let data = export_zip(index);
            let path = output_path.unwrap_or_else(|| PathBuf::from("export.zip"));
            fs::write(&path, &data)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "size": data.len()
                    })
                );
            } else {
                println!(
                    "Exported zip ({} bytes) to {}",
                    data.len(),
                    path.display()
                );
            }
        }
        _ => {
            anyhow::bail!(
                "Unknown format: {}. Use: llm.md, manifest, json, or zip",
                format
            );
        }
    }