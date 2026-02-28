---
chunk_index: 133
ref: "81d8dffbe5f8"
id: "81d8dffbe5f87627087d2d23c69e43d04deb86f257bb8f98e2b539c61be3429a"
slug: "agent-handoff--if-phase-5-agent-encounters-problems"
path: "/home/zack/dev/llmx/docs/AGENT_HANDOFF.md"
kind: "markdown"
lines: [482, 502]
token_estimate: 170
content_sha256: "946ef76cb8e21cc2e992a5577264ac8d1ee33d67abfdacaba24396c4523d82af"
compacted: false
heading_path: ["Agent Handoff Document","Questions? Issues?","If Phase 5 Agent Encounters Problems:"]
symbol: null
address: null
asset_path: null
---

### If Phase 5 Agent Encounters Problems:

**Build errors?**
- Check Rust version: `rustc --version` (need 1.70+)
- Clean rebuild: `cargo clean && cargo build --release --features mcp`

**Performance regressions?**
- Compare against `docs/PHASE_4_BASELINE_BENCHMARKS.md`
- Profile: `cargo build --release && perf record ./target/release/llmx-mcp`

**Architecture questions?**
- Review `src/bin/mcp_server.rs:14-31` (server docs)
- Review `src/mcp/storage.rs:70-96` (storage docs)

**Semantic search design questions?**
- **PRIMARY**: Read `docs/PHASE_5_DIRECTIONS.md`
- Consider: Embedding model size vs accuracy trade-offs
- Consider: Batch embedding generation vs per-chunk

---