---
chunk_index: 1376
ref: "fb14babd6c33"
id: "fb14babd6c3367f6f2512d9aef4e2cc137f64b882fbd3ab8922a693731b2202c"
slug: "ingestor-wasm--addtoexternreftable0"
path: "/home/zack/dev/llmx/web/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [1503, 1507]
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