// linking.rs — Entity linking via alias tables, fuzzy matching, and rule-based patterns
//
// Zero-dependency entity linker. Resolves text mentions to known entities
// using exact alias lookup, Levenshtein fuzzy matching, and syntactic
// pattern matching (possessives, "assigned to X", etc.).

use crate::nl_query::levenshtein;
use std::collections::HashMap;

/// A resolved entity link.
#[derive(Debug, Clone)]
pub struct ResolvedLink {
    pub mention: String,
    pub entity_type: String,
    pub entity_id: String,
    pub confidence: f64,
    pub method: LinkMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkMethod {
    ExactMatch,
    AliasMatch,
    FuzzyMatch,
    PatternMatch,
}

/// Entity linker with alias tables and known entities.
#[derive(Debug, Default)]
pub struct EntityLinker {
    /// Known entities: entity_type -> set of ids
    pub known_entities: HashMap<String, Vec<String>>,
    /// Alias -> (entity_type, entity_id)
    pub aliases: HashMap<String, (String, String)>,
}

impl EntityLinker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a known entity.
    pub fn register_entity(&mut self, entity_type: &str, id: &str) {
        self.known_entities
            .entry(entity_type.to_string())
            .or_default()
            .push(id.to_string());
    }

    /// Register an alias.
    pub fn register_alias(&mut self, alias: &str, entity_type: &str, id: &str) {
        self.aliases.insert(
            alias.to_lowercase(),
            (entity_type.to_string(), id.to_string()),
        );
    }

    /// Auto-generate aliases from known entities (lowercase, no special chars).
    pub fn build_aliases(&mut self) {
        let mut new_aliases = Vec::new();
        for (entity_type, ids) in &self.known_entities {
            for id in ids {
                // id itself as alias
                new_aliases.push((id.to_lowercase(), entity_type.clone(), id.clone()));
                // Without hyphens/underscores
                let clean = id.replace(['-', '_'], "").to_lowercase();
                if clean != id.to_lowercase() {
                    new_aliases.push((clean, entity_type.clone(), id.clone()));
                }
            }
        }
        for (alias, entity_type, id) in new_aliases {
            self.aliases.insert(alias, (entity_type, id));
        }
    }

    /// Resolve a text mention to an entity.
    pub fn resolve(&self, mention: &str) -> Option<ResolvedLink> {
        let lower = mention.trim().to_lowercase();

        // 1. Exact alias match
        if let Some((entity_type, id)) = self.aliases.get(&lower) {
            return Some(ResolvedLink {
                mention: mention.to_string(),
                entity_type: entity_type.clone(),
                entity_id: id.clone(),
                confidence: 1.0,
                method: LinkMethod::AliasMatch,
            });
        }

        // 2. Possessive pattern: "alice's team" -> resolve "alice"
        if let Some(pos) = lower.find("'s ") {
            let owner = &lower[..pos];
            if let Some(link) = self.resolve(owner) {
                return Some(ResolvedLink {
                    method: LinkMethod::PatternMatch,
                    confidence: link.confidence * 0.8,
                    ..link
                });
            }
        }

        // 3. "assigned to X" pattern
        for prefix in &["assigned to ", "owned by ", "created by ", "managed by "] {
            if let Some(target) = lower.strip_prefix(prefix) {
                if let Some(link) = self.resolve(target) {
                    return Some(ResolvedLink {
                        method: LinkMethod::PatternMatch,
                        confidence: link.confidence * 0.9,
                        ..link
                    });
                }
            }
        }

        // 4. Fuzzy match against all known entity ids
        let mut best: Option<(String, String, usize)> = None;
        for (entity_type, ids) in &self.known_entities {
            for id in ids {
                let dist = levenshtein(&lower, &id.to_lowercase());
                if dist <= 2 && dist > 0 && best.as_ref().is_none_or(|(_, _, d)| dist < *d) {
                    best = Some((entity_type.clone(), id.clone(), dist));
                }
            }
        }

        if let Some((entity_type, id, dist)) = best {
            return Some(ResolvedLink {
                mention: mention.to_string(),
                entity_type,
                entity_id: id,
                confidence: 1.0 - (dist as f64 * 0.15),
                method: LinkMethod::FuzzyMatch,
            });
        }

        None
    }

    /// Resolve with vector reranking -- finds string candidates then reranks by embedding similarity.
    pub fn resolve_with_vectors(
        &self,
        mention: &str,
        vector_index: &crate::vector_search::VectorIndex,
    ) -> Option<ResolvedLink> {
        // First try normal resolution
        if let Some(link) = self.resolve(mention) {
            if link.confidence >= 0.9 {
                return Some(link); // high confidence, don't bother with vectors
            }
        }

        // Use vector search to find candidates
        let results = vector_index.search(mention, 5);
        if let Some((key, score)) = results.first() {
            if *score > 0.3 {
                // Parse key: "@Entity:id"
                let key_trimmed = key.trim_start_matches('@');
                if let Some((entity_type, id)) = key_trimmed.split_once(':') {
                    return Some(ResolvedLink {
                        mention: mention.to_string(),
                        entity_type: entity_type.to_string(),
                        entity_id: id.to_string(),
                        confidence: *score,
                        method: LinkMethod::FuzzyMatch,
                    });
                }
            }
        }

        None
    }

    /// Resolve all mentions in a text string.
    pub fn resolve_all(&self, text: &str) -> Vec<ResolvedLink> {
        let mut links = Vec::new();
        let words: Vec<&str> = text.split_whitespace().collect();
        for i in 0..words.len() {
            // Try single word
            if let Some(link) = self.resolve(words[i]) {
                links.push(link);
            }
            // Try two-word spans
            if i + 1 < words.len() {
                let span = format!("{} {}", words[i], words[i + 1]);
                if let Some(link) = self.resolve(&span) {
                    links.push(link);
                }
            }
        }
        links
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_linker() -> EntityLinker {
        let mut linker = EntityLinker::new();
        linker.register_entity("User", "alice");
        linker.register_entity("User", "bob");
        linker.register_entity("Team", "eng-team");
        linker.register_alias("alice", "User", "alice");
        linker.register_alias("bob", "User", "bob");
        linker.register_alias("eng-team", "Team", "eng-team");
        linker
    }

    #[test]
    fn exact_alias_match() {
        let linker = test_linker();
        let link = linker.resolve("alice").unwrap();
        assert_eq!(link.entity_id, "alice");
        assert_eq!(link.entity_type, "User");
        assert_eq!(link.method, LinkMethod::AliasMatch);
        assert_eq!(link.confidence, 1.0);
    }

    #[test]
    fn fuzzy_match_typo() {
        let linker = test_linker();
        let link = linker.resolve("alce").unwrap();
        assert_eq!(link.entity_id, "alice");
        assert_eq!(link.method, LinkMethod::FuzzyMatch);
        assert!(link.confidence > 0.5);
    }

    #[test]
    fn possessive_pattern() {
        let linker = test_linker();
        let link = linker.resolve("alice's team").unwrap();
        assert_eq!(link.entity_id, "alice");
        assert_eq!(link.method, LinkMethod::PatternMatch);
        assert!(link.confidence < 1.0);
    }

    #[test]
    fn assigned_to_pattern() {
        let linker = test_linker();
        let link = linker.resolve("assigned to bob").unwrap();
        assert_eq!(link.entity_id, "bob");
        assert_eq!(link.method, LinkMethod::PatternMatch);
    }

    #[test]
    fn unknown_mention_returns_none() {
        let linker = test_linker();
        assert!(linker.resolve("zzzznotanentity").is_none());
    }

    #[test]
    fn resolve_all_finds_multiple() {
        let linker = test_linker();
        let links = linker.resolve_all("alice and bob worked together");
        let ids: Vec<&str> = links.iter().map(|l| l.entity_id.as_str()).collect();
        assert!(ids.contains(&"alice"));
        assert!(ids.contains(&"bob"));
    }

    #[test]
    fn resolve_with_vectors_finds_match() {
        let linker = EntityLinker::new(); // empty linker — no string matches
        let mut idx = crate::vector_search::VectorIndex::new();
        idx.add(
            "@User:alice",
            &serde_json::json!({"name": "Alice Smith", "role": "designer"}),
        );
        idx.add(
            "@Team:eng-team",
            &serde_json::json!({"name": "engineering team engineering backend engineering"}),
        );

        // "engineering" won't match via string linking, but vectors should find eng-team
        let result = linker.resolve_with_vectors("engineering", &idx);
        assert!(result.is_some(), "vector search should find a match");
        let link = result.unwrap();
        assert_eq!(link.entity_type, "Team");
    }

    #[test]
    fn high_confidence_string_match_bypasses_vectors() {
        let linker = test_linker();
        let idx = crate::vector_search::VectorIndex::new(); // empty index

        // "alice" is an exact alias match (confidence 1.0) — should resolve without vectors
        let result = linker.resolve_with_vectors("alice", &idx);
        assert!(result.is_some());
        assert_eq!(result.unwrap().entity_id, "alice");
    }

    #[test]
    fn build_aliases_auto_generates() {
        let mut linker = EntityLinker::new();
        linker.register_entity("Team", "eng-team");
        linker.build_aliases();
        // Should resolve with and without hyphen
        let link = linker.resolve("eng-team").unwrap();
        assert_eq!(link.entity_id, "eng-team");
        let link2 = linker.resolve("engteam").unwrap();
        assert_eq!(link2.entity_id, "eng-team");
    }
}
