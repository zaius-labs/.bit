use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema registry built from define:@Entity blocks in parsed documents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaRegistry {
    pub entities: HashMap<String, EntitySchema>,
    #[serde(default)]
    pub conflicts: Vec<SchemaConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    pub name: String,
    pub atoms: Vec<Atom>,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConflict {
    pub entity: String,
    pub field: String,
    pub existing_type: String,
    pub new_type: String,
}

/// Result of validating a mutation against the schema.
#[derive(Debug, Clone, Default)]
pub struct MutationValidation {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Coarse type category for a FieldDefault value.
fn field_type(default: &FieldDefault) -> &'static str {
    match default {
        FieldDefault::Str(_) => "string",
        FieldDefault::Int(_) => "int",
        FieldDefault::Float(_) => "float",
        FieldDefault::Bool(_) => "bool",
        FieldDefault::Atom(_) => "atom",
        FieldDefault::Enum(_) => "enum",
        FieldDefault::Ref(_) => "ref",
        FieldDefault::List => "list",
        FieldDefault::Timestamp(_) => "timestamp",
        FieldDefault::Nil => "unknown",
        FieldDefault::Trit(_) => "trit",
    }
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extract_from_doc(&mut self, doc: &Document) {
        self.extract_from_nodes(&doc.nodes);
    }

    fn extract_from_nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Define(d) => self.merge_define(d),
                Node::Group(g) => self.extract_from_nodes(&g.children),
                Node::Validate(v) => self.extract_from_nodes(&v.children),
                Node::Conditional(c) => self.extract_from_nodes(&c.children),
                Node::GateDef(g) => self.extract_from_nodes(&g.children),
                _ => {}
            }
        }
    }

    fn merge_define(&mut self, d: &Define) {
        if let Some(existing) = self.entities.get_mut(&d.entity) {
            // Merge atoms (deduplicate by name)
            for atom in &d.atoms {
                if !existing.atoms.iter().any(|a| a.name == atom.name) {
                    existing.atoms.push(atom.clone());
                }
            }

            // Merge fields: union, detect type conflicts
            for new_field in &d.fields {
                if let Some(existing_field) =
                    existing.fields.iter().find(|f| f.name == new_field.name)
                {
                    let existing_type = field_type(&existing_field.default);
                    let new_type = field_type(&new_field.default);
                    if existing_type != new_type
                        && existing_type != "unknown"
                        && new_type != "unknown"
                    {
                        self.conflicts.push(SchemaConflict {
                            entity: d.entity.clone(),
                            field: new_field.name.clone(),
                            existing_type: existing_type.to_string(),
                            new_type: new_type.to_string(),
                        });
                    }
                } else {
                    existing.fields.push(new_field.clone());
                }
            }
        } else {
            self.entities.insert(
                d.entity.clone(),
                EntitySchema {
                    name: d.entity.clone(),
                    atoms: d.atoms.clone(),
                    fields: d.fields.clone(),
                },
            );
        }
    }

    pub fn validate_mutation(
        &self,
        entity: &str,
        fields: &[(String, String)],
    ) -> MutationValidation {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        match self.entities.get(entity) {
            None => errors.push(format!("Unknown entity: @{}", entity)),
            Some(schema) => {
                for (field_name, value) in fields {
                    match schema.fields.iter().find(|f| f.name == *field_name) {
                        None => {
                            let known: Vec<&str> =
                                schema.fields.iter().map(|f| f.name.as_str()).collect();
                            errors.push(format!(
                                "Unknown field '{}' on @{}. Known: {:?}",
                                field_name, entity, known
                            ));
                        }
                        Some(field_def) => {
                            let trimmed = value.trim();
                            let is_list_op = trimmed.starts_with("+[") || trimmed.starts_with("-[");
                            let is_list_field =
                                matches!(field_def.default, FieldDefault::List) || field_def.plural;

                            // List operator on non-list field
                            if is_list_op && !is_list_field {
                                warnings.push(format!(
                                    "List operator on non-list field '{}' of @{}",
                                    field_name, entity
                                ));
                            }

                            // Skip further type checks for nil, refs, list ops, or empty values
                            if trimmed == "nil"
                                || trimmed.is_empty()
                                || is_list_op
                                || trimmed.starts_with('@')
                            {
                                continue;
                            }

                            match &field_def.default {
                                FieldDefault::Int(_) | FieldDefault::Float(_) => {
                                    if trimmed.parse::<f64>().is_err() {
                                        warnings.push(format!(
                                            "Type mismatch: field '{}' of @{} expects {}, got '{}'",
                                            field_name,
                                            entity,
                                            field_type(&field_def.default),
                                            trimmed
                                        ));
                                    }
                                }
                                FieldDefault::Bool(_) => {
                                    if trimmed != "true" && trimmed != "false" {
                                        warnings.push(format!(
                                            "Type mismatch: field '{}' of @{} expects bool, got '{}'",
                                            field_name, entity, trimmed
                                        ));
                                    }
                                }
                                FieldDefault::Enum(values) => {
                                    if !values.contains(&trimmed.to_string()) {
                                        warnings.push(format!(
                                            "Type mismatch: field '{}' of @{} expects one of {:?}, got '{}'",
                                            field_name, entity, values, trimmed
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        MutationValidation { errors, warnings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_field(name: &str, default: FieldDefault) -> FieldDef {
        FieldDef {
            name: name.to_string(),
            plural: false,
            default,
        }
    }

    fn make_define(entity: &str, fields: Vec<FieldDef>) -> Node {
        Node::Define(Define {
            entity: entity.to_string(),
            atoms: vec![],
            fields,
            from_scope: None,
            mod_scope: None,
            workspace_scope: None,
        })
    }

    #[test]
    fn merge_compatible_definitions() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![
                make_define(
                    "Task",
                    vec![make_field("title", FieldDefault::Str("".to_string()))],
                ),
                make_define(
                    "Task",
                    vec![make_field("status", FieldDefault::Atom("open".to_string()))],
                ),
            ], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let schema = &reg.entities["Task"];
        let names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
        assert!(reg.conflicts.is_empty());
    }

    #[test]
    fn detect_type_conflict() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![
                make_define("Task", vec![make_field("count", FieldDefault::Int(0))]),
                make_define(
                    "Task",
                    vec![make_field("count", FieldDefault::Str("".to_string()))],
                ),
            ], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        assert_eq!(reg.conflicts.len(), 1);
        assert_eq!(reg.conflicts[0].entity, "Task");
        assert_eq!(reg.conflicts[0].field, "count");
    }

    #[test]
    fn list_op_on_non_list_field_warns() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field("title", FieldDefault::Str("".to_string()))],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result =
            reg.validate_mutation("Task", &[("title".to_string(), "+[\"extra\"]".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("List operator on non-list field")));
    }

    #[test]
    fn list_op_on_list_field_ok() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field("tags", FieldDefault::List)],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result =
            reg.validate_mutation("Task", &[("tags".to_string(), "+[\"new\"]".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn list_op_on_plural_field_ok() {
        let mut reg = SchemaRegistry::new();
        let field = FieldDef {
            name: "items".to_string(),
            plural: true,
            default: FieldDefault::Str("".to_string()),
        };
        let doc = Document {
            nodes: vec![make_define("Task", vec![field])], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result =
            reg.validate_mutation("Task", &[("items".to_string(), "+[\"new\"]".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn type_mismatch_int_field_warns() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field("count", FieldDefault::Int(0))],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result =
            reg.validate_mutation("Task", &[("count".to_string(), "not-a-number".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("Type mismatch")));
    }

    #[test]
    fn type_mismatch_bool_field_warns() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field("done", FieldDefault::Bool(false))],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result = reg.validate_mutation("Task", &[("done".to_string(), "yes".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Type mismatch") && w.contains("bool")));
    }

    #[test]
    fn type_mismatch_enum_field_warns() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field(
                    "status",
                    FieldDefault::Enum(vec!["open".to_string(), "closed".to_string()]),
                )],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result =
            reg.validate_mutation("Task", &[("status".to_string(), "invalid".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Type mismatch") && w.contains("invalid")));
    }

    #[test]
    fn nil_value_skips_type_check() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![make_define(
                "Task",
                vec![make_field("count", FieldDefault::Int(0))],
            )], ..Default::default()
        };
        reg.extract_from_doc(&doc);

        let result = reg.validate_mutation("Task", &[("count".to_string(), "nil".to_string())]);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn no_conflict_on_same_type() {
        let mut reg = SchemaRegistry::new();
        let doc = Document {
            nodes: vec![
                make_define(
                    "Task",
                    vec![make_field("title", FieldDefault::Str("".to_string()))],
                ),
                make_define(
                    "Task",
                    vec![make_field(
                        "title",
                        FieldDefault::Str("default".to_string()),
                    )],
                ),
            ], ..Default::default()
        };
        reg.extract_from_doc(&doc);
        assert!(reg.conflicts.is_empty());
    }
}
