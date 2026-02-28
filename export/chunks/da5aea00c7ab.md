---
chunk_index: 1063
ref: "da5aea00c7ab"
id: "da5aea00c7abd5d813a91f4c71301e55e9fd7a99de6ac4166f738b6186e60335"
slug: "mcp-server-l108-156"
path: "/home/zack/dev/llmx/ingestor-core/src/bin/mcp_server.rs"
kind: "text"
lines: [108, 156]
token_estimate: 434
content_sha256: "cf7c516c9f4432615f63537c4fe00d61d2e28d69bcc10caee3e548133a6be811"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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