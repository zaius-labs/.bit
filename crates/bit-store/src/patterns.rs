use serde_json::Value;
use std::collections::HashMap;

/// Configuration for pattern detection.
#[derive(Debug, Clone)]
pub struct PatternConfig {
    /// Minimum occurrences before a pattern is flagged
    pub min_occurrences: usize,
    /// Window size (number of recent entities to check)
    pub window_size: usize,
    /// Enable duplicate detection (same field values repeated)
    pub detect_duplicates: bool,
    /// Enable frequency detection (same entity type inserted rapidly)
    pub detect_frequency: bool,
    /// Enable field value clustering (many entities share same field value)
    pub detect_clustering: bool,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            min_occurrences: 3,
            window_size: 50,
            detect_duplicates: true,
            detect_frequency: true,
            detect_clustering: true,
        }
    }
}

/// A detected pattern.
#[derive(Debug, Clone)]
pub struct DetectedPattern {
    /// Pattern type: "duplicate", "frequency", "clustering"
    pub pattern_type: String,
    /// Human-readable description
    pub description: String,
    /// Entity type involved
    pub entity_type: String,
    /// Evidence: the entity keys that triggered this pattern
    pub evidence: Vec<String>,
    /// Confidence: 0.0-1.0
    pub confidence: f64,
}

/// Pattern detector that maintains a sliding window of recent writes.
#[derive(Debug)]
pub struct PatternDetector {
    config: PatternConfig,
    /// Recent writes: (entity_type, id, record) in insertion order
    recent: Vec<(String, String, Value)>,
}

impl PatternDetector {
    pub fn new(config: PatternConfig) -> Self {
        Self {
            config,
            recent: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PatternConfig::default())
    }

    /// Record a write and check for patterns. Returns any newly detected patterns.
    pub fn observe(&mut self, entity_type: &str, id: &str, record: &Value) -> Vec<DetectedPattern> {
        self.recent
            .push((entity_type.to_string(), id.to_string(), record.clone()));

        // Trim window
        if self.recent.len() > self.config.window_size {
            self.recent.remove(0);
        }

        let mut patterns = Vec::new();

        if self.config.detect_duplicates {
            patterns.extend(self.check_duplicates(entity_type, record));
        }
        if self.config.detect_frequency {
            patterns.extend(self.check_frequency(entity_type));
        }
        if self.config.detect_clustering {
            patterns.extend(self.check_clustering(entity_type));
        }

        patterns
    }

    /// Check if the same field values appear repeatedly.
    fn check_duplicates(&self, entity_type: &str, record: &Value) -> Vec<DetectedPattern> {
        let mut matches = Vec::new();

        let current_idx = self.recent.len() - 1;
        for (i, (et, id, rec)) in self.recent.iter().enumerate() {
            if et != entity_type {
                continue;
            }
            if i == current_idx {
                continue;
            }

            let overlap = count_field_overlap(record, rec);
            if overlap >= 2 {
                matches.push(format!("@{}:{}", et, id));
            }
        }

        if matches.len() >= self.config.min_occurrences {
            vec![DetectedPattern {
                pattern_type: "duplicate".to_string(),
                description: format!(
                    "{} similar @{} entities detected in recent writes",
                    matches.len(),
                    entity_type
                ),
                entity_type: entity_type.to_string(),
                evidence: matches,
                confidence: 0.8,
            }]
        } else {
            vec![]
        }
    }

    /// Check if the same entity type is being written very frequently.
    fn check_frequency(&self, entity_type: &str) -> Vec<DetectedPattern> {
        let count = self
            .recent
            .iter()
            .filter(|(et, _, _)| et == entity_type)
            .count();

        let ratio = count as f64 / self.recent.len() as f64;

        if count >= self.config.min_occurrences && ratio > 0.5 {
            vec![DetectedPattern {
                pattern_type: "frequency".to_string(),
                description: format!(
                    "@{} makes up {}% of recent writes ({}/{})",
                    entity_type,
                    (ratio * 100.0) as u32,
                    count,
                    self.recent.len()
                ),
                entity_type: entity_type.to_string(),
                evidence: self
                    .recent
                    .iter()
                    .filter(|(et, _, _)| et == entity_type)
                    .map(|(et, id, _)| format!("@{}:{}", et, id))
                    .collect(),
                confidence: ratio,
            }]
        } else {
            vec![]
        }
    }

    /// Check if many entities share the same field value (clustering).
    fn check_clustering(&self, entity_type: &str) -> Vec<DetectedPattern> {
        let type_entries: Vec<_> = self
            .recent
            .iter()
            .filter(|(et, _, _)| et == entity_type)
            .collect();

        if type_entries.len() < self.config.min_occurrences {
            return vec![];
        }

        // Check each field for value clustering
        let mut field_values: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        for (_, id, rec) in &type_entries {
            if let Some(obj) = rec.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    let v_str = v.to_string();
                    field_values
                        .entry(k.clone())
                        .or_default()
                        .entry(v_str)
                        .or_default()
                        .push(id.clone());
                }
            }
        }

        let mut patterns = Vec::new();
        for (field, values) in &field_values {
            for (value, ids) in values {
                if ids.len() >= self.config.min_occurrences {
                    patterns.push(DetectedPattern {
                        pattern_type: "clustering".to_string(),
                        description: format!(
                            "{} @{} entities have {}={}",
                            ids.len(),
                            entity_type,
                            field,
                            value
                        ),
                        entity_type: entity_type.to_string(),
                        evidence: ids
                            .iter()
                            .map(|id| format!("@{}:{}", entity_type, id))
                            .collect(),
                        confidence: ids.len() as f64 / type_entries.len() as f64,
                    });
                }
            }
        }
        patterns
    }
}

fn count_field_overlap(a: &Value, b: &Value) -> usize {
    let (Some(a_obj), Some(b_obj)) = (a.as_object(), b.as_object()) else {
        return 0;
    };
    a_obj
        .iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .filter(|(k, v)| b_obj.get(*k) == Some(*v))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn duplicate_pattern_detected() {
        let mut det = PatternDetector::with_defaults();
        let record = json!({"status": "open", "priority": "high", "label": "bug"});

        for i in 0..5 {
            let patterns = det.observe("Task", &format!("t{}", i), &record);
            // Should trigger once we have enough similar entries
            if i >= 3 {
                let dups: Vec<_> = patterns
                    .iter()
                    .filter(|p| p.pattern_type == "duplicate")
                    .collect();
                assert!(!dups.is_empty(), "expected duplicate pattern at i={}", i);
            }
        }
    }

    #[test]
    fn frequency_pattern_detected() {
        let mut det = PatternDetector::with_defaults();

        for i in 0..5 {
            let record = json!({"name": format!("item_{}", i)});
            let patterns = det.observe("Event", &format!("e{}", i), &record);
            if i >= 2 {
                let freq: Vec<_> = patterns
                    .iter()
                    .filter(|p| p.pattern_type == "frequency")
                    .collect();
                assert!(!freq.is_empty(), "expected frequency pattern at i={}", i);
            }
        }
    }

    #[test]
    fn clustering_pattern_detected() {
        let mut det = PatternDetector::with_defaults();

        for i in 0..5 {
            let record = json!({"region": "us-east", "unique": format!("val_{}", i)});
            let patterns = det.observe("Server", &format!("s{}", i), &record);
            if i >= 2 {
                let clusters: Vec<_> = patterns
                    .iter()
                    .filter(|p| p.pattern_type == "clustering")
                    .collect();
                assert!(
                    !clusters.is_empty(),
                    "expected clustering pattern at i={}",
                    i
                );
                assert!(clusters[0].description.contains("region"));
            }
        }
    }

    #[test]
    fn below_threshold_no_pattern() {
        let mut det = PatternDetector::with_defaults();
        let record = json!({"status": "open", "priority": "high"});

        // Only 2 inserts — below min_occurrences of 3
        for i in 0..2 {
            let patterns = det.observe("Task", &format!("t{}", i), &record);
            let dups: Vec<_> = patterns
                .iter()
                .filter(|p| p.pattern_type == "duplicate")
                .collect();
            assert!(dups.is_empty(), "no duplicate pattern expected at i={}", i);
        }
    }

    #[test]
    fn mixed_types_only_relevant_flagged() {
        let mut det = PatternDetector::with_defaults();
        let task = json!({"status": "open", "priority": "high", "label": "bug"});
        let event = json!({"kind": "click"});

        // Insert 4 Tasks and 1 Event
        for i in 0..4 {
            det.observe("Task", &format!("t{}", i), &task);
        }
        let patterns = det.observe("Event", "e0", &event);

        // Frequency pattern should NOT fire for Event (only 1 of 5)
        let event_freq: Vec<_> = patterns
            .iter()
            .filter(|p| p.pattern_type == "frequency" && p.entity_type == "Event")
            .collect();
        assert!(event_freq.is_empty());
    }

    #[test]
    fn window_trimming() {
        let config = PatternConfig {
            window_size: 5,
            min_occurrences: 3,
            ..PatternConfig::default()
        };
        let mut det = PatternDetector::new(config);

        // Insert 5 "old" entries of type A
        for i in 0..5 {
            det.observe("TypeA", &format!("a{}", i), &json!({"x": 1, "y": 2}));
        }
        // Now insert 5 entries of type B — pushes TypeA out of window
        for i in 0..5 {
            det.observe("TypeB", &format!("b{}", i), &json!({"z": 3}));
        }

        // Insert one more TypeA — should NOT trigger duplicate (old ones fell out)
        let patterns = det.observe("TypeA", "a99", &json!({"x": 1, "y": 2}));
        let dups: Vec<_> = patterns
            .iter()
            .filter(|p| p.pattern_type == "duplicate" && p.entity_type == "TypeA")
            .collect();
        assert!(
            dups.is_empty(),
            "old TypeA entries should have fallen out of window"
        );
    }
}
