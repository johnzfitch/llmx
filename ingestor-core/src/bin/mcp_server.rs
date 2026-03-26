use clap::Parser;
use llmx_mcp::mcp::{
    llmx_explore_handler, llmx_get_chunk_handler, llmx_lookup_handler, llmx_manage_handler, llmx_refs_handler,
    llmx_search_handler, llmx_status_handler, llmx_symbols_handler,
    run_index_work,
    ExploreInput, GetChunkInput, IndexInput, IndexStatsOutput, IndexStore, ManageInput, SearchInput,
    SymbolsInput, LookupInput, RefsInput, StatusOutput,
    JobState, JobStatus, JobStore, new_job_id, new_job_store, active_job_count, MAX_CONCURRENT_JOBS,
};
use llmx_mcp::pathnorm::{normalize_root_path, relativize_path};
use llmx_mcp::walk::{collect_files, read_file, WalkConfig};
use llmx_mcp::{ingest_files_with_root, update_index_selective, IngestOptions};
use rmcp::handler::server::{router::tool::ToolRouter, tool::Parameters};
use rmcp::model::{ErrorData as McpError, *};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use rmcp::service::{NotificationContext, Peer, RequestContext, RoleServer};
use std::collections::BTreeSet;
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use url::Url;

/// LLMX MCP Server - Codebase indexing and semantic search
///
/// Provides tools for indexing codebases and searching with semantic understanding.
/// Designed for integration with Claude Code, Cursor, Codex, and other MCP clients.
///
/// # Environment Variables
///
/// - LLMX_STORAGE_DIR: Override index storage location (default: ~/.local/share/llmx/indexes)
///
/// # Example .mcp.json
///
/// ```json
/// {
///   "mcpServers": {
///     "llmx": {
///       "command": "llmx-mcp",
///       "args": ["--path", "/path/to/project"]
///     }
///   }
/// }
/// ```
#[derive(Parser)]
#[command(name = "llmx-mcp", version, about)]
struct Args {
    /// Paths to auto-index on startup
    #[arg(long = "path", short = 'p')]
    paths: Vec<PathBuf>,

    /// Override storage directory (default: ~/.local/share/llmx/indexes)
    /// Can also be set via LLMX_STORAGE_DIR environment variable.
    #[arg(long)]
    storage_dir: Option<PathBuf>,

    /// Run as a persistent REST backend on the specified port (default 19100).
    /// Other llmx-mcp instances connect to this backend automatically.
    #[arg(long)]
    serve: Option<u16>,
}

const STATUS_RESOURCE_URI: &str = "llmx://index/status";

/// Debounced file watcher shared by both stdio and --serve modes.
///
/// The notify callback only sends paths into an mpsc channel (instant, no locks).
/// A consumer task drains the channel after a 500ms quiet window and calls
/// `refresh_impacted_indexes` once with the full batch. In stdio mode an
/// optional MCP peer callback fires notifications after each refresh.
struct DebouncedWatcher {
    _watcher: RecommendedWatcher,
    watched_roots: BTreeSet<PathBuf>,
}

const DEBOUNCE_QUIET_MS: u64 = 500;

/// Callback invoked after a debounced refresh completes.
/// Stdio mode sends MCP notifications; backend mode is a no-op.
type PostRefreshFn = Box<dyn Fn(bool, &[PathBuf]) + Send + 'static>;

fn spawn_debounced_watcher(
    store: Arc<Mutex<IndexStore>>,
    stale_paths: Arc<Mutex<BTreeSet<String>>>,
    roots: Vec<PathBuf>,
    post_refresh: Option<PostRefreshFn>,
) -> anyhow::Result<DebouncedWatcher> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<PathBuf>>();

    let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Ok(event) = event else { return };
        if !event.paths.is_empty() {
            let _ = tx.send(event.paths);
        }
    })?;

    // Consumer task: debounce + batch refresh
    tokio::spawn(async move {
        loop {
            // Wait for first event
            let first = match rx.recv().await {
                Some(paths) => paths,
                None => break, // channel closed
            };
            let mut batch: BTreeSet<PathBuf> = first.into_iter().collect();

            // Drain more events during quiet window
            loop {
                match tokio::time::timeout(
                    Duration::from_millis(DEBOUNCE_QUIET_MS),
                    rx.recv(),
                )
                .await
                {
                    Ok(Some(paths)) => { batch.extend(paths); }
                    _ => break, // timeout or channel closed
                }
            }

            let changed_paths: Vec<PathBuf> = batch.into_iter().collect();
            let stale_markers = normalize_paths(&changed_paths);
            if let Ok(mut stale) = stale_paths.lock() {
                stale.extend(stale_markers);
            }

            let store2 = store.clone();
            let stale2 = stale_paths.clone();
            let changed2 = changed_paths.clone();
            let refresh = tokio::task::spawn_blocking(move || {
                refresh_impacted_indexes(&store2, &stale2, &changed2)
            })
            .await;

            let changed = match refresh {
                Ok(Ok(changed)) => changed,
                Ok(Err(err)) => {
                    tracing::warn!("debounced refresh failed: {err}");
                    false
                }
                Err(err) => {
                    tracing::warn!("debounced refresh join error: {err}");
                    false
                }
            };

            if let Some(ref cb) = post_refresh {
                cb(changed, &changed_paths);
            }
        }
    });

    let mut watched_roots = BTreeSet::new();
    let mut w = watcher;
    for root in &roots {
        if let Err(e) = w.watch(root, RecursiveMode::Recursive) {
            tracing::warn!("watcher: failed to watch {}: {e}", root.display());
        } else {
            watched_roots.insert(root.clone());
        }
    }
    if !watched_roots.is_empty() {
        tracing::info!("watcher: watching {} roots", watched_roots.len());
    }

    Ok(DebouncedWatcher {
        _watcher: w,
        watched_roots,
    })
}

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
enum DataSource {
    Local {
        store: Arc<Mutex<IndexStore>>,
        jobs: JobStore,
    },
    Remote(BackendClient),
}

#[derive(Clone)]
struct LlmxServer {
    data_source: DataSource,
    peer: Arc<Mutex<Option<Peer<RoleServer>>>>,
    watcher: Arc<Mutex<Option<DebouncedWatcher>>>,
    stale_paths: Arc<Mutex<BTreeSet<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl LlmxServer {
    fn new_local(store: Arc<Mutex<IndexStore>>, jobs: JobStore) -> Self {
        Self {
            data_source: DataSource::Local { store, jobs },
            peer: Arc::new(Mutex::new(None)),
            watcher: Arc::new(Mutex::new(None)),
            stale_paths: Arc::new(Mutex::new(BTreeSet::new())),
            tool_router: Self::tool_router(),
        }
    }

    fn new_remote(client: BackendClient) -> Self {
        Self {
            data_source: DataSource::Remote(client),
            peer: Arc::new(Mutex::new(None)),
            watcher: Arc::new(Mutex::new(None)),
            stale_paths: Arc::new(Mutex::new(BTreeSet::new())),
            tool_router: Self::tool_router(),
        }
    }

    fn status_snapshot(&self) -> Result<StatusOutput, McpError> {
        match &self.data_source {
            DataSource::Local { store, jobs } => {
                let mut store = store
                    .lock()
                    .map_err(|e| McpError::internal_error(format!("IndexStore mutex poisoned: {e}"), None))?;
                let mut status = llmx_status_handler(&mut store, jobs)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                status.stale_files = self
                    .stale_paths
                    .lock()
                    .map(|paths| paths.len())
                    .unwrap_or(0);
                Ok(status)
            }
            DataSource::Remote(_) => {
                Err(McpError::internal_error("status_snapshot not available in proxy mode", None))
            }
        }
    }

    fn remember_peer(&self, peer: Peer<RoleServer>) {
        if let Ok(mut slot) = self.peer.lock() {
            *slot = Some(peer);
        }
    }

    async fn sync_client_roots(&self, peer: Peer<RoleServer>) {
        self.remember_peer(peer.clone());

        let roots_result = peer.list_roots().await;
        let Ok(roots_result) = roots_result else {
            tracing::debug!("roots/list unavailable from client");
            return;
        };

        let root_paths = parse_root_paths(&roots_result.roots);
        if let Err(err) = self.replace_watch_roots(root_paths) {
            tracing::warn!("failed to refresh root watchers: {err}");
            return;
        }

        let _ = peer.notify_resource_list_changed().await;
        let _ = peer
            .notify_resource_updated(ResourceUpdatedNotificationParam {
                uri: STATUS_RESOURCE_URI.to_string(),
            })
            .await;
    }

    fn replace_watch_roots(&self, roots: Vec<PathBuf>) -> anyhow::Result<()> {
        let DataSource::Local { store, .. } = &self.data_source else {
            return Ok(()); // No-op in proxy mode
        };

        // Build MCP notification callback for stdio mode
        let peer_slot = self.peer.clone();
        let runtime = tokio::runtime::Handle::current();
        let post_refresh: PostRefreshFn = Box::new(move |changed, changed_paths: &[PathBuf]| {
            let peer = peer_slot.lock().ok().and_then(|slot| slot.clone());
            let Some(peer) = peer else { return };
            let uris: Vec<String> = changed_paths
                .iter()
                .filter_map(|path| file_path_to_uri(path))
                .collect();
            let has_structural = changed_paths.iter().any(|p| {
                // Approximate: treat creates/removes as structural
                !p.exists() || p.is_dir()
            });
            runtime.spawn(async move {
                let _ = peer
                    .notify_resource_updated(ResourceUpdatedNotificationParam {
                        uri: STATUS_RESOURCE_URI.to_string(),
                    })
                    .await;
                for uri in &uris {
                    let _ = peer
                        .notify_resource_updated(ResourceUpdatedNotificationParam {
                            uri: uri.clone(),
                        })
                        .await;
                }
                if has_structural {
                    let _ = peer.notify_resource_list_changed().await;
                }
                if changed {
                    let _ = peer.notify_tool_list_changed().await;
                }
            });
        });

        let new_watcher = spawn_debounced_watcher(
            store.clone(),
            self.stale_paths.clone(),
            roots.clone(),
            Some(post_refresh),
        )?;

        if let Ok(mut slot) = self.watcher.lock() {
            *slot = Some(new_watcher);
        }

        if let Ok(mut stale) = self.stale_paths.lock() {
            stale.retain(|path| roots.iter().any(|root| PathBuf::from(path).starts_with(root)));
        }

        Ok(())
    }

    /// Create or update a codebase index from file paths.
    /// Returns a job_id immediately; poll with llmx_manage(action='job_status').
    #[tool(description = "Create or update index from file paths. Returns job_id immediately; poll with llmx_manage(action='job_status', index_id='<job_id>')")]
    async fn llmx_index(
        &self,
        Parameters(mut input): Parameters<IndexInput>,
    ) -> Result<CallToolResult, McpError> {
        if let DataSource::Remote(client) = &self.data_source {
            // Resolve relative paths against client cwd so the backend
            // doesn't canonicalize them against its own working directory.
            input.paths = input.paths.into_iter().map(|p| {
                let path = PathBuf::from(&p);
                if path.is_relative() {
                    if let Ok(cwd) = env::current_dir() {
                        return cwd.join(&path).to_string_lossy().to_string();
                    }
                }
                p
            }).collect();
            let result = client.index(&input).await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            let content = serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(content)]));
        }
        let DataSource::Local { store, jobs } = &self.data_source else { unreachable!() };

        if active_job_count(jobs) >= MAX_CONCURRENT_JOBS {
            return Err(McpError::internal_error(
                format!("Too many active indexing jobs (max {MAX_CONCURRENT_JOBS}). Wait for existing jobs to complete."),
                None,
            ));
        }

        let job_id = new_job_id();

        jobs.lock()
            .map_err(|e| McpError::internal_error(format!("Job store lock poisoned: {e}"), None))?
            .insert(job_id.clone(), JobState::queued());

        let store = store.clone();
        let jobs = jobs.clone();
        let peer = self.peer.clone();
        let jid = job_id.clone();
        let runtime = tokio::runtime::Handle::current();

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

            let peer = peer.lock().ok().and_then(|slot| slot.clone());
            if let Some(peer) = peer {
                runtime.spawn(async move {
                    let _ = peer
                        .notify_resource_updated(ResourceUpdatedNotificationParam {
                            uri: STATUS_RESOURCE_URI.to_string(),
                        })
                        .await;
                    let _ = peer.notify_tool_list_changed().await;
                });
            }
        });

        let content = serde_json::to_string_pretty(&serde_json::json!({
            "job_id": job_id,
            "status": "queued",
            "message": "Indexing started. Poll with llmx_manage(action='job_status', index_id='<job_id>')."
        })).map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Report index readiness tier, indexed file counts, symbols, languages, and background task progress.")]
    async fn llmx_status(&self) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Local { .. } => {
                let output = self.status_snapshot()?;
                serde_json::to_string_pretty(&output)
            }
            DataSource::Remote(client) => {
                let output = client.status().await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Search index with inline content")]
    async fn llmx_search(
        &self,
        Parameters(mut input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.search(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_search_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Legacy compatibility tool: explore index structure (files, outline, symbols, callers/callees/importers)")]
    async fn llmx_explore(
        &self,
        Parameters(mut input): Parameters<ExploreInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.explore(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_explore_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Legacy compatibility tool: symbol table lookup by glob-like pattern.")]
    async fn llmx_symbols(
        &self,
        Parameters(mut input): Parameters<SymbolsInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.symbols(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_symbols_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Exact or prefix symbol resolution by name. Use this for 'find function parseConfig' and similar symbol lookups.")]
    async fn llmx_lookup(
        &self,
        Parameters(mut input): Parameters<LookupInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.lookup(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_lookup_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Graph traversal for code structure: callers, callees, imports, importers, and type users.")]
    async fn llmx_refs(
        &self,
        Parameters(mut input): Parameters<RefsInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.refs(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_refs_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Fetch the full chunk content by chunk ID, chunk ref, or ID prefix.")]
    async fn llmx_get_chunk(
        &self,
        Parameters(mut input): Parameters<GetChunkInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.get_chunk(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, .. } => {
                let mut store = store.lock()
                    .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                let output = llmx_get_chunk_handler(&mut store, input)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "List indexes, delete an index, inspect index stats, or check job status (action='job_status', index_id='<job_id>')")]
    async fn llmx_manage(
        &self,
        Parameters(mut input): Parameters<ManageInput>,
    ) -> Result<CallToolResult, McpError> {
        let content = match &self.data_source {
            DataSource::Remote(client) => {
                fill_loc_from_cwd(&mut input.loc);
                let result = client.manage(&input).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&result)
            }
            DataSource::Local { store, jobs } => {
                if input.action == "job_status" {
                    let job_id = input.index_id.as_deref()
                        .ok_or_else(|| McpError::invalid_params("index_id (job_id) required for job_status", None))?;
                    let jobs = jobs.lock()
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    let state = jobs.get(job_id)
                        .ok_or_else(|| McpError::invalid_params(format!("Unknown job_id: {job_id}"), None))?;
                    serde_json::to_string_pretty(&state.status)
                } else {
                    let mut store = store.lock()
                        .map_err(|e| McpError::internal_error(format!("lock poisoned: {e}"), None))?;
                    let output = llmx_manage_handler(&mut store, input)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    serde_json::to_string_pretty(&output)
                }
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }
}

#[tool_handler]
impl ServerHandler for LlmxServer {
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![RawResource {
                uri: STATUS_RESOURCE_URI.to_string(),
                name: "Index Status".to_string(),
                description: Some("Current llmx readiness tier and indexing status".to_string()),
                mime_type: Some("application/json".to_string()),
                size: None,
            }
            .no_annotation()],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri != STATUS_RESOURCE_URI {
            return Err(McpError::invalid_params(
                format!("Unknown resource URI: {}", request.uri),
                None,
            ));
        }

        let text = match &self.data_source {
            DataSource::Local { .. } => {
                let output = self.status_snapshot()?;
                serde_json::to_string_pretty(&output)
            }
            DataSource::Remote(client) => {
                let output = client.status().await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                serde_json::to_string_pretty(&output)
            }
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::TextResourceContents {
                uri: STATUS_RESOURCE_URI.to_string(),
                mime_type: Some("application/json".to_string()),
                text,
            }],
        })
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .enable_resources()
                .enable_resources_list_changed()
                .build(),
            server_info: Implementation {
                name: "llmx".to_string(),
                version: "2.1.0".to_string(),
            },
            instructions: Some("Structural code search and indexing MCP server. Use llmx_status first to inspect readiness and indexing quality, then query tools against the returned index state.".to_string()),
        }
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        match &self.data_source {
            DataSource::Local { .. } => self.sync_client_roots(context.peer).await,
            DataSource::Remote(client) => {
                self.remember_peer(context.peer.clone());
                if let Ok(roots_result) = context.peer.list_roots().await {
                    let root_paths = parse_root_paths(&roots_result.roots);
                    let root_uris: Vec<String> = root_paths
                        .iter()
                        .filter_map(|p| file_path_to_uri(p))
                        .collect();
                    let _ = client.post_roots(root_uris).await;
                }
            }
        }
    }

    async fn on_roots_list_changed(&self, context: NotificationContext<RoleServer>) {
        match &self.data_source {
            DataSource::Local { .. } => self.sync_client_roots(context.peer).await,
            DataSource::Remote(client) => {
                self.remember_peer(context.peer.clone());
                if let Ok(roots_result) = context.peer.list_roots().await {
                    let root_paths = parse_root_paths(&roots_result.roots);
                    let root_uris: Vec<String> = root_paths
                        .iter()
                        .filter_map(|p| file_path_to_uri(p))
                        .collect();
                    let _ = client.post_roots(root_uris).await;
                }
            }
        }
    }
}

fn parse_root_paths(roots: &[Root]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| Url::parse(&root.uri).ok())
        .filter_map(|uri| uri.to_file_path().ok())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn file_path_to_uri(path: &std::path::Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

fn normalize_paths(paths: &[PathBuf]) -> Vec<String> {
    let mut normalized: Vec<String> = paths
        .iter()
        .map(|path| normalize_root_path(path))
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn refresh_impacted_indexes(
    store: &Arc<Mutex<IndexStore>>,
    stale_paths: &Arc<Mutex<BTreeSet<String>>>,
    changed_paths: &[PathBuf],
) -> anyhow::Result<bool> {
    let normalized_changed = normalize_paths(changed_paths);
    let mut store = store
        .lock()
        .map_err(|e| anyhow::anyhow!("IndexStore mutex poisoned: {e}"))?;

    let indexes = store.list()?;
    let mut any_changed = false;
    let mut refreshed_roots = Vec::new();

    for metadata in indexes {
        let root = PathBuf::from(&metadata.root_path);
        let impacted: Vec<PathBuf> = changed_paths
            .iter()
            .filter(|path| path.starts_with(&root))
            .cloned()
            .collect();
        if impacted.is_empty() {
            continue;
        }

        let Some(index_id) = store.find_by_path(&root) else {
            continue;
        };

        let existing = store.load(&index_id)?.clone();
        let mut files = Vec::new();
        let mut changed_relative = Vec::new();
        let mut requires_full_refresh = false;

        for path in &impacted {
            if path.is_file() {
                if let Some(file) = read_file(path, &root)? {
                    changed_relative.push(file.path.clone());
                    files.push(file);
                } else {
                    changed_relative.push(relativize_path(path, &root));
                }
            } else {
                requires_full_refresh = true;
            }
        }

        let updated = if requires_full_refresh {
            let walk_config = WalkConfig {
                max_depth: 50,
                max_files: 200_000,
                max_total_bytes: usize::MAX,
                timeout_secs: 300,
                respect_gitignore: true,
            };
            let (all_files, _) = collect_files(&root, &root, &walk_config)?;
            ingest_files_with_root(all_files, IngestOptions::default(), Some(root.as_path()))
        } else {
            let changed_relative_set: BTreeSet<String> = changed_relative.iter().cloned().collect();
            let keep_paths = existing
                .files
                .iter()
                .map(|file| file.path.clone())
                .filter(|path| !changed_relative_set.contains(path))
                .collect();
            update_index_selective(existing, files, keep_paths, IngestOptions::default())
        };

        store.save(updated, metadata.root_path.clone())?;
        any_changed = true;
        refreshed_roots.push(root);
    }

    if any_changed {
        let refreshed_paths: BTreeSet<String> = refreshed_roots
            .into_iter()
            .map(|root| normalize_root_path(&root))
            .collect();
        if let Ok(mut stale) = stale_paths.lock() {
            stale.retain(|path| {
                !normalized_changed.iter().any(|changed| changed == path)
                    && !refreshed_paths.iter().any(|root| path.starts_with(root))
            });
        }
    }

    Ok(any_changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmx_mcp::{Chunk, ChunkKind, EdgeIndex, FileMeta, IndexFile, IndexStats, ResolutionTier, LanguageId, INDEX_VERSION};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn test_parse_root_paths_filters_non_file_uris() {
        let roots = vec![
            Root { uri: "file:///tmp/project".to_string(), name: None },
            Root { uri: "https://example.com/not-a-file".to_string(), name: None },
        ];
        let parsed = parse_root_paths(&roots);
        assert_eq!(parsed, vec![PathBuf::from("/tmp/project")]);
    }

    #[test]
    fn test_file_path_to_uri_round_trip() {
        let uri = file_path_to_uri(std::path::Path::new("/tmp/project/src/lib.rs"))
            .expect("file uri");
        assert_eq!(uri, "file:///tmp/project/src/lib.rs");
    }

    fn make_test_index(root: &str, relative_path: &str) -> IndexFile {
        IndexFile {
            version: INDEX_VERSION,
            index_id: "watch-test".to_string(),
            files: vec![FileMeta {
                path: relative_path.to_string(),
                root_path: root.to_string(),
                relative_path: relative_path.to_string(),
                kind: ChunkKind::Unknown,
                language: Some(LanguageId::Rust),
                bytes: 12,
                sha256: "abc".to_string(),
                line_count: 1,
                is_generated: false,
                resolution_tier: ResolutionTier::GenericTreeSitter,
                mtime_ms: None,
                fingerprint_sha256: None,
            }],
            chunks: vec![Chunk {
                id: "chunk-1".to_string(),
                short_id: "chunk-1".to_string(),
                slug: "chunk-1".to_string(),
                path: relative_path.to_string(),
                root_path: root.to_string(),
                relative_path: relative_path.to_string(),
                kind: ChunkKind::Unknown,
                language: Some(LanguageId::Rust),
                chunk_index: 0,
                start_line: 1,
                end_line: 1,
                content: "fn main() {}".to_string(),
                content_hash: "hash".to_string(),
                token_estimate: 4,
                heading_path: vec![],
                symbol: Some("main".to_string()),
                address: None,
                asset_path: None,
                is_generated: false,
                quality_score: None,
                resolution_tier: ResolutionTier::GenericTreeSitter,
                ast_kind: None,
                qualified_name: None,
                symbol_id: None,
                symbol_tail: None,
                signature: None,
                module_path: None,
                parent_symbol: None,
                visibility: None,
                imports: Vec::new(),
                exports: Vec::new(),
                calls: Vec::new(),
                type_refs: Vec::new(),
                doc_summary: None,
            }],
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
            stats: IndexStats {
                total_files: 1,
                total_chunks: 1,
                avg_chunk_chars: 12,
                avg_chunk_tokens: 4,
            },
            warnings: vec![],
            embeddings: None,
            embedding_model: None,
            symbols: BTreeMap::new(),
            edges: EdgeIndex::default(),
        }
    }

    #[test]
    fn test_refresh_impacted_indexes_updates_changed_file_and_clears_stale() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let file = root.join("src.rs");
        std::fs::write(&file, "fn updated() {}\n").unwrap();

        let store = Arc::new(Mutex::new(IndexStore::new(temp.path().join("store")).unwrap()));
        store
            .lock()
            .unwrap()
            .save(
                make_test_index(&normalize_root_path(root), "src.rs"),
                normalize_root_path(root),
            )
            .unwrap();

        let stale_paths = Arc::new(Mutex::new(BTreeSet::from([normalize_root_path(&file)])));
        let changed = refresh_impacted_indexes(&store, &stale_paths, std::slice::from_ref(&file)).unwrap();
        assert!(changed);

        let index_id = store.lock().unwrap().find_by_path(root).unwrap();
        let loaded = store.lock().unwrap().load(&index_id).unwrap().clone();
        assert_eq!(loaded.files[0].path, "src.rs");
        assert!(loaded.chunks[0].content.contains("updated"));
        assert!(stale_paths.lock().unwrap().is_empty());
    }
}

const DEFAULT_SERVE_PORT: u16 = 19100;

/// Fill in `loc` with the proxy process's cwd when unset, so the backend
/// resolves indexes against the client's working directory rather than its own.
fn fill_loc_from_cwd(loc: &mut Option<String>) {
    if loc.is_none() {
        if let Ok(cwd) = env::current_dir() {
            *loc = Some(cwd.to_string_lossy().to_string());
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_storage_dir(args_dir: Option<PathBuf>) -> PathBuf {
    args_dir
        .or_else(|| env::var("LLMX_STORAGE_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(llmx_mcp::default_storage_dir)
}

fn auto_index_paths(store: &Arc<Mutex<IndexStore>>, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    let path_strings: Vec<String> = paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()).to_string_lossy().to_string())
        .collect();
    tracing::info!("Auto-indexing {} paths: {:?}", path_strings.len(), path_strings);
    let input = IndexInput { paths: path_strings, options: None };
    match run_index_work(&input) {
        Ok((index, root_path, _)) => {
            let mut store_guard = store.lock().unwrap();
            match store_guard.save(index, root_path.clone()) {
                Ok(index_id) => tracing::info!("Auto-indexed {} as {}", root_path, index_id),
                Err(e) => tracing::warn!("Failed to save auto-index: {}", e),
            }
        }
        Err(e) => tracing::warn!("Failed to auto-index: {}", e),
    }
}

fn spawn_job_cleanup(jobs: JobStore) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Ok(mut jobs) = jobs.lock() {
                jobs.retain(|_, state| state.started_at.elapsed().as_secs() < 600);
            }
        }
    });
}

/// Spawn a background loop for --serve backend mode that discovers indexed
/// roots and watches them via the shared `spawn_debounced_watcher`. No MCP
/// peer notifications -- proxy sessions see fresh data on their next request.
fn spawn_backend_watcher(
    store: Arc<Mutex<IndexStore>>,
    stale_paths: Arc<Mutex<BTreeSet<String>>>,
) {
    tokio::spawn(async move {
        let mut current_watcher: Option<DebouncedWatcher> = None;

        loop {
            let indexed_roots: Vec<PathBuf> = store
                .lock()
                .ok()
                .and_then(|s| s.list().ok())
                .unwrap_or_default()
                .into_iter()
                .map(|m| PathBuf::from(&m.root_path))
                .filter(|p| p.exists())
                .collect();

            let need_update = match &current_watcher {
                None => !indexed_roots.is_empty(),
                Some(w) => {
                    let old: BTreeSet<_> = w.watched_roots.iter().collect();
                    let new: BTreeSet<_> = indexed_roots.iter().collect();
                    old != new
                }
            };

            if need_update {
                match spawn_debounced_watcher(
                    store.clone(),
                    stale_paths.clone(),
                    indexed_roots,
                    None, // no MCP notifications in backend mode
                ) {
                    Ok(w) => { current_watcher = Some(w); }
                    Err(e) => { tracing::warn!("backend watcher setup failed: {e}"); }
                }
            }

            // Re-scan every 30s for newly indexed roots
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

// ─── BackendClient ───────────────────────────────────────────────────────────

#[cfg(feature = "mcp-http")]
#[derive(Clone)]
struct BackendClient {
    client: hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        http_body_util::Full<bytes::Bytes>,
    >,
    base_url: String,
}

#[cfg(feature = "mcp-http")]
impl BackendClient {
    fn new(port: u16) -> Self {
        let client = hyper_util::client::legacy::Client::builder(
            hyper_util::rt::TokioExecutor::new(),
        )
        .build_http();
        Self {
            client,
            base_url: format!("http://127.0.0.1:{port}"),
        }
    }

    async fn health_check(&self) -> bool {
        let check = async {
            let resp: serde_json::Value = self.get("/api/health").await?;
            Ok::<bool, anyhow::Error>(resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
        };
        tokio::time::timeout(Duration::from_millis(500), check)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        use http_body_util::BodyExt;
        let uri: hyper::Uri = format!("{}{}", self.base_url, path).parse()?;
        let req = hyper::Request::get(uri)
            .body(http_body_util::Full::new(bytes::Bytes::new()))?;
        let resp = self.client.request(req).await?;
        let status = resp.status();
        let body = resp.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            let err_msg = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("error")?.as_str().map(String::from))
                .unwrap_or_else(|| format!("Backend returned {status}"));
            anyhow::bail!(err_msg);
        }
        Ok(serde_json::from_slice(&body)?)
    }

    async fn post<I: serde::Serialize, O: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        input: &I,
    ) -> anyhow::Result<O> {
        use http_body_util::BodyExt;
        let uri: hyper::Uri = format!("{}{}", self.base_url, path).parse()?;
        let body = serde_json::to_vec(input)?;
        let req = hyper::Request::post(uri)
            .header("content-type", "application/json")
            .body(http_body_util::Full::new(bytes::Bytes::from(body)))?;
        let resp = self.client.request(req).await?;
        let status = resp.status();
        let body = resp.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            let err_msg = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("error")?.as_str().map(String::from))
                .unwrap_or_else(|| format!("Backend returned {status}"));
            anyhow::bail!(err_msg);
        }
        Ok(serde_json::from_slice(&body)?)
    }

    async fn status(&self) -> anyhow::Result<StatusOutput> {
        self.get("/api/status").await
    }

    async fn search(&self, input: &SearchInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/search", input).await
    }

    async fn index(&self, input: &IndexInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/index", input).await
    }

    async fn explore(&self, input: &ExploreInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/explore", input).await
    }

    async fn manage(&self, input: &ManageInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/manage", input).await
    }

    async fn symbols(&self, input: &SymbolsInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/symbols", input).await
    }

    async fn lookup(&self, input: &LookupInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/lookup", input).await
    }

    async fn refs(&self, input: &RefsInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/refs", input).await
    }

    async fn get_chunk(&self, input: &GetChunkInput) -> anyhow::Result<serde_json::Value> {
        self.post("/api/get_chunk", input).await
    }

    async fn post_roots(&self, roots: Vec<String>) -> anyhow::Result<()> {
        let _: serde_json::Value =
            self.post("/api/roots", &serde_json::json!({ "roots": roots })).await?;
        Ok(())
    }
}

#[cfg(not(feature = "mcp-http"))]
#[derive(Clone)]
struct BackendClient;

#[cfg(not(feature = "mcp-http"))]
impl BackendClient {
    fn new(_port: u16) -> Self { Self }
    async fn health_check(&self) -> bool { false }
}

// ─── Auto-start ──────────────────────────────────────────────────────────────

async fn detect_or_start_backend(port: u16, storage_dir: Option<&PathBuf>) -> Option<BackendClient> {
    let client = BackendClient::new(port);

    if client.health_check().await {
        tracing::info!("Connected to existing llmx backend on port {port}");
        return Some(client);
    }

    if env::var("LLMX_NO_AUTOSTART").map(|v| v == "1").unwrap_or(false) {
        tracing::info!("Backend not found, auto-start disabled (LLMX_NO_AUTOSTART=1)");
        return None;
    }

    tracing::info!("No backend found, auto-starting llmx-mcp --serve {port}");
    let exe = match env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            tracing::warn!("Cannot determine own executable path: {e}");
            return None;
        }
    };

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--serve")
        .arg(port.to_string());
    if let Some(dir) = storage_dir {
        cmd.arg("--storage-dir").arg(dir);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    match cmd.spawn() {
        Ok(_) => {}
        Err(e) => {
            tracing::warn!("Failed to spawn backend: {e}");
            return None;
        }
    }

    for i in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if client.health_check().await {
            tracing::info!(
                "Backend started successfully on port {port} (took ~{}ms)",
                (i + 1) * 100
            );
            return Some(client);
        }
    }

    tracing::warn!("Backend failed to start within 5s, falling back to standalone");
    None
}

// ─── REST Backend ────────────────────────────────────────────────────────────

#[cfg(feature = "mcp-http")]
async fn serve_rest(
    store: Arc<Mutex<IndexStore>>,
    jobs: JobStore,
    stale_paths: Arc<Mutex<BTreeSet<String>>>,
    port: u16,
) -> anyhow::Result<()> {
    use hyper::body::Incoming;
    use hyper::{Method, Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use http_body_util::{BodyExt, Full};
    use bytes::Bytes;
    use std::net::SocketAddr;

    fn json_ok<T: serde::Serialize>(value: &T) -> Response<Full<Bytes>> {
        let body = serde_json::to_vec(value).unwrap_or_default();
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))
            .unwrap()
    }

    fn json_err(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
        let body =
            serde_json::to_vec(&serde_json::json!({ "error": message })).unwrap_or_default();
        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))
            .unwrap()
    }

    async fn read_body(req: Request<Incoming>) -> Result<Bytes, Response<Full<Bytes>>> {
        req.collect()
            .await
            .map(|collected| collected.to_bytes())
            .map_err(|_| json_err(StatusCode::BAD_REQUEST, "Failed to read request body"))
    }

    async fn handle(
        req: Request<Incoming>,
        store: Arc<Mutex<IndexStore>>,
        jobs: JobStore,
        stale_paths: Arc<Mutex<BTreeSet<String>>>,
    ) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        let response = match (method, path.as_str()) {
            (Method::GET, "/api/health") => json_ok(&serde_json::json!({ "ok": true })),

            (Method::GET, "/api/status") => {
                match store.lock() {
                    Ok(mut s) => match llmx_status_handler(&mut s, &jobs) {
                        Ok(mut status) => {
                            status.stale_files =
                                stale_paths.lock().map(|p| p.len()).unwrap_or(0);
                            json_ok(&status)
                        }
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/search") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: SearchInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_search_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/index") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: IndexInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                if active_job_count(&jobs) >= MAX_CONCURRENT_JOBS {
                    return Ok(json_err(
                        StatusCode::TOO_MANY_REQUESTS,
                        &format!("Too many active jobs (max {MAX_CONCURRENT_JOBS})"),
                    ));
                }
                let job_id = new_job_id();
                if let Ok(mut j) = jobs.lock() {
                    j.insert(job_id.clone(), JobState::queued());
                }
                let store2 = store.clone();
                let jobs2 = jobs.clone();
                let jid = job_id.clone();
                tokio::task::spawn_blocking(move || {
                    if let Ok(mut j) = jobs2.lock() {
                        if let Some(s) = j.get_mut(&jid) {
                            s.status = JobStatus::Running;
                        }
                    }
                    let result = run_index_work(&input);
                    let final_status = match result {
                        Ok((index, root_path, _)) => {
                            let stats = IndexStatsOutput {
                                total_files: index.stats.total_files,
                                total_chunks: index.stats.total_chunks,
                                avg_chunk_tokens: index.stats.avg_chunk_tokens,
                            };
                            let warnings = index.warnings.len();
                            match store2.lock() {
                                Ok(mut s) => match s.save(index, root_path) {
                                    Ok(index_id) => {
                                        JobStatus::Complete { index_id, stats, warnings }
                                    }
                                    Err(e) => JobStatus::Error { message: e.to_string() },
                                },
                                Err(e) => {
                                    JobStatus::Error { message: format!("Store lock: {e}") }
                                }
                            }
                        }
                        Err(e) => JobStatus::Error { message: e.to_string() },
                    };
                    if let Ok(mut j) = jobs2.lock() {
                        if let Some(s) = j.get_mut(&jid) {
                            s.status = final_status;
                        }
                    }
                });
                json_ok(&serde_json::json!({
                    "job_id": job_id,
                    "status": "queued",
                    "message": "Indexing started. Poll with llmx_manage(action='job_status')."
                }))
            }

            (Method::POST, "/api/explore") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: ExploreInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_explore_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/manage") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: ManageInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                if input.action == "job_status" {
                    let job_id = input.index_id.as_deref().unwrap_or("");
                    match jobs.lock() {
                        Ok(j) => match j.get(job_id) {
                            Some(state) => json_ok(&state.status),
                            None => json_err(
                                StatusCode::NOT_FOUND,
                                &format!("Unknown job_id: {job_id}"),
                            ),
                        },
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    }
                } else {
                    match store.lock() {
                        Ok(mut s) => match llmx_manage_handler(&mut s, input) {
                            Ok(output) => json_ok(&output),
                            Err(e) => {
                                json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
                            }
                        },
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    }
                }
            }

            (Method::POST, "/api/symbols") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: SymbolsInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_symbols_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/lookup") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: LookupInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_lookup_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/refs") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: RefsInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_refs_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/get_chunk") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let input: GetChunkInput = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                match store.lock() {
                    Ok(mut s) => match llmx_get_chunk_handler(&mut s, input) {
                        Ok(output) => json_ok(&output),
                        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                    },
                    Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                }
            }

            (Method::POST, "/api/roots") => {
                let body = match read_body(req).await {
                    Ok(b) => b,
                    Err(r) => return Ok(r),
                };
                let parsed: serde_json::Value = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(e) => return Ok(json_err(StatusCode::BAD_REQUEST, &e.to_string())),
                };
                let roots: Vec<String> = parsed
                    .get("roots")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                tracing::info!("Received {} root paths from proxy", roots.len());
                // Auto-index root paths that aren't already indexed
                let paths_to_index: Vec<PathBuf> = roots.iter()
                    .filter_map(|uri| url::Url::parse(uri).ok())
                    .filter_map(|u| u.to_file_path().ok())
                    .filter(|p| {
                        store.lock().ok()
                            .map(|s| s.find_by_path(p).is_none())
                            .unwrap_or(true)
                    })
                    .collect();
                if !paths_to_index.is_empty() {
                    let store2 = store.clone();
                    tokio::task::spawn_blocking(move || {
                        auto_index_paths(&store2, &paths_to_index);
                    });
                }
                json_ok(&serde_json::json!({ "ok": true, "roots": roots.len() }))
            }

            _ => json_err(StatusCode::NOT_FOUND, "Not Found"),
        };

        Ok(response)
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("REST backend ready on http://127.0.0.1:{port}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let store = store.clone();
        let jobs = jobs.clone();
        let stale_paths = stale_paths.clone();

        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |req| {
                handle(req, store.clone(), jobs.clone(), stale_paths.clone())
            });
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::warn!("HTTP connection error: {err}");
            }
        });
    }
}

#[cfg(not(feature = "mcp-http"))]
async fn serve_rest(
    _store: Arc<Mutex<IndexStore>>,
    _jobs: JobStore,
    _stale_paths: Arc<Mutex<BTreeSet<String>>>,
    _port: u16,
) -> anyhow::Result<()> {
    anyhow::bail!("REST backend requires the 'mcp-http' feature")
}

// ─── main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("llmx_mcp=info".parse()?),
        )
        .init();

    let args = Args::parse();

    // --serve mode: run REST backend
    if let Some(port) = args.serve {
        let storage_dir = resolve_storage_dir(args.storage_dir);
        tracing::info!("Starting LLMX REST backend, storage: {:?}", storage_dir);

        let store = Arc::new(Mutex::new(IndexStore::new(storage_dir)?));
        let jobs = new_job_store();
        let stale_paths = Arc::new(Mutex::new(BTreeSet::new()));

        auto_index_paths(&store, &args.paths);
        spawn_job_cleanup(jobs.clone());
        spawn_backend_watcher(store.clone(), stale_paths.clone());

        return serve_rest(store, jobs, stale_paths, port).await;
    }

    // Stdio mode: try backend, then fallback to standalone
    let port = env::var("LLMX_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SERVE_PORT);

    let server = if let Some(client) = detect_or_start_backend(port, args.storage_dir.as_ref()).await {
        tracing::info!("Running in proxy mode (backend on port {port})");
        // Forward --path args to the backend for indexing
        if !args.paths.is_empty() {
            let path_strings: Vec<String> = args.paths.iter()
                .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()).to_string_lossy().to_string())
                .collect();
            tracing::info!("Forwarding {} --path args to backend", path_strings.len());
            let input = IndexInput { paths: path_strings, options: None };
            if let Err(e) = client.index(&input).await {
                tracing::warn!("Failed to forward --path to backend: {e}");
            }
        }
        LlmxServer::new_remote(client)
    } else {
        tracing::info!("Running in standalone mode");
        let storage_dir = resolve_storage_dir(args.storage_dir);
        let store = Arc::new(Mutex::new(IndexStore::new(storage_dir)?));
        let jobs = new_job_store();

        auto_index_paths(&store, &args.paths);
        spawn_job_cleanup(jobs.clone());

        LlmxServer::new_local(store, jobs)
    };

    tracing::info!("Server ready, listening on stdio");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
