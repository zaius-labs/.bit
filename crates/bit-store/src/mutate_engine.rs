// mutate_engine.rs — Insert, update, and delete entities via a CLI-style interface.

use crate::store::{BitStore, StoreError};
use serde_json::{json, Value};

/// Insert a new entity from key=value pairs.
pub fn store_insert(
    store: &mut BitStore,
    entity: &str,
    id: &str,
    fields: &[(&str, &str)],
) -> Result<(), StoreError> {
    let mut record = serde_json::Map::new();
    for (k, v) in fields {
        record.insert(k.to_string(), parse_value(v));
    }
    store.insert_entity(entity, id, &Value::Object(record))
}

/// Update fields on an existing entity (merge).
/// Returns `true` if the entity existed and was updated, `false` if not found.
pub fn store_update(
    store: &mut BitStore,
    entity: &str,
    id: &str,
    fields: &[(&str, &str)],
) -> Result<bool, StoreError> {
    let existing = store.get_entity(entity, id)?;
    let Some(mut existing) = existing else {
        return Ok(false);
    };

    if let Some(obj) = existing.as_object_mut() {
        for (k, v) in fields {
            obj.insert(k.to_string(), parse_value(v));
        }
    }
    store.insert_entity(entity, id, &existing)?;
    Ok(true)
}

/// Delete an entity. Returns `true` if it existed.
pub fn store_delete(store: &mut BitStore, entity: &str, id: &str) -> Result<bool, StoreError> {
    Ok(store.delete_entity(entity, id)?.is_some())
}

/// Upsert an entity — merge fields if it exists, create if it doesn't.
pub fn store_upsert(
    store: &mut BitStore,
    entity: &str,
    id: &str,
    fields: &[(&str, &str)],
) -> Result<(), StoreError> {
    let mut record = serde_json::Map::new();
    for (k, v) in fields {
        record.insert(k.to_string(), parse_value(v));
    }
    store.upsert_entity(entity, id, &Value::Object(record))
}

/// Parse a string value into a JSON Value (detect numbers, booleans, null).
fn parse_value(s: &str) -> Value {
    if s == "true" {
        return json!(true);
    }
    if s == "false" {
        return json!(false);
    }
    if s == "nil" || s == "null" {
        return Value::Null;
    }
    if let Ok(n) = s.parse::<i64>() {
        return json!(n);
    }
    if let Ok(n) = s.parse::<f64>() {
        return json!(n);
    }
    json!(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp_store() -> (TempDir, BitStore) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        let store = BitStore::create(&path).unwrap();
        (dir, store)
    }

    #[test]
    fn insert_and_query_back() {
        let (_dir, mut store) = tmp_store();
        store_insert(
            &mut store,
            "User",
            "alice",
            &[("name", "Alice"), ("role", "admin")],
        )
        .unwrap();

        let val = store.get_entity("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["role"], "admin");
    }

    #[test]
    fn update_entity_field() {
        let (_dir, mut store) = tmp_store();
        store_insert(
            &mut store,
            "User",
            "alice",
            &[("name", "Alice"), ("role", "editor")],
        )
        .unwrap();

        let updated = store_update(&mut store, "User", "alice", &[("role", "admin")]).unwrap();
        assert!(updated);

        let val = store.get_entity("User", "alice").unwrap().unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["role"], "admin");
    }

    #[test]
    fn delete_entity() {
        let (_dir, mut store) = tmp_store();
        store_insert(&mut store, "User", "alice", &[("name", "Alice")]).unwrap();

        let deleted = store_delete(&mut store, "User", "alice").unwrap();
        assert!(deleted);

        let val = store.get_entity("User", "alice").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn insert_with_typed_values() {
        let (_dir, mut store) = tmp_store();
        store_insert(
            &mut store,
            "Config",
            "app",
            &[
                ("retries", "3"),
                ("enabled", "true"),
                ("rate", "1.5"),
                ("label", "hello"),
                ("empty", "null"),
            ],
        )
        .unwrap();

        let val = store.get_entity("Config", "app").unwrap().unwrap();
        assert_eq!(val["retries"], 3);
        assert_eq!(val["enabled"], true);
        assert_eq!(val["rate"], 1.5);
        assert_eq!(val["label"], "hello");
        assert!(val["empty"].is_null());
    }

    #[test]
    fn update_nonexistent_returns_false() {
        let (_dir, mut store) = tmp_store();
        let updated = store_update(&mut store, "User", "nobody", &[("x", "y")]).unwrap();
        assert!(!updated);
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let (_dir, mut store) = tmp_store();
        let deleted = store_delete(&mut store, "User", "nobody").unwrap();
        assert!(!deleted);
    }
}
