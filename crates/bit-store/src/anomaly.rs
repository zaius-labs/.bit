// anomaly.rs — Statistical anomaly detection for entity records
//
// Pure Rust implementation using z-scores for numeric fields and frequency
// analysis for categorical fields. The `ml` feature flag is reserved for
// future integration with extended-isolation-forest.

use serde_json::Value;
use std::collections::HashMap;

/// An anomaly detection result.
#[derive(Debug, Clone)]
pub struct AnomalyResult {
    pub entity_key: String,
    /// 0.0 = normal, 1.0 = highly anomalous
    pub anomaly_score: f64,
    pub anomalous_fields: Vec<AnomalousField>,
}

#[derive(Debug, Clone)]
pub struct AnomalousField {
    pub field: String,
    pub value: String,
    pub reason: String,
    pub z_score: f64,
}

/// Statistical anomaly detector using z-scores and frequency analysis.
#[derive(Debug, Default)]
pub struct AnomalyDetector {
    /// field -> (mean, std_dev, count) for numeric fields
    numeric_stats: HashMap<String, (f64, f64, usize)>,
    /// field -> value -> count for categorical fields
    categorical_stats: HashMap<String, HashMap<String, usize>>,
    total_records: usize,
}

impl AnomalyDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Train on a set of records (compute statistics).
    pub fn train(&mut self, records: &[Value]) {
        self.total_records = records.len();
        let mut numeric_values: HashMap<String, Vec<f64>> = HashMap::new();

        for record in records {
            if let Some(obj) = record.as_object() {
                for (k, v) in obj {
                    if k.starts_with('_') {
                        continue;
                    }
                    match v {
                        Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                numeric_values.entry(k.clone()).or_default().push(f);
                            }
                        }
                        Value::String(s) => {
                            *self
                                .categorical_stats
                                .entry(k.clone())
                                .or_default()
                                .entry(s.clone())
                                .or_default() += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Compute mean/stddev for numeric fields
        for (field, values) in numeric_values {
            let n = values.len() as f64;
            let mean = values.iter().sum::<f64>() / n;
            let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
            let std_dev = variance.sqrt();
            self.numeric_stats
                .insert(field, (mean, std_dev, values.len()));
        }
    }

    /// Score a single record for anomalies.
    pub fn score(&self, key: &str, record: &Value) -> AnomalyResult {
        let mut anomalous_fields = Vec::new();
        let mut max_score = 0.0f64;

        if let Some(obj) = record.as_object() {
            for (k, v) in obj {
                if k.starts_with('_') {
                    continue;
                }

                // Numeric: z-score
                if let Value::Number(n) = v {
                    if let Some(f) = n.as_f64() {
                        if let Some(&(mean, std_dev, _)) = self.numeric_stats.get(k) {
                            if std_dev > 0.0 {
                                let z = ((f - mean) / std_dev).abs();
                                if z > 2.0 {
                                    let field_score = (z - 2.0) / 3.0;
                                    max_score = max_score.max(field_score.min(1.0));
                                    anomalous_fields.push(AnomalousField {
                                        field: k.clone(),
                                        value: f.to_string(),
                                        reason: format!(
                                            "z-score {:.1} (mean={:.1}, std={:.1})",
                                            z, mean, std_dev
                                        ),
                                        z_score: z,
                                    });
                                }
                            }
                        }
                    }
                }

                // Categorical: rare value
                if let Value::String(s) = v {
                    if let Some(dist) = self.categorical_stats.get(k) {
                        let count = dist.get(s).copied().unwrap_or(0);
                        let total: usize = dist.values().sum();
                        if total > 0 {
                            let frequency = count as f64 / total as f64;
                            if frequency < 0.05 && count <= 2 {
                                let field_score = 1.0 - frequency;
                                max_score = max_score.max(field_score.min(1.0));
                                anomalous_fields.push(AnomalousField {
                                    field: k.clone(),
                                    value: s.clone(),
                                    reason: format!(
                                        "rare value ({} of {} records, {:.1}%)",
                                        count,
                                        total,
                                        frequency * 100.0
                                    ),
                                    z_score: 0.0,
                                });
                            }
                        }
                    }
                }
            }
        }

        AnomalyResult {
            entity_key: key.to_string(),
            anomaly_score: max_score,
            anomalous_fields,
        }
    }

    /// Score all records, return only anomalies (score > threshold).
    pub fn detect_anomalies(
        &self,
        records: &[(String, Value)],
        threshold: f64,
    ) -> Vec<AnomalyResult> {
        records
            .iter()
            .map(|(key, val)| self.score(key, val))
            .filter(|r| r.anomaly_score > threshold)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn training_data() -> Vec<Value> {
        // 20 records with age ~30 (std ~5) and status mostly "active"
        (0..20)
            .map(|i| {
                json!({
                    "age": 25 + (i % 10),
                    "status": if i < 19 { "active" } else { "suspended" },
                    "score": 50.0 + (i as f64) * 2.0,
                })
            })
            .collect()
    }

    #[test]
    fn normal_record_scores_low() {
        let mut det = AnomalyDetector::new();
        det.train(&training_data());

        let result = det.score(
            "test:1",
            &json!({"age": 30, "status": "active", "score": 60.0}),
        );
        assert!(
            result.anomaly_score < 0.1,
            "normal record should score low, got {}",
            result.anomaly_score
        );
    }

    #[test]
    fn numeric_outlier_detected() {
        let mut det = AnomalyDetector::new();
        det.train(&training_data());

        // age=200 is a massive outlier
        let result = det.score("test:2", &json!({"age": 200, "status": "active"}));
        assert!(
            result.anomaly_score > 0.5,
            "numeric outlier should score high, got {}",
            result.anomaly_score
        );
        assert!(result.anomalous_fields.iter().any(|f| f.field == "age"));
    }

    #[test]
    fn rare_categorical_detected() {
        let mut det = AnomalyDetector::new();
        // 50 records all "active", 1 "banned"
        let mut data: Vec<Value> = (0..50).map(|_| json!({"status": "active"})).collect();
        data.push(json!({"status": "banned"}));
        det.train(&data);

        let result = det.score("test:3", &json!({"status": "banned"}));
        assert!(
            result.anomaly_score > 0.5,
            "rare categorical should score high, got {}",
            result.anomaly_score
        );
    }

    #[test]
    fn empty_detector_returns_zero() {
        let det = AnomalyDetector::new();
        let result = det.score("test:4", &json!({"age": 30}));
        assert_eq!(result.anomaly_score, 0.0);
        assert!(result.anomalous_fields.is_empty());
    }

    #[test]
    fn detect_anomalies_filters_by_threshold() {
        let mut det = AnomalyDetector::new();
        det.train(&training_data());

        let records = vec![
            ("normal".to_string(), json!({"age": 30, "status": "active"})),
            (
                "outlier".to_string(),
                json!({"age": 999, "status": "active"}),
            ),
        ];

        let anomalies = det.detect_anomalies(&records, 0.3);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].entity_key, "outlier");
    }
}
