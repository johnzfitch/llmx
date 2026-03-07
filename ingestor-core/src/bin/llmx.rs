//! llmx_mcp CLI - Codebase indexing and semantic search
//!
//! A CLI for efficiently indexing and searching codebases with semantic chunking.
//! Designed for both human users and AI agents.
//!
//! ## Dynamic Search (default)
//!
//! By default, `llmx_mcp search` auto-detects the project root and builds an in-memory
//! index on the fly. Results are cached for repeat queries.
//!
//! ```bash
//! llmx_mcp search "handleError"              # Auto-detect project, use cache
//! llmx_mcp search "handleError" --dynamic    # Force fresh dynamic index
//! llmx_mcp search "handleError" --no-cache   # Skip cache, rebuild index
//! llmx_mcp search "handleError" --path ./src # Explicit search path
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use llmx_mcp::handlers::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_index_handler, llmx_manage_handler,
    llmx_search_dynamic_handler, llmx_search_handler, DynamicCache, DynamicSearchInput,
    ExploreInput, IndexInput, IndexStore, IngestOptionsInput, ManageInput, SearchFiltersInput,
    SearchInput,
};
use llmx_mcp::{export_llm, export_manifest_json, export_zip};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "llmx_mcp", version, about = "Codebase indexing and semantic search")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output JSON format (for agents)
    #[arg(long, global = true)]
    json: bool,

    /// Target specific index ID (bypasses dynamic search)
    #[arg(long, global = true)]
    index_id: Option<String>,

    /// Override storage directory (default: ~/.llmx_mcp/indexes)
    #[arg(long, global = true)]
    storage_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create or update persistent index from paths
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
    ///
    /// By default, auto-detects project root and uses dynamic indexing.
    /// Use --index-id to search a specific persistent index.
    Search {
        /// Search query
        query: String,

        /// Search path (default: auto-detect project root)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Force dynamic mode (ignore persistent index)
        #[arg(long)]
        dynamic: bool,

        /// Skip cache (force fresh index build)
        #[arg(long)]
        no_cache: bool,

        /// Allow searching dangerous paths (/, /home, etc.)
        #[arg(long)]
        force: bool,

        /// Token budget for inline content (default: 16000)
        #[arg(long, default_value = "16000")]
        max_tokens: usize,

        /// Maximum number of results (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Filter by path prefix (within the project)
        #[arg(long)]
        filter_path: Option<String>,

        /// Filter by chunk kind (markdown, javascript, json, html, text, image)
        #[arg(long)]
        kind: Option<String>,

        /// Use hybrid BM25+embeddings search
        #[arg(long)]
        semantic: bool,
    },

    /// List files, outline, or symbols from index
    Explore {
        /// What to list: 'files', 'outline', or 'symbols'
        mode: String,

        /// Filter by path prefix
        #[arg(long)]
        path: Option<String>,
    },

    /// List all persistent indexes
    List,

    /// Delete a persistent index
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
            .join(".llmx_mcp")
            .join("indexes")
    });

    let mut store = IndexStore::new(storage_dir)?;
    let mut cache = DynamicCache::default_size();

    match cli.command {
        Commands::Index {
            paths,
            chunk_size,
            max_file,
        } => cmd_index(&mut store, paths, chunk_size, max_file, cli.json),

        Commands::Search {
            query,
            path,
            dynamic,
            no_cache,
            force,
            max_tokens,
            limit,
            filter_path,
            kind,
            semantic,
        } => cmd_search(
            &mut store,
            &mut cache,
            &cli.index_id,
            query,
            path,
            dynamic,
            no_cache,
            force,
            max_tokens,
            limit,
            filter_path,
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

#[allow(clippy::too_many_arguments)]
fn cmd_search(
    store: &mut IndexStore,
    cache: &mut DynamicCache,
    index_id_override: &Option<String>,
    query: String,
    path: Option<PathBuf>,
    dynamic: bool,
    no_cache: bool,
    force: bool,
    max_tokens: usize,
    limit: usize,
    filter_path: Option<String>,
    kind: Option<String>,
    semantic: bool,
    json_output: bool,
) -> Result<()> {
    // If explicit --index-id is provided, use the old search handler
    if let Some(index_id) = index_id_override {
        return cmd_search_persistent(
            store,
            index_id.clone(),
            query,
            max_tokens,
            limit,
            filter_path,
            kind,
            semantic,
            json_output,
        );
    }

    // Use dynamic search
    let input = DynamicSearchInput {
        query: query.clone(),
        path,
        force_dynamic: dynamic,
        no_cache,
        force_dangerous: force,
        filters: Some(SearchFiltersInput {
            path_prefix: filter_path,
            kind,
            symbol_prefix: None,
            heading_prefix: None,
        }),
        limit: Some(limit),
        max_tokens: Some(max_tokens),
        use_semantic: Some(semantic),
    };

    let output = llmx_search_dynamic_handler(store, cache, input)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Print mode indicator with stats
        let mode_display = match output.mode.as_str() {
            "dynamic" => format!(
                "[dynamic] Indexing {} ({} files, {})",
                output.stats.root_path,
                output.stats.file_count,
                format_bytes(output.stats.total_bytes)
            ),
            "cached" => "[cached]".to_string(),
            "persistent" => "[persistent]".to_string(),
            _ => format!("[{}]", output.mode),
        };

        let total_ms = output.stats.index_time_ms + output.stats.search_time_ms;
        println!(
            "{} Found {} results in {}ms\n",
            mode_display, output.total_matches, total_ms
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

        if output.stats.truncated {
            println!(
                "Warning: Search was truncated due to safety limits (too many files or bytes)"
            );
        }
    }

    Ok(())
}

/// Search a specific persistent index (legacy behavior when --index-id is used)
#[allow(clippy::too_many_arguments)]
fn cmd_search_persistent(
    store: &mut IndexStore,
    index_id: String,
    query: String,
    max_tokens: usize,
    limit: usize,
    path: Option<String>,
    kind: Option<String>,
    semantic: bool,
    json_output: bool,
) -> Result<()> {
    let start = Instant::now();

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
            "[persistent] Found {} results in {:.1}ms\n",
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
            println!("No persistent indexes found. Run `llmx_mcp index <path>` to create one.");
            println!("\nNote: `llmx_mcp search` now works without a persistent index!");
            println!("It auto-detects project roots and builds indexes on the fly.");
        } else {
            println!("Persistent indexes:\n");
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
    } else if json_output {
        println!("null");
    } else {
        eprintln!("Chunk not found: {}", chunk_id);
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
        let markers = [
            ".git",
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
        ];
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
         Run `llmx_mcp index <path>` to create one, or use --index-id to specify."
    )
}

/// Format bytes as human-readable string
fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}
