# Semantic Search User Guide

Quick guide to using hybrid semantic search with LLMX MCP server.

---

## Quick Start

### Enable Semantic Search

Add `use_semantic: true` to your search requests:

```json
{
  "tool": "llmx_search",
  "arguments": {
    "index_id": "your-index-id",
    "query": "authentication logic",
    "use_semantic": true
  }
}
```

### When to Use Semantic Search

**✅ Use semantic search for**:
- Conceptual queries ("error handling patterns")
- Synonym variations ("fix" vs "repair" vs "correct")
- Natural language questions ("how to validate user input")
- Code pattern discovery ("retry logic with exponential backoff")

**❌ Use BM25 (default) for**:
- Exact identifier matching ("function getUserById")
- Specific variable names ("AUTH_TOKEN")
- File paths or extensions
- Keyword-based filtering

---

## Examples

### Example 1: Conceptual Search

**Query**: "authentication logic"

**BM25 only** (default):
```json
{
  "query": "authentication logic",
  "use_semantic": false
}
```
Returns: Exact matches for "authentication" OR "logic"

**Hybrid semantic**:
```json
{
  "query": "authentication logic",
  "use_semantic": true
}
```
Returns: Auth logic + login flows + user verification + session management

### Example 2: Synonym Handling

**Query**: "fix database errors"

**BM25 only**: Misses chunks that say "repair", "correct", "resolve"

**Hybrid semantic**: Finds all semantically related error handling:
- "fix database errors"
- "repair DB connection issues"
- "resolve query failures"
- "correct transaction errors"

### Example 3: Natural Language

**Query**: "how to validate user input"

**BM25 only**: Literal matches only

**Hybrid semantic**: Finds:
- Input validation functions
- Sanitization logic
- Schema validation
- Type checking
- Error handling for bad input

---

## How It Works

### Hybrid Scoring

```
final_score = 0.5 * normalized_bm25_score + 0.5 * semantic_similarity
```

- **BM25**: Exact keyword matching (traditional search)
- **Semantic**: Meaning-based matching (embeddings)
- **Hybrid**: Best of both worlds

### Embedding Models

| Phase | Model | Notes |
|-------|-------|-------|
| Phase 5 | Hash-based | Infrastructure testing, deterministic |
| Phase 6 | `bge-small-en-v1.5` | Real semantic understanding, 384-dim |
| Optional | `nomic-embed-text-v1.5` | Higher quality, 768-dim |

**Current model** is stored in `embedding_model` field of each index.

---

## Performance

### Latency

| Search Type | Phase 5 (hash) | Phase 6 (ONNX) |
|-------------|----------------|----------------|
| BM25 only | 2-30µs | 2-30µs |
| Hybrid semantic | 40-100µs | 50-150ms |

### When Semantic Search Helps Most

**Improves results when**:
- Query is conceptual or abstract
- Codebase uses varied terminology
- Looking for patterns, not names
- Natural language questions

**BM25 is sufficient when**:
- Searching for specific identifiers
- Need exact keyword matches
- Query is already precise

---

## API Reference

### SearchInput

```typescript
interface SearchInput {
  index_id: string;           // Required: Index to search
  query: string;              // Required: Search query
  use_semantic?: boolean;     // Optional: Enable hybrid search (default: false)
  limit?: number;             // Optional: Max results (default: 10)
  max_tokens?: number;        // Optional: Token budget (default: 16000)
  filters?: {                 // Optional: Filter results
    path_prefix?: string;
    kind?: string;
    symbol_prefix?: string;
    heading_prefix?: string;
  };
}
```

### SearchOutput

```typescript
interface SearchOutput {
  results: Array<{
    chunk_id: string;
    score: number;            // Combined BM25 + semantic score (if hybrid)
    path: string;
    start_line: number;
    end_line: number;
    content: string;          // Full chunk content
    symbol?: string;
    heading_path: string[];
  }>;
  truncated_ids?: string[];   // Chunks excluded due to token budget
  total_matches: number;      // Total matches before token budget filter
}
```

---

## Best Practices

### 1. Start with BM25 (Default)

Always try BM25 first. It's faster and often sufficient.

```json
{
  "query": "getUserById",
  "use_semantic": false
}
```

### 2. Use Semantic for Exploration

When exploring unfamiliar codebases or concepts:

```json
{
  "query": "how does this handle rate limiting",
  "use_semantic": true
}
```

### 3. Combine with Filters

Semantic search respects filters:

```json
{
  "query": "authentication logic",
  "use_semantic": true,
  "filters": {
    "path_prefix": "src/auth/"
  }
}
```

### 4. Iterate Your Queries

If semantic search returns too many irrelevant results:
- More specific query: "JWT token validation" vs "authentication"
- Add filters: Narrow to specific files/dirs
- Fall back to BM25 for precision over recall

---

## Troubleshooting

### Problem: Semantic search returns same results as BM25

**Cause**: Query contains specific technical terms BM25 handles well, or using hash-based embeddings.

**Solution**: Check `embedding_model` in index. If "hash-based-v1", reindex after ONNX models available.

### Problem: Semantic search is slower

**Expected latency**:
- Hash-based: 40-100µs
- ONNX models: 50-150ms

Still well under usability thresholds.

### Problem: Old indexes don't have embeddings

**Solution**: Re-index the codebase:
```json
{
  "tool": "llmx_index",
  "arguments": {
    "paths": ["/path/to/project"]
  }
}
```

### Problem: Model not found

**Cause**: ONNX model not downloaded.

**Solution**: Models auto-download on first use to `~/.llmx/models/`. Check network connectivity.

---

## Migration Notes

### From Phase 4 (BM25 only)

No changes required. Default behavior unchanged:
```json
{
  "query": "BM25 scoring",
  "limit": 10
}
```

### From Phase 5 (hash-based)

Indexes with hash-based embeddings work but won't have real semantic understanding. After ONNX models available:

1. Reindex to get real embeddings
2. New index will have `embedding_model: "bge-small-en-v1.5"`
3. Semantic search quality significantly improved

---

## Advanced: Hybrid Strategies (Phase 6+)

### Linear Combination (Default)
```json
{
  "query": "error handling",
  "use_semantic": true,
  "hybrid_strategy": "linear"
}
```

### Reciprocal Rank Fusion
More robust, no score normalization needed:
```json
{
  "query": "error handling",
  "use_semantic": true,
  "hybrid_strategy": "rrf"
}
```

---

**Happy searching! 🔍**
