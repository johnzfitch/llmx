---
chunk_index: 1351
ref: "abd0a6ea62a1"
id: "abd0a6ea62a1688035d4b0b35c290553482459daea482141c0dabb08526c188c"
slug: "app--saveindex"
path: "/home/zack/dev/llmx/web/app.js"
kind: "java_script"
lines: [867, 875]
token_estimate: 84
content_sha256: "dde07bfccb0e7fcbed180394d2ef9007857f34447ea9fe32671942b3b2fc1411"
compacted: false
heading_path: []
symbol: "saveIndex"
address: null
asset_path: null
---

async function saveIndex(id, json) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("indexes", "readwrite");
    tx.objectStore("indexes").put({ id, json, saved_at: new Date().toISOString() });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}