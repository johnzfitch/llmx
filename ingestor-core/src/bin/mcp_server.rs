use llmx_mcp::mcp::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_lookup_handler, llmx_manage_handler, llmx_refs_handler,
    llmx_search_handler, llmx_symbols_handler,
    run_index_work,
    ExploreInput, GetChunkInput, IndexInput, IndexStatsOutput, IndexStore, ManageInput, SearchInput,
    SymbolsInput, LookupInput, RefsInput,
    JobState, JobStatus, JobStore, new_job_id, new_job_store, active_job_count, MAX_CONCURRENT_JOBS,
};
use rmcp::handler::server::{router::tool::ToolRouter, tool::Parameters};
use rmcp::model::{ErrorData as McpError, *};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// MCP server for codebase indexing and semantic search.
///
/// Provides v2 tools for indexing, conceptual search, exact lookup, and graph traversal.
/// - `llmx_index`: Create/update codebase indexes from file paths
/// - `llmx_search`: Search with token-budgeted inline content (default 16K tokens)
/// - `llmx_lookup`: Exact or prefix symbol lookup using the persisted symbol table
/// - `llmx_refs`: Traverse callers/callees/imports/type references using the persisted edge index
/// - `llmx_get_chunk`: Fetch chunk content by ID/ref after lookup or refs
/// - `llmx_manage`: List or delete indexes
///
/// # Architecture
///
/// The server uses an `IndexStore` to manage persistent indexes on disk with an
/// in-memory cache for performance. Indexes are stored in the XDG data directory
/// (`~/.local/share/llmx/indexes/` on Linux) by default, configurable via `LLMX_STORAGE_DIR`.
///
/// # Thread Safety
///
/// The `IndexStore` is wrapped in `Arc<Mutex<>>` to enable shared access across
/// async tasks while maintaining interior mutability for the cache.
#[derive(Clone)]
struct LlmxServer {
    store: Arc<Mutex<IndexStore>>,
    jobs: JobStore,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl LlmxServer {
    fn new(store: Arc<Mutex<IndexStore>>, jobs: JobStore) -> Self {
        Self {
            store,
            jobs,
            tool_router: Self::tool_router(),
        }
    }

    /// Create or update a codebase index from file paths.
    /// Returns a job_id immediately; poll with llmx_manage(action='job_status').
    #[tool(description = "Create or update index from file paths. Returns job_id immediately; poll with llmx_manage(action='job_status', index_id='<job_id>')")]
    async fn llmx_index(
        &self,
        Parameters(input): Parameters<IndexInput>,
    ) -> Result<CallToolResult, McpError> {
        // Reject if too many jobs are already active
        if active_job_count(&self.jobs) >= MAX_CONCURRENT_JOBS {
            return Err(McpError::internal_error(
                format!("Too many active indexing jobs (max {MAX_CONCURRENT_JOBS}). Wait for existing jobs to complete."),
                None,
            ));
        }

        let job_id = new_job_id();

        self.jobs.lock()
            .map_err(|e| McpError::internal_error(format!("Job store lock poisoned: {e}"), None))?
            .insert(job_id.clone(), JobState::queued());

        let store = self.store.clone();
        let jobs = self.jobs.clone();
        let jid = job_id.clone();

        tokio::task::spawn_blocking(move || {
            // Update status to running
            if let Ok(mut jobs_guard) = jobs.lock() {
                if let Some(state) = jobs_guard.get_mut(&jid) {
                    state.status = JobStatus::Running;
                }
            }

            // Heavy work -- NO store lock held here
            let result = run_index_work(&input);

            let final_status = match result {
                Ok((index, root_path, _opts)) => {
                    let stats = IndexStatsOutput {
                        total_files: index.stats.total_files,
                        total_chunks: index.stats.total_chunks,
                        avg_chunk_tokens: index.stats.avg_chunk_tokens,
                    };
                    let warnings = index.warnings.len();
                    match store.lock() {
                        Ok(mut s) => match s.save(index, root_path) {
                            Ok(index_id) => JobStatus::Complete { index_id, stats, warnings },
                            Err(e) => JobStatus::Error { message: e.to_string() },
                        },
                        Err(e) => JobStatus::Error { message: format!("Store lock poisoned: {e}") },
                    }
                }
                Err(e) => JobStatus::Error { message: e.to_string() },
            };

            if let Ok(mut jobs_guard) = jobs.lock() {
                if let Some(state) = jobs_guard.get_mut(&jid) {
                    state.status = final_status;
                }
            }
        });

        let content = serde_json::to_string_pretty(&serde_json::json!({
            "job_id": job_id,
            "status": "queued",
            "message": "Indexing started. Poll with llmx_manage(action='job_status', index_id='<job_id>')."
        })).map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Search indexed codebase with inline content (token-budgeted)
    #[tool(description = "Search index with inline content")]
    async fn llmx_search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| {
                McpError::internal_error(
                    format!("IndexStore mutex poisoned - indicates a panic in a previous operation: {e}"),
                    None,
                )
            })?;
        let output = llmx_search_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Legacy structure exploration tool kept for compatibility.
    #[tool(description = "Legacy compatibility tool: explore index structure (files, outline, symbols, callers/callees/importers)")]
    async fn llmx_explore(
        &self,
        Parameters(input): Parameters<ExploreInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| {
                McpError::internal_error(
                    format!("IndexStore mutex poisoned - indicates a panic in a previous operation: {e}"),
                    None,
                )
            })?;
        let output = llmx_explore_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Legacy symbol lookup tool kept for compatibility.
    #[tool(description = "Legacy compatibility tool: symbol table lookup by glob-like pattern.")]
    async fn llmx_symbols(
        &self,
        Parameters(input): Parameters<SymbolsInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(
                format!("IndexStore mutex poisoned: {e}"), None,
            ))?;
        let output = llmx_symbols_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Exact or prefix symbol resolution by name. Use this for 'find function parseConfig' and similar symbol lookups.")]
    async fn llmx_lookup(
        &self,
        Parameters(input): Parameters<LookupInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(format!("IndexStore mutex poisoned: {e}"), None))?;
        let output = llmx_lookup_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Graph traversal for code structure: callers, callees, imports, importers, and type users.")]
    async fn llmx_refs(
        &self,
        Parameters(input): Parameters<RefsInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(format!("IndexStore mutex poisoned: {e}"), None))?;
        let output = llmx_refs_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Fetch the full chunk content by chunk ID, chunk ref, or ID prefix.")]
    async fn llmx_get_chunk(
        &self,
        Parameters(input): Parameters<GetChunkInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(format!("IndexStore mutex poisoned: {e}"), None))?;
        let output = llmx_get_chunk_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// List, delete, inspect stats for indexes, or check job status
    #[tool(description = "List indexes, delete an index, inspect index stats, or check job status (action='job_status', index_id='<job_id>')")]
    async fn llmx_manage(
        &self,
        Parameters(input): Parameters<ManageInput>,
    ) -> Result<CallToolResult, McpError> {
        // Handle job_status before taking the store lock
        if input.action == "job_status" {
            let job_id = input.index_id.as_deref()
                .ok_or_else(|| McpError::invalid_params("index_id (job_id) required for job_status", None))?;
            let jobs = self.jobs.lock()
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            let state = jobs.get(job_id)
                .ok_or_else(|| McpError::invalid_params(format!("Unknown job_id: {job_id}"), None))?;
            let content = serde_json::to_string_pretty(&state.status)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(content)]));
        }

        let mut store = self
            .store
            .lock()
            .map_err(|e| {
                McpError::internal_error(
                    format!("IndexStore mutex poisoned - indicates a panic in a previous operation: {e}"),
                    None,
                )
            })?;
        let output = llmx_manage_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }
}

#[tool_handler]
impl ServerHandler for LlmxServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "llmx_mcp-mcp".to_string(),
                version: "0.1.0".to_string(),
            },
            instructions: Some("Codebase indexing and semantic search with inline content. Supports indexing codebases, searching with token-budgeted results, exploring index structure, and managing indexes.".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging to stderr
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("llmx_mcp=info".parse()?),
        )
        .init();

    // Get storage directory from env or default
    let storage_dir = env::var("LLMX_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| llmx_mcp::default_storage_dir());

    tracing::info!("Starting LLMX MCP server, storage: {:?}", storage_dir);

    let store = IndexStore::new(storage_dir)?;
    let jobs = new_job_store();

    // Spawn cleanup task: remove completed/errored jobs older than 10 minutes
    let cleanup_jobs = jobs.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Ok(mut jobs) = cleanup_jobs.lock() {
                jobs.retain(|_, state| state.started_at.elapsed().as_secs() < 600);
            }
        }
    });

    let server = LlmxServer::new(Arc::new(Mutex::new(store)), jobs);

    // Run server with stdio transport
    tracing::info!("Server ready, listening on stdio");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
