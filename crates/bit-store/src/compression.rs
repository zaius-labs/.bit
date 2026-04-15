use serde_json::{json, Value};
use std::collections::HashMap;

/// Options for template compression.
#[derive(Debug, Clone)]
pub struct CompressionOptions {
    /// Minimum number of similar entities to trigger compression
    pub min_group_size: usize,
    /// Minimum field overlap ratio to consider entities "similar" (0.0-1.0)
    pub similarity_threshold: f64,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            min_group_size: 3,
            similarity_threshold: 0.5,
        }
    }
}

/// Result of compression.
#[derive(Debug)]
pub struct CompressionResult {
    /// Summary entities (one per group of similar entities)
    pub summaries: Vec<Value>,
    /// Number of original entities compressed
    pub original_count: usize,
    /// Number of summary entities produced
    pub summary_count: usize,
    /// Entities that weren't similar enough to compress
    pub uncompressed: Vec<Value>,
}

/// Compress a list of entity records into summaries.
/// Groups similar entities and produces a summary for each group.
pub fn compress_entities(
    entity_type: &str,
    records: &[(String, Value)],
    opts: &CompressionOptions,
) -> CompressionResult {
    if records.len() < opts.min_group_size {
        return CompressionResult {
            summaries: vec![],
            original_count: records.len(),
            summary_count: 0,
            uncompressed: records.iter().map(|(_, v)| v.clone()).collect(),
        };
    }

    // Group by field similarity
    let groups = group_by_similarity(records, opts.similarity_threshold);

    let mut summaries = Vec::new();
    let mut uncompressed = Vec::new();

    for group in &groups {
        if group.len() >= opts.min_group_size {
            let summary = build_summary(entity_type, group);
            summaries.push(summary);
        } else {
            for (_, val) in group {
                uncompressed.push(val.clone());
            }
        }
    }

    CompressionResult {
        original_count: records.len(),
        summary_count: summaries.len(),
        summaries,
        uncompressed,
    }
}

/// Group records by field value similarity.
fn group_by_similarity(records: &[(String, Value)], threshold: f64) -> Vec<Vec<&(String, Value)>> {
    let mut groups: Vec<Vec<&(String, Value)>> = Vec::new();
    let mut assigned = vec![false; records.len()];

    for i in 0..records.len() {
        if assigned[i] {
            continue;
        }

        let mut group = vec![&records[i]];
        assigned[i] = true;

        for j in (i + 1)..records.len() {
            if assigned[j] {
                continue;
            }

            let similarity = compute_similarity(&records[i].1, &records[j].1);
            if similarity >= threshold {
                group.push(&records[j]);
                assigned[j] = true;
            }
        }

        groups.push(group);
    }

    groups
}

/// Compute similarity between two records (Jaccard on field values).
fn compute_similarity(a: &Value, b: &Value) -> f64 {
    let (Some(a_obj), Some(b_obj)) = (a.as_object(), b.as_object()) else {
        return 0.0;
    };

    let a_fields: HashMap<&String, &Value> =
        a_obj.iter().filter(|(k, _)| !k.starts_with('_')).collect();
    let b_fields: HashMap<&String, &Value> =
        b_obj.iter().filter(|(k, _)| !k.starts_with('_')).collect();

    let all_keys: std::collections::HashSet<&String> =
        a_fields.keys().chain(b_fields.keys()).copied().collect();
    if all_keys.is_empty() {
        return 1.0;
    }

    let matching = all_keys
        .iter()
        .filter(|k| a_fields.get(*k) == b_fields.get(*k))
        .count();

    matching as f64 / all_keys.len() as f64
}

/// Build a summary from a group of similar entities.
fn build_summary(entity_type: &str, group: &[&(String, Value)]) -> Value {
    let ids: Vec<&str> = group.iter().map(|(id, _)| id.as_str()).collect();

    // Collect all field values across the group
    let mut field_values: HashMap<String, Vec<Value>> = HashMap::new();
    for (_, record) in group {
        if let Some(obj) = record.as_object() {
            for (k, v) in obj {
                if k.starts_with('_') {
                    continue;
                }
                field_values.entry(k.clone()).or_default().push(v.clone());
            }
        }
    }

    // For each field: if all values are the same, use that value.
    // If values differ, show most common with count.
    let mut summary_fields = serde_json::Map::new();
    for (field, values) in &field_values {
        if values.iter().all(|v| v == &values[0]) {
            summary_fields.insert(field.clone(), values[0].clone());
        } else {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for v in values {
                *counts.entry(v.to_string()).or_default() += 1;
            }
            let most_common = counts.iter().max_by_key(|(_, c)| *c).unwrap();
            summary_fields.insert(
                field.clone(),
                json!(format!(
                    "{} ({}x of {})",
                    most_common.0.trim_matches('"'),
                    most_common.1,
                    values.len()
                )),
            );
        }
    }

    json!({
        "_type": "summary",
        "_entity": entity_type,
        "_count": group.len(),
        "_ids": ids,
        "fields": summary_fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_records(items: Vec<(&str, Value)>) -> Vec<(String, Value)> {
        items
            .into_iter()
            .map(|(id, v)| (id.to_string(), v))
            .collect()
    }

    #[test]
    fn identical_entities_compressed() {
        let records = make_records(vec![
            ("t1", json!({"status": "open", "priority": "high"})),
            ("t2", json!({"status": "open", "priority": "high"})),
            ("t3", json!({"status": "open", "priority": "high"})),
            ("t4", json!({"status": "open", "priority": "high"})),
            ("t5", json!({"status": "open", "priority": "high"})),
        ]);

        let result = compress_entities("Task", &records, &CompressionOptions::default());
        assert_eq!(result.summary_count, 1);
        assert_eq!(result.original_count, 5);
        assert!(result.uncompressed.is_empty());

        let summary = &result.summaries[0];
        assert_eq!(summary["_count"], 5);
        assert_eq!(summary["_entity"], "Task");
        // Shared fields should appear as-is
        assert_eq!(summary["fields"]["status"], "open");
        assert_eq!(summary["fields"]["priority"], "high");
    }

    #[test]
    fn mixed_similar_and_different() {
        let records = make_records(vec![
            ("t1", json!({"status": "open", "priority": "high"})),
            ("t2", json!({"status": "open", "priority": "high"})),
            ("t3", json!({"status": "open", "priority": "high"})),
            ("t4", json!({"status": "closed", "category": "other"})),
            ("t5", json!({"status": "archived", "category": "misc"})),
        ]);

        let result = compress_entities("Task", &records, &CompressionOptions::default());
        assert_eq!(result.summary_count, 1);
        assert_eq!(result.uncompressed.len(), 2);
    }

    #[test]
    fn below_threshold_no_compression() {
        let records = make_records(vec![
            ("t1", json!({"status": "open"})),
            ("t2", json!({"status": "closed"})),
        ]);

        let result = compress_entities("Task", &records, &CompressionOptions::default());
        assert_eq!(result.summary_count, 0);
        assert_eq!(result.uncompressed.len(), 2);
    }

    #[test]
    fn summary_has_correct_count_and_ids() {
        let records = make_records(vec![
            ("a1", json!({"x": 1, "y": 2})),
            ("a2", json!({"x": 1, "y": 2})),
            ("a3", json!({"x": 1, "y": 2})),
        ]);

        let result = compress_entities("Item", &records, &CompressionOptions::default());
        assert_eq!(result.summary_count, 1);
        let summary = &result.summaries[0];
        assert_eq!(summary["_count"], 3);
        let ids = summary["_ids"].as_array().unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&json!("a1")));
        assert!(ids.contains(&json!("a2")));
        assert!(ids.contains(&json!("a3")));
    }

    #[test]
    fn different_field_values_show_frequency() {
        let records = make_records(vec![
            ("t1", json!({"status": "open", "priority": "high"})),
            ("t2", json!({"status": "open", "priority": "low"})),
            ("t3", json!({"status": "open", "priority": "high"})),
        ]);

        let opts = CompressionOptions {
            min_group_size: 3,
            similarity_threshold: 0.4,
        };
        let result = compress_entities("Task", &records, &opts);
        assert_eq!(result.summary_count, 1);
        let summary = &result.summaries[0];
        // status is shared
        assert_eq!(summary["fields"]["status"], "open");
        // priority differs — should contain frequency info
        let pri = summary["fields"]["priority"].as_str().unwrap();
        assert!(
            pri.contains("2x of 3"),
            "expected frequency info, got: {}",
            pri
        );
    }

    #[test]
    fn empty_input() {
        let records: Vec<(String, Value)> = vec![];
        let result = compress_entities("Task", &records, &CompressionOptions::default());
        assert_eq!(result.summary_count, 0);
        assert_eq!(result.original_count, 0);
        assert!(result.summaries.is_empty());
        assert!(result.uncompressed.is_empty());
    }
}
