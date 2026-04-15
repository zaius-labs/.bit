// diff.rs — Compare a .bitstore against expanded files on disk.

use std::collections::BTreeMap;
use std::path::Path;

use crate::store::{BitStore, StoreError};

/// Result of comparing a store against an expanded directory.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffResult {
    /// Files present on disk but not in the store.
    pub added: Vec<String>,
    /// Files present in both but with different content.
    pub modified: Vec<String>,
    /// Files in the store but missing from disk.
    pub deleted: Vec<String>,
}

/// Compare a .bitstore against the files in `dir`.
/// Only considers `.bit` files.
pub fn status(store_path: &Path, dir: &Path) -> Result<DiffResult, StoreError> {
    let mut store = BitStore::open(store_path)?;
    let blobs = store.list_all_blobs()?;

    // Build map of store paths -> hashes
    let store_files: BTreeMap<String, String> = blobs
        .iter()
        .map(|(p, _, h)| (p.clone(), h.clone()))
        .collect();

    // Collect .bit files on disk with hashes
    let mut disk_files = BTreeMap::new();
    collect_hashes(dir, dir, &mut disk_files)?;

    let mut result = DiffResult::default();

    for (path, store_hash) in &store_files {
        match disk_files.remove(path.as_str()) {
            Some(disk_hash) if disk_hash != *store_hash => result.modified.push(path.clone()),
            Some(_) => {} // unchanged
            None => result.deleted.push(path.clone()),
        }
    }
    for path in disk_files.into_keys() {
        result.added.push(path);
    }

    result.added.sort();
    result.modified.sort();
    result.deleted.sort();
    Ok(result)
}

fn collect_hashes(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, String>,
) -> Result<(), StoreError> {
    let entries = std::fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_hashes(root, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "bit") {
            let rel = path
                .strip_prefix(root)
                .expect("path under root")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read(&path)?;
            let hash = blake3::hash(&content).to_hex().to_string();
            out.insert(rel, hash);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collapse::collapse;
    use crate::expand::expand;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, std::path::PathBuf, TempDir) {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("a.bit"), "define:@A\n    name: \"\"!\n").unwrap();
        fs::write(src.path().join("b.bit"), "define:@B\n    val: 0#\n").unwrap();

        let store_path = src.path().join("test.bitstore");
        collapse(src.path(), &store_path).unwrap();

        let work = TempDir::new().unwrap();
        expand(&store_path, work.path()).unwrap();

        (src, store_path, work)
    }

    #[test]
    fn no_changes() {
        let (_src, store_path, work) = setup();
        let diff = status(&store_path, work.path()).unwrap();
        assert_eq!(diff, DiffResult::default());
    }

    #[test]
    fn modified_file() {
        let (_src, store_path, work) = setup();
        fs::write(work.path().join("a.bit"), "MODIFIED CONTENT").unwrap();

        let diff = status(&store_path, work.path()).unwrap();
        assert_eq!(diff.modified, vec!["a.bit".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.deleted.is_empty());
    }

    #[test]
    fn added_file() {
        let (_src, store_path, work) = setup();
        fs::write(work.path().join("new.bit"), "new content").unwrap();

        let diff = status(&store_path, work.path()).unwrap();
        assert_eq!(diff.added, vec!["new.bit".to_string()]);
        assert!(diff.modified.is_empty());
        assert!(diff.deleted.is_empty());
    }

    #[test]
    fn deleted_file() {
        let (_src, store_path, work) = setup();
        fs::remove_file(work.path().join("b.bit")).unwrap();

        let diff = status(&store_path, work.path()).unwrap();
        assert_eq!(diff.deleted, vec!["b.bit".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn mixed_changes() {
        let (_src, store_path, work) = setup();
        fs::write(work.path().join("a.bit"), "CHANGED").unwrap();
        fs::remove_file(work.path().join("b.bit")).unwrap();
        fs::write(work.path().join("c.bit"), "brand new").unwrap();

        let diff = status(&store_path, work.path()).unwrap();
        assert_eq!(diff.added, vec!["c.bit".to_string()]);
        assert_eq!(diff.modified, vec!["a.bit".to_string()]);
        assert_eq!(diff.deleted, vec!["b.bit".to_string()]);
    }
}
