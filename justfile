# llmx developer tasks. Run `just <task>` (https://github.com/casey/just).
set shell := ["bash", "-cu"]

mcp_bin := "target/release/llmx-mcp"
cli_bin := "target/release/llmx"
cpu_features := "treesitter,mcp,mcp-http,cli,ndarray-backend"

# List tasks
default:
    @just --list

# Build the MCP server + CLI with CPU embeddings (lighter/faster; recommended on laptops)
build:
    cargo build --release -p llmx-mcp --bin llmx --bin llmx-mcp \
      --no-default-features --features {{cpu_features}}

# Build with GPU embeddings (wgpu over Metal/Vulkan/DX12) -- faster bulk indexing
build-gpu:
    cargo build --release -p llmx-mcp --bin llmx --bin llmx-mcp

# Run the test suite
test:
    cargo test -p llmx-mcp

# Write .mcp.json pointing at the built server, auto-indexing this repo on startup
register:
    printf '{\n  "mcpServers": {\n    "llmx": {\n      "command": "%s/%s",\n      "args": ["--path", "%s"]\n    }\n  }\n}\n' "$PWD" "{{mcp_bin}}" "$PWD" > .mcp.json
    @echo "wrote .mcp.json -> $PWD/{{mcp_bin}}"

# Build (CPU) + register for MCP discovery in one step
setup: build register

# Index a path with the CLI (default: current directory)
index path=".":
    ./{{cli_bin}} index {{path}}

# Stop any running llmx-mcp backend
stop:
    -pkill -f "llmx-mcp" 2>/dev/null || true
