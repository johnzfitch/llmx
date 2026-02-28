---
chunk_index: 1228
ref: "ee5ccaf2c080"
id: "ee5ccaf2c08046d3584418bed65dc403ee66c6042fee61b8e483f669716ff3fa"
slug: "ingestor-wasm--addtoexternreftable0"
path: "/home/zack/dev/llmx/ingestor-wasm/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [947, 951]
token_estimate: 37
content_sha256: "01570b349682cd229c49168da4848bb61d69d79dbccb413b8194c45a1d905ff7"
compacted: false
heading_path: []
symbol: "addToExternrefTable0"
address: null
asset_path: null
---

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}