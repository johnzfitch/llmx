//! Index storage with in-memory cache and persistent disk backing.

use crate::{build_inverted_index, compute_stats, FileMeta, IndexFile, IndexStats};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Stored index format (without inverted_index for size efficiency).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIndex {
    pub id: String,
    pub root_path: String,
    pub created_at: u64,
    pub files: Vec<FileMeta>,
    pub chunks: Vec<crate::Chunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<Vec<Vec<f32>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

impl From<&IndexFile> for StoredIndex {
    fn from(index: &IndexFile) -> Self {
        StoredIndex {
            id: index.index_id.clone(),
            root_path: String::new(),
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
        IndexFile {
            version: 1,
            index_id: stored.id,
            files: stored.files,
            chunks: stored.chunks,
            chunk_refs: BTreeMap::new(),
            inverted_index: BTreeMap::new(),
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

/// Registry metadata for all indexes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    /// path_hash → index metadata mapping
    pub indexes: HashMap<String, IndexMetadata>,
}

/// Metadata for a single index.
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
pub struct IndexStore {
    cache: HashMap<String, IndexFile>,
    storage_dir: PathBuf,
    registry: Registry,
}

impl IndexStore {
    /// Create new IndexStore with the given storage directory.
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&storage_dir).context("Failed to create storage directory")?;

        let registry = Self::load_registry(&storage_dir)?;

        Ok(IndexStore {
            cache: HashMap::new(),
            storage_dir,
            registry,
        })
    }

    /// Create IndexStore with default storage directory (~/.llmx/indexes).
    pub fn default_store() -> Result<Self> {
        let storage_dir = dirs::home_dir()
            .context("Could not find home directory")?
            .join(".llmx")
            .join("indexes");
        Self::new(storage_dir)
    }

    /// Load index by ID with lazy loading and automatic inverted index rebuild.
    pub fn load(&mut self, id: &str) -> Result<&IndexFile> {
        if !self.cache.contains_key(id) {
            let stored = self.load_from_disk(id)?;
            let index = self.rebuild_index(stored)?;
            self.cache.insert(id.to_string(), index);
        }
        Ok(self.cache.get(id).unwrap())
    }

    /// Save index to disk with atomic writes.
    pub fn save(&mut self, index: IndexFile, root_path: String) -> Result<String> {
        let mut stored = StoredIndex::from(&index);
        stored.root_path = root_path.clone();

        // Atomic write: temp file + rename
        let temp = self
            .storage_dir
            .join(format!("{}.json.tmp", index.index_id));
        let target = self.storage_dir.join(format!("{}.json", index.index_id));

        let json = serde_json::to_vec(&stored).context("Failed to serialize index")?;
        fs::write(&temp, json).context("Failed to write temp index file")?;
        fs::rename(&temp, &target).context("Failed to rename temp index file")?;

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

    /// List all indexes.
    pub fn list(&self) -> Result<Vec<IndexMetadata>> {
        Ok(self.registry.indexes.values().cloned().collect())
    }

    /// Delete index by ID.
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let target = self.storage_dir.join(format!("{}.json", id));
        if target.exists() {
            fs::remove_file(&target).context("Failed to delete index file")?;
        }

        self.cache.remove(id);
        self.registry.indexes.retain(|_, meta| meta.id != id);
        self.save_registry()?;

        Ok(())
    }

    /// Find index ID by root path.
    pub fn find_by_path(&self, root: &Path) -> Option<String> {
        let path_hash = Self::hash_path(&root.to_string_lossy());
        self.registry
            .indexes
            .get(&path_hash)
            .map(|meta| meta.id.clone())
    }

    /// Find index by root path, returning full metadata.
    pub fn find_metadata_by_path(&self, root: &Path) -> Option<&IndexMetadata> {
        let path_hash = Self::hash_path(&root.to_string_lossy());
        self.registry.indexes.get(&path_hash)
    }

    /// Get mutable reference to cached index.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut IndexFile> {
        self.cache.get_mut(id)
    }

    // Private helpers

    fn load_registry(storage_dir: &Path) -> Result<Registry> {
        let registry_path = storage_dir.join("registry.json");
        if !registry_path.exists() {
            return Ok(Registry::default());
        }

        let data = fs::read(&registry_path).context("Failed to read registry")?;

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

        let json = serde_json::to_vec(&self.registry).context("Failed to serialize registry")?;
        fs::write(&temp, json).context("Failed to write temp registry")?;
        fs::rename(temp, target).context("Failed to rename temp registry")?;

        Ok(())
    }

    fn load_from_disk(&self, id: &str) -> Result<StoredIndex> {
        let path = self.storage_dir.join(format!("{}.json", id));
        let data =
            fs::read(&path).with_context(|| format!("Failed to read index file for {}", id))?;

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
