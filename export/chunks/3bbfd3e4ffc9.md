---
chunk_index: 579
ref: "3bbfd3e4ffc9"
id: "3bbfd3e4ffc9343987f746060053e09c06ed257f3ec7ed538f6b48bdd96767cb"
slug: "phase-4-directions--1-fix-mcp-server-foundation"
path: "/home/zack/dev/llmx/docs/PHASE_4_DIRECTIONS.md"
kind: "markdown"
lines: [11, 34]
token_estimate: 177
content_sha256: "8d36d16800f5e05b7eff7d6ce0ec4152c8c70d40c07207e44d22b9607ea8d4d1"
compacted: false
heading_path: ["Phase 4: MCP Server Implementation & Tool Consolidation","Primary Objectives","1. Fix MCP Server Foundation"]
symbol: null
address: null
asset_path: null
---

### 1. Fix MCP Server Foundation
**Status**: Blocked - needs rewrite using correct API

**Tasks**:
- [ ] Update `Cargo.toml`: Replace `rust-mcp-sdk` with `rmcp`
- [ ] Rewrite `mcp_server.rs` to use `#[tool_router]` pattern (~80 lines)
- [ ] Add `#[tool(aggr)]` annotations for struct parameters
- [ ] Implement proper `ServerHandler` trait with `get_info()` method
- [ ] Set up stdio transport: `server.serve(stdio()).await`

**Reference Pattern**:
```rust
#[tool_router]
impl LlmxServer {
    #[tool(description = "Search indexed codebase")]
    async fn llmx_search(
        &self,
        #[tool(aggr)] input: SearchInput,
    ) -> Result<CallToolResult, McpError> {
        // implementation
    }
}
```