---
chunk_index: 712
ref: "c3eac7ccbb25"
id: "c3eac7ccbb258bb3f17ec2004b385a81d0342a3ba4e859cc0ea0f5cdd92053f8"
slug: "post-p6-enhancements--4-incremental-indexing-filesystem-watching"
path: "/home/zack/dev/llmx/docs/POST_P6_ENHANCEMENTS.md"
kind: "markdown"
lines: [139, 191]
token_estimate: 335
content_sha256: "d9f219d377f64827bcaddf875f667489c941668605e4ceb71b539cfcc4ef48de"
compacted: false
heading_path: ["Post-Phase 6 Enhancements","4. Incremental Indexing (Filesystem Watching)"]
symbol: null
address: null
asset_path: null
---

## 4. Incremental Indexing (Filesystem Watching)

**Problem:** Full reindex on every change. Slow for large projects.

**Solution:** Watch filesystem, update only changed files.

```rust
use notify::{Watcher, RecursiveMode, Event};

pub struct IncrementalIndexer {
    watcher: RecommendedWatcher,
    debouncer: Debouncer,  // Handle rapid saves
}

impl IncrementalIndexer {
    pub fn watch(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        for path in paths {
            self.watcher.watch(&path, RecursiveMode::Recursive)?;
        }
    }

    async fn on_modify(&mut self, path: &Path) {
        // Remove old chunks for this file
        index.chunks.retain(|c| c.path != path);
        
        // Re-chunk
        let new_chunks = chunk_file(path)?;
        
        // Update inverted index incrementally
        for chunk in new_chunks {
            index.inverted_index.add_chunk(&chunk);
            index.chunks.push(chunk);
        }
        
        // Regenerate embeddings only for new chunks
        if let Some(emb) = &mut index.embeddings {
            for chunk in &new_chunks {
                emb.push(generate_embedding(&chunk.content)?);
            }
        }
    }
}
```

**Debouncing:** Wait 500ms after last change before reindexing (editors save frequently).

**Deps:**
```toml
notify = "6.0"
```

---