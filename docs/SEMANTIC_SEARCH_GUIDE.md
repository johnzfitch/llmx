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

### Embedding Model

**Current (Phase 5)**: Hash-based deterministic embeddings
- Fast: ~1-5µs per chunk
- Consistent: Same text = same embedding
- Testing: Infrastructure validation

**Future (Phase 6)**: all-MiniLM-L6-v2 ONNX model
- Real semantic understanding
- 384-dimensional vectors
- ~20-50ms per chunk

---

## Performance

### Latency

| Search Type | Typical Latency | Max Latency |
|-------------|-----------------|-------------|
| BM25 only | 2-30µs | <10ms |
| Hybrid semantic | 40-100µs | <150ms |

### When to Expect Better Results

**Semantic search improves results when**:
- Query is conceptual or abstract
- Codebase uses varied terminology
- Looking for patterns, not names
- Natural language questions

**BM25 is sufficient when**:
- Searching for specific identifiers
- Need exact keyword matches
- Query is already precise
- Fast response critical

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
  total_matches: number;       // Total matches before token budget filter
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

If semantic search returns too many irrelevant results, try:
- More specific query: "JWT token validation" vs "authentication"
- Add filters: Narrow to specific files/dirs
- Fall back to BM25: For precision over recall

---

## Troubleshooting

### Problem: Semantic search returns same results as BM25

**Cause**: Query contains specific technical terms that BM25 handles well.

**Solution**: This is expected. Hybrid search preserves BM25 quality.

### Problem: Semantic search is slower

**Cause**: Semantic search does more computation (embeddings + vector search).

**Expected**: 40-100µs vs 2-30µs for BM25 only.

**Acceptable**: Still well under 150ms target.

### Problem: Old indexes don't have embeddings

**Cause**: Index created before Phase 5.

**Solution**: Re-index the codebase:
```json
{
  "tool": "llmx_index",
  "arguments": {
    "paths": ["/path/to/project"]
  }
}
```

New index will include embeddings automatically.

### Problem: Semantic search not available

**Cause**: Server built without `embeddings` feature.

**Solution**: Rebuild with `--features mcp` (embeddings included by default):
```bash
cargo build --release --features mcp --bin llmx-mcp
```

---

## Examples in Claude Code

### Example Session

**User**: "Index /home/zack/dev/llmx"

**Claude**: *Calls llmx_index tool*

**User**: "Search for how this handles error recovery"

**Claude**: *Uses semantic search*
```json
{
  "tool": "llmx_search",
  "arguments": {
    "index_id": "abc123...",
    "query": "error recovery handling",
    "use_semantic": true,
    "limit": 10
  }
}
```

**Result**: Finds:
- Retry logic
- Fallback mechanisms
- Error handling patterns
- Recovery strategies
- Circuit breakers

---

## Migration from Phase 4

### No Breaking Changes

Phase 5 is 100% backward compatible:

```json
// Phase 4 searches work unchanged
{
  "query": "BM25 scoring",
  "limit": 10
}

// Phase 5 adds optional semantic search
{
  "query": "BM25 scoring",
  "use_semantic": true,
  "limit": 10
}
```

### Automatic Embedding Generation

All new indexes automatically include embeddings:

```bash
# Phase 4
llmx_index → Creates index with BM25 only

# Phase 5
llmx_index → Creates index with BM25 + embeddings
```

No configuration required.

---

## Advanced Usage

### Combining Multiple Searches

For complex queries, use multiple searches:

```typescript
// 1. Broad semantic search
{
  "query": "authentication patterns",
  "use_semantic": true,
  "limit": 20
}

// 2. Narrow to specific implementation
{
  "query": "JWT verify",
  "use_semantic": false,
  "filters": {
    "path_prefix": "src/auth/"
  },
  "limit": 5
}
```

### Exploring Unfamiliar Codebases

Use semantic search for discovery:

```json
{
  "query": "how does caching work",
  "use_semantic": true,
  "limit": 15
}
```

Then refine with BM25:

```json
{
  "query": "CacheManager class",
  "use_semantic": false,
  "limit": 5
}
```

---

## Limitations

### Current Phase 5 Limitations

1. **Hash-Based Embeddings**: Not true semantic understanding
   - Works for infrastructure testing
   - Deterministic and fast
   - Will be upgraded to real models in Phase 6

2. **Fixed Weights**: 50/50 BM25 + semantic
   - Cannot be adjusted per query
   - Works well as default
   - Future: Configurable weights

3. **No Query Expansion**: Queries used as-is
   - No synonym injection
   - No automatic query rewriting
   - Future: LLM-based query expansion

### Future Enhancements (Phase 6+)

- Real ONNX models (all-MiniLM-L6-v2)
- Configurable BM25/semantic weights
- Reciprocal Rank Fusion (RRF)
- Cross-encoder reranking
- Query expansion
- Embedding caching

---

## Performance Tips

### 1. Use Token Budgeting

Limit inline content to reduce latency:

```json
{
  "query": "error handling",
  "use_semantic": true,
  "max_tokens": 8000
}
```

### 2. Narrow with Filters

Reduce search space:

```json
{
  "query": "validate input",
  "use_semantic": true,
  "filters": {
    "path_prefix": "src/",
    "kind": "javascript"
  }
}
```

### 3. Adjust Limit

Request fewer results for faster response:

```json
{
  "query": "authentication",
  "use_semantic": true,
  "limit": 5
}
```

---

## Feedback & Support

### Report Issues

Found a bug or unexpected behavior? Report at:
https://github.com/anthropics/llmx/issues

### Performance Concerns

If semantic search is slower than expected:
1. Check index size (chunk count)
2. Verify embeddings are present
3. Try with smaller `limit` value
4. Fall back to BM25 for time-critical queries

---

**Happy searching! 🔍**
