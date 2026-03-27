//! Index storage with in-memory cache and persistent disk backing.

use crate::{
    build_inverted_index, compute_stats, embedding_store, graph::build_structural_indexes, EdgeIndex,
    FileMeta, IndexFile, IndexStats, SymbolTable, INDEX_VERSION,
};
use anyhow::{Context, Result};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

/// Stored index format (without inverted_index for size efficiency).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIndex {
    #[serde(default = "default_index_version")]
    pub version: u32,
    pub id: String,
    pub root_path: String,
    pub created_at: u64,
    pub files: Vec<FileMeta>,
    pub chunks: Vec<crate::Chunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<Vec<Vec<f32>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbols: SymbolTable,
    #[serde(default, skip_serializing_if = "EdgeIndex::is_empty")]
    pub edges: EdgeIndex,
}

fn default_index_version() -> u32 { 1 }

fn ensure_supported_index_version(version: u32, id: &str) -> Result<()> {
    if version != INDEX_VERSION {
        anyhow::bail!(
            "Index {id} uses schema version {version}, but llmx expects version {INDEX_VERSION}. Reindex the project to refresh structural metadata."
        );
    }
    Ok(())
}

impl From<&IndexFile> for StoredIndex {
    fn from(index: &IndexFile) -> Self {
        StoredIndex {
            version: index.version,
            id: index.index_id.clone(),
            root_path: String::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            files: index.files.clone(),
            chunks: index.chunks.clone(),
            embeddings: None,
            embedding_model: index.embedding_model.clone(),
            symbols: index.symbols.clone(),
            edges: index.edges.clone(),
        }
    }
}

impl From<StoredIndex> for IndexFile {
    fn from(stored: StoredIndex) -> Self {
        IndexFile {
            version: stored.version.max(1),
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
            symbols: stored.symbols,
            edges: stored.edges,
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

/// Maximum size of an index file we will read from disk before deserialization.
const MAX_INDEX_FILE_BYTES: u64 = 512 * 1024 * 1024; // 512 MB

/// Number of deserialized indexes kept in the in-memory LRU cache.
const CACHE_CAPACITY: usize = 20;

/// Validate that an index_id is safe to use in file paths.
///
/// Index IDs must be non-empty, ≤128 chars, and contain only ASCII
/// alphanumerics, hyphens, and underscores to prevent path traversal.
fn validate_index_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 128 {
        anyhow::bail!("Invalid index_id length");
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        anyhow::bail!("Invalid index_id: must contain only alphanumerics, hyphens, and underscores");
    }
    Ok(())
}

/// Index storage with in-memory cache and persistent disk backing.
///
/// # Overview
///
/// `IndexStore` manages codebase indexes with a two-tier architecture:
/// 1. **Disk Storage**: Atomic writes with temp-file-and-rename pattern
/// 2. **Memory Cache**: LRU-evicting cache for fast repeated access
///
/// # Storage Format
///
/// - Indexes: `{storage_dir}/{index_id}.json` (without inverted index for size)
/// - Registry: `{storage_dir}/registry.json` (path → index_id mapping)
pub struct IndexStore {
    cache: LruCache<String, IndexFile>,
    storage_dir: PathBuf,
    registry: Registry,
}

impl IndexStore {
    /// Create new IndexStore with the given storage directory.
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&storage_dir).context("Failed to create storage directory")?;

        let registry = Self::load_registry(&storage_dir)?;

        Ok(IndexStore {
            cache: LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap()),
            storage_dir,
            registry,
        })
    }

    /// Create IndexStore with default storage directory (platform data directory).
    pub fn default_store() -> Result<Self> {
        Self::new(crate::default_storage_dir())
    }

    /// Load index by ID with lazy loading and automatic inverted index rebuild.
    pub fn load(&mut self, id: &str) -> Result<&IndexFile> {
        validate_index_id(id)?;
        if !self.cache.contains(id) {
            let stored = self.load_from_disk(id)?;
            let index = self.rebuild_index(stored)?;
            self.cache.put(id.to_string(), index);
        }
        Ok(self.cache.get(id).unwrap())
    }

    /// Save index to disk with atomic writes.
    pub fn save(&mut self, index: IndexFile, root_path: String) -> Result<String> {
        validate_index_id(&index.index_id)?;
        let mut stored = StoredIndex::from(&index);
        stored.root_path = root_path.clone();

        // Atomic write: temp file + rename
        let temp = self
            .storage_dir
            .join(format!("{}.json.tmp", index.index_id));
        let target = self.storage_dir.join(format!("{}.json", index.index_id));

        let embeddings_path = embedding_store::sidecar_path(&self.storage_dir, &index.index_id);
        embedding_store::write_sidecar(&embeddings_path, index.embeddings.as_deref())?;

        let json = serde_json::to_vec(&stored).context("Failed to serialize index")?;
        fs::write(&temp, json).context("Failed to write temp index file")?;
        fs::rename(&temp, &target).context("Failed to rename temp index file")?;

        // Update cache
        self.cache.put(index.index_id.clone(), index.clone());

        // Update registry, cleaning up any orphaned index file from a previous ID
        let path_hash = Self::hash_path(&root_path);
        if let Some(old_meta) = self.registry.indexes.get(&path_hash) {
            if old_meta.id != index.index_id {
                let old_file = self.storage_dir.join(format!("{}.json", old_meta.id));
                let _ = fs::remove_file(&old_file);
                let old_embeddings = embedding_store::sidecar_path(&self.storage_dir, &old_meta.id);
                let _ = fs::remove_file(&old_embeddings);
                self.cache.pop(&old_meta.id);
            }
        }
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
        validate_index_id(id)?;
        let target = self.storage_dir.join(format!("{}.json", id));
        if target.exists() {
            fs::remove_file(&target).context("Failed to delete index file")?;
        }
        let embeddings_path = embedding_store::sidecar_path(&self.storage_dir, id);
        if embeddings_path.exists() {
            fs::remove_file(&embeddings_path).context("Failed to delete embedding sidecar")?;
        }

        self.cache.pop(id);
        self.registry.indexes.retain(|_, meta| meta.id != id);
        self.save_registry()?;

        Ok(())
    }

    /// Find index ID by root path.
    pub fn find_by_path(&self, root: &Path) -> Option<String> {
        let normalized = root.to_string_lossy().replace('\\', "/");
        let path_hash = Self::hash_path(&normalized);
        self.registry
            .indexes
            .get(&path_hash)
            .map(|meta| meta.id.clone())
    }

    /// Find index by root path, returning full metadata.
    pub fn find_metadata_by_path(&self, root: &Path) -> Option<&IndexMetadata> {
        let normalized = root.to_string_lossy().replace('\\', "/");
        let path_hash = Self::hash_path(&normalized);
        self.registry.indexes.get(&path_hash)
    }

    /// Find a persistent index whose root is an ancestor of the given path.
    ///
    /// Returns the metadata and the relative path from the index root to the given path.
    /// Prefers the deepest (most specific) ancestor match.
    pub fn find_metadata_containing_path(&self, path: &Path) -> Option<(&IndexMetadata, String)> {
        let normalized = path.to_string_lossy().replace('\\', "/");
        let mut best: Option<(&IndexMetadata, String)> = None;

        for meta in self.registry.indexes.values() {
            let root = meta.root_path.trim_end_matches('/');
            let prefix = format!("{}/", root);
            if normalized.starts_with(&prefix) {
                let relative = &normalized[prefix.len()..];
                // Prefer deepest ancestor (longest root_path)
                if best.as_ref().map_or(true, |(b, _)| meta.root_path.len() > b.root_path.len()) {
                    best = Some((meta, relative.to_string()));
                }
            }
        }

        best
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
        validate_index_id(id)?;
        let path = self.storage_dir.join(format!("{}.json", id));
        let file_size = fs::metadata(&path)
            .with_context(|| format!("Index not found: {}", id))?
            .len();
        if file_size > MAX_INDEX_FILE_BYTES {
            anyhow::bail!("Index file too large ({} bytes)", file_size);
        }
        let data =
            fs::read(&path).with_context(|| format!("Failed to read index file for {}", id))?;

        let mut stored: StoredIndex = serde_json::from_slice(&data)
            .with_context(|| format!("Failed to parse index file for {}", id))?;
        ensure_supported_index_version(stored.version, id)?;
        let embeddings_path = embedding_store::sidecar_path(&self.storage_dir, id);
        if let Some(embeddings) = embedding_store::read_sidecar(&embeddings_path)? {
            stored.embeddings = Some(embeddings);
        }
        Ok(stored)
    }

    fn rebuild_index(&self, stored: StoredIndex) -> Result<IndexFile> {
        let StoredIndex {
            version,
            id,
            root_path: _,
            created_at: _,
            files,
            chunks,
            embeddings,
            embedding_model,
            symbols,
            edges,
        } = stored;
        let chunk_refs = crate::util::build_chunk_refs(&chunks);
        let inverted_index = build_inverted_index(&chunks);
        let stats = compute_stats(&files, &chunks);
        let (symbols, edges) = if symbols.is_empty() && edges.is_empty() {
            build_structural_indexes(&chunks)
        } else {
            (symbols, edges)
        };

        Ok(IndexFile {
            version,
            index_id: id,
            files,
            chunks,
            chunk_refs,
            inverted_index,
            stats,
            warnings: vec![],
            embeddings,
            embedding_model,
            symbols,
            edges,
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

    /// Insert a fake metadata entry into the registry for testing lookups.
    fn insert_metadata(store: &mut IndexStore, root_path: &str) {
        let normalized = root_path.replace('\\', "/");
        let path_hash = IndexStore::hash_path(&normalized);
        store.registry.indexes.insert(
            path_hash,
            IndexMetadata {
                id: format!("idx-{}", normalized.replace('/', "_")),
                root_path: normalized,
                created_at: 0,
                file_count: 1,
                chunk_count: 1,
            },
        );
    }

    #[test]
    fn test_rejects_old_schema_versions() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        fs::write(
            temp_dir.path().join("legacy.json"),
            r#"{
                "version": 2,
                "id": "legacy",
                "root_path": "/tmp/project",
                "created_at": 0,
                "files": [],
                "chunks": []
            }"#,
        )?;

        let err = store.load("legacy").expect_err("v2 index should be rejected");
        let message = err.to_string();
        assert!(message.contains("schema version 2"), "{message}");
        assert!(message.contains("Reindex"), "{message}");
        Ok(())
    }

    #[test]
    fn test_find_metadata_containing_path_basic() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        insert_metadata(&mut store, "/home/user/project");

        let result = store.find_metadata_containing_path(Path::new("/home/user/project/src/lib.rs"));
        let (meta, relative) = result.expect("should find containing index");
        assert_eq!(meta.root_path, "/home/user/project");
        assert_eq!(relative, "src/lib.rs");
        Ok(())
    }

    #[test]
    fn test_find_metadata_containing_path_deepest_match() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        insert_metadata(&mut store, "/home/user/project");
        insert_metadata(&mut store, "/home/user/project/packages/core");

        let result = store.find_metadata_containing_path(
            Path::new("/home/user/project/packages/core/src/main.rs"),
        );
        let (meta, relative) = result.expect("should find deepest ancestor");
        assert_eq!(meta.root_path, "/home/user/project/packages/core");
        assert_eq!(relative, "src/main.rs");
        Ok(())
    }

    #[test]
    fn test_find_metadata_containing_path_no_prefix_collision() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        insert_metadata(&mut store, "/proj/src");

        // "/proj/src2/foo.rs" must NOT match "/proj/src" because the
        // trailing '/' check prevents prefix collisions.
        let result = store.find_metadata_containing_path(Path::new("/proj/src2/foo.rs"));
        assert!(result.is_none(), "should not match /proj/src for /proj/src2/foo.rs");
        Ok(())
    }

    #[test]
    fn test_find_metadata_containing_path_exact_root_no_match() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        insert_metadata(&mut store, "/home/user/project");

        // The exact root path should NOT match (it's not a subdirectory).
        let result = store.find_metadata_containing_path(Path::new("/home/user/project"));
        assert!(result.is_none(), "exact root path should not match as contained");
        Ok(())
    }

    #[test]
    fn test_find_metadata_containing_path_no_match() -> Result<()> {
        let temp_dir = tempdir()?;
        let mut store = IndexStore::new(temp_dir.path().to_path_buf())?;
        insert_metadata(&mut store, "/home/user/project");

        let result = store.find_metadata_containing_path(Path::new("/home/user/other/file.rs"));
        assert!(result.is_none());
        Ok(())
    }
}
