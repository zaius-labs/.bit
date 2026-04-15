// btree.rs — B-tree search, insert, and leaf/interior splitting for bitstore
//
// Sits on top of the Pager. Uses LeafPage and InteriorPage for storage.
// Convention: interior cell.child_page points LEFT (keys < cell.key).
// rightmost_child points RIGHT (keys >= last cell key).

use crate::page::*;
use crate::pager::*;

/// A list of key-value byte pairs returned by scan operations.
type KvPairs = Vec<(Vec<u8>, Vec<u8>)>;

/// Page header: [type: u8][page_num: u32][cell_count: u16][extra: u32] = 11 bytes
const PAGE_HEADER_SIZE: usize = 11;

#[derive(Debug)]
pub enum BTreeError {
    Pager(PagerError),
    Page(PageError),
    KeyNotFound,
    EmptyTree,
}

impl std::fmt::Display for BTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BTreeError::Pager(e) => write!(f, "btree pager error: {e}"),
            BTreeError::Page(e) => write!(f, "btree page error: {e}"),
            BTreeError::KeyNotFound => write!(f, "key not found"),
            BTreeError::EmptyTree => write!(f, "empty tree"),
        }
    }
}

impl std::error::Error for BTreeError {}

impl From<PagerError> for BTreeError {
    fn from(e: PagerError) -> Self {
        BTreeError::Pager(e)
    }
}

impl From<PageError> for BTreeError {
    fn from(e: PageError) -> Self {
        BTreeError::Page(e)
    }
}

/// Result of splitting a node: the median key and the new right child page number.
struct SplitResult {
    median_key: Vec<u8>,
    right_page: u32,
}

pub struct BTree<'a> {
    pager: &'a mut Pager,
    root_page: u32, // 0 means empty tree
}

impl<'a> BTree<'a> {
    pub fn new(pager: &'a mut Pager, root_page: u32) -> Self {
        BTree { pager, root_page }
    }

    /// Get the current root page number.
    pub fn root(&self) -> u32 {
        self.root_page
    }

    /// Search for a key. Returns the value if found.
    pub fn search(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, BTreeError> {
        if self.root_page == 0 {
            return Ok(None);
        }

        let mut current = self.root_page;
        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => {
                    let leaf = LeafPage::read_from(buf)?;
                    match leaf.cells.binary_search_by(|c| c.key.as_slice().cmp(key)) {
                        Ok(idx) => return Ok(Some(leaf.cells[idx].value.clone())),
                        Err(_) => return Ok(None),
                    }
                }
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    current = find_child(&interior, key);
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }
    }

    /// Insert a key-value pair. If key exists, updates the value.
    /// Returns the (possibly new) root page number.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<u32, BTreeError> {
        if self.root_page == 0 {
            // Create first leaf
            let page_num = self.pager.allocate()?;
            let mut leaf = LeafPage::new(page_num);
            leaf.cells.push(Cell {
                key: key.to_vec(),
                value: value.to_vec(),
                child_page: 0,
            });
            self.write_leaf(&leaf)?;
            self.root_page = page_num;
            return Ok(self.root_page);
        }

        // Find the leaf, tracking the path from root
        // path entries: (page_num of interior, index of child followed)
        let mut path: Vec<(u32, usize)> = Vec::new();
        let mut current = self.root_page;

        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => break,
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    let child_idx = find_child_index(&interior, key);
                    path.push((current, child_idx));
                    current = if child_idx < interior.cells.len() {
                        interior.cells[child_idx].child_page
                    } else {
                        interior.rightmost_child
                    };
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }

        // current is the leaf page
        let buf = self.pager.read_page(current)?;
        let mut leaf = LeafPage::read_from(buf)?;

        // Check for existing key (update in place)
        match leaf.cells.binary_search_by(|c| c.key.as_slice().cmp(key)) {
            Ok(idx) => {
                leaf.cells[idx].value = value.to_vec();
                self.write_leaf(&leaf)?;
                return Ok(self.root_page);
            }
            Err(idx) => {
                leaf.cells.insert(
                    idx,
                    Cell {
                        key: key.to_vec(),
                        value: value.to_vec(),
                        child_page: 0,
                    },
                );
            }
        }

        // Check if leaf still fits
        if PAGE_HEADER_SIZE + leaf.used_space() <= PAGE_SIZE {
            self.write_leaf(&leaf)?;
            return Ok(self.root_page);
        }

        // Need to split the leaf
        let split = self.split_leaf(&mut leaf)?;
        self.propagate_split(leaf.page_num, split, &mut path)?;

        Ok(self.root_page)
    }

    /// Split a leaf page. Left half stays, right half goes to new page.
    fn split_leaf(&mut self, leaf: &mut LeafPage) -> Result<SplitResult, BTreeError> {
        let mid = leaf.cells.len() / 2;
        let right_cells = leaf.cells.split_off(mid);

        let right_page_num = self.pager.allocate()?;
        let mut right_leaf = LeafPage::new(right_page_num);
        right_leaf.cells = right_cells;
        right_leaf.next_leaf = leaf.next_leaf;
        leaf.next_leaf = right_page_num;

        let median_key = right_leaf.cells[0].key.clone();

        self.write_leaf(leaf)?;
        self.write_leaf(&right_leaf)?;

        Ok(SplitResult {
            median_key,
            right_page: right_page_num,
        })
    }

    /// Propagate a split upward through the tree.
    /// left_page is the original page (now containing left half).
    /// split contains the median key and the new right page.
    fn propagate_split(
        &mut self,
        left_page: u32,
        split: SplitResult,
        path: &mut Vec<(u32, usize)>,
    ) -> Result<(), BTreeError> {
        let mut left_child = left_page;
        let mut current_split = split;

        loop {
            if path.is_empty() {
                // Need a new root
                let new_root_num = self.pager.allocate()?;
                let mut new_root = InteriorPage::new(new_root_num);
                new_root.cells.push(Cell {
                    key: current_split.median_key,
                    value: Vec::new(),
                    child_page: left_child,
                });
                new_root.rightmost_child = current_split.right_page;
                self.write_interior(&new_root)?;
                self.root_page = new_root_num;
                return Ok(());
            }

            let (parent_page_num, _child_idx) = path.pop().unwrap();

            let buf = self.pager.read_page(parent_page_num)?;
            let mut parent = InteriorPage::read_from(buf)?;

            // Insert the median key into the parent.
            // The new cell's child_page points LEFT (to left_child).
            // We need to find where to insert based on the median key.
            let insert_pos = parent
                .cells
                .iter()
                .position(|c| current_split.median_key.as_slice() < c.key.as_slice())
                .unwrap_or(parent.cells.len());

            parent.cells.insert(
                insert_pos,
                Cell {
                    key: current_split.median_key.clone(),
                    value: Vec::new(),
                    child_page: left_child,
                },
            );

            // The right child of the new median needs to be wired in:
            // - If there's a cell after insert_pos, that cell's child_page was pointing
            //   to the old (pre-split) page. Now left_child has taken that role via the
            //   new cell. The cell at insert_pos+1 should keep its existing child_page
            //   (which points to something else). So we need to set the "right pointer"
            //   of the median to current_split.right_page.
            //
            // The right pointer of cell at insert_pos is:
            //   - cells[insert_pos+1].child_page if insert_pos+1 < cells.len()
            //   - rightmost_child if insert_pos is the last cell
            //
            // But wait — the cell AFTER our insertion already has its own left-child pointer.
            // What we actually need: the pointer that USED to point to left_child (the
            // pre-split page) should now point to right_page instead, and our new cell
            // takes over pointing to left_child.
            //
            // Before insertion, the slot that pointed to the pre-split node was either:
            //   - Some cell's child_page, or rightmost_child.
            // After inserting, that same slot should now point to right_page.
            if insert_pos + 1 < parent.cells.len() {
                parent.cells[insert_pos + 1].child_page = current_split.right_page;
            } else {
                parent.rightmost_child = current_split.right_page;
            }

            // Check if parent fits
            let used: usize = parent.cells.iter().map(|c| c.interior_size()).sum();
            if PAGE_HEADER_SIZE + used <= PAGE_SIZE {
                self.write_interior(&parent)?;
                return Ok(());
            }

            // Split the interior node
            let interior_split = self.split_interior(&mut parent)?;
            left_child = parent.page_num;
            current_split = interior_split;
            // Continue loop to propagate upward
        }
    }

    /// Split an interior page.
    fn split_interior(&mut self, interior: &mut InteriorPage) -> Result<SplitResult, BTreeError> {
        let mid = interior.cells.len() / 2;

        // The median cell's key goes up. Its child_page becomes the left page's rightmost_child.
        // Cells after the median go to the right page.
        let right_cells = interior.cells.split_off(mid + 1);
        let median_cell = interior.cells.pop().unwrap(); // the cell at mid

        // Left page keeps cells [0..mid), rightmost_child = median_cell.child_page
        // Wait — the median cell's child_page points LEFT of the median key.
        // So everything left of median (including median's left child) stays in the left page.
        // The left page's rightmost_child should be median_cell.child_page
        // because those keys are >= left page's last cell key but < median key.
        let old_rightmost = interior.rightmost_child;
        interior.rightmost_child = median_cell.child_page;

        let right_page_num = self.pager.allocate()?;
        let mut right_interior = InteriorPage::new(right_page_num);
        right_interior.cells = right_cells;
        right_interior.rightmost_child = old_rightmost;

        // Wait — that's wrong. Let me think again.
        // Before split, the interior had cells: [c0, c1, ..., c_mid, ..., c_n]
        // with rightmost_child = R
        //
        // After split_off(mid+1), interior.cells = [c0..c_mid] (inclusive)
        // right_cells = [c_{mid+1}..c_n]
        // Then we pop c_mid from interior.cells, so interior.cells = [c0..c_{mid-1}]
        //
        // c_mid.key goes up as the median.
        // c_mid.child_page points to keys < c_mid.key but >= c_{mid-1}.key
        // So c_mid.child_page should be left page's rightmost_child. Correct.
        //
        // Right page has cells [c_{mid+1}..c_n].
        // Right page's rightmost_child should be R (the original rightmost). Correct.

        self.write_interior(interior)?;
        self.write_interior(&right_interior)?;

        Ok(SplitResult {
            median_key: median_cell.key,
            right_page: right_page_num,
        })
    }

    fn write_leaf(&mut self, leaf: &LeafPage) -> Result<(), BTreeError> {
        let mut buf = [0u8; PAGE_SIZE];
        leaf.write_to(&mut buf)?;
        self.pager.write_page(leaf.page_num, buf)?;
        Ok(())
    }

    fn write_interior(&mut self, interior: &InteriorPage) -> Result<(), BTreeError> {
        let mut buf = [0u8; PAGE_SIZE];
        interior.write_to(&mut buf)?;
        self.pager.write_page(interior.page_num, buf)?;
        Ok(())
    }

    /// Delete a key. Returns the old value if found, None if not present.
    /// Does not merge/rebalance — leaves may become underfull.
    pub fn delete(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, BTreeError> {
        if self.root_page == 0 {
            return Ok(None);
        }

        // Walk to the leaf
        let mut current = self.root_page;
        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => {
                    let mut leaf = LeafPage::read_from(buf)?;
                    match leaf.cells.binary_search_by(|c| c.key.as_slice().cmp(key)) {
                        Ok(idx) => {
                            let old_value = leaf.cells.remove(idx).value;
                            self.write_leaf(&leaf)?;
                            return Ok(Some(old_value));
                        }
                        Err(_) => return Ok(None),
                    }
                }
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    current = find_child(&interior, key);
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }
    }

    /// Range scan: returns all key-value pairs where start_key <= key < end_key, sorted.
    pub fn scan(&mut self, start_key: &[u8], end_key: &[u8]) -> Result<KvPairs, BTreeError> {
        if self.root_page == 0 {
            return Ok(Vec::new());
        }

        // Walk to the leaf containing start_key
        let mut current = self.root_page;
        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => break,
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    current = find_child(&interior, start_key);
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }

        // Now scan from current leaf forward
        let mut results = Vec::new();
        let mut leaf_page = current;
        loop {
            let buf = self.pager.read_page(leaf_page)?;
            let leaf = LeafPage::read_from(buf)?;

            for cell in &leaf.cells {
                if cell.key.as_slice() >= end_key {
                    return Ok(results);
                }
                if cell.key.as_slice() >= start_key {
                    results.push((cell.key.clone(), cell.value.clone()));
                }
            }

            if leaf.next_leaf == 0 {
                break;
            }
            leaf_page = leaf.next_leaf;
        }

        Ok(results)
    }

    /// Scan all keys with the given prefix.
    pub fn scan_prefix(&mut self, prefix: &[u8]) -> Result<KvPairs, BTreeError> {
        match prefix_end(prefix) {
            Some(end) => self.scan(prefix, &end),
            None => {
                // prefix is all 0xFF — scan from prefix to end of tree
                self.scan_from(prefix)
            }
        }
    }

    /// Scan all key-value pairs in the tree, sorted by key.
    pub fn scan_all(&mut self) -> Result<KvPairs, BTreeError> {
        if self.root_page == 0 {
            return Ok(Vec::new());
        }

        // Find leftmost leaf
        let mut current = self.root_page;
        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => break,
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    // Always go to the first (leftmost) child
                    if interior.cells.is_empty() {
                        current = interior.rightmost_child;
                    } else {
                        current = interior.cells[0].child_page;
                    }
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }

        // Follow leaf chain
        let mut results = Vec::new();
        let mut leaf_page = current;
        loop {
            let buf = self.pager.read_page(leaf_page)?;
            let leaf = LeafPage::read_from(buf)?;

            for cell in &leaf.cells {
                results.push((cell.key.clone(), cell.value.clone()));
            }

            if leaf.next_leaf == 0 {
                break;
            }
            leaf_page = leaf.next_leaf;
        }

        Ok(results)
    }

    /// Scan all keys >= start_key (no upper bound). Used by scan_prefix when prefix is all 0xFF.
    fn scan_from(&mut self, start_key: &[u8]) -> Result<KvPairs, BTreeError> {
        if self.root_page == 0 {
            return Ok(Vec::new());
        }

        // Walk to the leaf containing start_key
        let mut current = self.root_page;
        loop {
            let buf = self.pager.read_page(current)?;
            let page_type = PageType::from_u8(buf[0])?;

            match page_type {
                PageType::BTreeLeaf => break,
                PageType::BTreeInterior => {
                    let interior = InteriorPage::read_from(buf)?;
                    current = find_child(&interior, start_key);
                }
                _ => return Err(BTreeError::Page(PageError::InvalidPageType(buf[0]))),
            }
        }

        let mut results = Vec::new();
        let mut leaf_page = current;
        loop {
            let buf = self.pager.read_page(leaf_page)?;
            let leaf = LeafPage::read_from(buf)?;

            for cell in &leaf.cells {
                if cell.key.as_slice() >= start_key {
                    results.push((cell.key.clone(), cell.value.clone()));
                }
            }

            if leaf.next_leaf == 0 {
                break;
            }
            leaf_page = leaf.next_leaf;
        }

        Ok(results)
    }
}

/// Compute the exclusive upper bound for a prefix scan.
/// Returns None if prefix is all 0xFF (scan to end of tree).
fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.last_mut() {
        if *last < 0xFF {
            *last += 1;
            return Some(end);
        }
        end.pop();
    }
    None
}

/// Find which child page to follow for a given key in an interior node.
fn find_child(interior: &InteriorPage, key: &[u8]) -> u32 {
    let idx = find_child_index(interior, key);
    if idx < interior.cells.len() {
        interior.cells[idx].child_page
    } else {
        interior.rightmost_child
    }
}

/// Find the child index for a key in an interior node.
/// Returns the index of the first cell where key < cell.key (that cell's child_page is the target).
/// If key >= all cell keys, returns cells.len() (meaning rightmost_child).
fn find_child_index(interior: &InteriorPage, key: &[u8]) -> usize {
    // We want the first cell where key < cell.key
    // That cell's child_page points to keys < cell.key, which includes our key.
    // If no such cell, key >= all keys, so go to rightmost_child.
    match interior
        .cells
        .binary_search_by(|c| c.key.as_slice().cmp(key))
    {
        Ok(idx) => {
            // Exact match on cell key — key >= cell[idx].key
            // Go to the right of this cell, which is either cell[idx+1].child_page
            // or rightmost_child.
            idx + 1
        }
        Err(idx) => {
            // key would be inserted at idx, meaning key < cells[idx].key (if idx < len)
            // So cells[idx].child_page is the right subtree to follow.
            idx
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Pager) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        let pager = Pager::create(&path).unwrap();
        (dir, pager)
    }

    #[test]
    fn insert_and_search_one() {
        let (_dir, mut pager) = setup();
        let mut tree = BTree::new(&mut pager, 0);
        let root = tree.insert(b"hello", b"world").unwrap();
        let mut tree = BTree::new(&mut pager, root);
        assert_eq!(tree.search(b"hello").unwrap(), Some(b"world".to_vec()));
    }

    #[test]
    fn search_nonexistent() {
        let (_dir, mut pager) = setup();
        let mut tree = BTree::new(&mut pager, 0);
        let root = tree.insert(b"hello", b"world").unwrap();
        let mut tree = BTree::new(&mut pager, root);
        assert_eq!(tree.search(b"nope").unwrap(), None);
    }

    #[test]
    fn insert_ten_keys() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), Some(val.into_bytes()));
        }
    }

    #[test]
    fn insert_triggers_split() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..200u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        for i in 0..200u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), Some(val.into_bytes()));
        }
    }

    #[test]
    fn insert_500_keys() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..500u32 {
            let key = format!("key_{:04}", i);
            let val = format!("v{}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        for i in 0..500u32 {
            let key = format!("key_{:04}", i);
            let val = format!("v{}", i);
            let mut tree = BTree::new(&mut pager, root);
            let found = tree.search(key.as_bytes()).unwrap();
            assert_eq!(found, Some(val.into_bytes()), "missing key_{:04}", i);
        }
    }

    #[test]
    fn insert_reverse_order() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in (0..100u32).rev() {
            let key = format!("key_{:04}", i);
            let val = format!("v{}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        for i in 0..100u32 {
            let key = format!("key_{:04}", i);
            let val = format!("v{}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), Some(val.into_bytes()));
        }
    }

    #[test]
    fn update_existing_key() {
        let (_dir, mut pager) = setup();
        let mut tree = BTree::new(&mut pager, 0);
        let root = tree.insert(b"key", b"old").unwrap();
        let mut tree = BTree::new(&mut pager, root);
        let root = tree.insert(b"key", b"new").unwrap();
        let mut tree = BTree::new(&mut pager, root);
        assert_eq!(tree.search(b"key").unwrap(), Some(b"new".to_vec()));
    }

    #[test]
    fn empty_tree_search() {
        let (_dir, mut pager) = setup();
        let mut tree = BTree::new(&mut pager, 0);
        assert_eq!(tree.search(b"anything").unwrap(), None);
    }

    // -----------------------------------------------------------------------
    // Delete tests
    // -----------------------------------------------------------------------

    #[test]
    fn delete_existing_key() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        // Delete key_0005
        let mut tree = BTree::new(&mut pager, root);
        let old = tree.delete(b"key_0005").unwrap();
        assert_eq!(old, Some(b"val_5".to_vec()));

        // Search returns None
        let mut tree = BTree::new(&mut pager, root);
        assert_eq!(tree.search(b"key_0005").unwrap(), None);

        // Other 9 keys still there
        for i in 0..10u32 {
            if i == 5 {
                continue;
            }
            let key = format!("key_{:04}", i);
            let val = format!("val_{}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), Some(val.into_bytes()));
        }
    }

    #[test]
    fn delete_nonexistent_key() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..5u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), b"v").unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        assert_eq!(tree.delete(b"nope").unwrap(), None);

        // All 5 still there
        for i in 0..5u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), Some(b"v".to_vec()));
        }
    }

    #[test]
    fn delete_from_empty_tree() {
        let (_dir, mut pager) = setup();
        let mut tree = BTree::new(&mut pager, 0);
        assert_eq!(tree.delete(b"anything").unwrap(), None);
    }

    #[test]
    fn delete_all_keys() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), b"v").unwrap();
        }
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            let old = tree.delete(key.as_bytes()).unwrap();
            assert_eq!(old, Some(b"v".to_vec()));
        }
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            assert_eq!(tree.search(key.as_bytes()).unwrap(), None);
        }
    }

    // -----------------------------------------------------------------------
    // Scan tests
    // -----------------------------------------------------------------------

    #[test]
    fn scan_range() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..100u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan(b"key_0020", b"key_0030").unwrap();
        assert_eq!(results.len(), 10);
        for (j, (k, v)) in results.iter().enumerate() {
            let i = j + 20;
            assert_eq!(k, format!("key_{:04}", i).as_bytes());
            assert_eq!(v, format!("val_{:04}", i).as_bytes());
        }
    }

    #[test]
    fn scan_empty_range() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..10u32 {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), b"v").unwrap();
        }
        // start >= end
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan(b"key_0005", b"key_0005").unwrap();
        assert!(results.is_empty());

        // No keys in range
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan(b"zzz_0000", b"zzz_9999").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn scan_prefix_basic() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        let entries = vec![
            ("@Team:eng", "t1"),
            ("@User:alice", "u1"),
            ("@User:bob", "u2"),
        ];
        for (k, v) in &entries {
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(k.as_bytes(), v.as_bytes()).unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan_prefix(b"@User:").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, b"@User:alice");
        assert_eq!(results[1].0, b"@User:bob");
    }

    #[test]
    fn scan_prefix_no_matches() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        let mut tree = BTree::new(&mut pager, root);
        root = tree.insert(b"@User:alice", b"v").unwrap();

        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan_prefix(b"@Nothing:").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn scan_all_basic() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..20u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan_all().unwrap();
        assert_eq!(results.len(), 20);
        for (j, (k, _v)) in results.iter().enumerate() {
            assert_eq!(k, format!("key_{:04}", j).as_bytes());
        }
    }

    #[test]
    fn scan_across_multiple_leaves() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..300u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan_all().unwrap();
        assert_eq!(results.len(), 300);
        // Verify sorted order
        for i in 0..299 {
            assert!(results[i].0 < results[i + 1].0);
        }
    }

    #[test]
    fn delete_then_scan() {
        let (_dir, mut pager) = setup();
        let mut root = 0u32;
        for i in 0..20u32 {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            root = tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        // Delete 5 keys: 0, 5, 10, 15, 19
        for &i in &[0u32, 5, 10, 15, 19] {
            let key = format!("key_{:04}", i);
            let mut tree = BTree::new(&mut pager, root);
            tree.delete(key.as_bytes()).unwrap();
        }
        let mut tree = BTree::new(&mut pager, root);
        let results = tree.scan_all().unwrap();
        assert_eq!(results.len(), 15);
        // Verify none of the deleted keys are present
        let deleted: Vec<Vec<u8>> = [0u32, 5, 10, 15, 19]
            .iter()
            .map(|i| format!("key_{:04}", i).into_bytes())
            .collect();
        for (k, _) in &results {
            assert!(!deleted.contains(k));
        }
    }
}
