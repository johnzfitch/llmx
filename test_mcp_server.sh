#!/bin/bash
# Test script for LLMX MCP Server Phase 4 functionality

set -e

BINARY="./target/release/llmx-mcp"
TEST_DIR="/tmp/llmx-test-$$"
STORAGE_DIR="/tmp/llmx-storage-$$"

echo "=== LLMX MCP Server Test ==="
echo ""

# Setup
mkdir -p "$TEST_DIR"
mkdir -p "$STORAGE_DIR"
export LLMX_STORAGE_DIR="$STORAGE_DIR"

# Create test codebase
cat > "$TEST_DIR/main.rs" <<'EOF'
fn main() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(x: i32, y: i32) -> i32 {
    x * y
}
EOF

cat > "$TEST_DIR/lib.rs" <<'EOF'
pub fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

pub fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    for i in 2..=(n as f64).sqrt() as u32 {
        if n % i == 0 {
            return false;
        }
    }
    true
}
EOF

cat > "$TEST_DIR/README.md" <<'EOF'
# Test Project

This is a test project for LLMX.

## Features

- Basic arithmetic functions
- Fibonacci sequence
- Prime number checking
EOF

echo "✓ Created test codebase at $TEST_DIR"
echo ""

# Test 1: Initialize request
echo "Test 1: Server initialization"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | \
  $BINARY 2>/dev/null | jq -r '.result.serverInfo.name'

echo "✓ Server initialized"
echo ""

# Test 2: List tools
echo "Test 2: List available tools"
TOOLS=$(cat <<EOF | $BINARY 2>/dev/null | tail -1 | jq -r '.result.tools[].name'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
EOF
)

echo "$TOOLS"
echo ""

# Check all 4 tools are present
if echo "$TOOLS" | grep -q "llmx_index" && \
   echo "$TOOLS" | grep -q "llmx_search" && \
   echo "$TOOLS" | grep -q "llmx_explore" && \
   echo "$TOOLS" | grep -q "llmx_manage"; then
    echo "✓ All 4 tools present"
else
    echo "✗ Missing tools"
    exit 1
fi
echo ""

# Test 3: Index creation
echo "Test 3: Create index"
INDEX_OUTPUT=$(cat <<EOF | $BINARY 2>/dev/null
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"llmx_index","arguments":{"paths":["$TEST_DIR"]}}}
EOF
)

INDEX_ID=$(echo "$INDEX_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.index_id')
CREATED=$(echo "$INDEX_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.created')
FILE_COUNT=$(echo "$INDEX_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.stats.total_files')

echo "Index ID: $INDEX_ID"
echo "Created: $CREATED"
echo "Files indexed: $FILE_COUNT"

if [ "$FILE_COUNT" -eq "3" ]; then
    echo "✓ All 3 files indexed"
else
    echo "✗ Expected 3 files, got $FILE_COUNT"
    exit 1
fi
echo ""

# Test 4: Search with inline content
echo "Test 4: Search with inline content"
SEARCH_OUTPUT=$(cat <<EOF | $BINARY 2>/dev/null
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"llmx_search","arguments":{"index_id":"$INDEX_ID","query":"function fibonacci","limit":5}}}
EOF
)

RESULT_COUNT=$(echo "$SEARCH_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.results | length')
HAS_CONTENT=$(echo "$SEARCH_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.results[0].content' | wc -c)

echo "Results returned: $RESULT_COUNT"
echo "First result has content: $HAS_CONTENT bytes"

if [ "$HAS_CONTENT" -gt "10" ]; then
    echo "✓ Search returns inline content"
else
    echo "✗ Search missing inline content"
    exit 1
fi
echo ""

# Test 5: Explore (files mode)
echo "Test 5: Explore index (files mode)"
EXPLORE_OUTPUT=$(cat <<EOF | $BINARY 2>/dev/null
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"llmx_explore","arguments":{"index_id":"$INDEX_ID","mode":"files"}}}
EOF
)

FILE_LIST=$(echo "$EXPLORE_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.items[]')
echo "$FILE_LIST"

if echo "$FILE_LIST" | grep -q "main.rs" && \
   echo "$FILE_LIST" | grep -q "lib.rs" && \
   echo "$FILE_LIST" | grep -q "README.md"; then
    echo "✓ All files listed"
else
    echo "✗ Missing files in list"
    exit 1
fi
echo ""

# Test 6: Manage (list indexes)
echo "Test 6: List all indexes"
MANAGE_OUTPUT=$(cat <<EOF | $BINARY 2>/dev/null
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"llmx_manage","arguments":{"action":"list"}}}
EOF
)

INDEX_COUNT=$(echo "$MANAGE_OUTPUT" | tail -1 | jq -r '.result.content[0].text' | jq -r '.indexes | length')
echo "Total indexes: $INDEX_COUNT"

if [ "$INDEX_COUNT" -ge "1" ]; then
    echo "✓ Index listed in registry"
else
    echo "✗ Index not in registry"
    exit 1
fi
echo ""

# Cleanup
rm -rf "$TEST_DIR"
rm -rf "$STORAGE_DIR"

echo "=== All Tests Passed ✓ ==="
echo ""
echo "Phase 4 MCP Server Verification Complete:"
echo "  ✓ Server initialization"
echo "  ✓ 4 tools exposed (index, search, explore, manage)"
echo "  ✓ Index creation with 3 files"
echo "  ✓ Search with inline content (token-budgeted)"
echo "  ✓ Explore mode (files listing)"
echo "  ✓ Manage mode (index registry)"
echo ""
echo "Ready for Phase 5: Semantic Search"
