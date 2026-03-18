use std::path::{Component, Path, PathBuf};

pub fn normalize_separators(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn normalize_root_path(path: &Path) -> String {
    normalize_separators(&path.to_string_lossy())
}

pub fn normalize_relative_path(path: &Path) -> String {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_string_lossy().to_string()),
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => parts.push("..".to_string()),
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("/")
    }
}

pub fn relativize_path(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => normalize_relative_path(relative),
        Err(_) => normalize_relative_path(path),
    }
}

pub fn common_ancestor<'a, I>(paths: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = &'a Path>,
{
    let mut iter = paths.into_iter();
    let first = iter.next()?.to_path_buf();
    let mut common: Vec<Component<'_>> = first.components().collect();

    for path in iter {
        let components: Vec<Component<'_>> = path.components().collect();
        let shared = common
            .iter()
            .zip(components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common.truncate(shared);
        if common.is_empty() {
            break;
        }
    }

    if common.is_empty() {
        return None;
    }

    let mut out = PathBuf::new();
    for component in common {
        out.push(component.as_os_str());
    }
    Some(out)
}

pub fn infer_root_path(paths: &[PathBuf]) -> Option<PathBuf> {
    let dirs: Vec<&Path> = paths
        .iter()
        .map(|path| if path.is_dir() { path.as_path() } else { path.parent().unwrap_or(path.as_path()) })
        .collect();
    common_ancestor(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relativize_path_uses_root_relative_layout() {
        let root = Path::new("/tmp/project");
        let path = Path::new("/tmp/project/core/src/exec.rs");
        assert_eq!(relativize_path(path, root), "core/src/exec.rs");
    }

    #[test]
    fn test_infer_root_path_for_multiple_inputs() {
        let root = infer_root_path(&[
            PathBuf::from("/tmp/project/core/src/exec.rs"),
            PathBuf::from("/tmp/project/core/src/lib.rs"),
        ])
        .expect("shared root");
        assert_eq!(normalize_root_path(&root), "/tmp/project/core/src");
    }
}
