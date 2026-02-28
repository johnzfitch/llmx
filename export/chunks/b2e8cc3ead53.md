---
chunk_index: 1161
ref: "b2e8cc3ead53"
id: "b2e8cc3ead53bc39d45931b34e4af0ccd48d6f541e223b3e6b6884167b0ef375"
slug: "sample--isuser"
path: "/home/zack/dev/llmx/ingestor-core/tests/fixtures/filetypes/javascript/sample.ts"
kind: "java_script"
lines: [40, 42]
token_estimate: 33
content_sha256: "0f104df2755b453c19af354593515fd53d9f4b77c7471b5c0268cddc0901bfa2"
compacted: false
heading_path: []
symbol: "isUser"
address: null
asset_path: null
---

function isUser(obj: unknown): obj is User {
    return typeof obj === 'object' && obj !== null && 'id' in obj && 'name' in obj;
}