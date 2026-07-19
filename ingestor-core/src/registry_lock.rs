//! Cross-process serialization for the shared on-disk index registry.
//!
//! Both the CLI (`handlers::storage`) and the MCP backend (`mcp::storage`)
//! read-modify-write `{storage_dir}/registry.json`. With no coordination, two
//! concurrent `llmx` processes race in two ways:
//!
//! 1. **Torn write → corruption.** Both wrote a *fixed* temp path
//!    `registry.json.tmp` and renamed it into place. Interleaved writes to that
//!    shared temp leave a half-serialized file, so the next reader hits
//!    "corrupted registry, creating new one" and the registry is reset.
//! 2. **Lost update.** Each process loads the registry at startup, mutates its
//!    cached copy, then writes the whole thing back. A second process writing a
//!    stale snapshot silently drops entries the first one just added.
//!
//! This module fixes both: [`RegistryLock`] is an exclusive advisory lock held
//! across a read-modify-write so updates serialize, and [`write_registry_atomic`]
//! writes to a per-writer unique temp file before an atomic rename so a temp can
//! never be torn by another writer.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::Serialize;

/// Exclusive, cross-process advisory lock over the registry.
///
/// Acquire it before a read-modify-write of `registry.json` and hold it until
/// the write completes. The lock is released when this guard is dropped (the
/// underlying file handle closes, which releases the OS lock on every platform).
pub struct RegistryLock {
    _file: File,
}

impl RegistryLock {
    /// Acquire the exclusive lock, blocking until no other process holds it.
    ///
    /// The lock is keyed on `{storage_dir}/registry.lock`; every process that
    /// touches the registry in `storage_dir` contends on the same file, so the
    /// CLI and the MCP backend serialize against each other.
    pub fn acquire(storage_dir: &Path) -> Result<Self> {
        // The storage dir may not exist yet on a first run.
        fs::create_dir_all(storage_dir)
            .with_context(|| format!("Failed to create storage dir {}", storage_dir.display()))?;
        let path = storage_dir.join("registry.lock");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("Failed to open registry lock {}", path.display()))?;
        // Fully-qualified so we bind fs2's trait method rather than a same-named
        // inherent `File::lock_exclusive` on newer std toolchains.
        FileExt::lock_exclusive(&file)
            .context("Failed to acquire exclusive registry lock")?;
        Ok(Self { _file: file })
    }
}

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A temp path unique across processes (pid) and within a process (monotonic
/// counter), so concurrent writers never share a temp file and cannot tear one.
fn unique_temp_path(storage_dir: &Path) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    storage_dir.join(format!("registry.json.{pid}.{seq}.tmp"))
}

/// Atomically replace `registry.json` by writing a unique temp file and renaming
/// it into place. Callers should hold a [`RegistryLock`] across the surrounding
/// read-modify-write; the unique temp keeps even unlocked/legacy writers from
/// tearing each other's temp file.
pub fn write_registry_atomic<T: Serialize>(storage_dir: &Path, registry: &T) -> Result<()> {
    let json = serde_json::to_vec(registry).context("Failed to serialize registry")?;
    let temp = unique_temp_path(storage_dir);

    let result = (|| -> Result<()> {
        let mut file = File::create(&temp).context("Failed to create temp registry")?;
        file.write_all(&json).context("Failed to write temp registry")?;
        file.sync_all().context("Failed to fsync temp registry")?;
        drop(file);
        fs::rename(&temp, storage_dir.join("registry.json"))
            .context("Failed to rename temp registry")?;
        Ok(())
    })();

    // Never leave our own temp behind on failure.
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::thread;

    fn load(dir: &Path) -> HashMap<String, u64> {
        let path = dir.join("registry.json");
        if !path.exists() {
            return HashMap::new();
        }
        let data = fs::read(&path).unwrap();
        // A torn/half-written registry would fail to parse here — that panic is
        // exactly the "corrupted registry" failure this module prevents.
        serde_json::from_slice(&data).unwrap()
    }

    /// Many threads (standing in for concurrent llmx processes, since fs2 locks
    /// exclude across distinct file handles even in one process) hammer the same
    /// registry with locked read-modify-writes. With the old fixed-temp,
    /// lock-free writer this loses entries and/or tears the temp file; with the
    /// lock + unique-temp writer every entry must survive and the file must stay
    /// parseable throughout.
    #[test]
    fn concurrent_updates_neither_lose_entries_nor_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = Arc::new(dir.path().to_path_buf());
        let threads = 16u64;
        let rounds = 8u64;

        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let dir_path = Arc::clone(&dir_path);
                thread::spawn(move || {
                    for r in 0..rounds {
                        let _lock = RegistryLock::acquire(&dir_path).unwrap();
                        let mut registry = load(&dir_path);
                        registry.insert(format!("key-{t}-{r}"), t * 1000 + r);
                        write_registry_atomic(&dir_path, &registry).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let final_registry = load(&dir_path);
        assert_eq!(final_registry.len() as u64, threads * rounds);
        for t in 0..threads {
            for r in 0..rounds {
                assert_eq!(final_registry.get(&format!("key-{t}-{r}")), Some(&(t * 1000 + r)));
            }
        }
    }
}
