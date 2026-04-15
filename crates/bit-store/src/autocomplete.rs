// autocomplete.rs — Predictive autocomplete from observed entity field values

use serde_json::Value;
use std::collections::HashMap;

/// Autocomplete suggestion for a field value.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub value: String,
    pub confidence: f64,
    pub frequency: usize,
}

/// Per-value frequency count and the turn it was last observed.
type ValueFrequency = HashMap<String, (usize, u64)>;
/// Per-field mapping of value frequencies.
type FieldFrequencies = HashMap<String, ValueFrequency>;

/// Tracks field value frequencies for autocomplete suggestions.
#[derive(Debug, Default)]
pub struct AutocompleteIndex {
    /// entity_type -> field_name -> value -> (count, last_seen_turn)
    frequencies: HashMap<String, FieldFrequencies>,
    current_turn: u64,
    decay_rate: f64,
}

impl AutocompleteIndex {
    pub fn new() -> Self {
        Self {
            decay_rate: 0.05,
            ..Default::default()
        }
    }

    /// Record an entity write (call on every insert/upsert).
    pub fn observe(&mut self, entity_type: &str, record: &Value, turn: u64) {
        self.current_turn = turn;
        if let Some(obj) = record.as_object() {
            for (field, value) in obj {
                if field.starts_with('_') {
                    continue;
                }
                let val_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                let entry = self
                    .frequencies
                    .entry(entity_type.to_string())
                    .or_default()
                    .entry(field.clone())
                    .or_default()
                    .entry(val_str)
                    .or_default();
                entry.0 += 1;
                entry.1 = turn;
            }
        }
    }

    /// Get top-N suggestions for a field.
    pub fn suggest(&self, entity_type: &str, field: &str, limit: usize) -> Vec<Suggestion> {
        let Some(fields) = self.frequencies.get(entity_type) else {
            return vec![];
        };
        let Some(values) = fields.get(field) else {
            return vec![];
        };

        let total: usize = values.values().map(|(c, _)| c).sum();
        if total == 0 {
            return vec![];
        }

        let mut scored: Vec<_> = values
            .iter()
            .map(|(val, (count, last_turn))| {
                let frequency_score = *count as f64 / total as f64;
                let recency_score = (-self.decay_rate
                    * (self.current_turn.saturating_sub(*last_turn)) as f64)
                    .exp();
                let confidence = 0.7 * frequency_score + 0.3 * recency_score;
                Suggestion {
                    value: val.clone(),
                    confidence,
                    frequency: *count,
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        scored
    }

    /// Build index from existing store data.
    pub fn build_from_records(entity_type: &str, records: &[(String, Value)]) -> Self {
        let mut index = Self::new();
        for (i, (_, record)) in records.iter().enumerate() {
            index.observe(entity_type, record, i as u64);
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn suggest_most_common_value() {
        let mut idx = AutocompleteIndex::new();
        for i in 0..10 {
            let role = if i < 7 { "admin" } else { "viewer" };
            idx.observe("User", &json!({"role": role}), i);
        }
        let suggestions = idx.suggest("User", "role", 5);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].value, "admin");
        assert!(suggestions[0].confidence > suggestions[1].confidence);
    }

    #[test]
    fn recency_boosts_recent_value() {
        let mut idx = AutocompleteIndex::new();
        // "old_val" seen many times early
        for i in 0..5 {
            idx.observe("Task", &json!({"status": "old_val"}), i);
        }
        // "new_val" seen fewer times but very recently
        for i in 100..104 {
            idx.observe("Task", &json!({"status": "new_val"}), i);
        }
        let suggestions = idx.suggest("Task", "status", 5);
        assert!(suggestions.len() >= 2);
        // new_val should have higher confidence due to recency despite fewer counts
        let new_val = suggestions.iter().find(|s| s.value == "new_val").unwrap();
        let old_val = suggestions.iter().find(|s| s.value == "old_val").unwrap();
        assert!(
            new_val.confidence > old_val.confidence,
            "new_val ({:.4}) should beat old_val ({:.4})",
            new_val.confidence,
            old_val.confidence
        );
    }

    #[test]
    fn suggest_unknown_entity_empty() {
        let idx = AutocompleteIndex::new();
        let suggestions = idx.suggest("Unknown", "field", 5);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_unknown_field_empty() {
        let mut idx = AutocompleteIndex::new();
        idx.observe("User", &json!({"name": "alice"}), 0);
        let suggestions = idx.suggest("User", "nonexistent", 5);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn build_from_records_populates() {
        let records: Vec<(String, Value)> = vec![
            ("1".into(), json!({"role": "admin", "name": "alice"})),
            ("2".into(), json!({"role": "admin", "name": "bob"})),
            ("3".into(), json!({"role": "viewer", "name": "carol"})),
        ];
        let idx = AutocompleteIndex::build_from_records("User", &records);
        let suggestions = idx.suggest("User", "role", 5);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].value, "admin");
        assert_eq!(suggestions[0].frequency, 2);
    }
}
