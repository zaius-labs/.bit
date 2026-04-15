// nl_query.rs — Natural language query translation via slot-filling and fuzzy matching
//
// Zero-dependency NL→StoreQuery translator. Uses tokenization, alias tables,
// Levenshtein distance, and simple pattern matching to convert natural language
// into the structured query format from query_engine.rs.

use crate::query_engine::{QueryTarget, SortSpec, StoreQuery};
use std::collections::HashMap;

/// Result of NL query parsing.
#[derive(Debug)]
pub struct NlQueryResult {
    pub query: StoreQuery,
    pub confidence: f64,
    /// Human-readable explanation of what we understood.
    pub interpretation: String,
}

/// Known schema context for NL parsing.
#[derive(Debug, Default)]
pub struct SchemaContext {
    /// entity_name -> list of field names
    pub entities: HashMap<String, Vec<String>>,
    /// field_name -> list of known enum values
    pub enum_values: HashMap<String, Vec<String>>,
    /// alias -> canonical entity name (e.g., "users" -> "User", "admins" -> "User")
    pub entity_aliases: HashMap<String, String>,
    /// alias -> (field_name, value) (e.g., "active" -> ("active", "true"))
    pub value_aliases: HashMap<String, (String, String)>,
}

/// Parse a natural language query into a structured StoreQuery.
pub fn parse_nl_query(input: &str, context: &SchemaContext) -> NlQueryResult {
    let input = input.trim();

    // Strip common prefixes
    let cleaned = strip_prefixes(input);
    let tokens = tokenize(&cleaned);

    // Step 1: Find entity type
    let (entity, entity_confidence) = resolve_entity(&tokens, context);

    // Step 2: Find filter predicates
    let filters = resolve_filters(&tokens, &entity, context);

    // Step 3: Find sort/limit
    let sort = resolve_sort(&tokens);
    let limit = resolve_limit(&tokens);

    // Build query
    let filter_str = if filters.is_empty() {
        None
    } else {
        Some(filters.join(" and "))
    };

    let query = StoreQuery {
        target: if let Some(ref e) = entity {
            QueryTarget::Entity {
                name: e.clone(),
                id: None,
            }
        } else {
            QueryTarget::AllEntities
        },
        filter: filter_str,
        sort,
        limit,
    };

    // Build interpretation
    let mut parts = Vec::new();
    if let Some(ref e) = entity {
        parts.push(format!("@{}", e));
    }
    if !filters.is_empty() {
        parts.push(format!("where {}", filters.join(" and ")));
    }

    let confidence = entity_confidence * if filters.is_empty() { 0.8 } else { 1.0 };

    NlQueryResult {
        query,
        confidence,
        interpretation: if parts.is_empty() {
            "all entities".to_string()
        } else {
            parts.join(" ")
        },
    }
}

fn strip_prefixes(input: &str) -> String {
    let prefixes = [
        "show me ",
        "find ",
        "get ",
        "list ",
        "show ",
        "fetch ",
        "search for ",
        "look up ",
    ];
    let lower = input.to_lowercase();
    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            return input[prefix.len()..].to_string();
        }
    }
    input.to_string()
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn resolve_entity(tokens: &[String], context: &SchemaContext) -> (Option<String>, f64) {
    for token in tokens {
        let lower = token.to_lowercase();
        // Check aliases first
        if let Some(entity) = context.entity_aliases.get(&lower) {
            return (Some(entity.clone()), 1.0);
        }
        // Check entity names (case-insensitive, singular/plural)
        for entity_name in context.entities.keys() {
            let en_lower = entity_name.to_lowercase();
            if lower == en_lower
                || lower == format!("{}s", en_lower)
                || lower.trim_end_matches('s') == en_lower
            {
                return (Some(entity_name.clone()), 0.95);
            }
        }
        // Fuzzy match
        for entity_name in context.entities.keys() {
            if levenshtein(&token.to_lowercase(), &entity_name.to_lowercase()) <= 2 {
                return (Some(entity_name.clone()), 0.7);
            }
        }
    }
    (None, 0.3)
}

fn resolve_filters(
    tokens: &[String],
    entity: &Option<String>,
    context: &SchemaContext,
) -> Vec<String> {
    let mut filters = Vec::new();

    for token in tokens {
        let lower = token.to_lowercase();
        // Check value aliases ("active" -> active=true, "admins" -> role=admin)
        if let Some((field, value)) = context.value_aliases.get(&lower) {
            filters.push(format!("{}={}", field, value));
            continue;
        }

        // Check if token matches an enum value for any field on this entity
        if let Some(ref e) = entity {
            if let Some(fields) = context.entities.get(e) {
                for field in fields {
                    if let Some(enums) = context.enum_values.get(field) {
                        for enum_val in enums {
                            if lower == enum_val.to_lowercase()
                                || levenshtein(&lower, &enum_val.to_lowercase()) <= 1
                            {
                                filters.push(format!("{}={}", field, enum_val));
                            }
                        }
                    }
                }
            }
        }
    }

    filters
}

fn resolve_sort(tokens: &[String]) -> Option<SortSpec> {
    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_lowercase();
        if (lower == "sorted" || lower == "ordered" || lower == "sort" || lower == "order")
            && i + 1 < tokens.len()
        {
            let next_idx = if tokens.get(i + 1).is_some_and(|t| t.to_lowercase() == "by") {
                i + 2
            } else {
                i + 1
            };
            if let Some(field) = tokens.get(next_idx) {
                return Some(SortSpec {
                    field: field.clone(),
                    descending: false,
                });
            }
        }
    }
    None
}

fn resolve_limit(tokens: &[String]) -> Option<usize> {
    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_lowercase();
        if (lower == "top" || lower == "first" || lower == "limit") && i + 1 < tokens.len() {
            if let Ok(n) = tokens[i + 1].parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut m = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in m.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, val) in m[0].iter_mut().enumerate() {
        *val = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            m[i][j] = (m[i - 1][j] + 1)
                .min(m[i][j - 1] + 1)
                .min(m[i - 1][j - 1] + cost);
        }
    }
    m[a.len()][b.len()]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> SchemaContext {
        let mut ctx = SchemaContext::default();
        ctx.entities.insert(
            "User".to_string(),
            vec!["name".into(), "role".into(), "active".into()],
        );
        ctx.entities
            .insert("Task".to_string(), vec!["title".into(), "status".into()]);
        ctx.enum_values.insert(
            "role".to_string(),
            vec!["admin".into(), "editor".into(), "viewer".into()],
        );
        ctx.enum_values
            .insert("status".to_string(), vec!["open".into(), "closed".into()]);
        ctx.entity_aliases
            .insert("admins".to_string(), "User".to_string());
        ctx.value_aliases.insert(
            "active".to_string(),
            ("active".to_string(), "true".to_string()),
        );
        ctx.value_aliases.insert(
            "admins".to_string(),
            ("role".to_string(), "admin".to_string()),
        );
        ctx
    }

    #[test]
    fn nl_active_users() {
        let ctx = test_context();
        let result = parse_nl_query("show me active users", &ctx);
        assert!(
            matches!(result.query.target, QueryTarget::Entity { ref name, .. } if name == "User")
        );
        assert!(result
            .query
            .filter
            .as_deref()
            .unwrap()
            .contains("active=true"));
    }

    #[test]
    fn nl_find_admins_alias() {
        let ctx = test_context();
        let result = parse_nl_query("find admins", &ctx);
        assert!(
            matches!(result.query.target, QueryTarget::Entity { ref name, .. } if name == "User")
        );
        assert!(result
            .query
            .filter
            .as_deref()
            .unwrap()
            .contains("role=admin"));
    }

    #[test]
    fn nl_sorted_by_name() {
        let ctx = test_context();
        let result = parse_nl_query("users sorted by name", &ctx);
        assert!(
            matches!(result.query.target, QueryTarget::Entity { ref name, .. } if name == "User")
        );
        let sort = result.query.sort.as_ref().unwrap();
        assert_eq!(sort.field, "name");
    }

    #[test]
    fn nl_top_5_tasks() {
        let ctx = test_context();
        let result = parse_nl_query("top 5 tasks", &ctx);
        assert!(
            matches!(result.query.target, QueryTarget::Entity { ref name, .. } if name == "Task")
        );
        assert_eq!(result.query.limit, Some(5));
    }

    #[test]
    fn nl_unknown_entity_low_confidence() {
        let ctx = test_context();
        let result = parse_nl_query("show me all widgets", &ctx);
        assert!(matches!(result.query.target, QueryTarget::AllEntities));
        assert!(result.confidence < 0.5);
    }

    #[test]
    fn nl_fuzzy_match_typo() {
        let ctx = test_context();
        let result = parse_nl_query("Usrs", &ctx);
        assert!(
            matches!(result.query.target, QueryTarget::Entity { ref name, .. } if name == "User")
        );
        assert!(result.confidence > 0.5);
    }
}
