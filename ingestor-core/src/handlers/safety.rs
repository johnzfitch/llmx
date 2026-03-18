//! Safety checks and limits for dynamic file walking.
//!
//! Provides protection against:
//! - Dangerous paths (/, /home, /usr, etc.)
//! - Runaway file counts (max 10K files)
//! - Excessive total size (max 100MB)
//! - Timeout protection (max 30 seconds)
//! - Respects .gitignore patterns

use crate::walk::{collect_files, WalkConfig};
use anyhow::Result;
use std::path::Path;

/// Safety limits for dynamic file walking.
#[derive(Debug, Clone)]
pub struct SafetyLimits {
    /// Maximum directory depth to traverse (default: 10)
    pub max_depth: usize,
    /// Maximum number of files to process (default: 10,000)
    pub max_files: usize,
    /// Maximum total bytes to read (default: 100MB)
    pub max_total_bytes: usize,
    /// Timeout in seconds (default: 30)
    pub timeout_secs: u64,
    /// Whether to respect .gitignore files (default: true)
    pub respect_gitignore: bool,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_files: 10_000,
            max_total_bytes: 100 * 1024 * 1024, // 100MB
            timeout_secs: 30,
            respect_gitignore: true,
        }
    }
}

pub type WalkStats = crate::walk::WalkStats;

/// Known dangerous paths that should be rejected without --force.
const DANGEROUS_PATHS: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/home",
    "/lib",
    "/lib64",
    "/opt",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/srv",
    "/sys",
    "/tmp",
    "/usr",
    "/var",
];

/// Project root markers - if none exist, warn the user.
pub const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "Makefile",
    "CMakeLists.txt",
    ".hg",
    ".svn",
    "pom.xml",
    "build.gradle",
    "setup.py",
    "requirements.txt",
    "Gemfile",
    "composer.json",
];

/// Check if a path is considered dangerous (system root, home, etc.)
pub fn is_dangerous_path(path: &Path) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = canonical.to_string_lossy();

    // Check exact matches
    for dangerous in DANGEROUS_PATHS {
        if path_str == *dangerous {
            return true;
        }
    }

    // Check if it's a direct child of /home (but not deeper)
    if path_str.starts_with("/home/") {
        let parts: Vec<&str> = path_str.trim_start_matches('/').split('/').collect();
        if parts.len() <= 2 {
            // /home or /home/username
            return true;
        }
    }

    false
}

/// Check if path has any project markers.
pub fn has_project_marker(path: &Path) -> bool {
    for marker in PROJECT_MARKERS {
        if path.join(marker).exists() {
            return true;
        }
    }
    false
}

/// Find the project root by walking up from the given path.
pub fn find_project_root(start: &Path) -> Option<std::path::PathBuf> {
    let canonical = start.canonicalize().ok()?;
    let mut current = canonical.as_path();

    loop {
        for marker in PROJECT_MARKERS {
            if current.join(marker).exists() {
                return Some(current.to_path_buf());
            }
        }

        current = current.parent()?;
    }
}

/// Perform a safe file walk with limits and .gitignore awareness.
///
/// Uses the `ignore` crate (same as ripgrep) for fast, .gitignore-respecting traversal.
pub fn dynamic_walk(root: &Path, limits: &SafetyLimits) -> Result<(Vec<crate::FileInput>, WalkStats)> {
    let config = WalkConfig {
        max_depth: limits.max_depth,
        max_files: limits.max_files,
        max_total_bytes: limits.max_total_bytes,
        timeout_secs: limits.timeout_secs,
        respect_gitignore: limits.respect_gitignore,
    };
    collect_files(root, root, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_path_detection() {
        assert!(is_dangerous_path(Path::new("/")));
        assert!(is_dangerous_path(Path::new("/home")));
        assert!(is_dangerous_path(Path::new("/usr")));
        assert!(is_dangerous_path(Path::new("/etc")));
        assert!(is_dangerous_path(Path::new("/tmp")));

        // /home/user is dangerous, but /home/user/project is not
        // (these tests may fail if paths don't exist, so we check what we can)
    }

    #[test]
    fn test_project_markers() {
        // Create a temp dir with a Cargo.toml
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("Cargo.toml");
        std::fs::write(&marker, "[package]").unwrap();

        assert!(has_project_marker(temp.path()));

        // Without marker
        let empty = tempfile::tempdir().unwrap();
        assert!(!has_project_marker(empty.path()));
    }

    #[test]
    fn test_find_project_root() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join(".git");
        std::fs::create_dir(&marker).unwrap();

        let subdir = temp.path().join("src").join("lib");
        std::fs::create_dir_all(&subdir).unwrap();

        let root = find_project_root(&subdir);
        assert!(root.is_some());
        assert_eq!(root.unwrap(), temp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_dynamic_walk_respects_file_limit() {
        let temp = tempfile::tempdir().unwrap();

        // Create 5 files
        for i in 0..5 {
            let path = temp.path().join(format!("file{}.rs", i));
            std::fs::write(&path, "fn main() {}").unwrap();
        }

        // Walk with limit of 3
        let limits = SafetyLimits {
            max_files: 3,
            ..Default::default()
        };

        let (files, stats) = dynamic_walk(temp.path(), &limits).unwrap();

        assert_eq!(files.len(), 3);
        assert!(stats.truncated);
        assert_eq!(stats.truncation_reason, Some("file_limit".to_string()));
    }

    #[test]
    fn test_dynamic_walk_respects_size_limit() {
        let temp = tempfile::tempdir().unwrap();

        // Create files with known sizes
        for i in 0..3 {
            let path = temp.path().join(format!("file{}.rs", i));
            // Each file is 100 bytes
            std::fs::write(&path, "x".repeat(100)).unwrap();
        }

        // Walk with limit of 250 bytes (should get 2 files)
        let limits = SafetyLimits {
            max_total_bytes: 250,
            ..Default::default()
        };

        let (files, stats) = dynamic_walk(temp.path(), &limits).unwrap();

        assert_eq!(files.len(), 2);
        assert!(stats.truncated);
        assert_eq!(stats.truncation_reason, Some("size_limit".to_string()));
    }
}
