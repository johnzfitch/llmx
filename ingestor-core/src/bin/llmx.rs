//! llmx CLI - Codebase indexing and semantic search
//!
//! A CLI for efficiently indexing and searching codebases with semantic chunking.
//! Designed for both human users and AI agents.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ingestor_core::handlers::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_index_handler, llmx_manage_handler,
    llmx_search_handler, ExploreInput, IndexInput, IngestOptionsInput, ManageInput, SearchInput,
    SearchFiltersInput, IndexStore,
};
use ingestor_core::{export_llm, export_manifest_json, export_zip};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "llmx", version, about = "Codebase indexing and semantic search")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output JSON format (for agents)
    #[arg(long, global = true)]
    json: bool,

    /// Target specific index ID (default: auto-detect from cwd)
    #[arg(long, global = true)]
    index_id: Option<String>,

    /// Override storage directory (default: ~/.llmx/indexes)
    #[arg(long, global = true)]
    storage_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create or update index from paths
    Index {
        /// File or directory paths to index
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Target chunk size in characters (default: 4000)
        #[arg(long, default_value = "4000")]
        chunk_size: usize,

        /// Maximum file size in bytes (default: 10MB)
        #[arg(long, default_value = "10485760")]
        max_file: usize,
    },

    /// Search with inline content (token-budgeted)
    Search {
        /// Search query
        query: String,

        /// Token budget for inline content (default: 16000)
        #[arg(long, default_value = "16000")]
        max_tokens: usize,

        /// Maximum number of results (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Filter by path prefix
        #[arg(long)]
        path: Option<String>,

        /// Filter by chunk kind (markdown, javascript, json, html, text, image)
        #[arg(long)]
        kind: Option<String>,

        /// Use hybrid BM25+embeddings search
        #[arg(long)]
        semantic: bool,
    },

    /// List files, outline, or symbols
    Explore {
        /// What to list: 'files', 'outline', or 'symbols'
        mode: String,

        /// Filter by path prefix
        #[arg(long)]
        path: Option<String>,
    },

    /// List all indexes
    List,

    /// Delete an index
    Delete {
        /// Index ID to delete
        id: String,
    },

    /// Export index to file
    Export {
        /// Index ID to export (or auto-detect from cwd)
        id: Option<String>,

        /// Output format: llm.md, zip, json, manifest (default: zip)
        #[arg(long, default_value = "zip")]
        format: String,

        /// Output file (default: stdout for llm.md/json, export.zip for zip)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Get single chunk by ID
    Get {
        /// Chunk ID or ref to retrieve
        chunk_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let storage_dir = cli.storage_dir.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".llmx")
            .join("indexes")
    });

    let mut store = IndexStore::new(storage_dir)?;

    match cli.command {
        Commands::Index {
            paths,
            chunk_size,
            max_file,
        } => cmd_index(&mut store, paths, chunk_size, max_file, cli.json),

        Commands::Search {
            query,
            max_tokens,
            limit,
            path,
            kind,
            semantic,
        } => cmd_search(
            &mut store,
            &cli.index_id,
            query,
            max_tokens,
            limit,
            path,
            kind,
            semantic,
            cli.json,
        ),

        Commands::Explore { mode, path } => {
            cmd_explore(&mut store, &cli.index_id, mode, path, cli.json)
        }

        Commands::List => cmd_list(&mut store, cli.json),

        Commands::Delete { id } => cmd_delete(&mut store, id, cli.json),

        Commands::Export { id, format, output } => {
            cmd_export(&mut store, &cli.index_id, id, format, output, cli.json)
        }

        Commands::Get { chunk_id } => cmd_get(&mut store, &cli.index_id, chunk_id, cli.json),
    }
}

fn cmd_index(
    store: &mut IndexStore,
    paths: Vec<PathBuf>,
    chunk_size: usize,
    max_file: usize,
    json_output: bool,
) -> Result<()> {
    let start = Instant::now();

    let path_strings: Vec<String> = paths
        .iter()
        .map(|p| {
            p.canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .to_string()
        })
        .collect();

    let input = IndexInput {
        paths: path_strings,
        options: Some(IngestOptionsInput {
            chunk_target_chars: Some(chunk_size),
            max_file_bytes: Some(max_file),
        }),
    };

    let output = llmx_index_handler(store, input)?;
    let elapsed = start.elapsed();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if output.created {
            println!("Created new index: {}", output.index_id);
        } else {
            println!("Updated index: {}", output.index_id);
        }
        println!(
            "  {} files, {} chunks (avg {} tokens/chunk)",
            output.stats.total_files, output.stats.total_chunks, output.stats.avg_chunk_tokens
        );
        println!("  Completed in {:.1}ms", elapsed.as_secs_f64() * 1000.0);

        if !output.warnings.is_empty() {
            println!("\nWarnings:");
            for w in &output.warnings {
                println!("  - {} ({}): {}", w.path, w.code, w.message);
            }
        }
    }

    Ok(())
}

fn cmd_search(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    query: String,
    max_tokens: usize,
    limit: usize,
    path: Option<String>,
    kind: Option<String>,
    semantic: bool,
    json_output: bool,
) -> Result<()> {
    let start = Instant::now();
    let index_id = resolve_index_id(store, index_id_override)?;

    let input = SearchInput {
        index_id,
        query: query.clone(),
        filters: Some(SearchFiltersInput {
            path_prefix: path,
            kind,
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(limit),
        max_tokens: Some(max_tokens),
        use_semantic: Some(semantic),
    };

    let output = llmx_search_handler(store, input)?;
    let elapsed = start.elapsed();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Found {} results in {:.1}ms\n",
            output.total_matches,
            elapsed.as_secs_f64() * 1000.0
        );

        for (i, result) in output.results.iter().enumerate() {
            println!(
                "[{}] {}:{}-{} (score: {:.2})",
                i + 1,
                result.path,
                result.start_line,
                result.end_line,
                result.score
            );

            if let Some(ref sym) = result.symbol {
                println!("    Symbol: {}", sym);
            }
            if !result.heading_path.is_empty() {
                println!("    Heading: {}", result.heading_path.join(" > "));
            }

            println!("    ───────────────────────────────────");
            for line in result.content.lines().take(10) {
                println!("    {}", line);
            }
            if result.content.lines().count() > 10 {
                println!("    ...");
            }
            println!("    ───────────────────────────────────");
            println!();
        }

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
