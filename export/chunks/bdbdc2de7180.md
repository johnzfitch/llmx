---
chunk_index: 1217
ref: "bdbdc2de7180"
id: "bdbdc2de7180c0c33ed11195ff2f0dbb21d6db4ac629d67e8a029d9cc32c943b"
slug: "tokenizer--post-processor"
path: "/home/zack/dev/llmx/ingestor-wasm/models/tokenizer.json"
kind: "json"
lines: [1, 30684]
token_estimate: 256
content_sha256: "9fb37a377c9f1367b9ce5d8fd4146ae02cf7c37093c23536f74ec118af63ab55"
compacted: false
heading_path: ["post_processor"]
symbol: "post_processor"
address: "$.post_processor"
asset_path: null
---

{
  "pair": [
    {
      "SpecialToken": {
        "id": "[CLS]",
        "type_id": 0
      }
    },
    {
      "Sequence": {
        "id": "A",
        "type_id": 0
      }
    },
    {
      "SpecialToken": {
        "id": "[SEP]",
        "type_id": 0
      }
    },
    {
      "Sequence": {
        "id": "B",
        "type_id": 1
      }
    },
    {
      "SpecialToken": {
        "id": "[SEP]",
        "type_id": 1
      }
    }
  ],
  "single": [
    {
      "SpecialToken": {
        "id": "[CLS]",
        "type_id": 0
      }
    },
    {
      "Sequence": {
        "id": "A",
        "type_id": 0
      }
    },
    {
      "SpecialToken": {
        "id": "[SEP]",
        "type_id": 0
      }
    }
  ],
  "special_tokens": {
    "[CLS]": {
      "id": "[CLS]",
      "ids": [
        101
      ],
      "tokens": [
        "[CLS]"
      ]
    },
    "[SEP]": {
      "id": "[SEP]",
      "ids": [
        102
      ],
      "tokens": [
        "[SEP]"
      ]
    }
  },
  "type": "TemplateProcessing"
}