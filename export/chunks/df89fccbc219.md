---
chunk_index: 1077
ref: "df89fccbc219"
id: "df89fccbc21947ce88ea040a9cee62a45899671465ee9f8d8ea1f0b934266ec9"
slug: "storage-l131-248"
path: "/home/zack/dev/llmx/ingestor-core/src/handlers/storage.rs"
kind: "text"
lines: [131, 248]
token_estimate: 1050
content_sha256: "ce106c4dcf3f8d4ca79a622e7941e6236d44d9c670f53b0cd2375a774e7d1af6"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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