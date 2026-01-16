# Phase 4: MCP Server Implementation & Tool Consolidation

## Overview
Complete the working MCP server implementation using `rmcp` and consolidate tools based on agent ergonomics.

## Background
Previous attempts used `rust-mcp-sdk` with manual tool registration. The correct approach uses `rmcp` with the `#[tool_router]` macro pattern for automatic tool registration.

## Primary Objectives

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

### 2. Tool Consolidation
**Rationale**: 10 tools = 10 decisions for agents. Reduce cognitive load by collapsing related operations.

**Current Tools** (too granular):
- `llmx_index_folder`
- `llmx_search` → returns IDs only
- `llmx_get_chunks` 
- `llmx_get_chunks_batch`
- `llmx_list_files`
- `llmx_list_outline`
- `llmx_list_symbols`
- `llmx_list_indexes`
- `llmx_update_index`
- `llmx_delete_index`

**Target Tools** (ergonomic):
- [ ] `llmx_index` - Create or update index (auto-detects)
- [ ] `llmx_search` - Returns full chunk content inline (up to token budget)
  - Falls back to IDs only if exceeds budget (e.g., 8K tokens)
  - Returns both content and optional truncated_ids
- [ ] `llmx_explore` - Combines list_files, list_outline, list_symbols based on query
- [ ] `llmx_manage` - List/delete indexes

**Key Insight**: Agents prefer fewer round-trips. `search` → `get_chunks` is two LLM reasoning cycles. Better: return content directly in search 90% of the time.

### 3. Simplify State Management
**Current**: `Arc<Mutex<IndexStore>>` - unnecessary for single-threaded stdio MCP server

**Action**:
- [ ] Remove `Arc<Mutex<>>` wrapper
- [ ] Use plain `HashMap<String, IndexFile>` 
- [ ] Change methods to `&mut self` where needed
- [ ] Let tokio runtime handle async I/O (no manual concurrency needed)

**Rationale**: stdio is inherently serial. The mutex adds cognitive overhead and potential deadlock surface with zero benefit.

### 4. Enhanced Search Response
**Current Schema**:
```rust
SearchOutput {
    chunk_ids: Vec<String>,
    scores: Vec<f32>,
}
```

**New Schema**:
```rust
SearchOutput {
    results: Vec<SearchResultWithContent>,  // includes full chunk content
    truncated_ids: Option<Vec<String>>,     // only if over token budget
    total_matches: usize,
    query_analysis: Option<String>,         // explain why these results matched
}
```

- [ ] Implement token counting for response size
- [ ] Add configurable budget parameter (default 8K tokens)
- [ ] Include query analysis in verbose mode

## Success Criteria
- [ ] `cargo build` succeeds with `rmcp` dependency
- [ ] MCP server starts and responds to `initialize` request
- [ ] Tools appear in Claude Code's tool list
- [ ] `llmx_search` returns content inline for typical queries
- [ ] Agent workflow: index → search → read (1-2 tool calls, not 3-4)

## Testing Plan
1. **Unit tests**: Tool handlers with mock IndexStore
2. **Integration test**: Full stdio communication with test client
3. **Agent workflow test**: Use Claude Code to index small project and search
4. **Performance test**: Measure round-trip latency for search+content vs search+fetch

## Implementation Order
1. **Week 1**: Fix `mcp_server.rs` foundation (tasks 1.1-1.5)
2. **Week 2**: Consolidate to 4 core tools (task 2)
3. **Week 3**: Enhanced search with inline content (task 4)
4. **Week 4**: Remove unnecessary concurrency primitives (task 3)

## Known Issues to Address
- Search results currently require separate `get_chunks` call
- IndexStore uses unnecessary `Arc<Mutex<>>` for stdio context
- 10 tools create decision paralysis for agents
- No query explanation in search results

## Future Considerations (Post-Phase 4)
- Semantic search integration
- Incremental index updates
- Multi-index search across related projects
- Export index to standard formats (llms.txt, embeddings)

## Dependencies
- `rmcp` - correct MCP server implementation crate
- `tokio` - async runtime (already present)
- `anyhow` - error handling (already present)
- `serde_json` - JSON serialization (already present)

## Resources
- [rmcp documentation](https://docs.rs/rmcp)
- Previous conversation analysis: https://claude.ai/chat/255417e5-a8c5-4fe4-b425-6d00f2c8afbf
- MCP specification: https://modelcontextprotocol.io

## Notes
- The `#[tool_router]` macro eliminates manual tool registration boilerplate
- `#[tool(aggr)]` tells macro to aggregate struct fields into single JSON object
- stdio transport means no HTTP/WebSocket complexity needed
- Focus on agent UX: fewer tools, more complete responses per call
