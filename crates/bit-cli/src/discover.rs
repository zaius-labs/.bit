use std::path::{Path, PathBuf};

/// Search for a .bitstore file starting from `start_dir`, walking up to the filesystem root.
///
/// Priority: project.bitstore > .bitstore > first *.bitstore found
pub fn find_store(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        if let Some(found) = find_store_in_dir(&dir) {
            return Some(found);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Find the best .bitstore file in a single directory.
fn find_store_in_dir(dir: &Path) -> Option<PathBuf> {
    let project = dir.join("project.bitstore");
    if project.is_file() {
        return Some(project);
    }

    let dot = dir.join(".bitstore");
    if dot.is_file() {
        return Some(dot);
    }

    // Fallback: first *.bitstore file found
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "bitstore" {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Resolve a store path: use the provided path if Some, otherwise auto-discover from CWD.
/// Returns an error message if no store is found.
pub fn resolve_store(explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(PathBuf::from(path));
    }
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot get current directory: {}", e))?;
    find_store(&cwd)
        .ok_or_else(|| "No .bitstore found. Run 'bit collapse .' to create one.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_find_store_project_priority() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Create multiple .bitstore files
        fs::write(dir.join("project.bitstore"), b"").unwrap();
        fs::write(dir.join("other.bitstore"), b"").unwrap();

        let found = find_store(dir).unwrap();
        assert_eq!(found, dir.join("project.bitstore"));
    }

    #[test]
    fn test_find_store_dot_priority() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        fs::write(dir.join(".bitstore"), b"").unwrap();
        fs::write(dir.join("other.bitstore"), b"").unwrap();

        let found = find_store(dir).unwrap();
        assert_eq!(found, dir.join(".bitstore"));
    }

    #[test]
    fn test_find_store_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        fs::write(dir.join("my.bitstore"), b"").unwrap();

        let found = find_store(dir).unwrap();
        assert_eq!(found, dir.join("my.bitstore"));
    }

    #[test]
    fn test_find_store_walks_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let child = parent.join("sub");
        fs::create_dir(&child).unwrap();

        fs::write(parent.join("project.bitstore"), b"").unwrap();

        let found = find_store(&child).unwrap();
        assert_eq!(found, parent.join("project.bitstore"));
    }

    #[test]
    fn test_find_store_none() {
        let tmp = tempfile::tempdir().unwrap();
        // Empty dir, no .bitstore files
        assert!(find_store(tmp.path()).is_none());
    }
}
