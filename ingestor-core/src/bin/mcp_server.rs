use ingestor_core::mcp::{
    llmx_explore_handler, llmx_index_handler, llmx_manage_handler, llmx_search_handler,
    ExploreInput, IndexInput, IndexStore, ManageInput, SearchInput,
};
use rmcp::handler::server::{router::tool::ToolRouter, tool::Parameters};
use rmcp::model::{ErrorData as McpError, *};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;

/// MCP server for codebase indexing and semantic search.
///
/// Provides four tools:
/// - `llmx_index`: Create/update codebase indexes from file paths
/// - `llmx_search`: Search with token-budgeted inline content (default 16K tokens)
/// - `llmx_explore`: List files, outline headings, or symbols in an index
/// - `llmx_manage`: List or delete indexes
///
/// # Architecture
///
/// The server uses an `IndexStore` to manage persistent indexes on disk with an
/// in-memory cache for performance. All indexes are stored in `~/.llmx/indexes/`
/// by default (configurable via `LLMX_STORAGE_DIR`).
///
/// # Thread Safety
///
/// The `IndexStore` is wrapped in `Arc<Mutex<>>` to enable shared access across
/// async tasks while maintaining interior mutability for the cache.
#[derive(Clone)]
struct LlmxServer {
    store: Arc<Mutex<IndexStore>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl LlmxServer {
    fn new(store: Arc<Mutex<IndexStore>>) -> Self {
        Self {
            store,
            tool_router: Self::tool_router(),
        }
    }

    /// Create or update a codebase index from file paths
    #[tool(description = "Create or update index from file paths")]
    async fn llmx_index(
        &self,
        Parameters(input): Parameters<IndexInput>,
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
        let output = llmx_index_handler(&mut store, input)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

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

    /// Explore index structure: files, outline, or symbols
    #[tool(description = "Explore index structure: files, outline, symbols")]
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

    /// List or delete indexes
    #[tool(description = "List or delete indexes")]
    async fn llmx_manage(
        &self,
        Parameters(input): Parameters<ManageInput>,
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
                name: "llmx-mcp".to_string(),
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
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".llmx/indexes"));

    tracing::info!("Starting LLMX MCP server, storage: {:?}", storage_dir);

    let store = IndexStore::new(storage_dir)?;
    let server = LlmxServer::new(Arc::new(Mutex::new(store)));

    // Run server with stdio transport
    tracing::info!("Server ready, listening on stdio");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
