// vector_search.rs — Vector similarity search over entity records
//
// Uses the simple bag-of-words embedding from embeddings.rs as the default.
// When the `embeddings` feature is enabled, this can be upgraded to use
// real model-based embeddings.

use crate::embeddings::{cosine_similarity, simple_embed};
use serde_json::Value;

/// A vector index over entity records.
#[derive(Debug, Default)]
pub struct VectorIndex {
    /// (entity_key, embedding, original_text)
    entries: Vec<(String, Vec<f64>, String)>,
}

impl VectorIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entity to the vector index.
    pub fn add(&mut self, key: &str, record: &Value) {
        let text = collect_text(record);
        let embedding = simple_embed(&text);
        self.entries.push((key.to_string(), embedding, text));
    }

    /// Remove an entity from the index.
    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(k, _, _)| k != key);
    }

    /// Search for similar entities. Returns (key, similarity_score) sorted by similarity.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, f64)> {
        let query_embedding = simple_embed(query);

        let mut scores: Vec<(String, f64)> = self
            .entries
            .iter()
            .map(|(key, emb, _)| {
                let sim = cosine_similarity(&query_embedding, emb);
                (key.clone(), sim)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    /// Find entities similar to a given entity.
    pub fn find_similar(&self, key: &str, limit: usize) -> Vec<(String, f64)> {
        let Some(entry) = self.entries.iter().find(|(k, _, _)| k == key) else {
            return vec![];
        };
        let emb = &entry.1;

        let mut scores: Vec<(String, f64)> = self
            .entries
            .iter()
            .filter(|(k, _, _)| k != key)
            .map(|(k, e, _)| (k.clone(), cosine_similarity(emb, e)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn collect_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => obj
            .iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .map(|(_, v)| collect_text(v))
            .collect::<Vec<_>>()
            .join(" "),
        Value::Array(arr) => arr.iter().map(collect_text).collect::<Vec<_>>().join(" "),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn search_finds_most_relevant() {
        let mut idx = VectorIndex::new();
        idx.add(
            "bug:1",
            &json!({"title": "login page crashes on submit login error"}),
        );
        idx.add("feat:1", &json!({"title": "add dark mode theme colors"}));
        idx.add(
            "bug:2",
            &json!({"title": "login authentication fails login timeout"}),
        );

        let results = idx.search("login error", 3);
        assert!(!results.is_empty());
        // The query shares "login" and "error" with bug:1, should rank it high
        let top_keys: Vec<&str> = results.iter().take(2).map(|r| r.0.as_str()).collect();
        assert!(
            top_keys.contains(&"bug:1"),
            "expected bug:1 in top 2, got {top_keys:?}"
        );
    }

    #[test]
    fn find_similar_returns_closest() {
        let mut idx = VectorIndex::new();
        idx.add("a", &json!({"text": "rust programming language systems"}));
        idx.add("b", &json!({"text": "rust cargo build system"}));
        idx.add(
            "c",
            &json!({"text": "python machine learning data science"}),
        );

        let similar = idx.find_similar("a", 2);
        assert!(!similar.is_empty());
        // "b" shares "rust" vocabulary with "a", should be more similar than "c"
        assert_eq!(similar[0].0, "b");
    }

    #[test]
    fn remove_entity_from_index() {
        let mut idx = VectorIndex::new();
        idx.add("x", &json!({"text": "hello"}));
        idx.add("y", &json!({"text": "world"}));
        assert_eq!(idx.len(), 2);

        idx.remove("x");
        assert_eq!(idx.len(), 1);

        let results = idx.search("hello", 5);
        assert!(results.iter().all(|(k, _)| k != "x"));
    }

    #[test]
    fn empty_index_returns_empty() {
        let idx = VectorIndex::new();
        assert!(idx.is_empty());
        let results = idx.search("anything", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn cosine_identical_is_one() {
        use crate::embeddings::cosine_similarity;
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn build_vector_index_from_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bitstore");
        let mut store = crate::store::BitStore::create(&path).unwrap();
        store
            .insert_entity("Bug", "1", &json!({"title": "crash on login"}))
            .unwrap();
        store
            .insert_entity("Bug", "2", &json!({"title": "timeout on search"}))
            .unwrap();

        let index = store.build_vector_index().unwrap();
        assert_eq!(index.len(), 2);

        let results = index.search("login crash", 5);
        assert!(!results.is_empty());
    }
}
