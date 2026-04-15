// scoring.rs — Composite relevance scorer (Generative Agents formula)
//
// score = alpha * recency + beta * importance + gamma * relevance

use serde_json::Value;

/// Configuration for the composite relevance scorer.
#[derive(Debug, Clone)]
pub struct ScoringConfig {
    /// Weight for recency (0.0 - 1.0). Default: 0.4
    pub recency_weight: f64,
    /// Weight for importance (0.0 - 1.0). Default: 0.3
    pub importance_weight: f64,
    /// Weight for relevance (0.0 - 1.0). Default: 0.3
    pub relevance_weight: f64,
    /// Decay rate for recency. Higher = faster decay. Default: 0.1
    pub decay_rate: f64,
    /// Current turn/timestamp for recency calculation
    pub current_turn: f64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            recency_weight: 0.4,
            importance_weight: 0.3,
            relevance_weight: 0.3,
            decay_rate: 0.1,
            current_turn: 0.0,
        }
    }
}

/// Score a single entity record.
pub fn score_entity(record: &Value, config: &ScoringConfig, query_terms: &[&str]) -> f64 {
    let recency = compute_recency(record, config);
    let importance = compute_importance(record);
    let relevance = compute_relevance(record, query_terms);

    config.recency_weight * recency
        + config.importance_weight * importance
        + config.relevance_weight * relevance
}

/// Recency: exponential decay since last update.
/// Uses _turn or turn field. Returns 0.0-1.0.
fn compute_recency(record: &Value, config: &ScoringConfig) -> f64 {
    let entity_turn = record
        .get("_turn")
        .or_else(|| record.get("turn"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let delta = (config.current_turn - entity_turn).max(0.0);
    (-config.decay_rate * delta).exp() // e^(-decay * delta)
}

/// Importance: read from _importance field. Normalized to 0.0-1.0 (assumes 1-10 scale).
fn compute_importance(record: &Value) -> f64 {
    let raw = record
        .get("_importance")
        .or_else(|| record.get("importance"))
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);
    (raw.clamp(1.0, 10.0) - 1.0) / 9.0 // normalize 1-10 → 0-1
}

/// Relevance: keyword overlap between record text fields and query terms.
/// Simple TF-based scoring (BM25 in search.rs replaces this for full-text).
fn compute_relevance(record: &Value, query_terms: &[&str]) -> f64 {
    if query_terms.is_empty() {
        return 0.5;
    } // neutral if no query

    let text = collect_text(record);
    let text_lower = text.to_lowercase();

    let mut matches = 0;
    for term in query_terms {
        if text_lower.contains(&term.to_lowercase()) {
            matches += 1;
        }
    }
    matches as f64 / query_terms.len() as f64
}

fn collect_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => obj.values().map(collect_text).collect::<Vec<_>>().join(" "),
        Value::Array(arr) => arr.iter().map(collect_text).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// Score and rank a list of entities. Returns sorted by score descending.
pub fn rank_entities(
    entities: &[(String, String, Value)],
    config: &ScoringConfig,
    query_terms: &[&str],
) -> Vec<(String, String, Value, f64)> {
    let mut scored: Vec<_> = entities
        .iter()
        .map(|(entity, id, val)| {
            let score = score_entity(val, config, query_terms);
            (entity.clone(), id.clone(), val.clone(), score)
        })
        .collect();
    scored.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recent_entity_scores_higher_than_old() {
        let config = ScoringConfig {
            current_turn: 20.0,
            ..Default::default()
        };
        let recent = json!({"_turn": 19.0, "_importance": 5});
        let old = json!({"_turn": 1.0, "_importance": 5});

        let recent_score = score_entity(&recent, &config, &[]);
        let old_score = score_entity(&old, &config, &[]);
        assert!(
            recent_score > old_score,
            "recent {recent_score} should be > old {old_score}"
        );
    }

    #[test]
    fn high_importance_scores_higher() {
        let config = ScoringConfig {
            current_turn: 10.0,
            ..Default::default()
        };
        let high = json!({"_turn": 10.0, "_importance": 10});
        let low = json!({"_turn": 10.0, "_importance": 1});

        let high_score = score_entity(&high, &config, &[]);
        let low_score = score_entity(&low, &config, &[]);
        assert!(
            high_score > low_score,
            "high {high_score} should be > low {low_score}"
        );
    }

    #[test]
    fn matching_query_scores_higher_on_relevance() {
        let config = ScoringConfig {
            current_turn: 10.0,
            ..Default::default()
        };
        let matching =
            json!({"_turn": 10.0, "_importance": 5, "description": "rust compiler error"});
        let non_matching =
            json!({"_turn": 10.0, "_importance": 5, "description": "python web framework"});

        let m_score = score_entity(&matching, &config, &["rust", "compiler"]);
        let nm_score = score_entity(&non_matching, &config, &["rust", "compiler"]);
        assert!(
            m_score > nm_score,
            "matching {m_score} should be > non-matching {nm_score}"
        );
    }

    #[test]
    fn default_config_produces_bounded_scores() {
        let config = ScoringConfig {
            current_turn: 10.0,
            ..Default::default()
        };
        let record = json!({"_turn": 5.0, "_importance": 7, "text": "hello world"});

        let score = score_entity(&record, &config, &["hello"]);
        assert!(score >= 0.0, "score {score} should be >= 0");
        assert!(score <= 1.0, "score {score} should be <= 1");
    }

    #[test]
    fn rank_entities_sorted_descending() {
        let config = ScoringConfig {
            current_turn: 20.0,
            ..Default::default()
        };
        let entities = vec![
            (
                "Task".to_string(),
                "old".to_string(),
                json!({"_turn": 1.0, "_importance": 3}),
            ),
            (
                "Task".to_string(),
                "recent_important".to_string(),
                json!({"_turn": 19.0, "_importance": 10}),
            ),
            (
                "Task".to_string(),
                "mid".to_string(),
                json!({"_turn": 10.0, "_importance": 5}),
            ),
        ];

        let ranked = rank_entities(&entities, &config, &[]);
        assert_eq!(ranked[0].1, "recent_important");
        assert!(ranked[0].3 >= ranked[1].3);
        assert!(ranked[1].3 >= ranked[2].3);
    }
}
