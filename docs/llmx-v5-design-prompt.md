# LLMX v5 Format Redesign - External Review Prompt

## Context

I'm building **LLMX** (LLM-eXchange), an open format for packaging codebases/documents for LLM consumption. Think of it as "PDFs for LLMs" - a way to export a project so any LLM can efficiently ingest and understand it.

The format is used by an ingestion tool that:
1. Chunks source files semantically (respecting function/class boundaries)
2. Counts tokens per chunk
3. Generates embeddings for search
4. Exports a portable archive

## Current Format (v4) - The Problem

### Structure
```
project.llmx/
├── llm.md              # 800 bytes - format documentation
├── manifest.llm.tsv    # 20KB - index of all files and chunks
├── chunks/
│   ├── c0001.md        # Individual chunk files
│   ├── c0002.md
│   ├── ...
│   └── c0120.md        # (288 files for a 28-file project)
└── images/             # Optional extracted images
```

### Manifest Format (TSV)
```
llmx_manifest_llm_tsv	4	{index_hash}
D	0
D	1	src/
P	0	1	auth.js
P	1	1	api.js
K	0	javascript
F	0	0	9	9262	774	auth-module
C	c0001	0	0	1	97	1024	auth-module > init
C	c0002	0	0	98	197	1008	auth-module > validate
```

Row types:
- `D` = Directory index
- `P` = Path index (references directory)
- `K` = Kind/language index
- `F` = File summary (path_i, kind_i, chunk_count, total_tokens, end_line, label)
- `C` = Chunk (ref, path_i, kind_i, start_line, end_line, tokens, label)

### Chunk File Format
```markdown
@llmx	c0001	0	0	1	97	auth-module > init

// Authentication initialization
const Auth = {
    init: () => { ... }
};
```

### Real-World Example Stats
For a 28-file JavaScript project (960KB source):
- **288 chunk files** generated
- **20KB manifest** (349 rows)
- **2.9MB uncompressed** (3x source size)
- **576KB compressed** (.zip)
- **511k tokens** total content

## The Problems

### 1. Filenames Are Useless
`c0001.md`, `c0050.md`, `c00qj.md` tell you nothing. You MUST read the manifest to know what's inside.

### 2. Too Many Files
288 separate files for 28 source files. Filesystem overhead, impossible to browse.

### 3. Manifest Is Too Large
20KB of TSV that an LLM must parse before understanding anything. That's ~5k tokens just for the index.

### 4. Redundant Metadata
Every chunk has a 7-field header (`@llmx ref path_i kind_i start end label`) that duplicates manifest data.

### 5. No Human Readability
A human cannot browse this format without tooling. The manifest is machine-oriented TSV with integer references.

### 6. No Selective Loading
You can't easily say "just give me the latest version" - everything is fragmented across hundreds of chunks.

### 7. Similar Content Not Condensed
If a project has 16 versions of a file (v1-v16), each is chunked separately even though they're 90% identical.

## Design Goals for v5

### Must Have
1. **Filenames describe content** - No opaque references
2. **< 10 files** for typical projects - Browsable
3. **Manifest < 1KB** - Token efficient
4. **Human readable** - No tooling required to understand
5. **Token counts visible** - For context budget planning
6. **Selective loading** - Load only what you need

### Nice to Have
1. Delta compression for versioned content
2. Automatic summarization layer
3. Backward compatibility with v4

### Constraints
1. Must be plain text (no binary formats)
2. Must work with any LLM (no model-specific features)
3. Must preserve source code accurately (no lossy compression)
4. Should compress well with standard tools (gzip, zstd)

## Proposed Directions

### Option A: Single-File Archive
Everything in one markdown file with internal table of contents.
```markdown
# project.llmx [511k tokens]

## Index
| Section | Tokens | Purpose |
|---------|--------|---------|
| #auth | 9k | Authentication module |
| #api | 12k | REST API endpoints |

---
## auth
```javascript
...
```
```

### Option B: Semantic Bundles
Group by meaning, not by arbitrary chunks.
```
project.llmx/
├── README.md           # Manifest + overview (500 tokens)
├── core_45k.md         # Core functionality bundled
├── utils_12k.md        # Utilities bundled
└── docs_8k.md          # Documentation bundled
```

### Option C: Progressive Outline
Outline file with summaries, content file with full code.
```
project.llmx/
├── outline.md          # Summaries + structure (5k tokens)
└── content.md          # Full code (500k tokens)
```

### Option D: Smart Filenames Only
Keep chunks but make filenames meaningful.
```
project.llmx/
├── _index.md           # 300 bytes
├── auth-oauth-login_2k.md
├── api-user-crud_1k8.md
└── utils-helpers_500.md
```

### Option E: Delta Compression
Store base version + patches for versioned content.
```
project.llmx/
├── README.md
├── latest.md           # Current version (9k)
└── patches.md          # Reverse diffs (40k vs 400k for all versions)
```

## Questions for Review

1. **Which option (A-E) best balances human readability and LLM efficiency?**

2. **What's the optimal file count?** Single file vs ~5 bundles vs many small files?

3. **How should token counts be displayed?** In filenames (`auth_2k.md`), in headers, in manifest only?

4. **Should the manifest be a separate file or embedded in README?**

5. **For versioned projects, is delta compression worth the complexity?**

6. **What metadata is actually necessary per chunk/file?**
   - Current: ref, path_i, kind_i, start_line, end_line, tokens, label (7 fields)
   - Minimum viable: ???

7. **Are there existing formats we should learn from?**
   - Jupyter notebooks (.ipynb)
   - Web archives (WARC)
   - Literate programming (noweb, org-mode)
   - Static site generators (Hugo, Jekyll)
   - Other?

8. **How do RAG/retrieval systems affect the design?**
   - Small chunks (512 tokens) are better for vector search
   - But LLM direct consumption wants larger coherent sections
   - Can one format serve both?

9. **What's the right balance between compression and readability?**
   - Maximum compression: custom DSL, binary format
   - Maximum readability: verbose markdown
   - Sweet spot: ???

10. **Any novel approaches we haven't considered?**

## Success Metrics

| Metric | Current v4 | Target v5 |
|--------|------------|-----------|
| Files for 28-file project | 288 | < 10 |
| Manifest size | 20KB | < 1KB |
| Human can browse without tools | No | Yes |
| Minimum useful token load | 511k (all) | < 20k |
| Filename tells you content | 0% | 100% |
| Format learning curve | High | Low |

## Additional Context

### Use Cases
1. **Code review**: Load a PR's changed files into LLM context
2. **Documentation**: Package docs for LLM Q&A
3. **Codebase onboarding**: New developer asks LLM about architecture
4. **Debugging**: Load relevant modules to diagnose issue
5. **Migration**: Understand legacy code for rewrite

### Target Users
- Developers packaging code for LLM consumption
- LLM agents that need to ingest codebases
- RAG systems building knowledge bases
- Anyone sharing code context with AI

### Non-Goals
- Replacing git (version control)
- Replacing package managers
- Being a database format
- Supporting real-time updates

---

**Please provide your analysis and recommendations. Feel free to propose entirely different approaches not listed above.**
