// embeddings.rs — Embedding infrastructure for bit-store
//
// Provides simple bag-of-words embeddings (always available) and infrastructure
// for real model-based embeddings behind the `embeddings` feature flag.

use std::collections::HashMap;

#[cfg(feature = "embeddings")]
pub mod model {
    //! Real embedding implementation using tract-onnx + tokenizers.
    //!
    //! Requires an ONNX model file (e.g. MiniLM) and a tokenizer.json.
    //! This module is available when the `embeddings` feature is enabled.
    //! For now it provides the type stubs; the actual model loading will
    //! be wired once we ship the model binary.

    use std::path::Path;

    /// A model-backed embedding engine.
    #[derive(Debug)]
    pub struct ModelEmbedder {
        _dim: usize,
    }

    impl ModelEmbedder {
        /// Create a new model embedder from ONNX + tokenizer paths.
        ///
        /// Returns an error if the files can't be loaded.
        pub fn load(_model_path: &Path, _tokenizer_path: &Path) -> Result<Self, String> {
            // TODO: wire tract-onnx model loading
            Err("model loading not yet implemented — use simple_embed fallback".into())
        }

        /// Embed a text string into a dense vector.
        pub fn embed(&self, _text: &str) -> Vec<f64> {
            vec![0.0; self._dim]
        }

        /// Embedding dimensionality.
        pub fn dim(&self) -> usize {
            self._dim
        }
    }
}

// ---------------------------------------------------------------------------
// Always-available: simple bag-of-words embedding for testing/fallback
// ---------------------------------------------------------------------------

/// A simple embedding -- normalized TF vector. Not semantic, but testable.
pub fn simple_embed(text: &str) -> Vec<f64> {
    let tokens = tokenize(text);
    let mut vocab: HashMap<String, usize> = HashMap::new();
    for token in &tokens {
        let idx = vocab.len();
        vocab.entry(token.clone()).or_insert(idx);
    }

    let dim = vocab.len().max(1);
    let mut vec = vec![0.0f64; dim];
    for token in &tokens {
        if let Some(&idx) = vocab.get(token) {
            vec[idx] += 1.0;
        }
    }

    // L2 normalize
    let norm: f64 = vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        for x in &mut vec {
            *x /= norm;
        }
    }
    vec
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let dot: f64 = (0..len).map(|i| a[i] * b[i]).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_embed_produces_normalized_vector() {
        let v = simple_embed("hello world hello");
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_identical_vectors() {
        let v = simple_embed("foo bar baz");
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-9,
            "identical vectors should have cosine 1.0, got {sim}"
        );
    }

    #[test]
    fn same_text_embeds_identically() {
        let a = simple_embed("rust programming language");
        let b = simple_embed("rust programming language");
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-9,
            "same text should embed identically, got {sim}"
        );
    }

    #[test]
    fn empty_text_returns_nonpanic() {
        let v = simple_embed("");
        assert!(!v.is_empty()); // at least 1 dim
    }

    #[test]
    fn cosine_zero_vector() {
        let sim = cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]);
        assert_eq!(sim, 0.0);
    }
}
