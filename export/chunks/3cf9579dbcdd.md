---
chunk_index: 1386
ref: "3cf9579dbcdd"
id: "3cf9579dbcdded4c68ddb7e65a96242d1da5d67345e4f447ce34e68800ea3c99"
slug: "ingestor-wasm--handleerror"
path: "/home/zack/dev/llmx/web/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [1630, 1637]
token_estimate: 47
content_sha256: "f0ef19098374cf579645f4e29bbc58df78c6f1ba33c61d14f8d2f4e2567a92a2"
compacted: false
heading_path: []
symbol: "handleError"
address: null
asset_path: null
---

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}