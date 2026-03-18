use crate::model::FileInput;
use crate::pathnorm::{infer_root_path, normalize_root_path, relativize_path};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

pub const ALLOWED_EXTENSIONS: &[&str] = &[
    "rs",
    "js", "ts", "tsx", "jsx", "mjs", "cjs",
    "html", "css", "scss", "sass", "less",
    "json", "yaml", "yml", "toml",
    "md", "txt",
    "py",
    "go",
    "c", "cpp", "cc", "cxx", "h", "hpp", "hxx",
    "java",
    "rb",
    "php",
    "swift",
    "sh", "bash", "zsh",
    "sql",
    "log", "jsonl", "csv", "xml", "ini", "cfg", "conf",
];

pub const ALLOWED_DOTFILES: &[&str] = &[".npmrc", ".nvmrc", ".editorconfig", ".gitignore"];

const EXCLUDED_DIR_PREFIXES: &[&str] = &[
    "target",
    "dist",
    "build",
    "node_modules",
    "coverage",
    "export",
    "llmx-export",
    ".next",
    ".turbo",
    "web/pkg",
    "web/models",
];

#[derive(Debug, Clone)]
pub struct WalkConfig {
    pub max_depth: usize,
    pub max_files: usize,
    pub max_total_bytes: usize,
    pub timeout_secs: u64,
    pub respect_gitignore: bool,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_files: 10_000,
            max_total_bytes: 100 * 1024 * 1024,
            timeout_secs: 30,
            respect_gitignore: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WalkStats {
    pub file_count: usize,
    pub total_bytes: usize,
    pub skipped_count: usize,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub elapsed_ms: u64,
}

pub fn collect_input_files(paths: &[String], config: &WalkConfig) -> Result<(Vec<FileInput>, String, WalkStats)> {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(config.timeout_secs);
    let canonical_paths: Vec<PathBuf> = paths
        .iter()
        .map(|path| PathBuf::from(path).canonicalize().with_context(|| format!("Invalid path: {path}")))
        .collect::<Result<_>>()?;

    let root = infer_root_path(&canonical_paths)
        .context("Could not determine a common root path for the requested inputs")?;
    let root_path = normalize_root_path(&root);

    let mut files = Vec::new();
    let mut stats = WalkStats::default();

    for path in &canonical_paths {
        if start.elapsed() > timeout {
            stats.truncated = true;
            stats.truncation_reason = Some("timeout".to_string());
            break;
        }
        if stats.file_count >= config.max_files {
            stats.truncated = true;
            stats.truncation_reason = Some("file_limit".to_string());
            break;
        }
        if stats.total_bytes >= config.max_total_bytes {
            stats.truncated = true;
            stats.truncation_reason = Some("size_limit".to_string());
            break;
        }

        let remaining_timeout = config.timeout_secs.saturating_sub(start.elapsed().as_secs());
        let remaining_config = WalkConfig {
            max_depth: config.max_depth,
            max_files: config.max_files.saturating_sub(stats.file_count),
            max_total_bytes: config.max_total_bytes.saturating_sub(stats.total_bytes),
            timeout_secs: remaining_timeout.max(1),
            respect_gitignore: config.respect_gitignore,
        };

        if path.is_dir() {
            let (mut dir_files, dir_stats) = collect_files(path, &root, &remaining_config)?;
            merge_stats(&mut stats, dir_stats);
            files.append(&mut dir_files);
        } else if path.is_file() {
            let relative = relativize_path(path, &root);
            if should_exclude_relative_path(&relative) {
                stats.skipped_count += 1;
                continue;
            }
            if let Some(file) = read_file(path, &root)? {
                if stats.total_bytes + file.data.len() > config.max_total_bytes {
                    stats.truncated = true;
                    stats.truncation_reason = Some("size_limit".to_string());
                    break;
                }
                stats.total_bytes += file.data.len();
                stats.file_count += 1;
                files.push(file);
            } else {
                stats.skipped_count += 1;
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);

    Ok((files, root_path, stats))
}

pub fn collect_files(root: &Path, path_root: &Path, config: &WalkConfig) -> Result<(Vec<FileInput>, WalkStats)> {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(config.timeout_secs);
    let mut files = Vec::new();
    let mut stats = WalkStats::default();

    let mut builder = WalkBuilder::new(root);
    builder
        .max_depth(Some(config.max_depth))
        .hidden(true)
        .git_ignore(config.respect_gitignore)
        .git_global(config.respect_gitignore)
        .git_exclude(config.respect_gitignore)
        .ignore(true)
        .follow_links(false)
        .same_file_system(true);

    let walker = builder.build();

    for entry in walker {
        if start.elapsed() > timeout {
            stats.truncated = true;
            stats.truncation_reason = Some("timeout".to_string());
            break;
        }
        if stats.file_count >= config.max_files {
            stats.truncated = true;
            stats.truncation_reason = Some("file_limit".to_string());
            break;
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                stats.skipped_count += 1;
                continue;
            }
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let relative = relativize_path(path, path_root);
        if should_exclude_relative_path(&relative) {
            stats.skipped_count += 1;
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => {
                stats.skipped_count += 1;
                continue;
            }
        };
        let file_size = metadata.len() as usize;
        if stats.total_bytes + file_size > config.max_total_bytes {
            stats.truncated = true;
            stats.truncation_reason = Some("size_limit".to_string());
            break;
        }

        match read_file(path, path_root)? {
            Some(file) => {
                stats.total_bytes += file_size;
                stats.file_count += 1;
                files.push(file);
            }
            None => {
                stats.skipped_count += 1;
            }
        }
    }

    stats.elapsed_ms = start.elapsed().as_millis() as u64;
    Ok((files, stats))
}

pub fn read_file(path: &Path, path_root: &Path) -> Result<Option<FileInput>> {
    if !should_index_path(path) {
        return Ok(None);
    }

    let metadata = fs::metadata(path)?;
    let data = fs::read(path)?;
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    Ok(Some(FileInput {
        path: relativize_path(path, path_root),
        data,
        mtime_ms,
        fingerprint_sha256: None,
    }))
}

pub fn should_index_path(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return ALLOWED_EXTENSIONS.contains(&ext);
    }

    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        return ALLOWED_DOTFILES.contains(&name);
    }

    false
}

pub fn should_exclude_relative_path(relative_path: &str) -> bool {
    let relative_path = relative_path.trim_start_matches("./");
    if relative_path.is_empty() {
        return false;
    }

    for excluded in EXCLUDED_DIR_PREFIXES {
        if relative_path == *excluded || relative_path.starts_with(&format!("{excluded}/")) {
            return true;
        }
    }

    relative_path.ends_with(".zip") || relative_path.ends_with(".log") || relative_path.ends_with(".pid")
}

fn merge_stats(into: &mut WalkStats, next: WalkStats) {
    into.file_count += next.file_count;
    into.total_bytes += next.total_bytes;
    into.skipped_count += next.skipped_count;
    into.elapsed_ms += next.elapsed_ms;
    if next.truncated {
        into.truncated = true;
        if into.truncation_reason.is_none() {
            into.truncation_reason = next.truncation_reason;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_excludes_generated_and_log_paths() {
        assert!(should_exclude_relative_path("export/index.json"));
        assert!(should_exclude_relative_path("llmx-export/chunks/0001.md"));
        assert!(should_exclude_relative_path("target/debug/app"));
        assert!(should_exclude_relative_path("server.log"));
        assert!(!should_exclude_relative_path("core/src/exec.rs"));
    }

    #[test]
    fn test_collect_input_files_enforces_global_file_limit() -> Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fs::write(root.join("a.rs"), "fn a() {}\n")?;
        fs::write(root.join("b.rs"), "fn b() {}\n")?;

        let (files, root_path, stats) = collect_input_files(
            &[root.join("a.rs").to_string_lossy().to_string(), root.join("b.rs").to_string_lossy().to_string()],
            &WalkConfig {
                max_depth: 5,
                max_files: 1,
                max_total_bytes: 1024,
                timeout_secs: 30,
                respect_gitignore: true,
            },
        )?;

        assert_eq!(files.len(), 1);
        assert_eq!(stats.file_count, 1);
        assert!(stats.truncated);
        assert_eq!(stats.truncation_reason.as_deref(), Some("file_limit"));
        assert_eq!(root_path, normalize_root_path(root));
        Ok(())
    }
}
