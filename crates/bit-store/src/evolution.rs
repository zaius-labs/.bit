// evolution.rs — Schema evolution via mismatch detection and migration proposals
//
// Zero-dependency schema evolution. Compares a declared schema against observed
// data (using the infer module) and proposes field additions, removals, and
// type changes as a structured migration.

use crate::infer::{infer_schema, InferredType};
use serde_json::Value;
use std::collections::HashMap;

/// A proposed schema migration.
#[derive(Debug, Clone)]
pub struct MigrationProposal {
    pub entity_type: String,
    pub changes: Vec<SchemaChange>,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub enum SchemaChange {
    AddField {
        name: String,
        inferred_type: InferredType,
        occurrence_rate: f64,
    },
    RemoveField {
        name: String,
        last_seen_rate: f64,
    },
    ChangeType {
        name: String,
        from: InferredType,
        to: InferredType,
    },
    MakeOptional {
        name: String,
    },
    MakeRequired {
        name: String,
        occurrence_rate: f64,
    },
}

/// Compare declared schema against observed data and propose migrations.
pub fn propose_migration(
    entity_type: &str,
    declared_fields: &HashMap<String, String>, // field_name -> type_string
    records: &[Value],
) -> MigrationProposal {
    let inferred = infer_schema(entity_type, records);
    let mut changes = Vec::new();

    // Check for new fields (in data but not in schema)
    for field in &inferred.fields {
        if !declared_fields.contains_key(&field.name) {
            changes.push(SchemaChange::AddField {
                name: field.name.clone(),
                inferred_type: field.field_type.clone(),
                occurrence_rate: field.occurrence_rate,
            });
        }
    }

    // Check for removed fields (in schema but not in data)
    for field_name in declared_fields.keys() {
        if !inferred.fields.iter().any(|f| f.name == *field_name) {
            changes.push(SchemaChange::RemoveField {
                name: field_name.clone(),
                last_seen_rate: 0.0,
            });
        }
    }

    // Check for type changes
    for field in &inferred.fields {
        if let Some(declared_type) = declared_fields.get(&field.name) {
            let inferred_type_str = type_to_string(&field.field_type);
            if inferred_type_str != *declared_type {
                changes.push(SchemaChange::ChangeType {
                    name: field.name.clone(),
                    from: string_to_type(declared_type),
                    to: field.field_type.clone(),
                });
            }
        }
    }

    let confidence = if changes.is_empty() {
        1.0
    } else {
        // More records = higher confidence in proposed changes
        (inferred.record_count as f64 / 100.0).min(1.0)
    };

    MigrationProposal {
        entity_type: entity_type.to_string(),
        changes,
        confidence,
    }
}

fn type_to_string(t: &InferredType) -> String {
    match t {
        InferredType::Bool => "bool".into(),
        InferredType::Int => "int".into(),
        InferredType::Float => "float".into(),
        InferredType::Timestamp => "timestamp".into(),
        InferredType::String => "string".into(),
        InferredType::Enum(_) => "enum".into(),
        InferredType::List => "list".into(),
        InferredType::Object => "object".into(),
        _ => "unknown".into(),
    }
}

fn string_to_type(s: &str) -> InferredType {
    match s {
        "bool" => InferredType::Bool,
        "int" => InferredType::Int,
        "float" => InferredType::Float,
        "timestamp" => InferredType::Timestamp,
        "list" => InferredType::List,
        "object" => InferredType::Object,
        "enum" => InferredType::Enum(vec![]),
        _ => InferredType::String,
    }
}

/// Render a migration proposal as human-readable text.
pub fn render_migration(proposal: &MigrationProposal) -> String {
    if proposal.changes.is_empty() {
        return format!(
            "@{}: schema matches data (no changes needed)",
            proposal.entity_type
        );
    }
    let mut lines = vec![format!(
        "@{} migration proposal (confidence: {:.0}%):",
        proposal.entity_type,
        proposal.confidence * 100.0
    )];
    for change in &proposal.changes {
        match change {
            SchemaChange::AddField {
                name,
                inferred_type,
                occurrence_rate,
            } => lines.push(format!(
                "  + add field '{}': {} (seen in {:.0}% of records)",
                name,
                type_to_string(inferred_type),
                occurrence_rate * 100.0
            )),
            SchemaChange::RemoveField { name, .. } => {
                lines.push(format!("  - remove field '{}' (no longer present)", name))
            }
            SchemaChange::ChangeType { name, from, to } => lines.push(format!(
                "  ~ change '{}': {} -> {}",
                name,
                type_to_string(from),
                type_to_string(to)
            )),
            SchemaChange::MakeOptional { name } => {
                lines.push(format!("  ? make '{}' optional", name))
            }
            SchemaChange::MakeRequired {
                name,
                occurrence_rate,
            } => lines.push(format!(
                "  ! make '{}' required (present in {:.0}%)",
                name,
                occurrence_rate * 100.0
            )),
        }
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detect_new_field() {
        let declared: HashMap<String, String> =
            [("name".into(), "string".into())].into_iter().collect();
        let records = vec![
            json!({"name": "alice", "email": "a@x.com"}),
            json!({"name": "bob", "email": "b@x.com"}),
            json!({"name": "carol", "email": "c@x.com"}),
        ];
        let proposal = propose_migration("User", &declared, &records);
        assert!(proposal
            .changes
            .iter()
            .any(|c| matches!(c, SchemaChange::AddField { name, .. } if name == "email")));
    }

    #[test]
    fn detect_removed_field() {
        let declared: HashMap<String, String> = [
            ("name".into(), "string".into()),
            ("phone".into(), "string".into()),
        ]
        .into_iter()
        .collect();
        let records = vec![
            json!({"name": "alice"}),
            json!({"name": "bob"}),
            json!({"name": "carol"}),
        ];
        let proposal = propose_migration("User", &declared, &records);
        assert!(proposal
            .changes
            .iter()
            .any(|c| matches!(c, SchemaChange::RemoveField { name, .. } if name == "phone")));
    }

    #[test]
    fn detect_type_change() {
        let declared: HashMap<String, String> = [
            ("name".into(), "string".into()),
            ("age".into(), "string".into()), // declared as string, data has int
        ]
        .into_iter()
        .collect();
        let records = vec![
            json!({"name": "alice", "age": 30}),
            json!({"name": "bob", "age": 25}),
            json!({"name": "carol", "age": 28}),
        ];
        let proposal = propose_migration("User", &declared, &records);
        assert!(proposal
            .changes
            .iter()
            .any(|c| matches!(c, SchemaChange::ChangeType { name, .. } if name == "age")));
    }

    #[test]
    fn no_changes_when_schema_matches() {
        let declared: HashMap<String, String> = [
            ("name".into(), "string".into()),
            ("age".into(), "int".into()),
        ]
        .into_iter()
        .collect();
        let records = vec![
            json!({"name": "alice", "age": 30}),
            json!({"name": "bob", "age": 25}),
        ];
        let proposal = propose_migration("User", &declared, &records);
        assert!(proposal.changes.is_empty());
        assert_eq!(proposal.confidence, 1.0);
    }

    #[test]
    fn render_migration_readable() {
        let proposal = MigrationProposal {
            entity_type: "User".to_string(),
            changes: vec![
                SchemaChange::AddField {
                    name: "email".into(),
                    inferred_type: InferredType::String,
                    occurrence_rate: 0.95,
                },
                SchemaChange::RemoveField {
                    name: "phone".into(),
                    last_seen_rate: 0.0,
                },
            ],
            confidence: 0.8,
        };
        let text = render_migration(&proposal);
        assert!(text.contains("@User migration proposal"));
        assert!(text.contains("+ add field 'email'"));
        assert!(text.contains("- remove field 'phone'"));
    }
}
