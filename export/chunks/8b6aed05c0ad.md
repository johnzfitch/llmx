---
chunk_index: 1381
ref: "8b6aed05c0ad"
id: "8b6aed05c0addaadb2cba3dcea1390e88d61bef43309ce449dde237f8e3d5391"
slug: "ingestor-wasm--getdataviewmemory0"
path: "/home/zack/dev/llmx/web/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [1594, 1599]
token_estimate: 86
content_sha256: "4777d9b12333510bb501bed444ad5027b9d144f1d5ed4b563a68a43366f60294"
compacted: false
heading_path: []
symbol: "getDataViewMemory0"
address: null
asset_path: null
---

function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}