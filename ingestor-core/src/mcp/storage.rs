use crate::{build_inverted_index, compute_stats, FileMeta, IndexFile, IndexStats};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Stored index format (without inverted_index)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIndex {
    pub id: String,
    pub root_path: String,
    pub created_at: u64,
    pub files: Vec<FileMeta>,
    pub chunks: Vec<crate::Chunk>,
    /// Phase 5: Embeddings for semantic search
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<Vec<Vec<f32>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

impl From<&IndexFile> for StoredIndex {
    fn from(index: &IndexFile) -> Self {
        StoredIndex {
            id: index.index_id.clone(),
            root_path: String::new(), // Will be set by caller
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            files: index.files.clone(),
            chunks: index.chunks.clone(),
            embeddings: index.embeddings.clone(),
            embedding_model: index.embedding_model.clone(),
        }
    }
}

impl From<StoredIndex> for IndexFile {
    fn from(stored: StoredIndex) -> Self {
        // Note: inverted_index will be rebuilt by IndexStore
        IndexFile {
            version: 1,
            index_id: stored.id,
            files: stored.files,
            chunks: stored.chunks,
            chunk_refs: BTreeMap::new(), // Will be built
            inverted_index: BTreeMap::new(), // Will be rebuilt
            stats: IndexStats {
                total_files: 0,
                total_chunks: 0,
                avg_chunk_chars: 0,
                avg_chunk_tokens: 0,
            },
            warnings: vec![],
            embeddings: stored.embeddings,
            embedding_model: stored.embedding_model,
        }
    }
}

/// Registry metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    /// path_hash → index_id mapping
    pub indexes: HashMap<String, IndexMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub id: String,
    pub root_path: String,
    pub created_at: u64,
    pub file_count: usize,
    pub chunk_count: usize,
}

/// Index storage with in-memory cache and persistent disk backing.
///
/// # Overview
///
/// `IndexStore` manages codebase indexes with a two-tier architecture:
/// 1. **Disk Storage**: Atomic writes with temp-file-and-rename pattern
/// 2. **Memory Cache**: Lazy-loaded indexes for fast repeated access
///
/// # Storage Format
///
/// - Indexes: `{storage_dir}/{index_id}.json` (without inverted index for size)
/// - Registry: `{storage_dir}/registry.json` (path → index_id mapping)
///
/// # Performance Characteristics
///
/// - Save: O(n) where n = index size, includes atomic write
/// - Load: O(n) first access (rebuild inverted index), O(1) cached
/// - Search: O(1) cache lookup + O(m) search where m = matching chunks
///
/// # Thread Safety
///
/// Not thread-safe internally. Use `Arc<Mutex<IndexStore>>` for concurrent access.
pub struct IndexStore {
    cache: HashMap<String, IndexFile>,
    storage_dir: PathBuf,
    registry: Registry,
}

impl IndexStore {
    /// Create new IndexStore with the given storage directory.
    ///
    /// # Arguments
    ///
    /// * `storage_dir` - Directory for storing index files and registry
    ///
    /// # Behavior
    ///
    /// - Creates the storage directory if it doesn't exist
    /// - Loads existing registry or creates empty one
    /// - Recovers gracefully from corrupted registry files
    ///
    /// # Errors
    ///
    /// Returns error if unable to create storage directory or read filesystem.
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        // Create storage directory if it doesn't exist
        fs::create_dir_all(&storage_dir)
            .context("Failed to create storage directory")?;

        // Load registry or create empty one
        let registry = Self::load_registry(&storage_dir)?;

        Ok(IndexStore {
            cache: HashMap::new(),
            storage_dir,
            registry,
        })
    }

    /// Load index by ID with lazy loading and automatic inverted index rebuild.
    ///
    /// # Arguments
    ///
    /// * `id` - Index ID to load
    ///
    /// # Behavior
    ///
    /// - First access: Loads from disk and rebuilds inverted index (O(n))
    /// - Subsequent access: Returns cached reference (O(1))
    /// - Rebuilds chunk_refs, inverted_index, and stats on load
    ///
    /// # Errors
    ///
    /// Returns error if index file doesn't exist or is corrupted.
    pub fn load(&mut self, id: &str) -> Result<&IndexFile> {
        if !self.cache.contains_key(id) {
            let stored = self.load_from_disk(id)?;
            let index = self.rebuild_index(stored)?;
            self.cache.insert(id.to_string(), index);
        }
        Ok(self.cache.get(id).unwrap())
    }

    /// Save index to disk with atomic writes.
    ///
    /// # Arguments
    ///
    /// * `index` - The index to save
    /// * `root_path` - Root path of the indexed codebase
    ///
    /// # Behavior
    ///
    /// - Uses temp-file-and-rename pattern for atomic writes
    /// - Updates in-memory cache
    /// - Updates registry with metadata
    /// - Omits inverted_index from disk format (rebuilt on load)
    ///
    /// # Returns
    ///
    /// Returns the index ID on success.
    ///
    /// # Errors
    ///
    /// Returns error if unable to write to disk or update registry.
    pub fn save(&mut self, index: IndexFile, root_path: String) -> Result<String> {
        let mut stored = StoredIndex::from(&index);
        stored.root_path = root_path.clone();

        // Atomic write: temp file + rename
        let temp = self.storage_dir.join(format!("{}.json.tmp", index.index_id));
        let target = self.storage_dir.join(format!("{}.json", index.index_id));

        let json = serde_json::to_vec(&stored)
            .context("Failed to serialize index")?;
        fs::write(&temp, json)
            .context("Failed to write temp index file")?;
        fs::rename(&temp, &target)
            .context("Failed to rename temp index file")?;

        // Update cache
        self.cache.insert(index.index_id.clone(), index.clone());

        // Update registry
        let path_hash = Self::hash_path(&root_path);
        self.registry.indexes.insert(
            path_hash,
            IndexMetadata {
                id: index.index_id.clone(),
                root_path,
                created_at: stored.created_at,
                file_count: index.files.len(),
                chunk_count: index.chunks.len(),
            },
        );
        self.save_registry()?;

        Ok(index.index_id)
    }

    /// List all indexes
    pub fn list(&self) -> Result<Vec<IndexMetadata>> {
        Ok(self.registry.indexes.values().cloned().collect())
    }

    /// Delete index by ID
    pub fn delete(&mut self, id: &str) -> Result<()> {
        // Remove from disk
        let target = self.storage_dir.join(format!("{}.json", id));
        if target.exists() {
            fs::remove_file(&target)
                .context("Failed to delete index file")?;
        }

        // Remove from cache
        self.cache.remove(id);

        // Remove from registry
        self.registry.indexes.retain(|_, meta| meta.id != id);
        self.save_registry()?;

        Ok(())
    }

    /// Find index ID by root path
    pub fn find_by_path(&self, root: &Path) -> Option<String> {
        let path_hash = Self::hash_path(&root.to_string_lossy());
        self.registry.indexes.get(&path_hash)
            .map(|meta| meta.id.clone())
    }

    /// Get mutable reference to cached index
    pub fn get_mut(&mut self, id: &str) -> Option<&mut IndexFile> {
        self.cache.get_mut(id)
    }

    // Private helper methods

    fn load_registry(storage_dir: &Path) -> Result<Registry> {
        let registry_path = storage_dir.join("registry.json");
        if !registry_path.exists() {
            return Ok(Registry::default());
        }

        let data = fs::read(&registry_path)
            .context("Failed to read registry")?;

        // Handle corrupted registry gracefully
        match serde_json::from_slice(&data) {
            Ok(registry) => Ok(registry),
            Err(_) => {
                eprintln!("Warning: corrupted registry, creating new one");
                Ok(Registry::default())
            }
        }
    }

    fn save_registry(&self) -> Result<()> {
        let temp = self.storage_dir.join("registry.json.tmp");
        let target = self.storage_dir.join("registry.json");

        let json = serde_json::to_vec(&self.registry)
            .context("Failed to serialize registry")?;
        fs::write(&temp, json)
            .context("Failed to write temp registry")?;
        fs::rename(temp, target)
            .context("Failed to rename temp registry")?;

        Ok(())
    }

    fn load_from_disk(&self, id: &str) -> Result<StoredIndex> {
        let path = self.storage_dir.join(format!("{}.json", id));
        let data = fs::read(&path)
            .with_context(|| format!("Failed to read index file for {}", id))?;

        serde_json::from_slice(&data)
            .with_context(|| format!("Failed to parse index file for {}", id))
    }

    fn rebuild_index(&self, stored: StoredIndex) -> Result<IndexFile> {
        let chunk_refs = crate::util::build_chunk_refs(&stored.chunks);
        let inverted_index = build_inverted_index(&stored.chunks);
        let stats = compute_stats(&stored.files, &stored.chunks);

        Ok(IndexFile {
            version: 1,
            index_id: stored.id,
            files: stored.files,
            chunks: stored.chunks,
            chunk_refs,
            inverted_index,
            stats,
            warnings: vec![],
            embeddings: stored.embeddings,
            embedding_model: stored.embedding_model,
        })
    }

    fn hash_path(path: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(path.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_save_load() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;

        // Create a minimal index
        let index = IndexFile {
            version: 1,
            index_id: "test123".to_string(),
            files: vec![],
            chunks: vec![],
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
            stats: IndexStats {
                total_files: 0,
                total_chunks: 0,
                avg_chunk_chars: 0,
                avg_chunk_tokens: 0,
            },
            warnings: vec![],
        };

        // Save
        let id = store.save(index.clone(), "/test/path".to_string())?;
        assert_eq!(id, "test123");

        // Load
        let loaded = store.load(&id)?;
        assert_eq!(loaded.index_id, "test123");

        Ok(())
    }

    #[test]
    fn test_atomic_writes() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;

        let index = IndexFile {
            version: 1,
            index_id: "atomic_test".to_string(),
            files: vec![],
            chunks: vec![],
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
            stats: IndexStats {
                total_files: 0,
                total_chunks: 0,
                avg_chunk_chars: 0,
                avg_chunk_tokens: 0,
            },
            warnings: vec![],
        };

        store.save(index, "/test".to_string())?;

        // Verify no .tmp files left behind
        let temp_files: Vec<_> = fs::read_dir(temp_dir.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();

        assert_eq!(temp_files.len(), 0, "Temp files should be cleaned up");

        Ok(())
    }
}
