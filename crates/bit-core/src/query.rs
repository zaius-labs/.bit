use crate::mutate::{
    normalize_record, parse_literal_value, value_equals, value_to_f64, value_to_string, Record,
    RecordStore,
};
use crate::schema::SchemaRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub entity: String,
    pub plural: bool,
    pub filter: Option<String>,
    pub sort: Option<SortSpec>,
    pub limit: Option<usize>,
    pub include: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortSpec {
    pub field: String,
    pub descending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub records: Vec<Record>,
    pub count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

pub fn execute_query(query: &QueryRequest, store: &RecordStore) -> QueryResult {
    execute_query_with_schemas(query, store, None)
}

pub fn execute_query_with_schemas(
    query: &QueryRequest,
    store: &RecordStore,
    schemas: Option<&SchemaRegistry>,
) -> QueryResult {
    // Hard error: entity not present in store (with schema validation)
    if let Some(schemas) = schemas {
        if !schemas.entities.contains_key(&query.entity) {
            return QueryResult {
                records: vec![],
                count: 0,
                errors: vec![format!(
                    "Query on unknown entity @{}: not defined in schema",
                    query.entity
                )],
            };
        }
    }

    let all = store
        .tables
        .get(&query.entity)
        .map(|t| t.values().map(normalize_record).collect::<Vec<_>>())
        .unwrap_or_default();

    let filtered = if let Some(filter) = &query.filter {
        apply_filter(all, filter)
    } else {
        all
    };

    let sorted = if let Some(sort) = &query.sort {
        apply_sort(filtered, sort)
    } else {
        filtered
    };

    let limited = if let Some(limit) = query.limit {
        sorted.into_iter().take(limit).collect()
    } else {
        sorted
    };

    let count = limited.len();
    QueryResult {
        records: limited,
        count,
        errors: vec![],
    }
}

fn apply_filter(records: Vec<Record>, filter: &str) -> Vec<Record> {
    let or_clauses: Vec<&str> = filter.split(" or ").collect();

    records
        .into_iter()
        .filter(|record| {
            or_clauses.iter().any(|or_clause| {
                let and_clauses: Vec<&str> = or_clause.split(" and ").collect();
                and_clauses
                    .iter()
                    .all(|clause| eval_predicate(record, clause.trim()))
            })
        })
        .collect()
}

type FilterOp = fn(Option<&Value>, &str) -> bool;

fn eval_predicate(record: &Record, clause: &str) -> bool {
    let ops: &[(&str, FilterOp)] = &[
        ("!~", op_not_fuzzy),
        ("~~", op_semantic),
        ("~=", op_fuzzy),
        ("!=", op_neq),
        (">=", op_gte),
        ("<=", op_lte),
        ("=", op_eq),
        (">", op_gt),
        ("<", op_lt),
    ];

    for &(op_str, op_fn) in ops {
        if let Some(pos) = clause.find(op_str) {
            let field = clause[..pos].trim();
            let value = clause[pos + op_str.len()..].trim().trim_matches('"');
            let actual = record.get(field);
            return op_fn(actual, value);
        }
    }
    true
}

fn op_eq(actual: Option<&Value>, expected: &str) -> bool {
    if expected == "nil" {
        return actual.is_none() || actual == Some(&Value::Null);
    }
    let expected = parse_literal_value(expected);
    actual.is_some_and(|value| value_equals(value, &expected))
}

fn op_neq(actual: Option<&Value>, expected: &str) -> bool {
    if expected == "nil" {
        return actual.is_some() && actual != Some(&Value::Null);
    }
    let expected = parse_literal_value(expected);
    actual.is_none_or(|value| !value_equals(value, &expected))
}

fn op_gt(actual: Option<&Value>, expected: &str) -> bool {
    num_cmp(actual, expected, |a, b| a > b)
}

fn op_lt(actual: Option<&Value>, expected: &str) -> bool {
    num_cmp(actual, expected, |a, b| a < b)
}

fn op_gte(actual: Option<&Value>, expected: &str) -> bool {
    num_cmp(actual, expected, |a, b| a >= b)
}

fn op_lte(actual: Option<&Value>, expected: &str) -> bool {
    num_cmp(actual, expected, |a, b| a <= b)
}

fn op_fuzzy(actual: Option<&Value>, expected: &str) -> bool {
    actual.is_some_and(|value| fuzzy_match(&value_to_string(value), expected))
}

fn op_not_fuzzy(actual: Option<&Value>, expected: &str) -> bool {
    !op_fuzzy(actual, expected)
}

fn op_semantic(actual: Option<&Value>, expected: &str) -> bool {
    op_fuzzy(actual, expected)
}

fn num_cmp(actual: Option<&Value>, expected: &str, cmp: fn(f64, f64) -> bool) -> bool {
    let a: f64 = actual.and_then(value_to_f64).unwrap_or(0.0);
    let b: f64 = expected.parse().unwrap_or(0.0);
    cmp(a, b)
}

fn fuzzy_match(a: &str, b: &str) -> bool {
    let al = a.to_lowercase();
    let bl = b.to_lowercase();
    if al.contains(&bl) || bl.contains(&al) {
        return true;
    }
    // Check individual words for fuzzy substring match
    for word in al.split_whitespace() {
        if levenshtein(word, &bl) <= 2 {
            return true;
        }
    }
    levenshtein(&al, &bl) <= 3
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut m = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in m.iter_mut().enumerate().take(a.len() + 1) {
        row[0] = i;
    }
    for (j, val) in m[0].iter_mut().enumerate().take(b.len() + 1) {
        *val = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            m[i][j] = (m[i - 1][j] + 1)
                .min(m[i][j - 1] + 1)
                .min(m[i - 1][j - 1] + cost);
        }
    }
    m[a.len()][b.len()]
}

fn apply_sort(mut records: Vec<Record>, sort: &SortSpec) -> Vec<Record> {
    records.sort_by(|a, b| {
        let av = a
            .get(&sort.field)
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let bv = b
            .get(&sort.field)
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));

        let cmp = if let (Some(an), Some(bn)) = (value_to_f64(&av), value_to_f64(&bv)) {
            an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            value_to_string(&av).cmp(&value_to_string(&bv))
        };

        if sort.descending {
            cmp.reverse()
        } else {
            cmp
        }
    });
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutate::RecordStore;
    use serde_json::json;
    use std::collections::HashMap;

    fn test_store() -> RecordStore {
        let mut store = RecordStore::new();
        let mut table = HashMap::new();

        let mut r1 = HashMap::new();
        r1.insert("title".to_string(), json!("Build API"));
        r1.insert("status".to_string(), json!("open"));
        r1.insert("points".to_string(), json!(5));
        table.insert("t1".to_string(), r1);

        let mut r2 = HashMap::new();
        r2.insert("title".to_string(), json!("Write docs"));
        r2.insert("status".to_string(), json!("done"));
        r2.insert("points".to_string(), json!(3));
        table.insert("t2".to_string(), r2);

        let mut r3 = HashMap::new();
        r3.insert("title".to_string(), json!("Fix bug"));
        r3.insert("status".to_string(), json!("open"));
        r3.insert("points".to_string(), json!(8));
        table.insert("t3".to_string(), r3);

        store.tables.insert("Task".to_string(), table);
        store
    }

    // ── execute_query ──

    #[test]
    fn query_all_records() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Task".to_string(),
            plural: true,
            filter: None,
            sort: None,
            limit: None,
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.count, 3);
    }

    #[test]
    fn query_with_filter() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Task".to_string(),
            plural: true,
            filter: Some("status=open".to_string()),
            sort: None,
            limit: None,
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.count, 2);
    }

    #[test]
    fn query_with_limit() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Task".to_string(),
            plural: true,
            filter: None,
            sort: None,
            limit: Some(1),
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn query_with_sort_ascending() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Task".to_string(),
            plural: true,
            filter: None,
            sort: Some(SortSpec {
                field: "points".to_string(),
                descending: false,
            }),
            limit: None,
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.records[0]["points"], json!(3));
        assert_eq!(result.records[2]["points"], json!(8));
    }

    #[test]
    fn query_with_sort_descending() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Task".to_string(),
            plural: true,
            filter: None,
            sort: Some(SortSpec {
                field: "points".to_string(),
                descending: true,
            }),
            limit: None,
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.records[0]["points"], json!(8));
        assert_eq!(result.records[2]["points"], json!(3));
    }

    #[test]
    fn query_nonexistent_entity() {
        let store = test_store();
        let q = QueryRequest {
            entity: "Bug".to_string(),
            plural: true,
            filter: None,
            sort: None,
            limit: None,
            include: None,
        };
        let result = execute_query(&q, &store);
        assert_eq!(result.count, 0);
    }

    // ── filter ops ──

    #[test]
    fn op_eq_match() {
        assert!(op_eq(Some(&json!("open")), "open"));
    }

    #[test]
    fn op_eq_no_match() {
        assert!(!op_eq(Some(&json!("open")), "done"));
    }

    #[test]
    fn op_eq_nil() {
        assert!(op_eq(None, "nil"));
        assert!(!op_eq(Some(&json!("x")), "nil"));
    }

    #[test]
    fn op_neq_match() {
        assert!(op_neq(Some(&json!("open")), "done"));
    }

    #[test]
    fn op_neq_nil() {
        assert!(op_neq(Some(&json!("x")), "nil"));
        assert!(!op_neq(None, "nil"));
    }

    #[test]
    fn op_gt_numbers() {
        assert!(op_gt(Some(&json!(10)), "5"));
        assert!(!op_gt(Some(&json!(3)), "5"));
    }

    #[test]
    fn op_lt_numbers() {
        assert!(op_lt(Some(&json!(3)), "5"));
        assert!(!op_lt(Some(&json!(10)), "5"));
    }

    #[test]
    fn op_gte_numbers() {
        assert!(op_gte(Some(&json!(5)), "5"));
        assert!(op_gte(Some(&json!(6)), "5"));
        assert!(!op_gte(Some(&json!(4)), "5"));
    }

    #[test]
    fn op_lte_numbers() {
        assert!(op_lte(Some(&json!(5)), "5"));
        assert!(op_lte(Some(&json!(4)), "5"));
        assert!(!op_lte(Some(&json!(6)), "5"));
    }

    #[test]
    fn op_fuzzy_match() {
        assert!(op_fuzzy(Some(&json!("Build API")), "build"));
        assert!(op_fuzzy(Some(&json!("hello")), "hell"));
    }

    #[test]
    fn op_not_fuzzy_match() {
        assert!(!op_not_fuzzy(Some(&json!("Build API")), "build"));
        assert!(op_not_fuzzy(
            Some(&json!("xyz")),
            "completely different long string"
        ));
    }

    // ── or filter ──

    #[test]
    fn filter_or_clauses() {
        let records = vec![
            HashMap::from([("s".to_string(), json!("a"))]),
            HashMap::from([("s".to_string(), json!("b"))]),
            HashMap::from([("s".to_string(), json!("c"))]),
        ];
        let result = apply_filter(records, "s=a or s=b");
        assert_eq!(result.len(), 2);
    }

    // ── levenshtein ──

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("test", "test"), 0);
    }

    #[test]
    fn levenshtein_one_char() {
        assert_eq!(levenshtein("cat", "bat"), 1);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
    }
}
