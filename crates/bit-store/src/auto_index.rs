// auto_index.rs — Self-organizing indexes via access pattern tracking
//
// Zero-dependency index advisor. Tracks which fields are filtered on across
// queries and recommends creating or dropping secondary indexes based on
// observed access frequency.

use std::collections::HashMap;

/// Tracks field access patterns for auto-indexing.
#[derive(Debug, Default)]
pub struct IndexAdvisor {
    /// field_name -> access count in current window
    filter_counts: HashMap<String, usize>,
    /// Total queries in current window
    total_queries: usize,
    /// Threshold: create index when filter_count/total_queries > this
    create_threshold: f64,
    /// Maximum secondary indexes to maintain
    max_indexes: usize,
    /// Currently active secondary indexes
    active_indexes: Vec<String>,
}

/// Recommendation from the index advisor.
#[derive(Debug, Clone)]
pub struct IndexRecommendation {
    pub action: IndexAction,
    pub field: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexAction {
    Create,
    Drop,
    Keep,
}

impl IndexAdvisor {
    pub fn new() -> Self {
        Self {
            create_threshold: 0.1, // field filtered in >10% of queries
            max_indexes: 8,
            ..Default::default()
        }
    }

    /// Record that a query filtered on these fields.
    pub fn observe_query(&mut self, filtered_fields: &[String]) {
        self.total_queries += 1;
        for field in filtered_fields {
            *self.filter_counts.entry(field.clone()).or_default() += 1;
        }
    }

    /// Get index recommendations based on observed query patterns.
    pub fn recommend(&self) -> Vec<IndexRecommendation> {
        if self.total_queries < 10 {
            return vec![]; // need minimum sample
        }

        let mut recommendations = Vec::new();

        // Sort fields by filter frequency
        let mut field_scores: Vec<_> = self
            .filter_counts
            .iter()
            .map(|(f, c)| (f.clone(), *c as f64 / self.total_queries as f64))
            .collect();
        field_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Recommend creation for hot fields
        for (field, ratio) in &field_scores {
            if *ratio > self.create_threshold && !self.active_indexes.contains(field) {
                let create_count = recommendations
                    .iter()
                    .filter(|r: &&IndexRecommendation| r.action == IndexAction::Create)
                    .count();
                if self.active_indexes.len() + create_count < self.max_indexes {
                    recommendations.push(IndexRecommendation {
                        action: IndexAction::Create,
                        field: field.clone(),
                        reason: format!("Filtered in {:.0}% of queries", ratio * 100.0),
                    });
                }
            }
        }

        // Recommend dropping cold indexes
        for active in &self.active_indexes {
            let ratio = self.filter_counts.get(active).copied().unwrap_or(0) as f64
                / self.total_queries as f64;
            if ratio < 0.01 {
                recommendations.push(IndexRecommendation {
                    action: IndexAction::Drop,
                    field: active.clone(),
                    reason: format!("Only {:.1}% query usage", ratio * 100.0),
                });
            }
        }

        recommendations
    }

    /// Mark an index as created.
    pub fn mark_created(&mut self, field: &str) {
        if !self.active_indexes.contains(&field.to_string()) {
            self.active_indexes.push(field.to_string());
        }
    }

    /// Mark an index as dropped.
    pub fn mark_dropped(&mut self, field: &str) {
        self.active_indexes.retain(|f| f != field);
    }

    /// Reset counters (call periodically to adapt to changing workloads).
    pub fn reset_window(&mut self) {
        self.filter_counts.clear();
        self.total_queries = 0;
    }

    /// Get current active indexes.
    pub fn active_indexes(&self) -> &[String] {
        &self.active_indexes
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_create_for_hot_field() {
        let mut advisor = IndexAdvisor::new();
        // 100 queries, 50 filter on "role"
        for i in 0..100 {
            if i < 50 {
                advisor.observe_query(&["role".to_string()]);
            } else {
                advisor.observe_query(&[]);
            }
        }
        let recs = advisor.recommend();
        assert!(recs
            .iter()
            .any(|r| r.field == "role" && r.action == IndexAction::Create));
    }

    #[test]
    fn no_recommendation_below_minimum_queries() {
        let mut advisor = IndexAdvisor::new();
        for _ in 0..5 {
            advisor.observe_query(&["role".to_string()]);
        }
        let recs = advisor.recommend();
        assert!(recs.is_empty());
    }

    #[test]
    fn recommend_drop_unused_active_index() {
        let mut advisor = IndexAdvisor::new();
        advisor.mark_created("old_field");
        // 100 queries, none filter on old_field
        for _ in 0..100 {
            advisor.observe_query(&["name".to_string()]);
        }
        let recs = advisor.recommend();
        assert!(recs
            .iter()
            .any(|r| r.field == "old_field" && r.action == IndexAction::Drop));
    }

    #[test]
    fn max_indexes_cap_respected() {
        let mut advisor = IndexAdvisor::new();
        // Fill up to max (8)
        for i in 0..8 {
            advisor.mark_created(&format!("field_{}", i));
        }
        // Observe queries on a new field
        for _ in 0..100 {
            advisor.observe_query(&["new_field".to_string()]);
        }
        let recs = advisor.recommend();
        // Should not recommend creating because we're at max
        assert!(!recs
            .iter()
            .any(|r| r.field == "new_field" && r.action == IndexAction::Create));
    }

    #[test]
    fn reset_window_clears_counts() {
        let mut advisor = IndexAdvisor::new();
        for _ in 0..50 {
            advisor.observe_query(&["role".to_string()]);
        }
        advisor.reset_window();
        assert_eq!(advisor.total_queries, 0);
        // After reset, not enough queries for recommendations
        let recs = advisor.recommend();
        assert!(recs.is_empty());
    }
}
