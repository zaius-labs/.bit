// query_engine.rs — Parse CLI-style queries and execute against a BitStore.

use crate::store::{BitStore, StoreError};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StoreQuery {
    pub target: QueryTarget,
    pub filter: Option<String>,
    pub sort: Option<SortSpec>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryTarget {
    Entity { name: String, id: Option<String> },
    AllEntities,
    Tasks,
    Flows,
    Schemas,
}

#[derive(Debug, Clone)]
pub struct SortSpec {
    pub field: String,
    pub descending: bool,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a query string like `@User where role=admin sort:name limit:10`
///
/// Grammar:
///   [@Entity[:id]] [where <filter>] [sort:<field>[-]] [limit:<n>]
///   "entities" | "tasks" | "flows" | "schemas"
pub fn parse_query(input: &str) -> Result<StoreQuery, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty query".to_string());
    }

    let mut rest = input;

    // Parse target
    let target = if rest.starts_with('@') {
        // Entity target: @Name or @Name:id
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let token = &rest[1..end]; // skip '@'
        rest = rest[end..].trim_start();

        if let Some((name, id)) = token.split_once(':') {
            QueryTarget::Entity {
                name: name.to_string(),
                id: Some(id.to_string()),
            }
        } else {
            QueryTarget::Entity {
                name: token.to_string(),
                id: None,
            }
        }
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let keyword = &rest[..end];
        rest = rest[end..].trim_start();

        match keyword {
            "entities" => QueryTarget::AllEntities,
            "tasks" => QueryTarget::Tasks,
            "flows" => QueryTarget::Flows,
            "schemas" => QueryTarget::Schemas,
            other => return Err(format!("unknown target: {other}")),
        }
    };

    // Parse optional clauses
    let mut filter = None;
    let mut sort = None;
    let mut limit = None;

    while !rest.is_empty() {
        if rest.starts_with("where ") {
            rest = rest["where ".len()..].trim_start();
            // Everything up to next keyword (sort: or limit:) is the filter
            let filter_end = find_keyword_boundary(rest);
            let f = rest[..filter_end].trim();
            if !f.is_empty() {
                filter = Some(f.to_string());
            }
            rest = rest[filter_end..].trim_start();
        } else if let Some(s) = rest.strip_prefix("sort:") {
            rest = s;
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let spec = &rest[..end];
            rest = rest[end..].trim_start();

            if let Some(field) = spec.strip_suffix('-') {
                sort = Some(SortSpec {
                    field: field.to_string(),
                    descending: true,
                });
            } else {
                sort = Some(SortSpec {
                    field: spec.to_string(),
                    descending: false,
                });
            }
        } else if let Some(s) = rest.strip_prefix("limit:") {
            rest = s;
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let n: usize = rest[..end]
                .parse()
                .map_err(|_| format!("invalid limit: {}", &rest[..end]))?;
            limit = Some(n);
            rest = rest[end..].trim_start();
        } else {
            return Err(format!("unexpected token: {rest}"));
        }
    }

    Ok(StoreQuery {
        target,
        filter,
        sort,
        limit,
    })
}

/// Find the start of the next keyword (sort: or limit:) in the remainder.
fn find_keyword_boundary(s: &str) -> usize {
    for keyword in &[" sort:", " limit:"] {
        if let Some(pos) = s.find(keyword) {
            return pos;
        }
    }
    s.len()
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a parsed query against a store.
pub fn execute_query(store: &mut BitStore, query: &StoreQuery) -> Result<Vec<Value>, StoreError> {
    let records = match &query.target {
        QueryTarget::Entity { name, id: Some(id) } => match store.get_entity(name, id)? {
            Some(v) => vec![v],
            None => vec![],
        },
        QueryTarget::Entity { name, id: None } => store
            .list_entities(name)?
            .into_iter()
            .map(|(_id, v)| v)
            .collect(),
        QueryTarget::AllEntities => {
            // The store doesn't expose a list_all_entities yet —
            // return empty for now; callers should query by type.
            vec![]
        }
        QueryTarget::Tasks => store.list_all_tasks()?,
        QueryTarget::Flows => store
            .list_flows()?
            .into_iter()
            .map(|(_name, v)| v)
            .collect(),
        QueryTarget::Schemas => store
            .list_schemas()?
            .into_iter()
            .map(|(_name, v)| v)
            .collect(),
    };

    let filtered = apply_filter(records, &query.filter);
    let sorted = apply_sort(filtered, &query.sort);
    let limited = apply_limit(sorted, query.limit);
    Ok(limited)
}

// ---------------------------------------------------------------------------
// Filter / Sort / Limit
// ---------------------------------------------------------------------------

fn apply_filter(records: Vec<Value>, filter: &Option<String>) -> Vec<Value> {
    let Some(filter) = filter else {
        return records;
    };
    records
        .into_iter()
        .filter(|r| eval_filter(r, filter))
        .collect()
}

fn eval_filter(record: &Value, filter: &str) -> bool {
    // Support: field=value, field!=value, field>value, field<value
    // Support: "and" / "or" connectors
    let filter = filter.trim();

    // Split on " or " first (lower precedence)
    if let Some((left, right)) = filter.split_once(" or ") {
        return eval_filter(record, left) || eval_filter(record, right);
    }

    // Split on " and "
    if let Some((left, right)) = filter.split_once(" and ") {
        return eval_filter(record, left) && eval_filter(record, right);
    }

    // Single condition
    if let Some((field, value)) = filter.split_once("!=") {
        !field_matches(record, field.trim(), value.trim())
    } else if let Some((field, value)) = filter.split_once(">=") {
        field_cmp(record, field.trim(), value.trim())
            .is_some_and(|o| o == std::cmp::Ordering::Greater || o == std::cmp::Ordering::Equal)
    } else if let Some((field, value)) = filter.split_once("<=") {
        field_cmp(record, field.trim(), value.trim())
            .is_some_and(|o| o == std::cmp::Ordering::Less || o == std::cmp::Ordering::Equal)
    } else if let Some((field, value)) = filter.split_once('>') {
        field_cmp(record, field.trim(), value.trim())
            .is_some_and(|o| o == std::cmp::Ordering::Greater)
    } else if let Some((field, value)) = filter.split_once('<') {
        field_cmp(record, field.trim(), value.trim()).is_some_and(|o| o == std::cmp::Ordering::Less)
    } else if let Some((field, value)) = filter.split_once('=') {
        field_matches(record, field.trim(), value.trim())
    } else {
        // Just a field name — check if truthy
        record.get(filter).is_some_and(|v| !v.is_null())
    }
}

fn field_matches(record: &Value, field: &str, expected: &str) -> bool {
    let Some(val) = record.get(field) else {
        return false;
    };
    match val {
        Value::String(s) => s == expected,
        Value::Number(n) => n.to_string() == expected,
        Value::Bool(b) => b.to_string() == expected,
        Value::Null => expected == "null" || expected == "nil",
        _ => false,
    }
}

fn field_cmp(record: &Value, field: &str, expected: &str) -> Option<std::cmp::Ordering> {
    let val = record.get(field)?;
    // Try numeric comparison first
    let val_f = match val {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    let exp_f = expected.parse::<f64>().ok();

    if let (Some(v), Some(e)) = (val_f, exp_f) {
        return v.partial_cmp(&e);
    }

    // Fall back to string comparison
    let val_s = match val {
        Value::String(s) => s.as_str().to_string(),
        other => other.to_string(),
    };
    Some(val_s.as_str().cmp(expected))
}

fn apply_sort(mut records: Vec<Value>, sort: &Option<SortSpec>) -> Vec<Value> {
    let Some(sort) = sort else {
        return records;
    };
    records.sort_by(|a, b| {
        let va = a.get(&sort.field);
        let vb = b.get(&sort.field);
        let ord = cmp_values(va, vb);
        if sort.descending {
            ord.reverse()
        } else {
            ord
        }
    });
    records
}

fn cmp_values(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(va), Some(vb)) => {
            // Try numeric
            if let (Some(na), Some(nb)) = (va.as_f64(), vb.as_f64()) {
                return na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
            }
            // Fall back to string
            let sa = value_to_sort_string(va);
            let sb = value_to_sort_string(vb);
            sa.cmp(&sb)
        }
    }
}

fn value_to_sort_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn apply_limit(records: Vec<Value>, limit: Option<usize>) -> Vec<Value> {
    match limit {
        Some(n) => records.into_iter().take(n).collect(),
        None => records,
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

    // -- Parse tests --

    #[test]
    fn parse_entity_no_id() {
        let q = parse_query("@User").unwrap();
        assert_eq!(
            q.target,
            QueryTarget::Entity {
                name: "User".to_string(),
                id: None
            }
        );
        assert!(q.filter.is_none());
        assert!(q.sort.is_none());
        assert!(q.limit.is_none());
    }

    #[test]
    fn parse_entity_with_id() {
        let q = parse_query("@User:alice").unwrap();
        assert_eq!(
            q.target,
            QueryTarget::Entity {
                name: "User".to_string(),
                id: Some("alice".to_string())
            }
        );
    }

    #[test]
    fn parse_entity_with_filter() {
        let q = parse_query("@User where role=admin").unwrap();
        assert_eq!(
            q.target,
            QueryTarget::Entity {
                name: "User".to_string(),
                id: None
            }
        );
        assert_eq!(q.filter.as_deref(), Some("role=admin"));
    }

    #[test]
    fn parse_tasks_target() {
        let q = parse_query("tasks").unwrap();
        assert_eq!(q.target, QueryTarget::Tasks);
    }

    #[test]
    fn parse_flows_target() {
        let q = parse_query("flows").unwrap();
        assert_eq!(q.target, QueryTarget::Flows);
    }

    #[test]
    fn parse_schemas_target() {
        let q = parse_query("schemas").unwrap();
        assert_eq!(q.target, QueryTarget::Schemas);
    }

    #[test]
    fn parse_entities_target() {
        let q = parse_query("entities").unwrap();
        assert_eq!(q.target, QueryTarget::AllEntities);
    }

    #[test]
    fn parse_sort_and_limit() {
        let q = parse_query("@User sort:name limit:5").unwrap();
        let sort = q.sort.as_ref().unwrap();
        assert_eq!(sort.field, "name");
        assert!(!sort.descending);
        assert_eq!(q.limit, Some(5));
    }

    #[test]
    fn parse_sort_descending() {
        let q = parse_query("@User sort:age-").unwrap();
        let sort = q.sort.as_ref().unwrap();
        assert_eq!(sort.field, "age");
        assert!(sort.descending);
    }

    #[test]
    fn parse_full_query() {
        let q = parse_query("@User where role=admin sort:name limit:10").unwrap();
        assert_eq!(
            q.target,
            QueryTarget::Entity {
                name: "User".to_string(),
                id: None
            }
        );
        assert_eq!(q.filter.as_deref(), Some("role=admin"));
        assert_eq!(q.sort.as_ref().unwrap().field, "name");
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parse_empty_errors() {
        assert!(parse_query("").is_err());
    }

    #[test]
    fn parse_unknown_target_errors() {
        assert!(parse_query("foobar").is_err());
    }

    // -- Filter eval tests --

    #[test]
    fn filter_eq() {
        let r = json!({"role": "admin", "name": "Alice"});
        assert!(eval_filter(&r, "role=admin"));
        assert!(!eval_filter(&r, "role=editor"));
    }

    #[test]
    fn filter_neq() {
        let r = json!({"role": "admin"});
        assert!(eval_filter(&r, "role!=editor"));
        assert!(!eval_filter(&r, "role!=admin"));
    }

    #[test]
    fn filter_gt() {
        let r = json!({"age": 30});
        assert!(eval_filter(&r, "age>20"));
        assert!(!eval_filter(&r, "age>40"));
    }

    #[test]
    fn filter_and_or() {
        let r = json!({"role": "admin", "age": 30});
        assert!(eval_filter(&r, "role=admin and age>20"));
        assert!(!eval_filter(&r, "role=editor and age>20"));
        assert!(eval_filter(&r, "role=editor or age>20"));
    }

    // -- Execute tests --

    fn make_test_store() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bitstore");
        let mut store = BitStore::create(&path).unwrap();
        store
            .insert_entity(
                "User",
                "alice",
                &json!({"name": "Alice", "role": "admin", "age": 30}),
            )
            .unwrap();
        store
            .insert_entity(
                "User",
                "bob",
                &json!({"name": "Bob", "role": "editor", "age": 25}),
            )
            .unwrap();
        store
            .insert_entity(
                "User",
                "carol",
                &json!({"name": "Carol", "role": "admin", "age": 35}),
            )
            .unwrap();
        store
            .insert_task("f.bit", 1, 0, &json!({"text": "Do thing"}))
            .unwrap();
        store
            .insert_flow("release", &json!({"name": "release"}))
            .unwrap();
        store
            .insert_schema("User", &json!({"entity": "User"}))
            .unwrap();
        store.flush().unwrap();
        (dir, path)
    }

    #[test]
    fn execute_entity_by_id() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("@User:alice").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "Alice");
    }

    #[test]
    fn execute_entity_list() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("@User").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn execute_entity_with_filter() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("@User where role=admin").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn execute_with_sort_and_limit() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("@User sort:name limit:2").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["name"], "Alice");
        assert_eq!(results[1]["name"], "Bob");
    }

    #[test]
    fn execute_tasks() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("tasks").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn execute_flows() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("flows").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn execute_schemas() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("schemas").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn execute_missing_entity() {
        let (_dir, path) = make_test_store();
        let mut store = BitStore::open(&path).unwrap();
        let q = parse_query("@User:nobody").unwrap();
        let results = execute_query(&mut store, &q).unwrap();
        assert!(results.is_empty());
    }
}
