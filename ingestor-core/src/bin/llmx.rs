//! llmx CLI - Codebase indexing and semantic search
//!
//! A CLI for efficiently indexing and searching codebases with semantic chunking.
//! Designed for both human users and AI agents.
//!
//! ## Dynamic Search (default)
//!
//! By default, `llmx search` auto-detects the project root and builds an in-memory
//! index on the fly. Results are cached for repeat queries.
//!
//! ```bash
//! llmx search "handleError"              # Auto-detect project, use cache
//! llmx search "handleError" --dynamic    # Force fresh dynamic index
//! llmx search "handleError" --no-cache   # Skip cache, rebuild index
//! llmx search "handleError" --path ./src # Explicit search path
//! ```

use anyhow::{Context, Result};
use clap::{error::ErrorKind, Parser, Subcommand};
use llmx_mcp::handlers::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_index_handler, llmx_manage_handler,
    llmx_lookup_handler, llmx_refs_handler, llmx_search_dynamic_handler, llmx_search_handler,
    llmx_symbols_handler, DynamicCache, DynamicSearchInput, ExploreInput, IndexInput, IndexStore,
    IngestOptionsInput, LookupInput, ManageInput, RefsInput, SearchFiltersInput, SearchInput,
    SymbolsInput,
};
use llmx_mcp::{export_llm, export_manifest_json, export_zip, DEFAULT_MAX_FILE_BYTES};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const DEFAULT_SEARCH_MAX_TOKENS: usize = 8000;

#[derive(Parser)]
#[command(name = "llmx", version, about = "Codebase indexing and semantic search")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output JSON format (for agents)
    #[arg(long, global = true)]
    json: bool,

    /// Target specific index by ID (bypasses dynamic search)
    #[arg(long, visible_alias = "index-id", global = true)]
    index: Option<String>,

    /// Override storage directory (default: ~/.local/share/llmx/indexes)
    #[arg(long, global = true)]
    storage_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create or update persistent index from paths (default: current directory)
    Index {
        /// File or directory paths to index (default: current directory)
        paths: Vec<PathBuf>,

        /// Target chunk size in characters (default: 4000)
        #[arg(long, default_value = "4000")]
        chunk_size: usize,

        /// Maximum file size in bytes (default: 256MB)
        #[arg(long, default_value_t = DEFAULT_MAX_FILE_BYTES)]
        max_file: usize,
    },

    /// Search with inline content (token-budgeted)
    ///
    /// By default, auto-detects project root and uses dynamic indexing.
    /// Use --index-id to search a specific persistent index.
    Search {
        /// Search query (omit to see help and examples)
        query: Option<String>,

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

        /// Token budget for inline content (default: 8000)
        #[arg(long, default_value_t = DEFAULT_SEARCH_MAX_TOKENS)]
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

        /// Search strategy: auto, bm25, semantic, or hybrid
        #[arg(long)]
        strategy: Option<String>,

        /// Hybrid fusion strategy: rrf or linear
        #[arg(long)]
        hybrid_strategy: Option<String>,

        /// Query intent routing: auto, symbol, semantic, or keyword
        #[arg(long)]
        intent: Option<String>,

        /// Include human-readable explanations for why each result matched
        #[arg(long)]
        explain: bool,
    },

    /// List files, outline, or symbols from index
    Explore {
        /// What to list: 'files', 'outline', or 'symbols'
        mode: String,

        /// Filter by path prefix
        #[arg(long)]
        path: Option<String>,
    },

    /// List structural symbols with rich metadata
    Symbols {
        /// Symbol name pattern: exact `foo`, prefix `foo*`, or substring `*foo*`
        #[arg(long)]
        pattern: Option<String>,

        /// Filter by kind: function, method, class, interface, type, enum, constant, variable, test
        #[arg(long)]
        kind: Option<String>,

        /// Filter by file path prefix
        #[arg(long)]
        path: Option<String>,

        /// Maximum number of results (default: 50)
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Look up exact or prefix-matched symbols from the structural index
    Lookup {
        /// Exact symbol or prefix pattern, for example `parseConfig` or `parse*`
        symbol: String,

        /// Filter by kind: function, method, class, interface, type, enum, constant, variable, test
        #[arg(long)]
        kind: Option<String>,

        /// Filter by file path prefix
        #[arg(long)]
        path: Option<String>,

        /// Maximum number of results (default: 20)
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Trace structural references between indexed symbols
    Refs {
        /// Symbol to trace references for
        symbol: String,

        /// Direction: callers, callees, importers, imports, or type_users
        #[arg(long)]
        direction: String,

        /// Traversal depth in hops (default: 1)
        #[arg(long, default_value = "1")]
        depth: usize,

        /// Maximum number of results (default: 20)
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// List all persistent indexes
    List,

    /// Show detailed stats for a persistent index
    Stats,

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
    // Handle fuzzy command matching for typos
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            if e.kind() == ErrorKind::InvalidSubcommand {
                if let Some(suggestion) = suggest_command(&std::env::args().collect::<Vec<_>>()) {
                    eprintln!("{}\n", e);
                    eprintln!("Did you mean: {}", suggestion);
                    std::process::exit(1);
                }
            }
            e.exit();
        }
    };

    let storage_dir = cli.storage_dir
        .unwrap_or_else(llmx_mcp::default_storage_dir);

    let mut store = IndexStore::new(storage_dir)?;
    let mut cache = DynamicCache::default_size();

    match cli.command {
        Commands::Index {
            paths,
            chunk_size,
            max_file,
        } => {
            // Default to current directory if no paths specified
            let paths = if paths.is_empty() {
                vec![std::env::current_dir().context("Could not get current directory")?]
            } else {
                paths
            };
            cmd_index(&mut store, paths, chunk_size, max_file, cli.json)
        }

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
            strategy,
            hybrid_strategy,
            intent,
            explain,
        } => {
            // If no query provided, show directory info and search examples
            let query = match query {
                Some(q) => q,
                None => {
                    return show_search_help(&mut store, &mut cache, path.as_ref());
                }
            };
            cmd_search(
                &mut store,
                &mut cache,
                &cli.index,
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
                strategy,
                hybrid_strategy,
                intent,
                explain,
                cli.json,
            )
        }

        Commands::Explore { mode, path } => {
            cmd_explore(&mut store, &cli.index, mode, path, cli.json)
        }

        Commands::Symbols {
            pattern,
            kind,
            path,
            limit,
        } => cmd_symbols(
            &mut store,
            &cli.index,
            pattern,
            kind,
            path,
            limit,
            cli.json,
        ),

        Commands::Lookup {
            symbol,
            kind,
            path,
            limit,
        } => cmd_lookup(&mut store, &cli.index, symbol, kind, path, limit, cli.json),

        Commands::Refs {
            symbol,
            direction,
            depth,
            limit,
        } => cmd_refs(
            &mut store,
            &cli.index,
            symbol,
            direction,
            depth,
            limit,
            cli.json,
        ),

        Commands::List => cmd_list(&mut store, cli.json),

        Commands::Stats => cmd_stats(&mut store, &cli.index, cli.json),

        Commands::Delete { id } => cmd_delete(&mut store, id, cli.json),

        Commands::Export { id, format, output } => {
            cmd_export(&mut store, &cli.index, id, format, output, cli.json)
        }

        Commands::Get { chunk_id } => cmd_get(&mut store, &cli.index, chunk_id, cli.json),
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
            max_total_bytes: None,
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
    strategy: Option<String>,
    hybrid_strategy: Option<String>,
    intent: Option<String>,
    explain: bool,
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
            strategy,
            hybrid_strategy,
            intent,
            explain,
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
        hybrid_strategy,
        intent,
        explain: Some(explain),
        strategy,
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

        for notice in &output.notices {
            println!("Note: {}", notice.message);
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
    strategy: Option<String>,
    hybrid_strategy: Option<String>,
    intent: Option<String>,
    explain: bool,
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
        hybrid_strategy,
        intent,
        explain: Some(explain),
        strategy,
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

        for notice in &output.notices {
            println!("Note: {}", notice.message);
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

fn cmd_lookup(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    symbol: String,
    kind: Option<String>,
    path: Option<String>,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;
    let output = llmx_lookup_handler(
        store,
        LookupInput {
            index_id,
            symbol,
            kind,
            path_prefix: path,
            limit: Some(limit),
        },
    )?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("matches (total: {})\n", output.total);
        for entry in &output.matches {
            println!(
                "{} {}:{}-{}",
                entry.qualified_name, entry.path, entry.start_line, entry.end_line
            );
            println!("  kind: {}", entry.ast_kind);
            if let Some(signature) = &entry.signature {
                println!("  signature: {}", signature);
            }
            if let Some(doc_summary) = &entry.doc_summary {
                println!("  docs: {}", doc_summary);
            }
            println!();
        }
    }

    Ok(())
}

fn cmd_symbols(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    pattern: Option<String>,
    kind: Option<String>,
    path: Option<String>,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;
    let output = llmx_symbols_handler(
        store,
        SymbolsInput {
            index_id,
            pattern,
            ast_kind: kind,
            path_prefix: path,
            limit: Some(limit),
        },
    )?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("symbols (total: {})\n", output.total);
        for entry in &output.symbols {
            println!(
                "{} {}:{}-{}",
                entry.qualified_name, entry.path, entry.start_line, entry.end_line
            );
            println!("  kind: {}", entry.ast_kind);
            if let Some(signature) = &entry.signature {
                println!("  signature: {}", signature);
            }
            if let Some(doc_summary) = &entry.doc_summary {
                println!("  docs: {}", doc_summary);
            }
            if entry.exported {
                println!("  exported: true");
            }
            println!();
        }
    }

    Ok(())
}

fn cmd_refs(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    symbol: String,
    direction: String,
    depth: usize,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;
    let output = llmx_refs_handler(
        store,
        RefsInput {
            index_id,
            symbol,
            direction,
            depth: Some(depth),
            limit: Some(limit),
        },
    )?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("refs (total: {})\n", output.total);
        for entry in &output.refs {
            println!(
                "{} -> {} {}:{}-{}",
                entry.source_symbol, entry.target_symbol, entry.path, entry.start_line, entry.end_line
            );
            if let Some(ast_kind) = &entry.ast_kind {
                println!("  kind: {}", ast_kind);
            }
            if let Some(signature) = &entry.signature {
                println!("  signature: {}", signature);
            }
            println!("  context: {}", entry.context);
            println!();
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

fn cmd_stats(
    store: &mut IndexStore,
    index_id_override: &Option<String>,
    json_output: bool,
) -> Result<()> {
    let index_id = resolve_index_id(store, index_id_override)?;
    let output = llmx_manage_handler(
        store,
        ManageInput {
            action: "stats".to_string(),
            index_id: Some(index_id),
        },
    )?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if let Some(stats) = &output.stats {
        println!("stats\n");
        println!("files: {}", stats.total_files);
        println!("chunks: {}", stats.total_chunks);
        println!("avg_chunk_tokens: {}", stats.avg_chunk_tokens);
        println!("symbols: {}", stats.symbol_count);
        println!("edges: {}", stats.edge_count);
        println!("languages: {}", stats.language_count);
        print_breakdown("file kinds", &stats.file_kind_breakdown);
        print_breakdown("extensions", &stats.extension_breakdown);
        print_breakdown("ast kinds", &stats.ast_kind_breakdown);
        print_breakdown("edge kinds", &stats.edge_kind_breakdown);
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

fn print_breakdown(title: &str, breakdown: &std::collections::BTreeMap<String, usize>) {
    if breakdown.is_empty() {
        return;
    }

    println!("\n{}:", title);
    for (label, count) in breakdown {
        println!("  {}: {}", label, count);
    }
}

/// Suggest a command based on fuzzy matching for typos.
fn suggest_command(args: &[String]) -> Option<String> {
    use strsim::jaro_winkler;

    const COMMANDS: &[&str] = &[
        "index", "search", "explore", "symbols", "lookup", "refs",
        "list", "stats", "delete", "export", "get",
    ];

    // Find the invalid subcommand (usually the second arg)
    let typo = args.get(1)?;
    if typo.starts_with('-') {
        return None;
    }

    let mut best_match: Option<(&str, f64)> = None;
    for cmd in COMMANDS {
        let score = jaro_winkler(typo, cmd);
        if score > 0.7 {
            if best_match.is_none() || score > best_match.unwrap().1 {
                best_match = Some((cmd, score));
            }
        }
    }

    best_match.map(|(cmd, _)| format!("llmx {}", cmd))
}

/// Show search help with examples when no query is provided.
fn show_search_help(
    store: &mut IndexStore,
    cache: &mut DynamicCache,
    path: Option<&PathBuf>,
) -> Result<()> {
    use llmx_mcp::handlers::{find_project_root, has_project_marker};

    let cwd = std::env::current_dir().context("Could not get current directory")?;
    let root = path
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .unwrap_or_else(|| find_project_root(&cwd).unwrap_or(cwd.clone()));

    println!("llmx search - Semantic codebase search\n");
    println!("Directory: {}", root.display());

    // Check index status
    let indexed = if let Some(meta) = store.find_metadata_by_path(&root) {
        println!("Status: Indexed ({} files, {} chunks)", meta.file_count, meta.chunk_count);
        true
    } else if cache.get(&root).is_some() {
        println!("Status: Cached (in-memory)");
        true
    } else if has_project_marker(&root) {
        println!("Status: Not indexed (will auto-index on first search)");
        false
    } else {
        println!("Status: No project markers found");
        false
    };

    println!("\n--- Search Query Examples ---\n");
    println!("  Symbol lookup (function/class names):");
    println!("    llmx search getUserById");
    println!("    llmx search \"verify_token\"");
    println!("    llmx search \"auth::Claims\"");
    println!();
    println!("  Semantic questions (natural language):");
    println!("    llmx search \"how does authentication work\"");
    println!("    llmx search \"where is the database connection handled\"");
    println!("    llmx search \"error handling strategy\"");
    println!();
    println!("  Keyword grep (literal matches):");
    println!("    llmx search TODO");
    println!("    llmx search FIXME");
    println!("    llmx search \"unsafe block\"");
    println!();
    println!("--- Options ---\n");
    println!("  --limit N        Max results (default: 10)");
    println!("  --max-tokens N   Token budget for content (default: 8000)");
    println!("  --filter-path P  Filter by path prefix");
    println!("  --kind K         Filter by type: markdown, javascript, json, html, text");
    println!("  --strategy S     Search mode: auto, bm25, semantic, hybrid");
    println!("  --explain        Show why each result matched");

    if !indexed {
        println!("\nTip: Run any search query to auto-index this directory.");
    }

    Ok(())
}
