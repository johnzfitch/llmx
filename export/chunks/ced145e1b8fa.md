---
chunk_index: 56
ref: "ced145e1b8fa"
id: "ced145e1b8fab81c7fda282de0a35781a01c1113d9a1c21ae969d8287813516f"
slug: "claude-model-selection-for-c--assistant"
path: "/home/zack/dev/llmx/claude_Model selection for current project_2026-01-17.md"
kind: "markdown"
lines: [418, 486]
token_estimate: 800
content_sha256: "4ae06e250ae56461a750cb88f4f31b799bbaed1a36c3b1161a6e0ce5ffa1ee3d"
compacted: false
heading_path: ["Model selection for current project","Assistant"]
symbol: null
address: null
asset_path: null
---

## Assistant

The agent is asking whether to implement Phase 6 for both native and WASM targets, or focus exclusively on browser/WASM.

Let me think about this:

1. The original llmx project was an MCP server (native) that works with Claude Code
2. llm.cat is a browser-based tool for generating llms.txt files
3. The P6 directions I just wrote are specifically for browser/WebGPU deployment for llm.cat

But wait - there are actually TWO codebases here potentially:
- llmx: The MCP server that runs natively with Claude Code
- llm.cat: Browser-based tool

The question is whether they share code or are separate projects.

Looking back at the context:
- The LLMCAT_INTEGRATION_PLAN.md talks about integrating specho-v2 quality scoring into llm.cat
- The Phase 5 handoff docs are about llmx MCP server
- Zack asked "Is this still going to work on llm.cat?" which implies they're related but maybe separate

Given the agent is asking this question, they seem to be working on a shared codebase or considering whether to make Burn work for both.

Actually, Burn DOES work for both:
- Native: burn-wgpu uses Vulkan/Metal/DX12
- WASM: burn-wgpu uses WebGPU
- CPU fallback: burn-ndarray works everywhere

So Option 1 is actually not that much more work IF:
- The embedding generation code is abstracted behind a trait
- The weight loading is abstracted (filesystem vs IndexedDB)
- Feature flags separate the native-specific and wasm-specific code

The question for Zack is really: does he need the native MCP server to have real embeddings too, or is hash-based good enough for the MCP server while llm.cat gets real embeddings?

Given:
- llm.cat is browser-only, needs WebGPU


- llmx MCP server is native with hash-based embeddings already working
- Burn's architecture supports both with shared core logic

The real decision hinges on whether the MCP server requires actual embeddings now or if the current hash-based approach suffices. If it's adequate for now, prioritizing the browser implementation makes sense. If both need real embeddings, Burn's abstraction layer keeps the overhead manageable. I should clarify with Zack what his actual deployment timeline and requirements are.Depends on whether llmx (MCP server) and llm.cat share a codebase or are separate projects.

**If separate codebases:**
→ Option 2. Focus on browser for llm.cat. The MCP server can stay on hash-based embeddings (Phase 5) until you need to upgrade it.

**If shared codebase (same Rust lib, different build targets):**
→ Option 1 is actually not much extra work. Burn abstracts the backend:

```rust
// Same code, different feature flags
#[cfg(target_arch = "wasm32")]
type Backend = burn_wgpu::Wgpu;  // WebGPU in browser

#[cfg(not(target_arch = "wasm32"))]
type Backend = burn_wgpu::Wgpu;  // Vulkan/Metal native
```

The main divergence is weight loading:
- Native: filesystem (`~/.llmx/models/`)
- WASM: IndexedDB + fetch

That's ~50 lines of platform-specific code behind a trait.

**My read:** If llm.cat is the priority and llmx MCP server is working fine with hash-based, go Option 2. You can always add native Burn support later—the core embedding/RRF code will be identical.

What's the codebase situation?