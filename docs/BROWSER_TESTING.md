# Browser Testing (UI)

This repo’s prototype UI is intentionally minimal and runs fully offline.
For regression testing “search clunkiness” or UX issues, prefer deterministic browser automation first.

## Option A (recommended): Playwright smoke testing (no LLM, no network)

This avoids introducing an external model into the loop and keeps tests reproducible.

High-level approach:

1) Serve `web/` locally (see `docs/USAGE.md`)
2) Use Playwright to load `http://127.0.0.1:<port>/`
3) Assert:
   - WASM loads (no console errors)
   - ingest a small fixture set
   - search returns results
   - outline/symbol filters populate for a chosen file

## Option B: browser-use MCP (LLM-driven, requires API key)

This is useful for exploratory testing but is not deterministic and may make external network calls.

### Run the MCP server (SSE mode)

From `/home/zack/dev/mcp/browser-use`:

```bash
uv sync
uv pip install playwright
uv run playwright install --with-deps --no-shell chromium
uv run server --port 8765
```

Then add it to Codex as an MCP server:

```bash
codex mcp add browser-use --url http://127.0.0.1:8765/sse
codex mcp list
```

Notes:

- Do not store secrets in `~/.codex/config.toml`. Prefer exporting `OPENAI_API_KEY` in your shell, or using a local `.env` file in the browser-use repo.
- If you only want offline testing, use Option A instead.

