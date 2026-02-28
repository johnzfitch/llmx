---
chunk_index: 50
ref: "61e7caf4cd4f"
id: "61e7caf4cd4f7ef2aa0994bb9b71dbfe53048f043dc54c026b28e246aecbae00"
slug: "claude-model-selection-for-c--assistant"
path: "/home/zack/dev/llmx/claude_Model selection for current project_2026-01-17.md"
kind: "markdown"
lines: [230, 290]
token_estimate: 911
content_sha256: "b7ecfe3a2b3404f84fb2c03f7eb28c8b34e9aaf1fb4344a29b21705d707fddce"
compacted: false
heading_path: ["Model selection for current project","Assistant"]
symbol: null
address: null
asset_path: null
---

## Assistant

The user wants to use Burn for the WASM + WebGPU path, which makes sense given they want to keep everything in Rust. They're asking if it's a problem that an agent has already started Phase 6 - presumably using the directions I just created which assume server-side `ort`.

This is a problem because:
1. The P6 directions I wrote assume `ort` crate which doesn't compile to WASM
2. Burn has a completely different API and workflow
3. The agent may have already started implementing the wrong approach

I should:
1. Acknowledge the issue
2. Ask what progress the agent has made (to understand how much needs to be undone)
3. Offer to rewrite the P6 directions for Burn + WebGPU

Actually, let me just rewrite the directions now since that's clearly what's needed. The user said "Yes, lets go with burn" so they want updated directions.

I should create new P6 directions that:
- Use Burn instead of ort
- Target WASM + WebGPU
- Keep the same model recommendations (bge-small, nomic) but with Burn's ONNX import
- Keep RRF and hybrid search logic (that's runtime-agnostic)
- Note the differences from the server-side approach

Let me write comprehensive Burn-based P6 directions.Depends how far they got. If they're already deep into `ort` integration, that code won't work for browser deployment.

Let me rewrite P6 for Burn + WebGPU. The hybrid search logic (RRF, quality scoring) is runtime-agnostic—that stays. Just the embedding generation changes.
Now I need to update the rest of the file to reflect the Burn/WebGPU context. Let me also add browser-specific sections and update the testing/gotchas sections.
Now I need to update the testing checklist and gotchas to reflect browser/Burn context.
The file might have different content now. Let me check what's in it.
I see there's still some old ort-related content that didn't get replaced. Let me fix the testing checklist, gotchas, and resources sections to be Burn/WebGPU specific.
The text I'm looking for has slight differences. Let me view more carefully and fix line by line or just rewrite the entire file at this point.
I see the issue now. There are some different characters in my replacement string vs the actual file (e.g., the URL has a different username). Let me just rewrite the whole file cleanly since it's gotten into an inconsistent state.
I need to use the bash_tool or create_file to write to /home/claude.
Need to use bash to overwrite the file.
Good, the file has been overwritten. Now let me copy it to the outputs directory and present it.
Now I need to present the files again.
Good. Now let me answer the user's question about whether it's a problem that the agent has already begun Phase 6.Done. Updated P6 directions now target Burn + WebGPU.

**Re: agent already started P6** — depends on what they've done:

**If they added `ort` deps and started ONNX loading code:**
- That code won't compile to WASM
- Need to rip it out and start over with Burn
- RRF logic, backward compat checks, and SearchInput changes can stay

**If they're still on RRF/hybrid search logic:**
- That's runtime-agnostic, keep it
- Just swap the embedding generation layer

**If they downloaded `all-MiniLM-L6-v2`:**
- Swap for `bge-small-en-v1.5` (same 384-dim, better quality)
- Burn's ONNX import handles the conversion

Key things to communicate to the agent:
1. **No `ort`** — it's server-only
2. **Burn + WGPU** — compiles to WASM with WebGPU backend
3. **Build-time ONNX conversion** — `burn-import` generates Rust code, no runtime parsing
4. **Async everywhere** — WebGPU ops are async in browser

Want me to draft a message for the agent explaining the pivot?