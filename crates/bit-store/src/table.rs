// table.rs — Typed table wrappers over the B-tree
//
// Each table handles key formatting and JSON serialization so the rest of
// the system works with structured data, not raw bytes.

use crate::btree::{BTree, BTreeError};
use crate::pager::Pager;
use serde_json::Value;

/// Error type for table operations.
#[derive(Debug)]
pub enum TableError {
    BTree(BTreeError),
    Json(serde_json::Error),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::BTree(e) => write!(f, "table btree error: {e}"),
            TableError::Json(e) => write!(f, "table json error: {e}"),
        }
    }
}

impl std::error::Error for TableError {}

impl From<BTreeError> for TableError {
    fn from(e: BTreeError) -> Self {
        TableError::BTree(e)
    }
}

impl From<serde_json::Error> for TableError {
    fn from(e: serde_json::Error) -> Self {
        TableError::Json(e)
    }
}

// ── EntityTable ──────────────────────────────────────────────

/// Stores entity instances. Key format: `@{EntityName}:{id}`
pub struct EntityTable<'a> {
    pager: &'a mut Pager,
    root: u32,
}

impl<'a> EntityTable<'a> {
    pub fn new(pager: &'a mut Pager, root: u32) -> Self {
        EntityTable { pager, root }
    }

    fn make_key(entity: &str, id: &str) -> Vec<u8> {
        format!("@{}:{}", entity, id).into_bytes()
    }

    fn make_prefix(entity: &str) -> Vec<u8> {
        format!("@{}:", entity).into_bytes()
    }

    /// Parse a key back into (entity_name, id).
    fn parse_key(key: &[u8]) -> Option<(String, String)> {
        let s = std::str::from_utf8(key).ok()?;
        let s = s.strip_prefix('@')?;
        let colon = s.find(':')?;
        Some((s[..colon].to_string(), s[colon + 1..].to_string()))
    }

    /// Insert or update an entity instance. Returns new root page.
    pub fn insert(&mut self, entity: &str, id: &str, record: &Value) -> Result<u32, TableError> {
        let key = Self::make_key(entity, id);
        let value = serde_json::to_vec(record)?;
        let mut tree = BTree::new(self.pager, self.root);
        let new_root = tree.insert(&key, &value)?;
        self.root = new_root;
        Ok(new_root)
    }

    /// Get a single entity by name and id.
    pub fn get(&mut self, entity: &str, id: &str) -> Result<Option<Value>, TableError> {
        let key = Self::make_key(entity, id);
        let mut tree = BTree::new(self.pager, self.root);
        match tree.search(&key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List all instances of an entity type. Returns vec of (id, record).
    pub fn list(&mut self, entity: &str) -> Result<Vec<(String, Value)>, TableError> {
        let prefix = Self::make_prefix(entity);
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_prefix(&prefix)?;
        let mut results = Vec::with_capacity(pairs.len());
        for (key, val) in pairs {
            if let Some((_, id)) = Self::parse_key(&key) {
                let record: Value = serde_json::from_slice(&val)?;
                results.push((id, record));
            }
        }
        Ok(results)
    }

    /// List ALL entities across all types. Returns vec of (entity_name, id, record).
    pub fn list_all(&mut self) -> Result<Vec<(String, String, Value)>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut results = Vec::with_capacity(pairs.len());
        for (key, val) in pairs {
            if let Some((entity, id)) = Self::parse_key(&key) {
                let record: Value = serde_json::from_slice(&val)?;
                results.push((entity, id, record));
            }
        }
        Ok(results)
    }

    /// Delete an entity instance. Returns old value if existed.
    pub fn delete(&mut self, entity: &str, id: &str) -> Result<Option<Value>, TableError> {
        let key = Self::make_key(entity, id);
        let mut tree = BTree::new(self.pager, self.root);
        match tree.delete(&key)? {
            Some(bytes) => {
                self.root = tree.root();
                Ok(Some(serde_json::from_slice(&bytes)?))
            }
            None => Ok(None),
        }
    }

    /// Count instances of an entity type.
    pub fn count(&mut self, entity: &str) -> Result<usize, TableError> {
        let prefix = Self::make_prefix(entity);
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_prefix(&prefix)?;
        Ok(pairs.len())
    }

    /// Current root page.
    pub fn root(&self) -> u32 {
        self.root
    }
}

// ── TaskTable ────────────────────────────────────────────────

/// Stores tasks. Key format: `{file}:{line}:{idx}`
pub struct TaskTable<'a> {
    pager: &'a mut Pager,
    root: u32,
}

impl<'a> TaskTable<'a> {
    pub fn new(pager: &'a mut Pager, root: u32) -> Self {
        TaskTable { pager, root }
    }

    fn make_key(file: &str, line: u32, idx: u32) -> Vec<u8> {
        format!("{}:{}:{}", file, line, idx).into_bytes()
    }

    fn make_file_prefix(file: &str) -> Vec<u8> {
        format!("{}:", file).into_bytes()
    }

    /// Insert or update a task. Returns new root page.
    pub fn insert(
        &mut self,
        file: &str,
        line: u32,
        idx: u32,
        task: &Value,
    ) -> Result<u32, TableError> {
        let key = Self::make_key(file, line, idx);
        let value = serde_json::to_vec(task)?;
        let mut tree = BTree::new(self.pager, self.root);
        let new_root = tree.insert(&key, &value)?;
        self.root = new_root;
        Ok(new_root)
    }

    /// Get a single task.
    pub fn get(&mut self, file: &str, line: u32, idx: u32) -> Result<Option<Value>, TableError> {
        let key = Self::make_key(file, line, idx);
        let mut tree = BTree::new(self.pager, self.root);
        match tree.search(&key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List all tasks in a file (prefix scan).
    pub fn list_file(&mut self, file: &str) -> Result<Vec<Value>, TableError> {
        let prefix = Self::make_file_prefix(file);
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_prefix(&prefix)?;
        let mut results = Vec::with_capacity(pairs.len());
        for (_, val) in pairs {
            results.push(serde_json::from_slice(&val)?);
        }
        Ok(results)
    }

    /// List all tasks across all files.
    pub fn list_all(&mut self) -> Result<Vec<Value>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut results = Vec::with_capacity(pairs.len());
        for (_, val) in pairs {
            results.push(serde_json::from_slice(&val)?);
        }
        Ok(results)
    }

    /// Count all tasks.
    pub fn count(&mut self) -> Result<usize, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        Ok(tree.scan_all()?.len())
    }

    /// Current root page.
    pub fn root(&self) -> u32 {
        self.root
    }
}

// ── FlowTable ────────────────────────────────────────────────

/// Stores flows. Key format: flow name.
pub struct FlowTable<'a> {
    pager: &'a mut Pager,
    root: u32,
}

impl<'a> FlowTable<'a> {
    pub fn new(pager: &'a mut Pager, root: u32) -> Self {
        FlowTable { pager, root }
    }

    /// Insert or update a flow. Returns new root page.
    pub fn insert(&mut self, name: &str, flow: &Value) -> Result<u32, TableError> {
        let key = name.as_bytes();
        let value = serde_json::to_vec(flow)?;
        let mut tree = BTree::new(self.pager, self.root);
        let new_root = tree.insert(key, &value)?;
        self.root = new_root;
        Ok(new_root)
    }

    /// Get a flow by name.
    pub fn get(&mut self, name: &str) -> Result<Option<Value>, TableError> {
        let key = name.as_bytes();
        let mut tree = BTree::new(self.pager, self.root);
        match tree.search(key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List all flows. Returns vec of (name, flow).
    pub fn list_all(&mut self) -> Result<Vec<(String, Value)>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut results = Vec::with_capacity(pairs.len());
        for (key, val) in pairs {
            let name = String::from_utf8_lossy(&key).to_string();
            let flow: Value = serde_json::from_slice(&val)?;
            results.push((name, flow));
        }
        Ok(results)
    }

    /// Delete a flow. Returns old value if existed.
    pub fn delete(&mut self, name: &str) -> Result<Option<Value>, TableError> {
        let key = name.as_bytes();
        let mut tree = BTree::new(self.pager, self.root);
        match tree.delete(key)? {
            Some(bytes) => {
                self.root = tree.root();
                Ok(Some(serde_json::from_slice(&bytes)?))
            }
            None => Ok(None),
        }
    }

    /// Count all flows.
    pub fn count(&mut self) -> Result<usize, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        Ok(tree.scan_all()?.len())
    }

    /// Current root page.
    pub fn root(&self) -> u32 {
        self.root
    }
}

// ── SchemaTable ──────────────────────────────────────────────

/// Stores entity schemas. Key format: `@{EntityName}`
pub struct SchemaTable<'a> {
    pager: &'a mut Pager,
    root: u32,
}

impl<'a> SchemaTable<'a> {
    pub fn new(pager: &'a mut Pager, root: u32) -> Self {
        SchemaTable { pager, root }
    }

    fn make_key(entity: &str) -> Vec<u8> {
        format!("@{}", entity).into_bytes()
    }

    /// Parse a schema key back into entity name.
    fn parse_key(key: &[u8]) -> Option<String> {
        let s = std::str::from_utf8(key).ok()?;
        s.strip_prefix('@').map(|s| s.to_string())
    }

    /// Insert or update a schema. Returns new root page.
    pub fn insert(&mut self, entity: &str, schema: &Value) -> Result<u32, TableError> {
        let key = Self::make_key(entity);
        let value = serde_json::to_vec(schema)?;
        let mut tree = BTree::new(self.pager, self.root);
        let new_root = tree.insert(&key, &value)?;
        self.root = new_root;
        Ok(new_root)
    }

    /// Get a schema by entity name.
    pub fn get(&mut self, entity: &str) -> Result<Option<Value>, TableError> {
        let key = Self::make_key(entity);
        let mut tree = BTree::new(self.pager, self.root);
        match tree.search(&key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List all schemas. Returns vec of (entity_name, schema).
    pub fn list_all(&mut self) -> Result<Vec<(String, Value)>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut results = Vec::with_capacity(pairs.len());
        for (key, val) in pairs {
            if let Some(name) = Self::parse_key(&key) {
                let schema: Value = serde_json::from_slice(&val)?;
                results.push((name, schema));
            }
        }
        Ok(results)
    }

    /// Count all schemas.
    pub fn count(&mut self) -> Result<usize, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        Ok(tree.scan_all()?.len())
    }

    /// Current root page.
    pub fn root(&self) -> u32 {
        self.root
    }
}

// ── BlobTable ────────────────────────────────────────────────

/// Stores raw .bit file contents. Key format: relative file path.
/// Value format: `[hash_len: u16][hash_str: bytes][content: remaining bytes]`
pub struct BlobTable<'a> {
    pager: &'a mut Pager,
    root: u32,
}

impl<'a> BlobTable<'a> {
    pub fn new(pager: &'a mut Pager, root: u32) -> Self {
        BlobTable { pager, root }
    }

    /// Encode hash + content into the value format.
    fn encode_value(content: &[u8], hash: &str) -> Vec<u8> {
        let hash_bytes = hash.as_bytes();
        let hash_len = hash_bytes.len() as u16;
        let mut buf = Vec::with_capacity(2 + hash_bytes.len() + content.len());
        buf.extend_from_slice(&hash_len.to_le_bytes());
        buf.extend_from_slice(hash_bytes);
        buf.extend_from_slice(content);
        buf
    }

    /// Decode value back into (content, hash).
    fn decode_value(data: &[u8]) -> Option<(Vec<u8>, String)> {
        if data.len() < 2 {
            return None;
        }
        let hash_len = u16::from_le_bytes([data[0], data[1]]) as usize;
        if data.len() < 2 + hash_len {
            return None;
        }
        let hash = std::str::from_utf8(&data[2..2 + hash_len])
            .ok()?
            .to_string();
        let content = data[2 + hash_len..].to_vec();
        Some((content, hash))
    }

    /// Insert or update a blob. Returns new root page.
    pub fn insert(&mut self, path: &str, content: &[u8], hash: &str) -> Result<u32, TableError> {
        let key = path.as_bytes();
        let value = Self::encode_value(content, hash);
        let mut tree = BTree::new(self.pager, self.root);
        let new_root = tree.insert(key, &value)?;
        self.root = new_root;
        Ok(new_root)
    }

    /// Get a blob by path. Returns (content, blake3_hash).
    pub fn get(&mut self, path: &str) -> Result<Option<(Vec<u8>, String)>, TableError> {
        let key = path.as_bytes();
        let mut tree = BTree::new(self.pager, self.root);
        match tree.search(key)? {
            Some(bytes) => Ok(Self::decode_value(&bytes)),
            None => Ok(None),
        }
    }

    /// List all stored paths.
    pub fn list_paths(&mut self) -> Result<Vec<String>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut paths = Vec::with_capacity(pairs.len());
        for (key, _) in pairs {
            paths.push(String::from_utf8_lossy(&key).to_string());
        }
        Ok(paths)
    }

    /// List all blobs. Returns (path, content, hash).
    pub fn list_all(&mut self) -> Result<Vec<(String, Vec<u8>, String)>, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        let pairs = tree.scan_all()?;
        let mut results = Vec::with_capacity(pairs.len());
        for (key, val) in pairs {
            let path = String::from_utf8_lossy(&key).to_string();
            if let Some((content, hash)) = Self::decode_value(&val) {
                results.push((path, content, hash));
            }
        }
        Ok(results)
    }

    /// Delete a blob. Returns true if it existed.
    pub fn delete(&mut self, path: &str) -> Result<bool, TableError> {
        let key = path.as_bytes();
        let mut tree = BTree::new(self.pager, self.root);
        let existed = tree.delete(key)?.is_some();
        self.root = tree.root();
        Ok(existed)
    }

    /// Count all blobs.
    pub fn count(&mut self) -> Result<usize, TableError> {
        let mut tree = BTree::new(self.pager, self.root);
        Ok(tree.scan_all()?.len())
    }

    /// Current root page.
    pub fn root(&self) -> u32 {
        self.root
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Pager) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        let pager = Pager::create(&path).unwrap();
        (dir, pager)
    }

    #[test]
    fn entity_insert_and_get() {
        let (_dir, mut pager) = setup();
        let mut tbl = EntityTable::new(&mut pager, 0);
        let root = tbl
            .insert("User", "alice", &json!({"name": "Alice", "role": "admin"}))
            .unwrap();
        let mut tbl = EntityTable::new(&mut pager, root);
        let val = tbl.get("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
    }

    #[test]
    fn entity_list_by_type() {
        let (_dir, mut pager) = setup();
        let mut root = 0;
        let records = [("alice", "Alice"), ("bob", "Bob"), ("charlie", "Charlie")];
        for (id, name) in &records {
            let mut tbl = EntityTable::new(&mut pager, root);
            root = tbl.insert("User", id, &json!({"name": name})).unwrap();
        }
        // Also insert a Team
        let mut tbl = EntityTable::new(&mut pager, root);
        root = tbl
            .insert("Team", "eng", &json!({"name": "Engineering"}))
            .unwrap();

        let mut tbl = EntityTable::new(&mut pager, root);
        let users = tbl.list("User").unwrap();
        assert_eq!(users.len(), 3);

        let mut tbl = EntityTable::new(&mut pager, root);
        let teams = tbl.list("Team").unwrap();
        assert_eq!(teams.len(), 1);
    }

    #[test]
    fn entity_delete() {
        let (_dir, mut pager) = setup();
        let mut tbl = EntityTable::new(&mut pager, 0);
        let root = tbl
            .insert("User", "alice", &json!({"name": "Alice"}))
            .unwrap();
        let mut tbl = EntityTable::new(&mut pager, root);
        let old = tbl.delete("User", "alice").unwrap();
        assert!(old.is_some());
        let root = tbl.root();
        let mut tbl = EntityTable::new(&mut pager, root);
        assert!(tbl.get("User", "alice").unwrap().is_none());
    }

    #[test]
    fn task_insert_and_list() {
        let (_dir, mut pager) = setup();
        let mut root = 0;
        for i in 0..5 {
            let mut tbl = TaskTable::new(&mut pager, root);
            root = tbl
                .insert(
                    "sprint.bit",
                    i * 3,
                    0,
                    &json!({"text": format!("Task {}", i), "marker": "!"}),
                )
                .unwrap();
        }
        let mut tbl = TaskTable::new(&mut pager, root);
        let tasks = tbl.list_file("sprint.bit").unwrap();
        assert_eq!(tasks.len(), 5);
    }

    #[test]
    fn blob_insert_and_get() {
        let (_dir, mut pager) = setup();
        let content = b"define:@User\n    name: alice";
        let hash = blake3::hash(content).to_hex().to_string();
        let mut tbl = BlobTable::new(&mut pager, 0);
        let root = tbl.insert("users.bit", content, &hash).unwrap();
        let mut tbl = BlobTable::new(&mut pager, root);
        let (got_content, got_hash) = tbl.get("users.bit").unwrap().unwrap();
        assert_eq!(got_content, content);
        assert_eq!(got_hash, hash);
    }

    #[test]
    fn blob_list_paths() {
        let (_dir, mut pager) = setup();
        let mut root = 0;
        for name in &["a.bit", "b.bit", "sub/c.bit"] {
            let content = format!("# {}", name);
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let mut tbl = BlobTable::new(&mut pager, root);
            root = tbl.insert(name, content.as_bytes(), &hash).unwrap();
        }
        let mut tbl = BlobTable::new(&mut pager, root);
        let paths = tbl.list_paths().unwrap();
        assert_eq!(paths, vec!["a.bit", "b.bit", "sub/c.bit"]);
    }

    #[test]
    fn schema_roundtrip() {
        let (_dir, mut pager) = setup();
        let schema = json!({"fields": {"name": "string!", "email": "string!"}});
        let mut tbl = SchemaTable::new(&mut pager, 0);
        let root = tbl.insert("User", &schema).unwrap();
        let mut tbl = SchemaTable::new(&mut pager, root);
        let got = tbl.get("User").unwrap().unwrap();
        assert_eq!(got, schema);
    }

    #[test]
    fn flow_roundtrip() {
        let (_dir, mut pager) = setup();
        let flow = json!({"states": ["draft", "review", "done"], "edges": [["draft", "review"], ["review", "done"]]});
        let mut tbl = FlowTable::new(&mut pager, 0);
        let root = tbl.insert("lifecycle", &flow).unwrap();
        let mut tbl = FlowTable::new(&mut pager, root);
        let got = tbl.get("lifecycle").unwrap().unwrap();
        assert_eq!(got, flow);
    }

    #[test]
    fn entity_count() {
        let (_dir, mut pager) = setup();
        let mut root = 0;
        for i in 0..10 {
            let mut tbl = EntityTable::new(&mut pager, root);
            root = tbl
                .insert("User", &format!("u{}", i), &json!({"n": i}))
                .unwrap();
        }
        let mut tbl = EntityTable::new(&mut pager, root);
        assert_eq!(tbl.count("User").unwrap(), 10);
    }
}
