use crate::mutate::{
    normalize_record, parse_literal_value, value_equals, value_to_f64, value_to_string, Record,
    RecordStore,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EvalResult {
    Number {
        value: f64,
    },
    String {
        value: String,
    },
    Bool {
        value: bool,
    },
    List {
        items: Vec<EvalResult>,
    },
    Map {
        entries: HashMap<String, EvalResult>,
    },
    Null,
    Error {
        message: String,
    },
}

pub fn eval_compute(expr: &str, store: &RecordStore, vars: &HashMap<String, String>) -> EvalResult {
    let expr = expr.trim();

    if let Some(result) = try_variable(expr, vars) {
        return result;
    }

    if let Some(result) = try_field_access(expr, store) {
        return result;
    }

    if let Some(result) = try_aggregation(expr, store) {
        return result;
    }

    if let Some(result) = try_time_expr(expr) {
        return result;
    }

    if let Some(result) = try_conversion(expr) {
        return result;
    }

    if let Some(result) = try_comparison(expr, store, vars) {
        return result;
    }

    if let Some(result) = try_math(expr, store, vars) {
        return result;
    }

    EvalResult::Error {
        message: format!("Cannot evaluate: {}", expr),
    }
}

fn try_variable(expr: &str, vars: &HashMap<String, String>) -> Option<EvalResult> {
    if let Some(val) = vars.get(expr) {
        if let Ok(n) = val.parse::<f64>() {
            return Some(EvalResult::Number { value: n });
        }
        if val == "true" || val == "false" {
            return Some(EvalResult::Bool {
                value: val == "true",
            });
        }
        return Some(EvalResult::String { value: val.clone() });
    }
    None
}

fn try_field_access(expr: &str, store: &RecordStore) -> Option<EvalResult> {
    // Entity:id.field
    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    let field = parts[1];
    let entity_id: Vec<&str> = parts[0].splitn(2, ':').collect();
    if entity_id.len() != 2 {
        return None;
    }
    let entity = entity_id[0];
    let id = entity_id[1];

    let table = store.tables.get(entity)?;
    let record = table.get(id)?;
    let value = record.get(field)?;

    match value {
        Value::Null => Some(EvalResult::Null),
        Value::Bool(value) => Some(EvalResult::Bool { value: *value }),
        Value::Number(value) => Some(EvalResult::Number {
            value: value.as_f64().unwrap_or(0.0),
        }),
        Value::String(value) => Some(EvalResult::String {
            value: value.clone(),
        }),
        Value::Array(_) | Value::Object(_) => Some(EvalResult::String {
            value: value.to_string(),
        }),
    }
}

fn try_aggregation(expr: &str, store: &RecordStore) -> Option<EvalResult> {
    let words: Vec<&str> = expr.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    let op = words[0].to_lowercase();
    let has_group_by = words.iter().position(|w| *w == "group_by");

    match op.as_str() {
        "count" if words.len() >= 2 => {
            let entity = words[1];
            let where_idx = words.iter().position(|w| *w == "where");

            let records = if let Some(wi) = where_idx {
                let filter_str: String =
                    words[wi + 1..has_group_by.unwrap_or(words.len())].join(" ");
                filter_records(store, entity, &filter_str)
            } else {
                all_records(store, entity)
            };

            if let Some(gb) = has_group_by {
                if gb + 1 < words.len() {
                    let field = words[gb + 1];
                    return Some(group_by_count(&records, field));
                }
            }

            Some(EvalResult::Number {
                value: records.len() as f64,
            })
        }
        "sum" | "avg" | "min" | "max" if words.len() >= 2 => {
            let field_path = words[1];
            let (entity, field) = field_path.split_once('.').unwrap_or((field_path, ""));
            let where_idx = words.iter().position(|w| *w == "where");

            let records = if let Some(wi) = where_idx {
                let filter_str: String = words[wi + 1..].join(" ");
                filter_records(store, entity, &filter_str)
            } else {
                all_records(store, entity)
            };

            let values: Vec<f64> = records
                .iter()
                .filter_map(|r| r.get(field).and_then(value_to_f64))
                .collect();

            if values.is_empty() {
                return Some(EvalResult::Number { value: 0.0 });
            }

            let result = match op.as_str() {
                "sum" => values.iter().sum(),
                "avg" => values.iter().sum::<f64>() / values.len() as f64,
                "min" => values.iter().cloned().fold(f64::INFINITY, f64::min),
                "max" => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                _ => unreachable!(),
            };

            Some(EvalResult::Number { value: result })
        }
        _ => None,
    }
}

fn try_time_expr(expr: &str) -> Option<EvalResult> {
    let lower = expr.to_lowercase();

    if lower == "today" || lower == "now" {
        return Some(EvalResult::String { value: today_str() });
    }

    if lower.starts_with("today + ") || lower.starts_with("today - ") {
        if let Some(result) = parse_date_offset(&lower) {
            return Some(EvalResult::String { value: result });
        }
    }

    if lower.starts_with("days_between")
        || lower.starts_with("days_until")
        || lower.starts_with("hours_since")
    {
        return Some(EvalResult::Number { value: 0.0 });
    }

    if lower == "next friday" || lower == "end of month" || lower.starts_with("next ") {
        return Some(EvalResult::String { value: today_str() });
    }

    None
}

fn try_conversion(expr: &str) -> Option<EvalResult> {
    // convert N unit to unit
    let words: Vec<&str> = expr.split_whitespace().collect();
    if words.first().map(|w| w.to_lowercase()) != Some("convert".into()) {
        return None;
    }
    if words.len() < 5 || words[3].to_lowercase() != "to" {
        return None;
    }

    let value: f64 = words[1].parse().ok()?;
    let from = words[2].to_lowercase();
    let to = words[4].to_lowercase();

    let result = convert_units(value, &from, &to)?;
    Some(EvalResult::Number { value: result })
}

fn try_comparison(
    expr: &str,
    store: &RecordStore,
    vars: &HashMap<String, String>,
) -> Option<EvalResult> {
    for &op in &[">=", "<=", "!=", "==", ">", "<"] {
        let padded = format!(" {} ", op);
        if let Some(pos) = expr.find(&padded) {
            let left = expr[..pos].trim();
            let right = expr[pos + padded.len()..].trim();

            let l = resolve_to_number(left, store, vars);
            let r = resolve_to_number(right, store, vars);

            if let (Some(lv), Some(rv)) = (l, r) {
                let result = match op {
                    ">" => lv > rv,
                    "<" => lv < rv,
                    ">=" => lv >= rv,
                    "<=" => lv <= rv,
                    "==" => (lv - rv).abs() < f64::EPSILON,
                    "!=" => (lv - rv).abs() >= f64::EPSILON,
                    _ => false,
                };
                return Some(EvalResult::Bool { value: result });
            }

            let ls = resolve_to_string(left, vars);
            let rs = resolve_to_string(right, vars);
            let result = match op {
                "==" => ls == rs,
                "!=" => ls != rs,
                _ => false,
            };
            return Some(EvalResult::Bool { value: result });
        }
    }
    None
}

fn resolve_to_string(expr: &str, vars: &HashMap<String, String>) -> String {
    vars.get(expr).cloned().unwrap_or_else(|| expr.to_string())
}

fn try_math(expr: &str, store: &RecordStore, vars: &HashMap<String, String>) -> Option<EvalResult> {
    // Single function calls
    if let Some(result) = try_math_fn(expr, store, vars) {
        return Some(result);
    }

    // Binary operators
    for &op in &["+", "-", "*", "/", "^"] {
        let padded = format!(" {} ", op);
        if let Some(pos) = expr.find(&padded) {
            let left = expr[..pos].trim();
            let right = expr[pos + padded.len()..].trim();

            let l = resolve_to_number(left, store, vars)?;
            let r = resolve_to_number(right, store, vars)?;

            let result = match op {
                "+" => l + r,
                "-" => l - r,
                "*" => l * r,
                "/" => {
                    if r != 0.0 {
                        l / r
                    } else {
                        return Some(EvalResult::Error {
                            message: "Division by zero".into(),
                        });
                    }
                }
                "^" => l.powf(r),
                _ => unreachable!(),
            };

            return Some(EvalResult::Number { value: result });
        }
    }

    if let Ok(n) = expr.parse::<f64>() {
        return Some(EvalResult::Number { value: n });
    }

    None
}

fn try_math_fn(
    expr: &str,
    store: &RecordStore,
    vars: &HashMap<String, String>,
) -> Option<EvalResult> {
    let fns = ["sqrt", "abs", "sin", "cos", "ln", "log"];
    for func in &fns {
        if expr.starts_with(func) && expr.contains('(') {
            let start = expr.find('(')? + 1;
            let end = expr.rfind(')')?;
            let inner = &expr[start..end];
            let val = resolve_to_number(inner, store, vars)?;

            let result = match *func {
                "sqrt" => val.sqrt(),
                "abs" => val.abs(),
                "sin" => val.sin(),
                "cos" => val.cos(),
                "ln" => val.ln(),
                "log" => val.log10(),
                _ => unreachable!(),
            };

            return Some(EvalResult::Number { value: result });
        }
    }
    None
}

fn resolve_to_number(
    expr: &str,
    store: &RecordStore,
    vars: &HashMap<String, String>,
) -> Option<f64> {
    if let Ok(n) = expr.parse::<f64>() {
        return Some(n);
    }
    if let Some(val) = vars.get(expr) {
        return val.parse().ok();
    }
    if let Some(EvalResult::Number { value }) = try_field_access(expr, store) {
        return Some(value);
    }
    if let Some(EvalResult::Number { value }) = try_aggregation(expr, store) {
        return Some(value);
    }
    None
}

// ── Record filtering ────────────────────────────────────────────

fn all_records(store: &RecordStore, entity: &str) -> Vec<Record> {
    store
        .tables
        .get(entity)
        .map(|t| t.values().map(normalize_record).collect())
        .unwrap_or_default()
}

fn filter_records(store: &RecordStore, entity: &str, filter: &str) -> Vec<Record> {
    let all = all_records(store, entity);
    let predicates = parse_filter_predicates(filter);
    all.into_iter()
        .filter(|record| predicates.iter().all(|p| p.matches(record)))
        .collect()
}

struct Predicate {
    field: String,
    op: String,
    value: String,
}

impl Predicate {
    fn matches(&self, record: &Record) -> bool {
        let actual = record.get(&self.field);

        match self.op.as_str() {
            "=" if self.value == "nil" => actual.is_none() || actual == Some(&Value::Null),
            "!=" if self.value == "nil" => actual.is_some() && actual != Some(&Value::Null),
            "=" => {
                let expected = parse_literal_value(&self.value);
                actual.is_some_and(|value| value_equals(value, &expected))
            }
            "!=" => {
                let expected = parse_literal_value(&self.value);
                actual.is_none_or(|value| !value_equals(value, &expected))
            }
            "<" | ">" | "<=" | ">=" => {
                let av: f64 = actual.and_then(value_to_f64).unwrap_or(0.0);
                let ev: f64 = self.value.parse().unwrap_or(0.0);
                match self.op.as_str() {
                    "<" => av < ev,
                    ">" => av > ev,
                    "<=" => av <= ev,
                    ">=" => av >= ev,
                    _ => false,
                }
            }
            "~=" => actual.is_some_and(|value| fuzzy_match(&value_to_string(value), &self.value)),
            _ => true,
        }
    }
}

fn parse_filter_predicates(filter: &str) -> Vec<Predicate> {
    let mut predicates = Vec::new();
    let clauses: Vec<&str> = filter.split(" and ").collect();

    for clause in clauses {
        let clause = clause.trim();
        for &op in &["!=", ">=", "<=", "~=", "=", ">", "<"] {
            if let Some(pos) = clause.find(op) {
                let field = clause[..pos].trim().to_string();
                let value = clause[pos + op.len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string();
                predicates.push(Predicate {
                    field,
                    op: op.to_string(),
                    value,
                });
                break;
            }
        }
    }

    predicates
}

fn fuzzy_match(a: &str, b: &str) -> bool {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    a_lower.contains(&b_lower) || b_lower.contains(&a_lower) || levenshtein(&a_lower, &b_lower) <= 3
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut matrix = vec![vec![0usize; b.len() + 1]; a.len() + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(a.len() + 1) {
        row[0] = i;
    }
    for (j, val) in matrix[0].iter_mut().enumerate().take(b.len() + 1) {
        *val = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a.len()][b.len()]
}

fn group_by_count(records: &[Record], field: &str) -> EvalResult {
    let mut groups: HashMap<String, f64> = HashMap::new();
    for record in records {
        let key = record
            .get(field)
            .map(value_to_string)
            .unwrap_or_else(|| "null".to_string());
        *groups.entry(key).or_insert(0.0) += 1.0;
    }
    EvalResult::Map {
        entries: groups
            .into_iter()
            .map(|(k, v)| (k, EvalResult::Number { value: v }))
            .collect(),
    }
}

fn today_str() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn parse_date_offset(expr: &str) -> Option<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    // "today + 3d" or "today - 1w" → [today, +/-, N, unit]
    if parts.len() < 3 {
        return Some(today_str());
    }
    let sign: i64 = if parts[1] == "-" { -1 } else { 1 };
    let token = parts[2];
    let (num_str, unit) = token.split_at(token.len().saturating_sub(1));
    let n: i64 = num_str.parse().unwrap_or(0);
    let days = match unit {
        "d" => n,
        "w" => n * 7,
        "m" => n * 30,
        "y" => n * 365,
        _ => token.parse::<i64>().unwrap_or(0),
    };
    let today = Utc::now().date_naive();
    let result = today + chrono::Duration::days(sign * days);
    Some(result.format("%Y-%m-%d").to_string())
}

fn convert_units(value: f64, from: &str, to: &str) -> Option<f64> {
    match (from, to) {
        ("km", "mi") | ("kilometers", "miles") => Some(value * 0.621371),
        ("mi", "km") | ("miles", "kilometers") => Some(value * 1.60934),
        ("kg", "lb") | ("kilograms", "pounds") => Some(value * 2.20462),
        ("lb", "kg") | ("pounds", "kilograms") => Some(value * 0.453592),
        ("c", "f") | ("celsius", "fahrenheit") => Some(value * 9.0 / 5.0 + 32.0),
        ("f", "c") | ("fahrenheit", "celsius") => Some((value - 32.0) * 5.0 / 9.0),
        ("m", "ft") | ("meters", "feet") => Some(value * 3.28084),
        ("ft", "m") | ("feet", "meters") => Some(value * 0.3048),
        ("hours", "minutes") => Some(value * 60.0),
        ("minutes", "hours") => Some(value / 60.0),
        ("days", "hours") => Some(value * 24.0),
        ("hours", "days") => Some(value / 24.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_store() -> RecordStore {
        RecordStore::new()
    }

    fn empty_vars() -> HashMap<String, String> {
        HashMap::new()
    }

    fn store_with_records() -> RecordStore {
        let mut store = RecordStore::new();
        let mut task_table = HashMap::new();
        let mut rec = HashMap::new();
        rec.insert("title".to_string(), Value::String("Build API".to_string()));
        rec.insert("points".to_string(), Value::Number(5.into()));
        rec.insert("status".to_string(), Value::String("open".to_string()));
        task_table.insert("t1".to_string(), rec);

        let mut rec2 = HashMap::new();
        rec2.insert(
            "title".to_string(),
            Value::String("Write tests".to_string()),
        );
        rec2.insert("points".to_string(), Value::Number(3.into()));
        rec2.insert("status".to_string(), Value::String("done".to_string()));
        task_table.insert("t2".to_string(), rec2);

        store.tables.insert("Task".to_string(), task_table);
        store
    }

    // ── try_variable ──

    #[test]
    fn variable_lookup_number() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "42".to_string());
        let result = try_variable("x", &vars);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 42.0));
    }

    #[test]
    fn variable_lookup_bool() {
        let mut vars = HashMap::new();
        vars.insert("flag".to_string(), "true".to_string());
        let result = try_variable("flag", &vars);
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn variable_lookup_string() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        let result = try_variable("name", &vars);
        assert!(matches!(result, Some(EvalResult::String { value }) if value == "Alice"));
    }

    #[test]
    fn variable_lookup_missing() {
        let result = try_variable("missing", &empty_vars());
        assert!(result.is_none());
    }

    // ── try_field_access ──

    #[test]
    fn field_access_numeric() {
        let store = store_with_records();
        let result = try_field_access("Task:t1.points", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 5.0));
    }

    #[test]
    fn field_access_string() {
        let store = store_with_records();
        let result = try_field_access("Task:t1.title", &store);
        assert!(matches!(result, Some(EvalResult::String { value }) if value == "Build API"));
    }

    #[test]
    fn field_access_missing_entity() {
        let store = store_with_records();
        let result = try_field_access("Bug:b1.title", &store);
        assert!(result.is_none());
    }

    #[test]
    fn field_access_missing_record() {
        let store = store_with_records();
        let result = try_field_access("Task:nonexistent.title", &store);
        assert!(result.is_none());
    }

    #[test]
    fn field_access_missing_field() {
        let store = store_with_records();
        let result = try_field_access("Task:t1.nonexistent", &store);
        assert!(result.is_none());
    }

    #[test]
    fn field_access_no_dot() {
        let store = store_with_records();
        let result = try_field_access("Task:t1", &store);
        assert!(result.is_none());
    }

    // ── try_aggregation ──

    #[test]
    fn count_entity() {
        let store = store_with_records();
        let result = try_aggregation("count Task", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 2.0));
    }

    #[test]
    fn count_with_filter() {
        let store = store_with_records();
        let result = try_aggregation("count Task where status=open", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 1.0));
    }

    #[test]
    fn sum_field() {
        let store = store_with_records();
        let result = try_aggregation("sum Task.points", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 8.0));
    }

    #[test]
    fn avg_field() {
        let store = store_with_records();
        let result = try_aggregation("avg Task.points", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 4.0));
    }

    #[test]
    fn min_field() {
        let store = store_with_records();
        let result = try_aggregation("min Task.points", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 3.0));
    }

    #[test]
    fn max_field() {
        let store = store_with_records();
        let result = try_aggregation("max Task.points", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 5.0));
    }

    #[test]
    fn count_empty_entity() {
        let store = empty_store();
        let result = try_aggregation("count Task", &store);
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 0.0));
    }

    #[test]
    fn count_group_by() {
        let store = store_with_records();
        let result = try_aggregation("count Task group_by status", &store);
        assert!(matches!(result, Some(EvalResult::Map { .. })));
    }

    // ── try_time_expr ──

    #[test]
    fn time_today() {
        let result = try_time_expr("today");
        assert!(matches!(result, Some(EvalResult::String { .. })));
    }

    #[test]
    fn time_now() {
        let result = try_time_expr("now");
        assert!(matches!(result, Some(EvalResult::String { .. })));
    }

    #[test]
    fn time_offset() {
        let result = try_time_expr("today + 3d");
        assert!(matches!(result, Some(EvalResult::String { .. })));
    }

    #[test]
    fn time_days_between() {
        let result = try_time_expr("days_between(a, b)");
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 0.0));
    }

    #[test]
    fn time_unrelated() {
        let result = try_time_expr("not a time expr");
        assert!(result.is_none());
    }

    // ── try_conversion ──

    #[test]
    fn convert_km_to_mi() {
        let result = try_conversion("convert 10 km to mi");
        if let Some(EvalResult::Number { value }) = result {
            assert!((value - 6.21371).abs() < 0.01);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn convert_unknown_units() {
        let result = try_conversion("convert 10 zorbs to blips");
        assert!(result.is_none());
    }

    #[test]
    fn convert_not_convert() {
        let result = try_conversion("10 km to mi");
        assert!(result.is_none());
    }

    // ── try_comparison ──

    #[test]
    fn comparison_greater_than() {
        let result = try_comparison("5 > 3", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn comparison_less_than() {
        let result = try_comparison("2 < 10", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn comparison_equal_numbers() {
        let result = try_comparison("5 == 5", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn comparison_not_equal() {
        let result = try_comparison("3 != 5", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn comparison_strings() {
        let mut vars = HashMap::new();
        vars.insert("status".to_string(), "open".to_string());
        let result = try_comparison("status == open", &empty_store(), &vars);
        assert!(matches!(result, Some(EvalResult::Bool { value: true })));
    }

    #[test]
    fn comparison_no_operator() {
        let result = try_comparison("hello world", &empty_store(), &empty_vars());
        assert!(result.is_none());
    }

    // ── try_math ──

    #[test]
    fn math_addition() {
        let result = try_math("3 + 4", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 7.0));
    }

    #[test]
    fn math_subtraction() {
        let result = try_math("10 - 3", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 7.0));
    }

    #[test]
    fn math_multiplication() {
        let result = try_math("6 * 7", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 42.0));
    }

    #[test]
    fn math_division() {
        let result = try_math("10 / 2", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 5.0));
    }

    #[test]
    fn math_division_by_zero() {
        let result = try_math("10 / 0", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Error { .. })));
    }

    #[test]
    fn math_power() {
        let result = try_math("2 ^ 3", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 8.0));
    }

    #[test]
    fn math_literal() {
        let result = try_math("42", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 42.0));
    }

    // ── try_math_fn ──

    #[test]
    fn math_fn_sqrt() {
        let result = try_math_fn("sqrt(16)", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 4.0));
    }

    #[test]
    fn math_fn_abs() {
        let result = try_math_fn("abs(-5)", &empty_store(), &empty_vars());
        assert!(matches!(result, Some(EvalResult::Number { value }) if value == 5.0));
    }

    // ── eval_compute integration ──

    #[test]
    fn eval_compute_variable() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "10".to_string());
        let result = eval_compute("x", &empty_store(), &vars);
        assert!(matches!(result, EvalResult::Number { value } if value == 10.0));
    }

    #[test]
    fn eval_compute_unknown() {
        let result = eval_compute("@#$unknown_expr", &empty_store(), &empty_vars());
        assert!(matches!(result, EvalResult::Error { .. }));
    }

    // ── filter helpers ──

    #[test]
    fn predicate_eq() {
        let pred = Predicate {
            field: "status".to_string(),
            op: "=".to_string(),
            value: "open".to_string(),
        };
        let mut record = HashMap::new();
        record.insert("status".to_string(), Value::String("open".to_string()));
        assert!(pred.matches(&record));
    }

    #[test]
    fn predicate_neq() {
        let pred = Predicate {
            field: "status".to_string(),
            op: "!=".to_string(),
            value: "done".to_string(),
        };
        let mut record = HashMap::new();
        record.insert("status".to_string(), Value::String("open".to_string()));
        assert!(pred.matches(&record));
    }

    #[test]
    fn predicate_nil() {
        let pred = Predicate {
            field: "assignee".to_string(),
            op: "=".to_string(),
            value: "nil".to_string(),
        };
        let record: Record = HashMap::new();
        assert!(pred.matches(&record));
    }

    #[test]
    fn predicate_numeric_gt() {
        let pred = Predicate {
            field: "points".to_string(),
            op: ">".to_string(),
            value: "3".to_string(),
        };
        let mut record = HashMap::new();
        record.insert("points".to_string(), Value::Number(5.into()));
        assert!(pred.matches(&record));
    }

    #[test]
    fn predicate_fuzzy() {
        let pred = Predicate {
            field: "title".to_string(),
            op: "~=".to_string(),
            value: "build".to_string(),
        };
        let mut record = HashMap::new();
        record.insert("title".to_string(), Value::String("Build API".to_string()));
        assert!(pred.matches(&record));
    }

    #[test]
    fn parse_filter_predicates_basic() {
        let preds = parse_filter_predicates("status=open and points>3");
        assert_eq!(preds.len(), 2);
        assert_eq!(preds[0].field, "status");
        assert_eq!(preds[0].op, "=");
        assert_eq!(preds[0].value, "open");
        assert_eq!(preds[1].field, "points");
        assert_eq!(preds[1].op, ">");
        assert_eq!(preds[1].value, "3");
    }

    // ── levenshtein ──

    #[test]
    fn levenshtein_same() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_one_edit() {
        assert_eq!(levenshtein("cat", "bat"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    // ── convert_units ──

    #[test]
    fn convert_celsius_to_fahrenheit() {
        let result = convert_units(100.0, "c", "f").unwrap();
        assert!((result - 212.0).abs() < 0.01);
    }

    #[test]
    fn convert_hours_to_minutes() {
        let result = convert_units(2.0, "hours", "minutes").unwrap();
        assert!((result - 120.0).abs() < 0.01);
    }
}
