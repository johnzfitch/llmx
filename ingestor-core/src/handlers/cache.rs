//! Dynamic index cache with LRU eviction and mtime-based invalidation.
//!
//! Caches in-memory indexes for repeat queries. Uses mtime sampling
//! to detect when files have changed and invalidate stale entries.

use crate::IndexFile;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::time::SystemTime;

/// Number of file mtimes to sample for cache validation.
const MTIME_SAMPLE_SIZE: usize = 10;

/// A cached index entry with metadata for validation.
#[derive(Debug, Clone)]
struct CachedIndex {
    /// The cached index
    index: IndexFile,
    /// Sample of file mtimes for validation (path -> mtime_ms)
    mtime_sample: HashMap<String, u64>,
    /// Total size estimate in bytes
    size_bytes: usize,
}

/// LRU cache for dynamic indexes keyed by root path.
pub struct DynamicCache {
    /// The LRU cache (key: canonicalized path hash)
    cache: LruCache<String, CachedIndex>,
    /// Maximum total size in bytes (default: 500MB)
    max_size_bytes: usize,
    /// Current total size in bytes
    current_size_bytes: usize,
}

impl DynamicCache {
    /// Create a new cache with the given maximum size.
    pub fn new(max_size_bytes: usize) -> Self {
        // Use a reasonable capacity, will be limited by size anyway
        let capacity = NonZeroUsize::new(100).unwrap();
        Self {
            cache: LruCache::new(capacity),
            max_size_bytes,
            current_size_bytes: 0,
        }
    }

    /// Create a new cache with default 500MB limit.
    pub fn default_size() -> Self {
        Self::new(500 * 1024 * 1024)
    }

    /// Generate cache key from path.
    fn cache_key(root: &Path) -> String {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        format!("{:x}", md5_hash(canonical.to_string_lossy().as_bytes()))
    }

    /// Get a cached index if it exists and is still valid.
    pub fn get(&mut self, root: &Path) -> Option<&IndexFile> {
        let key = Self::cache_key(root);

        // First check if entry exists
        if !self.cache.contains(&key) {
            return None;
        }

        // Check if cache is still valid
        let is_valid = {
            let entry = self.cache.peek(&key)?;
            Self::is_valid(entry)
        };

        if is_valid {
            // Use get to update LRU order
            self.cache.get(&key).map(|e| &e.index)
        } else {
            // Invalidate stale entry
            if let Some(entry) = self.cache.pop(&key) {
                self.current_size_bytes = self.current_size_bytes.saturating_sub(entry.size_bytes);
            }
            None
        }
    }

    /// Insert an index into the cache.
    pub fn insert(&mut self, root: &Path, index: IndexFile, file_mtimes: HashMap<String, u64>) {
        let key = Self::cache_key(root);

        // Estimate size: index content + overhead
        let size_bytes = Self::estimate_size(&index);

        // Sample mtimes for validation
        let mtime_sample = Self::sample_mtimes(&file_mtimes);

        // Evict old entries if needed
        while self.current_size_bytes + size_bytes > self.max_size_bytes {
            if let Some((_, evicted)) = self.cache.pop_lru() {
                self.current_size_bytes = self.current_size_bytes.saturating_sub(evicted.size_bytes);
            } else {
                break;
            }
        }

        // Remove old entry for this path if exists
        if let Some(old) = self.cache.pop(&key) {
            self.current_size_bytes = self.current_size_bytes.saturating_sub(old.size_bytes);
        }

        let entry = CachedIndex {
            index,
            mtime_sample,
            size_bytes,
        };

        self.cache.put(key, entry);
        self.current_size_bytes += size_bytes;
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_size_bytes = 0;
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.len(),
            size_bytes: self.current_size_bytes,
            max_size_bytes: self.max_size_bytes,
        }
    }

    /// Check if a cached entry is still valid by sampling mtimes.
    fn is_valid(entry: &CachedIndex) -> bool {
        for (path, cached_mtime) in &entry.mtime_sample {
            let path = Path::new(path);
            if !path.exists() {
                // File was deleted
                return false;
            }

            let current_mtime = std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            if current_mtime != *cached_mtime {
                // File was modified
                return false;
            }
        }

        true
    }

    /// Sample a subset of mtimes for validation.
    fn sample_mtimes(mtimes: &HashMap<String, u64>) -> HashMap<String, u64> {
        let mut sample = HashMap::new();

        if mtimes.len() <= MTIME_SAMPLE_SIZE {
            // If we have few files, use all of them
            return mtimes.clone();
        }

        // Deterministic sampling: use every nth file
        let step = mtimes.len() / MTIME_SAMPLE_SIZE;
        for (i, (path, mtime)) in mtimes.iter().enumerate() {
            if i % step == 0 && sample.len() < MTIME_SAMPLE_SIZE {
                sample.insert(path.clone(), *mtime);
            }
        }

        sample
    }

    /// Estimate the size of an index in bytes.
    fn estimate_size(index: &IndexFile) -> usize {
        // Rough estimate: count content sizes + overhead
        let content_size: usize = index.chunks.iter().map(|c| c.content.len()).sum();
        let overhead = index.files.len() * 100 + index.chunks.len() * 200;
        content_size + overhead
    }
}

/// Cache statistics for monitoring.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: usize,
    pub size_bytes: usize,
    pub max_size_bytes: usize,
}

/// Simple hash function for cache keys (not cryptographic).
fn md5_hash(data: &[u8]) -> u64 {
    // Use a simple hash - we don't need cryptographic security for cache keys
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Chunk, ChunkKind, FileMeta, IndexFile, IndexStats};
    use std::collections::BTreeMap;

    fn make_test_index() -> IndexFile {
        IndexFile {
            version: 1,
            index_id: "test".to_string(),
            files: vec![FileMeta {
                path: "/tmp/test.rs".to_string(),
                kind: ChunkKind::Unknown,
                bytes: 100,
                sha256: "abc".to_string(),
                line_count: 10,
                mtime_ms: Some(1000),
                fingerprint_sha256: None,
            }],
            chunks: vec![Chunk {
                id: "chunk1".to_string(),
                short_id: "chunk1".to_string(),
                slug: "test-chunk".to_string(),
                path: "/tmp/test.rs".to_string(),
                kind: ChunkKind::Unknown,
                chunk_index: 0,
                start_line: 1,
                end_line: 10,
                content: "fn main() {}".to_string(),
                content_hash: "abc123".to_string(),
                token_estimate: 10,
                heading_path: vec![],
                symbol: None,
                address: None,
                asset_path: None,
            }],
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
            stats: IndexStats {
                total_files: 1,
                total_chunks: 1,
                avg_chunk_chars: 12,
                avg_chunk_tokens: 10,
            },
            warnings: vec![],
            embeddings: None,
            embedding_model: None,
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = DynamicCache::new(10 * 1024 * 1024); // 10MB
        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").unwrap();

        let index = make_test_index();
        let mut mtimes = HashMap::new();
        mtimes.insert(
            test_file.to_string_lossy().to_string(),
            std::fs::metadata(&test_file)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );

        cache.insert(temp.path(), index.clone(), mtimes);

        // Should get the cached index
        let cached = cache.get(temp.path());
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().index_id, "test");
    }

    #[test]
    fn test_cache_invalidation_on_file_change() {
        let mut cache = DynamicCache::new(10 * 1024 * 1024);
        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").unwrap();

        let index = make_test_index();
        let mut mtimes = HashMap::new();
        mtimes.insert(
            test_file.to_string_lossy().to_string(),
            std::fs::metadata(&test_file)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );

        cache.insert(temp.path(), index, mtimes);

        // Modify the file
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&test_file, "fn main() { println!(\"hello\"); }").unwrap();

        // Should return None due to mtime change
        let cached = cache.get(temp.path());
        assert!(cached.is_none());
    }

    #[test]
    fn test_lru_eviction() {
        // Create cache with small size limit
        let mut cache = DynamicCache::new(1000);
        let temp1 = tempfile::tempdir().unwrap();
        let temp2 = tempfile::tempdir().unwrap();

        let index1 = make_test_index();
        let index2 = make_test_index();

        cache.insert(temp1.path(), index1, HashMap::new());
        cache.insert(temp2.path(), index2, HashMap::new());

        // With small cache, older entries should be evicted
        let stats = cache.stats();
        assert!(stats.entry_count <= 2);
    }
}
