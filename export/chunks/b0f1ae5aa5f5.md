---
chunk_index: 1344
ref: "b0f1ae5aa5f5"
id: "b0f1ae5aa5f51ebb29b703ab6387568e30bef19139270a4c193024ec15eefa9e"
slug: "app--renderresults"
path: "/home/zack/dev/llmx/web/app.js"
kind: "java_script"
lines: [721, 765]
token_estimate: 413
content_sha256: "7ae38fe87e284ab06742b6da0ba31583cd019de5a754f61d34896ac435da62b7"
compacted: false
heading_path: []
symbol: "renderResults"
address: null
asset_path: null
---

function renderResults(results) {
  elements.results.replaceChildren();
  if (!results.length) {
    const empty = document.createElement("div");
    empty.textContent = "No matches.";
    elements.results.appendChild(empty);
    return;
  }
  for (const result of results) {
    const item = document.createElement("div");
    item.className = "result-item";

    const title = document.createElement("strong");
    title.textContent = result.path;

    const meta = document.createElement("div");
    meta.className = "meta";
    const heading = result.heading_path.length ? ` | ${result.heading_path.join("/")}` : "";
    const ref = result.chunk_ref ? ` | ${result.chunk_ref}` : "";
    meta.textContent = `Lines ${result.start_line}-${result.end_line}${ref}${heading}`;

    const snippet = document.createElement("div");
    snippet.textContent = result.snippet;

    const button = document.createElement("button");
    button.textContent = "View chunk";
    button.addEventListener("click", async () => {
      const { chunk } = await callWorker("getChunk", { chunkId: result.chunk_id });
      if (!chunk) {
        return;
      }
      const ref = chunk.short_id ? ` | Ref: ${chunk.short_id}` : "";
      const label = chunk.slug ? ` | ${chunk.slug}` : "";
      elements.chunkTitle.textContent = `${chunk.path} (${chunk.start_line}-${chunk.end_line})${ref}${label}`;
      elements.chunkContent.textContent = chunk.content;
      elements.chunkView.hidden = false;
    });

    item.appendChild(title);
    item.appendChild(meta);
    item.appendChild(snippet);
    item.appendChild(button);
    elements.results.appendChild(item);
  }
}