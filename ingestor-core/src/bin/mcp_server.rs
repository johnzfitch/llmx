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
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify::event::ModifyKind;
use rmcp::service::{NotificationContext, Peer, RequestContext, RoleServer};
use std::collections::BTreeSet;
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use url::Url;

const STATUS_RESOURCE_URI: &str = "llmx://index/status";

#[derive(Default)]
struct WatchState {
    watcher: Option<RecommendedWatcher>,
    watched_roots: BTreeSet<PathBuf>,
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
struct LlmxServer {
    store: Arc<Mutex<IndexStore>>,
    jobs: JobStore,
    peer: Arc<Mutex<Option<Peer<RoleServer>>>>,
    watch_state: Arc<Mutex<WatchState>>,
    stale_paths: Arc<Mutex<BTreeSet<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl LlmxServer {
    fn new(store: Arc<Mutex<IndexStore>>, jobs: JobStore) -> Self {
        Self {
            store,
            jobs,
            peer: Arc::new(Mutex::new(None)),
            watch_state: Arc::new(Mutex::new(WatchState::default())),
            stale_paths: Arc::new(Mutex::new(BTreeSet::new())),
            tool_router: Self::tool_router(),
        }
    }

    fn status_snapshot(&self) -> Result<StatusOutput, McpError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(format!("IndexStore mutex poisoned: {e}"), None))?;
        let mut status = llmx_status_handler(&mut store, &self.jobs)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        status.stale_files = self
            .stale_paths
            .lock()
            .map(|paths| paths.len())
            .unwrap_or(0);
        Ok(status)
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
        let peer_slot = self.peer.clone();
        let store = self.store.clone();
        let stale_paths = self.stale_paths.clone();
        let runtime = tokio::runtime::Handle::current();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else { return };
            let peer = peer_slot.lock().ok().and_then(|slot| slot.clone());
            let Some(peer) = peer else { return };
            let changed_paths = event.paths.clone();
            let stale_markers = normalize_paths(&changed_paths);
            if let Ok(mut stale) = stale_paths.lock() {
                stale.extend(stale_markers);
            }

            let uris: Vec<String> = event
                .paths
                .iter()
                .filter_map(|path| file_path_to_uri(path))
                .collect();
            let should_list_changed = should_emit_resource_list_changed(&event);
            let store = store.clone();
            let stale_paths = stale_paths.clone();

            runtime.spawn(async move {
                let _ = peer
                    .notify_resource_updated(ResourceUpdatedNotificationParam {
                        uri: STATUS_RESOURCE_URI.to_string(),
                    })
                    .await;
                for uri in uris {
                    let _ = peer
                        .notify_resource_updated(ResourceUpdatedNotificationParam { uri })
                        .await;
                }
                if should_list_changed {
                    let _ = peer.notify_resource_list_changed().await;
                }

                let refresh = tokio::task::spawn_blocking(move || {
                    refresh_impacted_indexes(&store, &stale_paths, &changed_paths)
                })
                .await;

                match refresh {
                    Ok(Ok(changed)) if changed => {
                        let _ = peer
                            .notify_resource_updated(ResourceUpdatedNotificationParam {
                                uri: STATUS_RESOURCE_URI.to_string(),
                            })
                            .await;
                        let _ = peer.notify_tool_list_changed().await;
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(err)) => tracing::warn!("failed to refresh impacted indexes: {err}"),
                    Err(err) => tracing::warn!("refresh task join error: {err}"),
                }
            });
        })?;

        let mut watched_roots = BTreeSet::new();
        for root in &roots {
            watcher.watch(root, RecursiveMode::Recursive)?;
            watched_roots.insert(root.clone());
        }

        if let Ok(mut state) = self.watch_state.lock() {
            state.watcher = Some(watcher);
            state.watched_roots = watched_roots;
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
        let output = self.status_snapshot()?;
        let content = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
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

        let output = self.status_snapshot()?;
        let text = serde_json::to_string_pretty(&output)
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
                version: "0.3.0".to_string(),
            },
            instructions: Some("Structural code search and indexing MCP server. Use llmx_status first to inspect readiness and indexing quality, then query tools against the returned index state.".to_string()),
        }
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        self.sync_client_roots(context.peer).await;
    }

    async fn on_roots_list_changed(&self, context: NotificationContext<RoleServer>) {
        self.sync_client_roots(context.peer).await;
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

fn should_emit_resource_list_changed(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(_))
    )
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
                max_files: 100_000,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging to stderr
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("llmx_mcp=info".parse()?),
        )
        .init();

    // Parse --http <port> flag from args (no clap needed for one flag)
    let http_port = parse_http_port();

    // Get storage directory from env or default
    let storage_dir = env::var("LLMX_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| llmx_mcp::default_storage_dir());

    tracing::info!("Starting LLMX MCP server, storage: {:?}", storage_dir);

    let store = Arc::new(Mutex::new(IndexStore::new(storage_dir)?));
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

    if let Some(port) = http_port {
        serve_http(store, jobs, port).await
    } else {
        let server = LlmxServer::new(store, jobs);
        tracing::info!("Server ready, listening on stdio");
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}

fn parse_http_port() -> Option<u16> {
    let args: Vec<String> = env::args().collect();
    args.iter()
        .position(|a| a == "--http")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
}

#[cfg(feature = "mcp-http")]
async fn serve_http(
    store: Arc<Mutex<IndexStore>>,
    jobs: JobStore,
    port: u16,
) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService,
        session::local::LocalSessionManager,
    };
    use hyper_util::rt::TokioIo;
    use hyper_util::service::TowerToHyperService;
    use std::net::SocketAddr;

    let session_manager = Arc::new(LocalSessionManager::default());
    let config = StreamableHttpServerConfig::default();

    let svc = StreamableHttpService::new(
        move || Ok(LlmxServer::new(store.clone(), jobs.clone())),
        session_manager,
        config,
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Server ready, listening on http://127.0.0.1:{port}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let svc = TowerToHyperService::new(svc.clone());
        tokio::spawn(async move {
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, svc)
                .await
            {
                tracing::warn!("HTTP connection error: {err}");
            }
        });
    }
}

#[cfg(not(feature = "mcp-http"))]
async fn serve_http(
    _store: Arc<Mutex<IndexStore>>,
    _jobs: JobStore,
    _port: u16,
) -> anyhow::Result<()> {
    anyhow::bail!("HTTP transport requires the 'mcp-http' feature. Rebuild with: cargo build --release --features mcp-http")
}
