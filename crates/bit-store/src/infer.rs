// infer.rs — Schema inference from observed entity data (Baazizi lattice-merge)

use serde_json::Value;
use std::collections::HashMap;

/// An inferred field type.
#[derive(Debug, Clone, PartialEq)]
pub enum InferredType {
    Bool,
    Int,
    Float,
    Timestamp,
    Enum(Vec<String>),
    String,
    List,
    Object,
    Null,
    Mixed(Vec<InferredType>),
}

/// An inferred field definition.
#[derive(Debug, Clone)]
pub struct InferredField {
    pub name: String,
    pub field_type: InferredType,
    pub required: bool,
    pub nullable: bool,
    pub occurrence_rate: f64,
    pub distinct_count: usize,
    pub sample_values: Vec<String>,
}

/// A complete inferred schema for an entity type.
#[derive(Debug, Clone)]
pub struct InferredSchema {
    pub entity_name: String,
    pub field_count: usize,
    pub record_count: usize,
    pub fields: Vec<InferredField>,
}

/// Infer a schema from a collection of entity records.
pub fn infer_schema(entity_name: &str, records: &[Value]) -> InferredSchema {
    if records.is_empty() {
        return InferredSchema {
            entity_name: entity_name.to_string(),
            field_count: 0,
            record_count: 0,
            fields: vec![],
        };
    }

    let mut field_stats: HashMap<String, FieldAccumulator> = HashMap::new();
    let record_count = records.len();

    for record in records {
        if let Some(obj) = record.as_object() {
            for (key, value) in obj {
                if key.starts_with('_') {
                    continue;
                }
                field_stats.entry(key.clone()).or_default().observe(value);
            }
        }
    }

    let mut fields: Vec<InferredField> = field_stats
        .into_iter()
        .map(|(name, acc)| acc.into_field(name, record_count))
        .collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));

    InferredSchema {
        entity_name: entity_name.to_string(),
        field_count: fields.len(),
        record_count,
        fields,
    }
}

#[derive(Default)]
struct FieldAccumulator {
    count: usize,
    null_count: usize,
    type_counts: HashMap<String, usize>,
    distinct_values: HashMap<String, usize>,
}

impl FieldAccumulator {
    fn observe(&mut self, value: &Value) {
        self.count += 1;
        match value {
            Value::Null => {
                self.null_count += 1;
            }
            Value::Bool(_) => {
                *self.type_counts.entry("bool".into()).or_default() += 1;
            }
            Value::Number(n) => {
                if n.is_f64() && n.as_f64().is_some_and(|f| f.fract() != 0.0) {
                    *self.type_counts.entry("float".into()).or_default() += 1;
                } else {
                    *self.type_counts.entry("int".into()).or_default() += 1;
                }
            }
            Value::String(s) => {
                if is_timestamp(s) {
                    *self.type_counts.entry("timestamp".into()).or_default() += 1;
                } else {
                    *self.type_counts.entry("string".into()).or_default() += 1;
                }
                *self.distinct_values.entry(s.clone()).or_default() += 1;
            }
            Value::Array(_) => {
                *self.type_counts.entry("list".into()).or_default() += 1;
            }
            Value::Object(_) => {
                *self.type_counts.entry("object".into()).or_default() += 1;
            }
        }
    }

    fn into_field(self, name: String, total_records: usize) -> InferredField {
        let occurrence_rate = self.count as f64 / total_records as f64;
        let required = occurrence_rate > 0.99;
        let nullable = self.null_count > 0;

        let dominant_type = self
            .type_counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(t, _)| t.as_str())
            .unwrap_or("string");

        let field_type = match dominant_type {
            "bool" => InferredType::Bool,
            "int" => InferredType::Int,
            "float" => InferredType::Float,
            "timestamp" => InferredType::Timestamp,
            "list" => InferredType::List,
            "object" => InferredType::Object,
            _ => {
                let distinct = self.distinct_values.len();
                if distinct > 0 && distinct <= 20 && self.count >= 3 {
                    let mut values: Vec<String> = self.distinct_values.keys().cloned().collect();
                    values.sort();
                    InferredType::Enum(values)
                } else {
                    InferredType::String
                }
            }
        };

        let sample_values: Vec<String> = self.distinct_values.keys().take(5).cloned().collect();

        InferredField {
            name,
            field_type,
            required,
            nullable,
            occurrence_rate,
            distinct_count: self.distinct_values.len(),
            sample_values,
        }
    }
}

fn is_timestamp(s: &str) -> bool {
    if s.len() >= 10 && s.len() <= 30 {
        let bytes = s.as_bytes();
        bytes.len() >= 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[0..4].iter().all(|b| b.is_ascii_digit())
            && bytes[5..7].iter().all(|b| b.is_ascii_digit())
            && bytes[8..10].iter().all(|b| b.is_ascii_digit())
    } else {
        false
    }
}

/// Render an inferred schema as .bit text.
pub fn render_inferred_schema(schema: &InferredSchema) -> String {
    let mut lines = vec![format!("define:@{}", schema.entity_name)];
    for field in &schema.fields {
        match &field.field_type {
            InferredType::Enum(vals) => {
                let enum_str = vals
                    .iter()
                    .map(|v| format!(":{}", v))
                    .collect::<Vec<_>>()
                    .join("/");
                lines.push(format!(
                    "    {}: {}{}",
                    field.name,
                    enum_str,
                    if field.required { "!" } else { "" }
                ));
            }
            _ => {
                let sigil = match &field.field_type {
                    InferredType::Bool => "?",
                    InferredType::Int => "#",
                    InferredType::Float => "##",
                    InferredType::Timestamp => "@",
                    InferredType::String => {
                        if field.required {
                            "!"
                        } else {
                            ""
                        }
                    }
                    InferredType::List => "[]",
                    InferredType::Object => "{}",
                    _ => "",
                };
                let default = match &field.field_type {
                    InferredType::Bool => "false",
                    InferredType::Int => "0",
                    InferredType::Float => "0.0",
                    InferredType::Timestamp | InferredType::String => "\"\"",
                    _ => "\"\"",
                };
                lines.push(format!("    {}: {}{}", field.name, default, sigil));
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infer_mixed_types() {
        let records = vec![
            json!({"name": "alice", "age": 30, "score": 1.5, "active": true, "tags": ["a"]}),
            json!({"name": "bob", "age": 25, "score": 2.0, "active": false, "tags": ["b"]}),
            json!({"name": "carol", "age": 28, "score": 3.7, "active": true, "tags": []}),
            json!({"name": "dave", "age": 22, "score": 0.5, "active": false, "tags": ["c"]}),
            json!({"name": "eve", "age": 35, "score": 4.2, "active": true, "tags": ["d"]}),
        ];
        let schema = infer_schema("User", &records);
        assert_eq!(schema.record_count, 5);
        assert_eq!(schema.field_count, 5);

        let age = schema.fields.iter().find(|f| f.name == "age").unwrap();
        assert_eq!(age.field_type, InferredType::Int);

        let score = schema.fields.iter().find(|f| f.name == "score").unwrap();
        assert_eq!(score.field_type, InferredType::Float);

        let active = schema.fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active.field_type, InferredType::Bool);

        let tags = schema.fields.iter().find(|f| f.name == "tags").unwrap();
        assert_eq!(tags.field_type, InferredType::List);
    }

    #[test]
    fn required_field_detection() {
        let records: Vec<Value> = (0..100)
            .map(|i| json!({"id": i, "name": format!("user_{}", i)}))
            .collect();
        let schema = infer_schema("User", &records);
        let id_field = schema.fields.iter().find(|f| f.name == "id").unwrap();
        assert!(id_field.required);
        assert_eq!(id_field.occurrence_rate, 1.0);
    }

    #[test]
    fn optional_field_detection() {
        let mut records: Vec<Value> = (0..10)
            .map(|i| json!({"id": i, "name": format!("u{}", i)}))
            .collect();
        // Only half have "email"
        for i in 0..5 {
            records[i]
                .as_object_mut()
                .unwrap()
                .insert("email".into(), json!(format!("u{}@x.com", i)));
        }
        let schema = infer_schema("User", &records);
        let email = schema.fields.iter().find(|f| f.name == "email").unwrap();
        assert!(!email.required);
        assert!((email.occurrence_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn enum_detection() {
        let records: Vec<Value> = (0..10)
            .map(|i| {
                let role = match i % 3 {
                    0 => "admin",
                    1 => "editor",
                    _ => "viewer",
                };
                json!({"id": i, "role": role})
            })
            .collect();
        let schema = infer_schema("User", &records);
        let role = schema.fields.iter().find(|f| f.name == "role").unwrap();
        match &role.field_type {
            InferredType::Enum(vals) => {
                assert_eq!(vals.len(), 3);
                assert!(vals.contains(&"admin".to_string()));
                assert!(vals.contains(&"editor".to_string()));
                assert!(vals.contains(&"viewer".to_string()));
            }
            other => panic!("Expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn timestamp_detection() {
        let records = vec![
            json!({"created": "2026-01-15T10:30:00Z"}),
            json!({"created": "2026-02-20T14:00:00Z"}),
            json!({"created": "2026-03-10"}),
        ];
        let schema = infer_schema("Event", &records);
        let created = schema.fields.iter().find(|f| f.name == "created").unwrap();
        assert_eq!(created.field_type, InferredType::Timestamp);
    }

    #[test]
    fn render_produces_valid_bit() {
        let records: Vec<Value> = (0..5)
            .map(|i| {
                json!({"name": format!("u{}", i), "age": 20 + i, "active": true, "created": "2026-01-01"})
            })
            .collect();
        let schema = infer_schema("User", &records);
        let bit = render_inferred_schema(&schema);
        assert!(bit.starts_with("define:@User"));
        assert!(bit.contains("age:"));
        assert!(bit.contains("active:"));
        // Should have one line per field plus the define line
        let lines: Vec<&str> = bit.lines().collect();
        assert_eq!(lines.len(), schema.field_count + 1);
    }
}
