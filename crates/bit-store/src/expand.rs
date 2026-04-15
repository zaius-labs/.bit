// expand.rs — Read blobs from a .bitstore and write them back to disk.

use std::path::Path;

use crate::store::{BitStore, StoreError};

/// Expand a .bitstore into a directory of .bit files.
/// Returns the number of files written.
pub fn expand(store_path: &Path, target_dir: &Path) -> Result<usize, StoreError> {
    let mut store = BitStore::open(store_path)?;
    let blobs = store.list_all_blobs()?;
    for (path, content, _hash) in &blobs {
        let dest = target_dir.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, content)?;
    }
    Ok(blobs.len())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collapse::collapse;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, std::path::PathBuf) {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("a.bit"), "define:@A\n    name: \"\"!\n").unwrap();
        fs::write(src.path().join("b.bit"), "define:@B\n    val: 0#\n").unwrap();
        fs::create_dir_all(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/c.bit"), "define:@C\n    x: \"\"!\n").unwrap();

        let store_path = src.path().join("test.bitstore");
        collapse(src.path(), &store_path).unwrap();
        (src, store_path)
    }

    #[test]
    fn expand_roundtrip_byte_for_byte() {
        let (src, store_path) = setup();
        let work = TempDir::new().unwrap();
        let count = expand(&store_path, work.path()).unwrap();
        assert_eq!(count, 3);

        // Each file should match byte-for-byte
        for name in &["a.bit", "b.bit", "sub/c.bit"] {
            let original = fs::read(src.path().join(name)).unwrap();
            let expanded = fs::read(work.path().join(name)).unwrap();
            assert_eq!(original, expanded, "mismatch for {name}");
        }
    }

    #[test]
    fn expand_creates_subdirs() {
        let (_src, store_path) = setup();
        let work = TempDir::new().unwrap();
        expand(&store_path, work.path()).unwrap();
        assert!(work.path().join("sub/c.bit").exists());
    }

    #[test]
    fn expand_empty_store() {
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("empty.bitstore");
        collapse(dir.path(), &store_path).unwrap();

        let work = TempDir::new().unwrap();
        let count = expand(&store_path, work.path()).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn collapse_expand_collapse_blobs_match() {
        let (src, store_path) = setup();

        // Expand
        let work = TempDir::new().unwrap();
        expand(&store_path, work.path()).unwrap();

        // Re-collapse from expanded dir
        let store_path2 = src.path().join("roundtrip.bitstore");
        let mut store2 = collapse(work.path(), &store_path2).unwrap();

        // Compare blob contents
        let mut store1 = BitStore::open(&store_path).unwrap();
        let blobs1 = store1.list_all_blobs().unwrap();
        let blobs2 = store2.list_all_blobs().unwrap();

        assert_eq!(blobs1.len(), blobs2.len());
        for ((p1, c1, h1), (p2, c2, h2)) in blobs1.iter().zip(blobs2.iter()) {
            assert_eq!(p1, p2);
            assert_eq!(c1, c2);
            assert_eq!(h1, h2);
        }
    }
}
