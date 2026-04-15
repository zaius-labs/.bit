use crate::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::collections::HashMap;

pub type Record = HashMap<String, Value>;

/// In-memory record store built from mutate: blocks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordStore {
    /// entity_name -> (record_id -> field_map)
    pub tables: HashMap<String, HashMap<String, Record>>,
    #[serde(default)]
    pub counters: HashMap<String, usize>,
}

impl RecordStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_doc(&mut self, doc: &Document) {
        self.apply_nodes(&doc.nodes);
    }

    fn apply_nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Mutate(m) => self.apply_mutate(m),
                Node::Delete(d) => self.apply_delete(d),
                Node::Group(g) => self.apply_nodes(&g.children),
                Node::Validate(v) => self.apply_nodes(&v.children),
                Node::Conditional(c) => self.apply_nodes(&c.children),
                _ => {}
            }
        }
    }

    pub fn apply_mutate(&mut self, m: &Mutate) {
        if let Some(batch) = &m.batch {
            let table = self.tables.entry(m.entity.clone()).or_default();
            for rec in batch {
                let record = table.entry(rec.id.clone()).or_default();
                for (k, v) in &rec.fields {
                    let value = parse_literal_value(v);
                    let field_name = k.clone();
                    if let Some(obj) = value.as_object() {
                        if let Some(op) = obj.get("__list_op").and_then(|v| v.as_str()) {
                            let items = obj.get("items").cloned().unwrap_or(Value::Array(vec![]));
                            let existing = record
                                .get(&field_name)
                                .cloned()
                                .unwrap_or(Value::Array(vec![]));
                            let existing_arr = existing.as_array().cloned().unwrap_or_default();
                            match op {
                                "append" => {
                                    let mut merged = existing_arr;
                                    if let Some(new_items) = items.as_array() {
                                        merged.extend(new_items.iter().cloned());
                                    }
                                    record.insert(field_name, Value::Array(merged));
                                }
                                "remove" => {
                                    let remove_set: Vec<&Value> = items
                                        .as_array()
                                        .map(|a| a.iter().collect())
                                        .unwrap_or_default();
                                    let filtered: Vec<Value> = existing_arr
                                        .into_iter()
                                        .filter(|v| !remove_set.contains(&v))
                                        .collect();
                                    record.insert(field_name, Value::Array(filtered));
                                }
                                _ => {
                                    record.insert(field_name, value);
                                }
                            }
                            continue;
                        }
                    }
                    record.insert(field_name, value);
                }
            }
            return;
        }

        let table = self.tables.entry(m.entity.clone()).or_default();
        let id = m.id.clone().unwrap_or_else(|| {
            let counter = self.counters.entry(m.entity.clone()).or_insert(0);
            *counter += 1;
            format!("auto-{}", counter)
        });

        let record = table.entry(id).or_default();
        for (k, v) in &m.fields {
            let value = parse_literal_value(v);
            let field_name = k.clone();
            if let Some(obj) = value.as_object() {
                if let Some(op) = obj.get("__list_op").and_then(|v| v.as_str()) {
                    let items = obj.get("items").cloned().unwrap_or(Value::Array(vec![]));
                    let existing = record
                        .get(&field_name)
                        .cloned()
                        .unwrap_or(Value::Array(vec![]));
                    let existing_arr = existing.as_array().cloned().unwrap_or_default();
                    match op {
                        "append" => {
                            let mut merged = existing_arr;
                            if let Some(new_items) = items.as_array() {
                                merged.extend(new_items.iter().cloned());
                            }
                            record.insert(field_name, Value::Array(merged));
                        }
                        "remove" => {
                            let remove_set: Vec<&Value> = items
                                .as_array()
                                .map(|a| a.iter().collect())
                                .unwrap_or_default();
                            let filtered: Vec<Value> = existing_arr
                                .into_iter()
                                .filter(|v| !remove_set.contains(&v))
                                .collect();
                            record.insert(field_name, Value::Array(filtered));
                        }
                        _ => {
                            record.insert(field_name, value);
                        }
                    }
                    continue;
                }
            }
            record.insert(field_name, value);
        }
    }

    fn apply_delete(&mut self, d: &Delete) {
        if let Some(table) = self.tables.get_mut(&d.entity) {
            table.remove(&d.id);
        }
    }

    pub fn query_simple(
        &self,
        entity: &str,
        filter_field: Option<&str>,
        filter_value: Option<&str>,
    ) -> Vec<Record> {
        let Some(table) = self.tables.get(entity) else {
            return Vec::new();
        };

        table
            .values()
            .filter(|record| match (filter_field, filter_value) {
                (Some(field), Some(value)) => {
                    let expected = parse_literal_value(value);
                    record
                        .get(field)
                        .is_some_and(|v| value_equals(v, &expected))
                }
                _ => true,
            })
            .cloned()
            .collect()
    }

    pub fn normalize_values(&mut self) {
        for table in self.tables.values_mut() {
            for record in table.values_mut() {
                for value in record.values_mut() {
                    *value = normalize_value(value);
                }
            }
        }
    }
}

pub fn parse_literal_value(value: &str) -> Value {
    let trimmed = value.trim();

    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return serde_json::from_str::<String>(trimmed)
            .map(Value::String)
            .unwrap_or_else(|_| Value::String(trimmed[1..trimmed.len() - 1].to_string()));
    }

    if trimmed == "nil" {
        return Value::Null;
    }

    if trimmed == "true" || trimmed == "false" {
        return Value::Bool(trimmed == "true");
    }

    // List append operator: +["item1", "item2"]
    if trimmed.starts_with("+[") && trimmed.ends_with(']') {
        let inner = &trimmed[1..]; // strip the +, leaving [...]
        if let Ok(arr) = serde_json::from_str::<Value>(inner) {
            return Value::Object(serde_json::Map::from_iter(vec![
                ("__list_op".to_string(), Value::String("append".to_string())),
                ("items".to_string(), arr),
            ]));
        }
    }

    // List remove operator: -["item1"]
    if trimmed.starts_with("-[") && trimmed.ends_with(']') {
        let inner = &trimmed[1..]; // strip the -, leaving [...]
        if let Ok(arr) = serde_json::from_str::<Value>(inner) {
            return Value::Object(serde_json::Map::from_iter(vec![
                ("__list_op".to_string(), Value::String("remove".to_string())),
                ("items".to_string(), arr),
            ]));
        }
    }

    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            return normalize_value(&parsed);
        }
    }

    if let Ok(parsed) = trimmed.parse::<i64>() {
        return Value::Number(parsed.into());
    }

    if let Ok(parsed) = trimmed.parse::<f64>() {
        if let Some(number) = Number::from_f64(parsed) {
            return Value::Number(number);
        }
    }

    Value::String(trimmed.to_string())
}

pub fn normalize_value(value: &Value) -> Value {
    match value {
        Value::String(raw) => parse_literal_value(raw),
        Value::Array(items) => Value::Array(items.iter().map(normalize_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), normalize_value(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn normalize_record(record: &Record) -> Record {
    record
        .iter()
        .map(|(key, value)| (key.clone(), normalize_value(value)))
        .collect()
}

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "nil".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

pub fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    }
}

pub fn value_equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::String(left), Value::String(right)) => left == right,
        _ => value_to_string(left) == value_to_string(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mutate(entity: &str, id: Option<&str>, fields: Vec<(&str, &str)>) -> Mutate {
        Mutate {
            entity: entity.to_string(),
            id: id.map(|s| s.to_string()),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            batch: None,
            gate: None,
            mod_scope: None,
            workspace_scope: None,
        }
    }

    fn make_delete(entity: &str, id: &str) -> Delete {
        Delete {
            entity: entity.to_string(),
            id: id.to_string(),
            mod_scope: None,
            workspace_scope: None,
        }
    }

    #[test]
    fn auto_id_no_collision_after_delete() {
        let mut store = RecordStore::new();
        store.apply_mutate(&make_mutate("Task", None, vec![("title", "First")]));
        store.apply_mutate(&make_mutate("Task", None, vec![("title", "Second")]));
        assert_eq!(store.tables["Task"].len(), 2);
        assert_eq!(
            store.tables["Task"]["auto-1"]["title"],
            Value::String("First".to_string())
        );
        assert_eq!(
            store.tables["Task"]["auto-2"]["title"],
            Value::String("Second".to_string())
        );

        store.apply_delete(&make_delete("Task", "auto-1"));
        assert_eq!(store.tables["Task"].len(), 1);

        store.apply_mutate(&make_mutate("Task", None, vec![("title", "Third")]));
        assert_eq!(store.tables["Task"].len(), 2);
        assert_eq!(
            store.tables["Task"]["auto-3"]["title"],
            Value::String("Third".to_string())
        );
        assert_eq!(
            store.tables["Task"]["auto-2"]["title"],
            Value::String("Second".to_string())
        );
    }

    #[test]
    fn explicit_id_not_affected_by_counter() {
        let mut store = RecordStore::new();
        store.apply_mutate(&make_mutate(
            "Task",
            Some("my-id"),
            vec![("title", "Explicit")],
        ));
        store.apply_mutate(&make_mutate("Task", None, vec![("title", "Auto")]));
        assert_eq!(
            store.tables["Task"]["my-id"]["title"],
            Value::String("Explicit".to_string())
        );
        assert_eq!(
            store.tables["Task"]["auto-1"]["title"],
            Value::String("Auto".to_string())
        );
    }

    #[test]
    fn counter_persists_through_serialization() {
        let mut store = RecordStore::new();
        store.apply_mutate(&make_mutate("Task", None, vec![("title", "First")]));
        store.apply_mutate(&make_mutate("Task", None, vec![("title", "Second")]));

        let json = serde_json::to_string(&store).unwrap();
        let mut deserialized: RecordStore = serde_json::from_str(&json).unwrap();

        deserialized.apply_mutate(&make_mutate("Task", None, vec![("title", "Third")]));
        assert_eq!(
            deserialized.tables["Task"]["auto-3"]["title"],
            Value::String("Third".to_string())
        );
    }

    #[test]
    fn normalize_legacy_string_store_values() {
        let mut store: RecordStore = serde_json::from_str(
            r#"{"tables":{"Task":{"a":{"title":"\"Ship\"","effort":"5","active":"true","notes":"nil"}}},"counters":{}}"#,
        )
        .unwrap();

        store.normalize_values();

        assert_eq!(
            store.tables["Task"]["a"]["title"],
            Value::String("Ship".to_string())
        );
        assert_eq!(store.tables["Task"]["a"]["effort"], Value::Number(5.into()));
        assert_eq!(store.tables["Task"]["a"]["active"], Value::Bool(true));
        assert_eq!(store.tables["Task"]["a"]["notes"], Value::Null);
    }
}
