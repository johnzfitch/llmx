---
chunk_index: 1261
ref: "76cbeb048dce"
id: "76cbeb048dcea9f3cb7eec71dc430d53480e67a5a1e62b24097f39e0a7541bf3"
slug: "ingestor-wasm--getarrayu8fromwasm0"
path: "/home/zack/dev/llmx/ingestor-wasm/web/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [74, 77]
token_estimate: 33
content_sha256: "bb47a84f9b4ca9150b1b4fa72d446c29a0ec1749f8f4a9876dca8117830111f8"
compacted: false
heading_path: []
symbol: "getArrayU8FromWasm0"
address: null
asset_path: null
---

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}