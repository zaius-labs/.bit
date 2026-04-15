// classify.rs — Naive Bayes auto-classification for entity records
//
// Pure Rust implementation with Laplace smoothing. No external crate needed
// for the base version. The `ml` feature flag is reserved for future
// integration with smartcore for more sophisticated classifiers.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// A trained Naive Bayes classifier.
#[derive(Debug, Default)]
pub struct NaiveBayesClassifier {
    /// class -> (count, word_counts)
    classes: HashMap<String, ClassStats>,
    total_docs: usize,
    vocabulary: HashSet<String>,
}

#[derive(Debug, Default)]
struct ClassStats {
    doc_count: usize,
    word_counts: HashMap<String, usize>,
    total_words: usize,
}

/// A classification prediction.
#[derive(Debug, Clone)]
pub struct Classification {
    pub label: String,
    pub confidence: f64,
    pub scores: Vec<(String, f64)>,
}

impl NaiveBayesClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Train on a labeled example. The label is the class, the record provides features.
    pub fn train(&mut self, label: &str, record: &Value) {
        let words = extract_words(record);
        let stats = self.classes.entry(label.to_string()).or_default();
        stats.doc_count += 1;
        for word in &words {
            *stats.word_counts.entry(word.clone()).or_default() += 1;
            stats.total_words += 1;
            self.vocabulary.insert(word.clone());
        }
        self.total_docs += 1;
    }

    /// Classify a record. Returns the most likely class with confidence.
    pub fn classify(&self, record: &Value) -> Option<Classification> {
        if self.total_docs == 0 || self.classes.is_empty() {
            return None;
        }

        let words = extract_words(record);
        let vocab_size = self.vocabulary.len() as f64;

        let mut scores: Vec<(String, f64)> = self
            .classes
            .iter()
            .map(|(class, stats)| {
                // Log prior
                let log_prior = (stats.doc_count as f64 / self.total_docs as f64).ln();

                // Log likelihood (with Laplace smoothing)
                let log_likelihood: f64 = words
                    .iter()
                    .map(|word| {
                        let count = stats.word_counts.get(word).copied().unwrap_or(0) as f64;
                        ((count + 1.0) / (stats.total_words as f64 + vocab_size)).ln()
                    })
                    .sum();

                (class.clone(), log_prior + log_likelihood)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Convert log scores to probabilities via softmax
        let max_score = scores[0].1;
        let exp_sum: f64 = scores.iter().map(|(_, s)| (s - max_score).exp()).sum();
        let probs: Vec<(String, f64)> = scores
            .iter()
            .map(|(c, s)| (c.clone(), (s - max_score).exp() / exp_sum))
            .collect();

        let label = probs[0].0.clone();
        let confidence = probs[0].1;

        Some(Classification {
            label,
            confidence,
            scores: probs,
        })
    }

    /// Number of trained examples.
    pub fn training_size(&self) -> usize {
        self.total_docs
    }

    /// Known class labels.
    pub fn classes(&self) -> Vec<String> {
        let mut c: Vec<String> = self.classes.keys().cloned().collect();
        c.sort();
        c
    }
}

fn extract_words(value: &Value) -> Vec<String> {
    let text = collect_text(value);
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
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

    fn trained_classifier() -> NaiveBayesClassifier {
        let mut clf = NaiveBayesClassifier::new();
        // Spam examples
        clf.train("spam", &json!({"text": "buy cheap viagra now"}));
        clf.train("spam", &json!({"text": "free money lottery winner"}));
        clf.train("spam", &json!({"text": "cheap pills buy now discount"}));
        clf.train("spam", &json!({"text": "win free prize money today"}));
        clf.train("spam", &json!({"text": "buy discount offer limited time"}));
        // Ham examples
        clf.train("ham", &json!({"text": "meeting at 3pm tomorrow"}));
        clf.train("ham", &json!({"text": "project update sprint review"}));
        clf.train("ham", &json!({"text": "code review pull request ready"}));
        clf.train("ham", &json!({"text": "quarterly planning meeting agenda"}));
        clf.train(
            "ham",
            &json!({"text": "deploy release notes version update"}),
        );
        clf
    }

    #[test]
    fn classifies_spam_correctly() {
        let clf = trained_classifier();
        let result = clf
            .classify(&json!({"text": "buy cheap discount pills free"}))
            .unwrap();
        assert_eq!(result.label, "spam", "expected spam, got {}", result.label);
    }

    #[test]
    fn classifies_ham_correctly() {
        let clf = trained_classifier();
        let result = clf
            .classify(&json!({"text": "sprint planning meeting review"}))
            .unwrap();
        assert_eq!(result.label, "ham", "expected ham, got {}", result.label);
    }

    #[test]
    fn untrained_returns_none() {
        let clf = NaiveBayesClassifier::new();
        assert!(clf.classify(&json!({"text": "anything"})).is_none());
    }

    #[test]
    fn confidence_between_zero_and_one() {
        let clf = trained_classifier();
        let result = clf.classify(&json!({"text": "buy cheap pills"})).unwrap();
        assert!(
            result.confidence > 0.0 && result.confidence <= 1.0,
            "confidence {} not in (0, 1]",
            result.confidence
        );
        // All scores should sum to ~1.0
        let sum: f64 = result.scores.iter().map(|(_, s)| s).sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "scores should sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn training_size_and_classes_correct() {
        let clf = trained_classifier();
        assert_eq!(clf.training_size(), 10);
        let classes = clf.classes();
        assert_eq!(classes.len(), 2);
        assert!(classes.contains(&"spam".to_string()));
        assert!(classes.contains(&"ham".to_string()));
    }
}
