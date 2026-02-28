---
chunk_index: 26
ref: "1db7f62bfce2"
id: "1db7f62bfce281d4e62fbde07c9354689e729a2cd388016e391076f8cff1d81c"
slug: "readme--browser-compatibility"
path: "/home/zack/dev/llmx/README.md"
kind: "markdown"
lines: [244, 253]
token_estimate: 70
content_sha256: "615bdaa0a27e311dd1fe8fcec7eaa47866db5032d2b9f448483c05e11b7c5939"
compacted: false
heading_path: ["![search](.github/assets/icons/search-24x24.png) llmx","![gear](.github/assets/icons/gear-24x24.png) Technical Details","Browser Compatibility"]
symbol: null
address: null
asset_path: null
---

### Browser Compatibility

- **Chromium** (Chrome, Edge): Full support (`showDirectoryPicker`)
- **WebKit** (Safari): Folder input via `webkitdirectory`
- **Firefox**: File selection or drag-and-drop (no folder picker)

Module workers fall back to main thread if unavailable.

---