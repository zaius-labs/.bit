// drift.rs — Drift detection for entity schemas and value distributions

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// A detected drift event.
#[derive(Debug, Clone)]
pub struct DriftAlert {
    pub drift_type: DriftType,
    pub entity_type: String,
    pub description: String,
    pub severity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriftType {
    NewField,
    MissingField,
    TypeChange,
    DistributionShift,
    CardinalityChange,
    NewEntityType,
}

/// Baseline profile for drift detection.
#[derive(Debug, Clone, Default)]
pub struct DriftBaseline {
    pub profiles: HashMap<String, EntityProfile>,
}

#[derive(Debug, Clone, Default)]
pub struct EntityProfile {
    pub field_names: HashSet<String>,
    pub field_types: HashMap<String, String>,
    pub value_distributions: HashMap<String, HashMap<String, f64>>,
    pub record_count: usize,
}

impl DriftBaseline {
    /// Build a baseline from current store data.
    pub fn build(entity_type: &str, records: &[Value]) -> Self {
        let mut baseline = Self::default();
        let mut profile = EntityProfile {
            record_count: records.len(),
            ..Default::default()
        };

        let mut field_values: HashMap<String, Vec<String>> = HashMap::new();
        let mut field_types: HashMap<String, HashMap<String, usize>> = HashMap::new();

        for record in records {
            if let Some(obj) = record.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    profile.field_names.insert(k.clone());

                    let type_name = value_type_name(v);
                    *field_types
                        .entry(k.clone())
                        .or_default()
                        .entry(type_name)
                        .or_default() += 1;

                    let val_str = v.to_string();
                    field_values.entry(k.clone()).or_default().push(val_str);
                }
            }
        }

        for (field, types) in &field_types {
            if let Some((dominant, _)) = types.iter().max_by_key(|(_, c)| *c) {
                profile.field_types.insert(field.clone(), dominant.clone());
            }
        }

        for (field, values) in &field_values {
            let total = values.len() as f64;
            let mut dist: HashMap<String, f64> = HashMap::new();
            for v in values {
                *dist.entry(v.clone()).or_default() += 1.0 / total;
            }
            profile.value_distributions.insert(field.clone(), dist);
        }

        baseline.profiles.insert(entity_type.to_string(), profile);
        baseline
    }

    /// Compare new records against the baseline. Returns drift alerts.
    pub fn detect(&self, entity_type: &str, new_records: &[Value]) -> Vec<DriftAlert> {
        let mut alerts = Vec::new();

        let Some(baseline_profile) = self.profiles.get(entity_type) else {
            alerts.push(DriftAlert {
                drift_type: DriftType::NewEntityType,
                entity_type: entity_type.to_string(),
                description: format!("New entity type @{} not in baseline", entity_type),
                severity: 0.5,
            });
            return alerts;
        };

        let mut new_fields: HashSet<String> = HashSet::new();
        let mut new_field_types: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut new_field_values: HashMap<String, Vec<String>> = HashMap::new();

        for record in new_records {
            if let Some(obj) = record.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    new_fields.insert(k.clone());
                    *new_field_types
                        .entry(k.clone())
                        .or_default()
                        .entry(value_type_name(v))
                        .or_default() += 1;
                    new_field_values
                        .entry(k.clone())
                        .or_default()
                        .push(v.to_string());
                }
            }
        }

        // New fields
        for field in &new_fields {
            if !baseline_profile.field_names.contains(field) {
                alerts.push(DriftAlert {
                    drift_type: DriftType::NewField,
                    entity_type: entity_type.to_string(),
                    description: format!("New field '{}' on @{}", field, entity_type),
                    severity: 0.3,
                });
            }
        }

        // Missing fields
        for field in &baseline_profile.field_names {
            if !new_fields.contains(field) {
                alerts.push(DriftAlert {
                    drift_type: DriftType::MissingField,
                    entity_type: entity_type.to_string(),
                    description: format!("Field '{}' missing from @{}", field, entity_type),
                    severity: 0.4,
                });
            }
        }

        // Type changes
        for (field, types) in &new_field_types {
            if let Some((new_dominant, _)) = types.iter().max_by_key(|(_, c)| *c) {
                if let Some(old_type) = baseline_profile.field_types.get(field) {
                    if new_dominant != old_type {
                        alerts.push(DriftAlert {
                            drift_type: DriftType::TypeChange,
                            entity_type: entity_type.to_string(),
                            description: format!(
                                "Field '{}' type changed: {} -> {}",
                                field, old_type, new_dominant
                            ),
                            severity: 0.7,
                        });
                    }
                }
            }
        }

        // Distribution shift (PSI)
        for (field, new_values) in &new_field_values {
            if let Some(baseline_dist) = baseline_profile.value_distributions.get(field) {
                let psi = compute_psi(baseline_dist, new_values);
                if psi > 0.2 {
                    alerts.push(DriftAlert {
                        drift_type: DriftType::DistributionShift,
                        entity_type: entity_type.to_string(),
                        description: format!("Distribution shift on '{}' (PSI={:.2})", field, psi),
                        severity: (psi / 0.5).min(1.0),
                    });
                }
            }
        }

        alerts
    }
}

fn value_type_name(v: &Value) -> String {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() && n.as_f64().is_some_and(|f| f.fract() != 0.0) => "float",
        Value::Number(_) => "int",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "object",
    }
    .to_string()
}

fn compute_psi(baseline: &HashMap<String, f64>, new_values: &[String]) -> f64 {
    let n = new_values.len() as f64;
    if n == 0.0 {
        return 0.0;
    }

    let mut new_dist: HashMap<String, f64> = HashMap::new();
    for v in new_values {
        *new_dist.entry(v.clone()).or_default() += 1.0 / n;
    }

    let mut psi = 0.0;
    let all_keys: HashSet<&String> = baseline.keys().chain(new_dist.keys()).collect();
    for key in all_keys {
        let p = baseline.get(key).copied().unwrap_or(0.001);
        let q = new_dist.get(key).copied().unwrap_or(0.001);
        psi += (p - q) * (p / q).ln();
    }
    psi.abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detect_new_field() {
        let baseline_records = vec![json!({"name": "alice", "age": 30})];
        let baseline = DriftBaseline::build("User", &baseline_records);

        let new_records = vec![json!({"name": "bob", "age": 25, "email": "bob@x.com"})];
        let alerts = baseline.detect("User", &new_records);
        assert!(alerts
            .iter()
            .any(|a| a.drift_type == DriftType::NewField && a.description.contains("email")));
    }

    #[test]
    fn detect_missing_field() {
        let baseline_records = vec![json!({"name": "alice", "age": 30, "email": "a@x.com"})];
        let baseline = DriftBaseline::build("User", &baseline_records);

        let new_records = vec![json!({"name": "bob", "age": 25})];
        let alerts = baseline.detect("User", &new_records);
        assert!(alerts
            .iter()
            .any(|a| a.drift_type == DriftType::MissingField && a.description.contains("email")));
    }

    #[test]
    fn detect_type_change() {
        let baseline_records = vec![
            json!({"name": "alice", "age": 30}),
            json!({"name": "bob", "age": 25}),
        ];
        let baseline = DriftBaseline::build("User", &baseline_records);

        // age changed from int to string
        let new_records = vec![
            json!({"name": "carol", "age": "thirty"}),
            json!({"name": "dave", "age": "twenty"}),
        ];
        let alerts = baseline.detect("User", &new_records);
        assert!(alerts
            .iter()
            .any(|a| a.drift_type == DriftType::TypeChange && a.description.contains("age")));
    }

    #[test]
    fn detect_distribution_shift() {
        // Baseline: all "admin"
        let baseline_records: Vec<Value> = (0..10).map(|_| json!({"role": "admin"})).collect();
        let baseline = DriftBaseline::build("User", &baseline_records);

        // New: all "viewer" — complete distribution shift
        let new_records: Vec<Value> = (0..10).map(|_| json!({"role": "viewer"})).collect();
        let alerts = baseline.detect("User", &new_records);
        assert!(
            alerts
                .iter()
                .any(|a| a.drift_type == DriftType::DistributionShift),
            "Expected distribution shift, got: {:?}",
            alerts
        );
    }

    #[test]
    fn detect_new_entity_type() {
        let baseline_records = vec![json!({"name": "alice"})];
        let baseline = DriftBaseline::build("User", &baseline_records);

        let new_records = vec![json!({"title": "task1"})];
        let alerts = baseline.detect("Task", &new_records);
        assert!(alerts
            .iter()
            .any(|a| a.drift_type == DriftType::NewEntityType));
    }

    #[test]
    fn no_drift_on_identical_data() {
        let records = vec![
            json!({"name": "alice", "age": 30}),
            json!({"name": "bob", "age": 25}),
        ];
        let baseline = DriftBaseline::build("User", &records);
        let alerts = baseline.detect("User", &records);
        // Should have no alerts (no new/missing fields, no type changes, no distribution shift)
        // PSI on identical data should be 0, so no distribution shift alerts
        assert!(
            alerts.is_empty(),
            "Expected no drift on identical data, got: {:?}",
            alerts
        );
    }
}
