// pipeline.rs — Composable write-path pipeline that chains intelligence features on entity insert

use crate::autocomplete::AutocompleteIndex;
use crate::drift::{DriftAlert, DriftBaseline};
use crate::patterns::{DetectedPattern, PatternDetector};
use crate::store::{BitStore, StoreError};
use serde_json::Value;

/// Events emitted by the intelligence pipeline.
#[derive(Debug)]
pub enum PipelineEvent {
    PatternDetected(DetectedPattern),
    DriftDetected(DriftAlert),
    SchemaEvolution(String),
}

/// The intelligence pipeline — wraps a BitStore with automatic feature chaining.
pub struct IntelligentStore {
    pub store: BitStore,
    pub pattern_detector: PatternDetector,
    pub autocomplete: AutocompleteIndex,
    pub baseline: Option<DriftBaseline>,
    turn: u64,
}

impl IntelligentStore {
    pub fn create(path: &std::path::Path) -> Result<Self, StoreError> {
        Ok(Self {
            store: BitStore::create(path)?,
            pattern_detector: PatternDetector::with_defaults(),
            autocomplete: AutocompleteIndex::new(),
            baseline: None,
            turn: 0,
        })
    }

    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        Ok(Self {
            store: BitStore::open(path)?,
            pattern_detector: PatternDetector::with_defaults(),
            autocomplete: AutocompleteIndex::new(),
            baseline: None,
            turn: 0,
        })
    }

    /// Insert an entity through the intelligence pipeline.
    /// Returns any events triggered by the insert.
    pub fn insert(
        &mut self,
        entity: &str,
        id: &str,
        record: &Value,
    ) -> Result<Vec<PipelineEvent>, StoreError> {
        let mut events = Vec::new();
        self.turn += 1;

        // 1. Insert into store
        self.store.insert_entity(entity, id, record)?;

        // 2. Update autocomplete
        self.autocomplete.observe(entity, record, self.turn);

        // 3. Check patterns
        let patterns = self.pattern_detector.observe(entity, id, record);
        for p in patterns {
            events.push(PipelineEvent::PatternDetected(p));
        }

        // 4. Check drift (if baseline exists)
        if let Some(ref baseline) = self.baseline {
            let alerts = baseline.detect(entity, std::slice::from_ref(record));
            for a in alerts {
                events.push(PipelineEvent::DriftDetected(a));
            }
        }

        Ok(events)
    }

    /// Establish a drift baseline from current data.
    pub fn establish_baseline(&mut self, entity: &str) -> Result<(), StoreError> {
        let records: Vec<Value> = self
            .store
            .list_entities(entity)?
            .into_iter()
            .map(|(_, v)| v)
            .collect();
        self.baseline = Some(DriftBaseline::build(entity, &records));
        Ok(())
    }

    /// Get autocomplete suggestions.
    pub fn suggest(
        &self,
        entity: &str,
        field: &str,
        limit: usize,
    ) -> Vec<crate::autocomplete::Suggestion> {
        self.autocomplete.suggest(entity, field, limit)
    }

    /// Delegate to inner store.
    pub fn flush(&mut self) -> Result<(), StoreError> {
        self.store.flush()
    }

    pub fn get_entity(&mut self, entity: &str, id: &str) -> Result<Option<Value>, StoreError> {
        self.store.get_entity(entity, id)
    }

    pub fn list_entities(&mut self, entity: &str) -> Result<Vec<(String, Value)>, StoreError> {
        self.store.list_entities(entity)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_store() -> IntelligentStore {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bitstore");
        let store = IntelligentStore::create(&path).unwrap();
        // Leak tempdir so it isn't deleted while store is open
        std::mem::forget(dir);
        store
    }

    #[test]
    fn insert_through_pipeline_stores_entity() {
        let mut store = tmp_store();
        let record = json!({"name": "alice", "role": "admin"});
        let events = store.insert("User", "u1", &record).unwrap();
        // First insert should not emit pattern events (below threshold)
        assert!(events.is_empty());

        let got = store.get_entity("User", "u1").unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap()["name"], "alice");
    }

    #[test]
    fn pattern_events_emitted_after_threshold() {
        let mut store = tmp_store();
        let record = json!({"status": "open", "priority": "high", "label": "bug"});

        let mut saw_pattern = false;
        for i in 0..6 {
            let events = store.insert("Task", &format!("t{}", i), &record).unwrap();
            if events
                .iter()
                .any(|e| matches!(e, PipelineEvent::PatternDetected(_)))
            {
                saw_pattern = true;
            }
        }
        assert!(
            saw_pattern,
            "expected pattern events after repeated inserts"
        );
    }

    #[test]
    fn autocomplete_learns_from_pipeline_inserts() {
        let mut store = tmp_store();
        for i in 0..5 {
            let role = if i < 3 { "admin" } else { "viewer" };
            store
                .insert("User", &format!("u{}", i), &json!({"role": role}))
                .unwrap();
        }

        let suggestions = store.suggest("User", "role", 5);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].value, "admin");
    }

    #[test]
    fn drift_detected_when_baseline_exists() {
        let mut store = tmp_store();
        // Insert baseline data
        for i in 0..10 {
            store
                .insert(
                    "User",
                    &format!("u{}", i),
                    &json!({"name": format!("user_{}", i), "age": 25 + i}),
                )
                .unwrap();
        }

        // Establish baseline
        store.establish_baseline("User").unwrap();

        // Insert a record with a new field — should trigger drift
        let events = store
            .insert(
                "User",
                "u_new",
                &json!({"name": "new_user", "age": 30, "new_field": "surprise"}),
            )
            .unwrap();

        let drift_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, PipelineEvent::DriftDetected(_)))
            .collect();
        assert!(
            !drift_events.is_empty(),
            "expected drift alert for new field"
        );
    }

    #[test]
    fn multiple_events_from_single_insert() {
        let mut store = tmp_store();
        let record = json!({"status": "open", "priority": "high", "label": "bug"});

        // Build up patterns first
        for i in 0..5 {
            store.insert("Task", &format!("t{}", i), &record).unwrap();
        }

        // Establish baseline then insert with new field
        store.establish_baseline("Task").unwrap();

        let events = store
            .insert(
                "Task",
                "t_drift",
                &json!({"status": "open", "priority": "high", "label": "bug", "new_col": "x"}),
            )
            .unwrap();

        let has_pattern = events
            .iter()
            .any(|e| matches!(e, PipelineEvent::PatternDetected(_)));
        let has_drift = events
            .iter()
            .any(|e| matches!(e, PipelineEvent::DriftDetected(_)));

        assert!(
            has_pattern || has_drift,
            "expected at least one event type, got {} events",
            events.len()
        );
    }
}
