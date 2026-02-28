---
chunk_index: 1239
ref: "29fbe61bb7ff"
id: "29fbe61bb7ff3c23fac9fa9f91414d40097a7334a96d735e4b1cb0036d2ced77"
slug: "ingestor-wasm--passarrayjsvaluetowasm0"
path: "/home/zack/dev/llmx/ingestor-wasm/pkg/ingestor_wasm.js"
kind: "java_script"
lines: [1102, 1110]
token_estimate: 81
content_sha256: "b841f8cfdec1753248b0946779cdad0bcf6f121fafedd7994865d99739b5d28e"
compacted: false
heading_path: []
symbol: "passArrayJsValueToWasm0"
address: null
asset_path: null
---

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}