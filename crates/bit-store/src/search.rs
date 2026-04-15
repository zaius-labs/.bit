// search.rs — BM25 keyword search with inverted index
//
// Zero-dependency text search. Build an inverted index at insert time,
// query with BM25 scoring.

use serde_json::Value;
use std::collections::HashMap;

/// A simple inverted index for BM25 search.
#[derive(Debug, Default)]
pub struct SearchIndex {
    /// term → list of (entity_key, term_frequency)
    pub postings: HashMap<String, Vec<(String, f64)>>,
    /// entity_key → document length (number of terms)
    pub doc_lengths: HashMap<String, f64>,
    /// Total number of documents
    pub doc_count: usize,
    /// Average document length
    pub avg_doc_length: f64,
}

/// BM25 parameters
const K1: f64 = 1.2;
const B: f64 = 0.75;

impl SearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document to the index.
    pub fn index_document(&mut self, key: &str, record: &Value) {
        let text = collect_text(record);
        let tokens = tokenize(&text);
        let doc_len = tokens.len() as f64;

        // Count term frequencies
        let mut tf: HashMap<String, f64> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.clone()).or_default() += 1.0;
        }

        // Update postings
        for (term, freq) in tf {
            self.postings
                .entry(term)
                .or_default()
                .push((key.to_string(), freq));
        }

        // Update doc stats
        self.doc_lengths.insert(key.to_string(), doc_len);
        self.doc_count += 1;
        self.avg_doc_length = self.doc_lengths.values().sum::<f64>() / self.doc_count as f64;
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, key: &str) {
        for postings in self.postings.values_mut() {
            postings.retain(|(k, _)| k != key);
        }
        self.doc_lengths.remove(key);
        self.doc_count = self.doc_lengths.len();
        if self.doc_count > 0 {
            self.avg_doc_length = self.doc_lengths.values().sum::<f64>() / self.doc_count as f64;
        } else {
            self.avg_doc_length = 0.0;
        }
    }

    /// Search with BM25 scoring. Returns (entity_key, score) pairs sorted by score.
    pub fn search(&self, query: &str) -> Vec<(String, f64)> {
        let query_tokens = tokenize(query);
        let mut scores: HashMap<String, f64> = HashMap::new();

        for token in &query_tokens {
            if let Some(postings) = self.postings.get(token) {
                let df = postings.len() as f64;
                let idf = ((self.doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

                for (key, tf) in postings {
                    let doc_len = self.doc_lengths.get(key).copied().unwrap_or(1.0);
                    let tf_norm = (tf * (K1 + 1.0))
                        / (tf + K1 * (1.0 - B + B * doc_len / self.avg_doc_length));
                    *scores.entry(key.clone()).or_default() += idf * tf_norm;
                }
            }
        }

        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

fn collect_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => obj.values().map(collect_text).collect::<Vec<_>>().join(" "),
        Value::Array(arr) => arr.iter().map(collect_text).collect::<Vec<_>>().join(" "),
        _ => String::new(),
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
    fn index_and_search_finds_right_document() {
        let mut idx = SearchIndex::new();
        idx.index_document("@Task:t1", &json!({"title": "fix rust compiler error"}));
        idx.index_document("@Task:t2", &json!({"title": "update python tests"}));
        idx.index_document("@Task:t3", &json!({"title": "deploy web server"}));

        let results = idx.search("rust compiler");
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "@Task:t1");
    }

    #[test]
    fn bm25_ranks_more_relevant_higher() {
        let mut idx = SearchIndex::new();
        idx.index_document(
            "@Doc:a",
            &json!({"text": "rust rust rust compiler error handling"}),
        );
        idx.index_document(
            "@Doc:b",
            &json!({"text": "the rust language is nice for web development"}),
        );

        let results = idx.search("rust compiler");
        assert!(results.len() >= 2);
        // Doc:a mentions "rust" 3x and "compiler" 1x, should rank higher
        assert_eq!(results[0].0, "@Doc:a");
        assert!(results[0].1 > results[1].1);
    }

    #[test]
    fn search_nonexistent_term_returns_empty() {
        let mut idx = SearchIndex::new();
        idx.index_document("@Task:t1", &json!({"title": "fix compiler error"}));

        let results = idx.search("nonexistentterm");
        assert!(results.is_empty());
    }

    #[test]
    fn remove_document_excludes_from_search() {
        let mut idx = SearchIndex::new();
        idx.index_document("@Task:t1", &json!({"title": "rust compiler"}));
        idx.index_document("@Task:t2", &json!({"title": "rust server"}));

        idx.remove_document("@Task:t1");
        let results = idx.search("rust compiler");
        // t1 should be gone — only t2 matches on "rust"
        for (key, _) in &results {
            assert_ne!(key, "@Task:t1");
        }
    }

    #[test]
    fn empty_index_search_returns_empty() {
        let idx = SearchIndex::new();
        let results = idx.search("anything");
        assert!(results.is_empty());
    }
}
