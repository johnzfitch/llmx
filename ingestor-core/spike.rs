// SDK API spike to verify rust-mcp-sdk works as expected
use rust_mcp_sdk::prelude::*;
use serde_json::json;

#[mcp_tool(name = "test", description = "test tool")]
async fn test_handler(input: serde_json::Value) -> Result<serde_json::Value, McpError> {
    Ok(json!({"ok": true, "input": input}))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Testing rust-mcp-sdk API...");

    // Test: can we create a server?
    let server = Server::new("test", "0.1.0")
        .with_tool("test", test_handler);

    println!("✅ Server created successfully");
    println!("✅ API shape confirmed");

    Ok(())
}
