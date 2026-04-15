// store.rs — High-level BitStore handle over the page-based engine
//
// Wraps Pager + typed tables (EntityTable, TaskTable, FlowTable, SchemaTable,
// BlobTable) into a single API.

use std::path::Path;

use crate::pager::{Pager, PagerError};
use crate::search::SearchIndex;
use crate::table::*;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StoreError {
    Pager(PagerError),
    Table(TableError),
    Io(std::io::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Pager(e) => write!(f, "store pager error: {e}"),
            StoreError::Table(e) => write!(f, "store table error: {e}"),
            StoreError::Io(e) => write!(f, "store I/O error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<PagerError> for StoreError {
    fn from(e: PagerError) -> Self {
        StoreError::Pager(e)
    }
}

impl From<TableError> for StoreError {
    fn from(e: TableError) -> Self {
        StoreError::Table(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// StoreInfo
// ---------------------------------------------------------------------------

/// Summary information about a store.
#[derive(Debug)]
pub struct StoreInfo {
    pub page_count: u32,
    pub entity_count: usize,
    pub task_count: usize,
    pub flow_count: usize,
    pub schema_count: usize,
    pub blob_count: usize,
}

// ---------------------------------------------------------------------------
// ContextWindow types
// ---------------------------------------------------------------------------

/// Options for a context window query.
#[derive(Debug, Clone)]
pub struct ContextWindowOptions {
    /// Maximum total size in bytes of rendered output
    pub budget_bytes: usize,
    /// Entity types to include (empty = all types)
    pub entity_types: Vec<String>,
    /// Field to sort by for priority (default: "_importance" or insertion order)
    pub priority_field: Option<String>,
    /// Whether to sort descending (highest priority first). Default: true.
    pub descending: bool,
}

impl Default for ContextWindowOptions {
    fn default() -> Self {
        Self {
            budget_bytes: 4096,
            entity_types: vec![],
            priority_field: None,
            descending: true,
        }
    }
}

/// Result of a context window query.
#[derive(Debug)]
pub struct ContextWindow {
    /// Rendered .bit text that fits within the budget
    pub content: String,
    /// Number of entities included
    pub entity_count: usize,
    /// Number of entities that didn't fit
    pub truncated_count: usize,
    /// Total bytes used
    pub bytes_used: usize,
}

// ---------------------------------------------------------------------------
// BitStore
// ---------------------------------------------------------------------------

pub struct BitStore {
    pager: Pager,
}

impl BitStore {
    /// Create a new empty .bitstore file.
    pub fn create(path: &Path) -> Result<Self, StoreError> {
        let pager = Pager::create(path)?;
        Ok(Self { pager })
    }

    /// Open an existing .bitstore file.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let pager = Pager::open(path)?;
        Ok(Self { pager })
    }

    // ── Entity methods ──────────────────────────────────────────

    /// Insert an entity and update the header's entity_root.
    pub fn insert_entity(
        &mut self,
        entity: &str,
        id: &str,
        record: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        let new_root = tbl.insert(entity, id, record)?;
        self.pager.header_mut().entity_root = new_root;
        Ok(())
    }

    /// Get an entity by type and id.
    pub fn get_entity(
        &mut self,
        entity: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        Ok(tbl.get(entity, id)?)
    }

    /// List all instances of an entity type. Returns vec of (id, record).
    pub fn list_entities(
        &mut self,
        entity: &str,
    ) -> Result<Vec<(String, serde_json::Value)>, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        Ok(tbl.list(entity)?)
    }

    /// List all entities across all types. Returns vec of (entity_type, id, record).
    pub fn list_all_entities(
        &mut self,
    ) -> Result<Vec<(String, String, serde_json::Value)>, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?)
    }

    /// List all distinct entity type names in the store.
    pub fn list_entity_types(&mut self) -> Result<Vec<String>, StoreError> {
        let all = self.list_all_entities()?;
        let mut types: Vec<String> = Vec::new();
        for (et, _, _) in &all {
            if !types.contains(et) {
                types.push(et.clone());
            }
        }
        Ok(types)
    }

    /// Delete an entity. Returns old value if it existed.
    pub fn delete_entity(
        &mut self,
        entity: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        let result = tbl.delete(entity, id)?;
        // delete may change the root
        self.pager.header_mut().entity_root = tbl.root();
        Ok(result)
    }

    /// Count entities of a type.
    pub fn count_entities(&mut self, entity: &str) -> Result<usize, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        Ok(tbl.count(entity)?)
    }

    /// Count all entities across all types.
    pub fn count_entities_total(&mut self) -> Result<usize, StoreError> {
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?.len())
    }

    /// Infer a schema for an entity type from its stored records.
    pub fn infer_entity_schema(
        &mut self,
        entity: &str,
    ) -> Result<crate::infer::InferredSchema, StoreError> {
        let records = self.list_entities(entity)?;
        let values: Vec<serde_json::Value> = records.into_iter().map(|(_, v)| v).collect();
        Ok(crate::infer::infer_schema(entity, &values))
    }

    /// Build a vector search index from all entities.
    pub fn build_vector_index(&mut self) -> Result<crate::vector_search::VectorIndex, StoreError> {
        let mut index = crate::vector_search::VectorIndex::new();
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        let all = tbl.list_all()?;
        for (entity, id, val) in all {
            index.add(&format!("@{}:{}", entity, id), &val);
        }
        Ok(index)
    }

    // ── Task methods ────────────────────────────────────────────

    /// Insert a task and update the header's task_root.
    pub fn insert_task(
        &mut self,
        file: &str,
        line: u32,
        idx: u32,
        task: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let root = self.pager.header().task_root;
        let mut tbl = TaskTable::new(&mut self.pager, root);
        let new_root = tbl.insert(file, line, idx, task)?;
        self.pager.header_mut().task_root = new_root;
        Ok(())
    }

    /// List all tasks in a file.
    pub fn list_tasks(&mut self, file: &str) -> Result<Vec<serde_json::Value>, StoreError> {
        let root = self.pager.header().task_root;
        let mut tbl = TaskTable::new(&mut self.pager, root);
        Ok(tbl.list_file(file)?)
    }

    /// List all tasks across all files.
    pub fn list_all_tasks(&mut self) -> Result<Vec<serde_json::Value>, StoreError> {
        let root = self.pager.header().task_root;
        let mut tbl = TaskTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?)
    }

    /// Count all tasks.
    pub fn count_tasks(&mut self) -> Result<usize, StoreError> {
        let root = self.pager.header().task_root;
        let mut tbl = TaskTable::new(&mut self.pager, root);
        Ok(tbl.count()?)
    }

    // ── Flow methods ────────────────────────────────────────────

    /// Insert a flow and update the header's flow_root.
    pub fn insert_flow(&mut self, name: &str, flow: &serde_json::Value) -> Result<(), StoreError> {
        let root = self.pager.header().flow_root;
        let mut tbl = FlowTable::new(&mut self.pager, root);
        let new_root = tbl.insert(name, flow)?;
        self.pager.header_mut().flow_root = new_root;
        Ok(())
    }

    /// Get a flow by name.
    pub fn get_flow(&mut self, name: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let root = self.pager.header().flow_root;
        let mut tbl = FlowTable::new(&mut self.pager, root);
        Ok(tbl.get(name)?)
    }

    /// List all flows. Returns vec of (name, flow).
    pub fn list_flows(&mut self) -> Result<Vec<(String, serde_json::Value)>, StoreError> {
        let root = self.pager.header().flow_root;
        let mut tbl = FlowTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?)
    }

    /// Count all flows.
    pub fn count_flows(&mut self) -> Result<usize, StoreError> {
        let root = self.pager.header().flow_root;
        let mut tbl = FlowTable::new(&mut self.pager, root);
        Ok(tbl.count()?)
    }

    // ── Schema methods ──────────────────────────────────────────

    /// Insert a schema and update the header's schema_root.
    pub fn insert_schema(
        &mut self,
        entity: &str,
        schema: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let root = self.pager.header().schema_root;
        let mut tbl = SchemaTable::new(&mut self.pager, root);
        let new_root = tbl.insert(entity, schema)?;
        self.pager.header_mut().schema_root = new_root;
        Ok(())
    }

    /// Get a schema by entity name.
    pub fn get_schema(&mut self, entity: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let root = self.pager.header().schema_root;
        let mut tbl = SchemaTable::new(&mut self.pager, root);
        Ok(tbl.get(entity)?)
    }

    /// List all schemas. Returns vec of (entity_name, schema).
    pub fn list_schemas(&mut self) -> Result<Vec<(String, serde_json::Value)>, StoreError> {
        let root = self.pager.header().schema_root;
        let mut tbl = SchemaTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?)
    }

    /// Count all schemas.
    pub fn count_schemas(&mut self) -> Result<usize, StoreError> {
        let root = self.pager.header().schema_root;
        let mut tbl = SchemaTable::new(&mut self.pager, root);
        Ok(tbl.count()?)
    }

    // ── Blob methods ────────────────────────────────────────────

    /// Insert a blob and update the header's blob_root.
    pub fn insert_blob(
        &mut self,
        path: &str,
        content: &[u8],
        hash: &str,
    ) -> Result<(), StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        let new_root = tbl.insert(path, content, hash)?;
        self.pager.header_mut().blob_root = new_root;
        Ok(())
    }

    /// Get a blob by path. Returns (content, hash).
    pub fn get_blob(&mut self, path: &str) -> Result<Option<(Vec<u8>, String)>, StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        Ok(tbl.get(path)?)
    }

    /// List all stored blob paths.
    pub fn list_blob_paths(&mut self) -> Result<Vec<String>, StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        Ok(tbl.list_paths()?)
    }

    /// List all blobs. Returns vec of (path, content, hash).
    pub fn list_all_blobs(&mut self) -> Result<Vec<(String, Vec<u8>, String)>, StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        Ok(tbl.list_all()?)
    }

    /// Delete a blob. Returns true if it existed.
    pub fn delete_blob(&mut self, path: &str) -> Result<bool, StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        let existed = tbl.delete(path)?;
        self.pager.header_mut().blob_root = tbl.root();
        Ok(existed)
    }

    /// Count all blobs.
    pub fn count_blobs(&mut self) -> Result<usize, StoreError> {
        let root = self.pager.header().blob_root;
        let mut tbl = BlobTable::new(&mut self.pager, root);
        Ok(tbl.count()?)
    }

    // ── TTL methods ─────────────────────────────────────────────

    /// Insert an entity with a TTL (time-to-live in seconds from now).
    /// The entity's record will include _ttl and _expires_at metadata fields.
    pub fn insert_entity_with_ttl(
        &mut self,
        entity: &str,
        id: &str,
        record: &Value,
        ttl_seconds: u64,
    ) -> Result<(), StoreError> {
        let mut enriched = record.clone();
        if let Some(obj) = enriched.as_object_mut() {
            obj.insert("_ttl".to_string(), json!(ttl_seconds));
            obj.insert(
                "_expires_at".to_string(),
                json!(chrono::Utc::now().timestamp() + ttl_seconds as i64),
            );
        }
        self.insert_entity(entity, id, &enriched)
    }

    /// Remove all expired entities. Returns count of removed entities.
    pub fn expire_entities(&mut self) -> Result<usize, StoreError> {
        let now = chrono::Utc::now().timestamp();
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        let all = tbl.list_all()?;

        let mut expired = Vec::new();
        for (entity, id, val) in &all {
            if let Some(expires) = val.get("_expires_at").and_then(|v| v.as_i64()) {
                if expires <= now {
                    expired.push((entity.clone(), id.clone()));
                }
            }
        }

        let count = expired.len();
        for (entity, id) in expired {
            self.delete_entity(&entity, &id)?;
        }
        Ok(count)
    }

    // ── Upsert ─────────────────────────────────────────────────

    /// Upsert an entity — if it exists, merge fields (new fields override,
    /// existing fields kept). If it doesn't exist, create it.
    pub fn upsert_entity(
        &mut self,
        entity: &str,
        id: &str,
        fields: &Value,
    ) -> Result<(), StoreError> {
        let existing = self.get_entity(entity, id)?;
        let merged = match existing {
            Some(mut existing) => {
                if let (Some(existing_obj), Some(new_obj)) =
                    (existing.as_object_mut(), fields.as_object())
                {
                    for (k, v) in new_obj {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
                existing
            }
            None => fields.clone(),
        };
        self.insert_entity(entity, id, &merged)
    }

    // ── Bulk insert ────────────────────────────────────────────

    /// Insert multiple entities in a single batch. More efficient than
    /// individual inserts because the B-tree only needs to flush once.
    pub fn bulk_insert_entities(
        &mut self,
        records: &[(&str, &str, &Value)],
    ) -> Result<usize, StoreError> {
        for (entity, id, record) in records {
            self.insert_entity(entity, id, record)?;
        }
        self.flush()?;
        Ok(records.len())
    }

    // ── Render ─────────────────────────────────────────────────

    /// Render a single entity as .bit text.
    pub fn render_entity(&mut self, entity: &str, id: &str) -> Result<Option<String>, StoreError> {
        let record = self.get_entity(entity, id)?;
        let Some(record) = record else {
            return Ok(None);
        };

        let mut lines = vec![format!("mutate:@{}:{}", entity, id)];
        if let Some(obj) = record.as_object() {
            for (k, v) in obj {
                if k.starts_with('_') {
                    continue;
                }
                let val_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "nil".to_string(),
                    other => other.to_string(),
                };
                lines.push(format!("    {}: {}", k, val_str));
            }
        }
        Ok(Some(lines.join("\n")))
    }

    /// Render all entities of a type as .bit text.
    pub fn render_entities(&mut self, entity: &str) -> Result<String, StoreError> {
        let records = self.list_entities(entity)?;
        let mut parts = Vec::new();
        for (id, record) in records {
            let mut lines = vec![format!("mutate:@{}:{}", entity, id)];
            if let Some(obj) = record.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    let val_str = match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "nil".to_string(),
                        other => other.to_string(),
                    };
                    lines.push(format!("    {}: {}", k, val_str));
                }
            }
            parts.push(lines.join("\n"));
        }
        Ok(parts.join("\n\n"))
    }

    // ── Context window ─────────────────────────────────────────

    /// Build a context window — top-N entities by priority that fit within a byte budget.
    /// Renders each entity as .bit text and packs as many as fit.
    pub fn context_window(
        &mut self,
        opts: &ContextWindowOptions,
    ) -> Result<ContextWindow, StoreError> {
        // 1. Collect entities (filtered by type if specified)
        let mut all_entities: Vec<(String, String, Value)> = Vec::new();

        if opts.entity_types.is_empty() {
            let root = self.pager.header().entity_root;
            let mut tbl = EntityTable::new(&mut self.pager, root);
            all_entities = tbl.list_all()?;
        } else {
            for entity_type in &opts.entity_types {
                let records = self.list_entities(entity_type)?;
                for (id, val) in records {
                    all_entities.push((entity_type.clone(), id, val));
                }
            }
        }

        // 2. Filter out expired entities
        let now = chrono::Utc::now().timestamp();
        all_entities.retain(|(_, _, val)| {
            val.get("_expires_at")
                .and_then(|v| v.as_i64())
                .is_none_or(|exp| exp > now)
        });

        // 3. Sort by priority field (or _importance, or insertion order)
        let sort_field = opts.priority_field.as_deref().unwrap_or("_importance");
        all_entities.sort_by(|a, b| {
            let av = a.2.get(sort_field).and_then(|v| v.as_f64()).unwrap_or(5.0);
            let bv = b.2.get(sort_field).and_then(|v| v.as_f64()).unwrap_or(5.0);
            if opts.descending {
                bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        // 4. Pack entities into budget
        let mut content_parts = Vec::new();
        let mut bytes_used = 0;
        let mut entity_count = 0;
        let mut truncated_count = 0;

        for (entity, id, record) in &all_entities {
            // Render this entity
            let mut lines = vec![format!("mutate:@{}:{}", entity, id)];
            if let Some(obj) = record.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    let val_str = match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "nil".to_string(),
                        other => other.to_string(),
                    };
                    lines.push(format!("    {}: {}", k, val_str));
                }
            }
            let rendered = lines.join("\n");
            let entry_size = rendered.len() + if content_parts.is_empty() { 0 } else { 2 }; // +2 for \n\n separator

            if bytes_used + entry_size > opts.budget_bytes {
                truncated_count += 1;
                continue;
            }

            content_parts.push(rendered);
            bytes_used += entry_size;
            entity_count += 1;
        }

        Ok(ContextWindow {
            content: content_parts.join("\n\n"),
            entity_count,
            truncated_count,
            bytes_used,
        })
    }

    // ── Search index ──────────────────────────────────────────

    /// Build a search index from all entities in the store.
    pub fn build_search_index(&mut self) -> Result<SearchIndex, StoreError> {
        let mut index = SearchIndex::new();
        let root = self.pager.header().entity_root;
        let mut tbl = EntityTable::new(&mut self.pager, root);
        let all = tbl.list_all()?;
        for (entity, id, val) in all {
            let key = format!("@{}:{}", entity, id);
            index.index_document(&key, &val);
        }
        Ok(index)
    }

    // ── Store-level operations ──────────────────────────────────

    /// Flush all changes to disk.
    pub fn flush(&mut self) -> Result<(), StoreError> {
        self.pager.flush()?;
        Ok(())
    }

    /// Close the store (flush + drop).
    pub fn close(mut self) -> Result<(), StoreError> {
        self.flush()
    }

    /// Total pages in the file.
    pub fn page_count(&self) -> u32 {
        self.pager.page_count()
    }

    /// Number of blob files stored.
    pub fn file_count(&mut self) -> Result<usize, StoreError> {
        self.count_blobs()
    }

    /// Summary info for the store.
    pub fn info(&mut self) -> Result<StoreInfo, StoreError> {
        Ok(StoreInfo {
            page_count: self.page_count(),
            entity_count: self.count_entities_total()?,
            task_count: self.count_tasks()?,
            flow_count: self.count_flows()?,
            schema_count: self.count_schemas()?,
            blob_count: self.count_blobs()?,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn tmp_store() -> (TempDir, BitStore) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        let store = BitStore::create(&path).unwrap();
        (dir, store)
    }

    #[test]
    fn create_and_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        {
            let mut store = BitStore::create(&path).unwrap();
            store
                .insert_entity("User", "alice", &json!({"name": "Alice"}))
                .unwrap();
            store.flush().unwrap();
        }
        {
            let mut store = BitStore::open(&path).unwrap();
            let val = store.get_entity("User", "alice").unwrap();
            assert!(val.is_some());
            assert_eq!(val.unwrap()["name"], "Alice");
        }
    }

    #[test]
    fn entity_crud() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice"}))
            .unwrap();
        store
            .insert_entity("User", "bob", &json!({"name": "Bob"}))
            .unwrap();

        assert_eq!(store.count_entities("User").unwrap(), 2);

        let users = store.list_entities("User").unwrap();
        assert_eq!(users.len(), 2);

        store.delete_entity("User", "alice").unwrap();
        assert_eq!(store.count_entities("User").unwrap(), 1);
        assert!(store.get_entity("User", "alice").unwrap().is_none());
    }

    #[test]
    fn blob_crud() {
        let (_dir, mut store) = tmp_store();
        let content = b"define:@User\n    name: alice";
        let hash = blake3::hash(content).to_hex().to_string();
        store.insert_blob("users.bit", content, &hash).unwrap();

        let (got, got_hash) = store.get_blob("users.bit").unwrap().unwrap();
        assert_eq!(got, content);
        assert_eq!(got_hash, hash);

        let paths = store.list_blob_paths().unwrap();
        assert_eq!(paths, vec!["users.bit"]);
    }

    #[test]
    fn store_info() {
        let (_dir, mut store) = tmp_store();
        store.insert_entity("User", "a", &json!({})).unwrap();
        store.insert_entity("User", "b", &json!({})).unwrap();
        store
            .insert_task("f.bit", 1, 0, &json!({"text": "task"}))
            .unwrap();
        store
            .insert_flow("lifecycle", &json!({"states": []}))
            .unwrap();
        store.insert_schema("User", &json!({"fields": {}})).unwrap();
        store.insert_blob("f.bit", b"content", "hash123").unwrap();

        let info = store.info().unwrap();
        assert_eq!(info.entity_count, 2);
        assert_eq!(info.task_count, 1);
        assert_eq!(info.flow_count, 1);
        assert_eq!(info.schema_count, 1);
        assert_eq!(info.blob_count, 1);
    }

    #[test]
    fn multiple_entity_types() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"role": "admin"}))
            .unwrap();
        store
            .insert_entity("Team", "eng", &json!({"name": "Engineering"}))
            .unwrap();
        store
            .insert_entity("User", "bob", &json!({"role": "editor"}))
            .unwrap();

        assert_eq!(store.list_entities("User").unwrap().len(), 2);
        assert_eq!(store.list_entities("Team").unwrap().len(), 1);
    }

    // ── TTL tests ──────────────────────────────────────────────

    #[test]
    fn insert_with_ttl_stores_metadata() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity_with_ttl("Session", "s1", &json!({"user": "alice"}), 3600)
            .unwrap();
        let val = store.get_entity("Session", "s1").unwrap().unwrap();
        assert_eq!(val["_ttl"], 3600);
        assert!(val["_expires_at"].as_i64().is_some());
        assert_eq!(val["user"], "alice");
    }

    #[test]
    fn expire_entities_removes_expired() {
        let (_dir, mut store) = tmp_store();
        // TTL=0 means already expired
        store
            .insert_entity_with_ttl("Session", "s1", &json!({"user": "alice"}), 0)
            .unwrap();
        let count = store.expire_entities().unwrap();
        assert_eq!(count, 1);
        assert!(store.get_entity("Session", "s1").unwrap().is_none());
    }

    #[test]
    fn expire_entities_keeps_non_ttl() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice"}))
            .unwrap();
        let count = store.expire_entities().unwrap();
        assert_eq!(count, 0);
        assert!(store.get_entity("User", "alice").unwrap().is_some());
    }

    #[test]
    fn expire_entities_keeps_future_ttl() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity_with_ttl("Session", "s1", &json!({"user": "alice"}), 3600)
            .unwrap();
        let count = store.expire_entities().unwrap();
        assert_eq!(count, 0);
        assert!(store.get_entity("Session", "s1").unwrap().is_some());
    }

    // ── Upsert tests ──────────────────────────────────────────

    #[test]
    fn upsert_creates_new_entity() {
        let (_dir, mut store) = tmp_store();
        store
            .upsert_entity("User", "alice", &json!({"name": "Alice", "role": "admin"}))
            .unwrap();
        let val = store.get_entity("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["role"], "admin");
    }

    #[test]
    fn upsert_adds_new_field_preserves_old() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice", "role": "admin"}))
            .unwrap();
        store
            .upsert_entity("User", "alice", &json!({"email": "alice@example.com"}))
            .unwrap();
        let val = store.get_entity("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["role"], "admin");
        assert_eq!(val["email"], "alice@example.com");
    }

    #[test]
    fn upsert_overrides_existing_field() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice", "role": "editor"}))
            .unwrap();
        store
            .upsert_entity("User", "alice", &json!({"role": "admin"}))
            .unwrap();
        let val = store.get_entity("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["role"], "admin");
    }

    // ── Bulk insert tests ──────────────────────────────────────

    #[test]
    fn bulk_insert_many_entities() {
        let (_dir, mut store) = tmp_store();
        let records: Vec<(&str, &str, serde_json::Value)> = (0..100)
            .map(|i| ("User", "", json!({"name": format!("user_{}", i)})))
            .collect();
        // Need owned ids
        let ids: Vec<String> = (0..100).map(|i| format!("u{}", i)).collect();
        let refs: Vec<(&str, &str, &serde_json::Value)> = records
            .iter()
            .enumerate()
            .map(|(i, (e, _, v))| (*e, ids[i].as_str(), v))
            .collect();
        let count = store.bulk_insert_entities(&refs).unwrap();
        assert_eq!(count, 100);
        assert_eq!(store.count_entities("User").unwrap(), 100);
    }

    #[test]
    fn bulk_insert_empty() {
        let (_dir, mut store) = tmp_store();
        let count = store.bulk_insert_entities(&[]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn bulk_insert_multiple_types() {
        let (_dir, mut store) = tmp_store();
        let u = json!({"name": "Alice"});
        let t = json!({"name": "Engineering"});
        let records: Vec<(&str, &str, &serde_json::Value)> =
            vec![("User", "alice", &u), ("Team", "eng", &t)];
        let count = store.bulk_insert_entities(&records).unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.count_entities("User").unwrap(), 1);
        assert_eq!(store.count_entities("Team").unwrap(), 1);
    }

    // ── Render tests ───────────────────────────────────────────

    #[test]
    fn render_single_entity() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice", "role": "admin"}))
            .unwrap();
        let text = store.render_entity("User", "alice").unwrap().unwrap();
        assert!(text.contains("mutate:@User:alice"));
        assert!(text.contains("name: Alice"));
        assert!(text.contains("role: admin"));
    }

    #[test]
    fn render_nonexistent_entity() {
        let (_dir, mut store) = tmp_store();
        let result = store.render_entity("User", "nobody").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn render_all_entities() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("User", "alice", &json!({"name": "Alice"}))
            .unwrap();
        store
            .insert_entity("User", "bob", &json!({"name": "Bob"}))
            .unwrap();
        let text = store.render_entities("User").unwrap();
        assert!(text.contains("mutate:@User:alice"));
        assert!(text.contains("mutate:@User:bob"));
        assert!(text.contains("\n\n")); // blocks separated by blank line
    }

    #[test]
    fn render_skips_metadata_fields() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity_with_ttl("Session", "s1", &json!({"user": "alice"}), 3600)
            .unwrap();
        let text = store.render_entity("Session", "s1").unwrap().unwrap();
        assert!(text.contains("user: alice"));
        assert!(!text.contains("_ttl"));
        assert!(!text.contains("_expires_at"));
    }

    // ── Context window tests ──────────────────────────────────

    #[test]
    fn context_window_top_n_by_importance() {
        let (_dir, mut store) = tmp_store();
        for i in 1..=5 {
            store
                .insert_entity(
                    "Note",
                    &format!("n{}", i),
                    &json!({"text": format!("note {}", i), "_importance": i}),
                )
                .unwrap();
        }
        // Set a budget that fits ~3 entities
        // Each entity is roughly "mutate:@Note:nX\n    text: note X" = ~30 bytes + separator
        let opts = ContextWindowOptions {
            budget_bytes: 120,
            ..Default::default()
        };
        let cw = store.context_window(&opts).unwrap();
        assert_eq!(cw.entity_count, 3);
        assert_eq!(cw.truncated_count, 2);
        // Highest importance first (descending)
        assert!(cw.content.contains("mutate:@Note:n5"));
        assert!(cw.content.contains("mutate:@Note:n4"));
        assert!(cw.content.contains("mutate:@Note:n3"));
    }

    #[test]
    fn context_window_empty_store() {
        let (_dir, mut store) = tmp_store();
        let opts = ContextWindowOptions::default();
        let cw = store.context_window(&opts).unwrap();
        assert_eq!(cw.entity_count, 0);
        assert_eq!(cw.truncated_count, 0);
        assert!(cw.content.is_empty());
    }

    #[test]
    fn context_window_zero_budget() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("Note", "n1", &json!({"text": "hello"}))
            .unwrap();
        store
            .insert_entity("Note", "n2", &json!({"text": "world"}))
            .unwrap();
        let opts = ContextWindowOptions {
            budget_bytes: 0,
            ..Default::default()
        };
        let cw = store.context_window(&opts).unwrap();
        assert_eq!(cw.entity_count, 0);
        assert_eq!(cw.truncated_count, 2);
    }

    #[test]
    fn context_window_filter_by_entity_type() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("Note", "n1", &json!({"text": "note", "_importance": 10}))
            .unwrap();
        store
            .insert_entity("Task", "t1", &json!({"text": "task", "_importance": 10}))
            .unwrap();
        let opts = ContextWindowOptions {
            entity_types: vec!["Task".to_string()],
            ..Default::default()
        };
        let cw = store.context_window(&opts).unwrap();
        assert_eq!(cw.entity_count, 1);
        assert!(cw.content.contains("mutate:@Task:t1"));
        assert!(!cw.content.contains("Note"));
    }

    #[test]
    fn context_window_excludes_expired() {
        let (_dir, mut store) = tmp_store();
        // Insert one expired, one alive
        store
            .insert_entity_with_ttl("Session", "expired", &json!({"user": "old"}), 0)
            .unwrap();
        store
            .insert_entity("Session", "alive", &json!({"user": "current"}))
            .unwrap();
        let opts = ContextWindowOptions::default();
        let cw = store.context_window(&opts).unwrap();
        assert_eq!(cw.entity_count, 1);
        assert!(cw.content.contains("alive"));
        assert!(!cw.content.contains("expired"));
    }

    // ── Search index from store test ──────────────────────────

    #[test]
    fn build_search_index_from_store() {
        let (_dir, mut store) = tmp_store();
        store
            .insert_entity("Task", "t1", &json!({"title": "fix rust compiler"}))
            .unwrap();
        store
            .insert_entity("Task", "t2", &json!({"title": "update python tests"}))
            .unwrap();

        let index = store.build_search_index().unwrap();
        let results = index.search("rust compiler");
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "@Task:t1");
    }
}
