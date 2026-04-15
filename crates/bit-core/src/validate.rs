use crate::schema::SchemaRegistry;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String, // E_EMPTY_TASK, E_INVALID_MUTATION, etc.
    pub kind: String, // keep existing kind for backwards compat
    pub message: String,
    pub context: Option<String>,
    pub line: usize, // 0 = unknown (AST doesn't have line info yet)
    pub col: usize,  // 0 = unknown
}

fn kind_to_code(kind: &str) -> String {
    match kind {
        "empty_task" => "E_EMPTY_TASK",
        "invalid_mutation" => "E_INVALID_MUTATION",
        "unknown_query_entity" => "E_UNKNOWN_ENTITY",
        "unknown_query_field" => "E_UNKNOWN_FIELD",
        "empty_flow_node" => "E_EMPTY_FLOW_NODE",
        "unknown_sort_field" => "E_UNKNOWN_SORT_FIELD",
        "empty_state" => "E_EMPTY_STATE",
        _ => "E_VALIDATION_ERROR",
    }
    .into()
}

impl ValidationResult {
    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate(doc: &Document, schemas: &SchemaRegistry) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut known_vars: HashSet<String> = HashSet::new();
    let mut known_validators: HashSet<String> = HashSet::new();
    let mut known_tasks: HashSet<String> = HashSet::new();

    collect_names(
        &doc.nodes,
        &mut known_vars,
        &mut known_validators,
        &mut known_tasks,
    );
    validate_nodes(
        &doc.nodes,
        schemas,
        &mut errors,
        &mut warnings,
        &[],
        &known_vars,
        &known_validators,
        &known_tasks,
    );

    check_referential_integrity(&doc.nodes, &mut warnings);
    check_lattice_construct_sections(&doc.nodes, &mut warnings);

    ValidationResult { errors, warnings }
}

fn collect_names(
    nodes: &[Node],
    vars: &mut HashSet<String>,
    validators: &mut HashSet<String>,
    tasks: &mut HashSet<String>,
) {
    for node in nodes {
        match node {
            Node::Variable(v) => {
                vars.insert(v.name.clone());
            }
            Node::Validate(v) => {
                validators.insert(v.name.clone());
                collect_names(&v.children, vars, validators, tasks);
            }
            Node::Task(t) => {
                if let Some(label) = &t.label {
                    tasks.insert(label.clone());
                }
                tasks.insert(t.text.clone());
                collect_names(&t.children, vars, validators, tasks);
            }
            Node::Group(g) => collect_names(&g.children, vars, validators, tasks),
            Node::Conditional(c) => collect_names(&c.children, vars, validators, tasks),
            Node::LatticeValidates(lv) => collect_names(&lv.children, vars, validators, tasks),
            Node::LatticeConstraint(lc) => collect_names(&lc.children, vars, validators, tasks),
            Node::LatticeSchema(ls) => collect_names(&ls.children, vars, validators, tasks),
            Node::LatticeFrontier(lf) => collect_names(&lf.children, vars, validators, tasks),
            Node::UnitCell(uc) => collect_names(&uc.children, vars, validators, tasks),
            Node::Symmetry(sy) => collect_names(&sy.children, vars, validators, tasks),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_nodes(
    nodes: &[Node],
    schemas: &SchemaRegistry,
    errors: &mut Vec<ValidationError>,
    warnings: &mut Vec<String>,
    path: &[String],
    known_vars: &HashSet<String>,
    known_validators: &HashSet<String>,
    known_tasks: &HashSet<String>,
) {
    let mut seq_tracker: Option<u32> = None;

    for node in nodes {
        match node {
            Node::Group(g) => {
                let mut child_path = path.to_vec();
                child_path.push(g.name.clone());
                validate_nodes(
                    &g.children,
                    schemas,
                    errors,
                    warnings,
                    &child_path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::Task(t) => {
                if t.text.trim().is_empty() {
                    errors.push(ValidationError {
                        code: kind_to_code("empty_task"),
                        kind: "empty_task".into(),
                        message: "Task has no text".into(),
                        context: Some(format!("in {}", path.join(" > "))),
                        line: 0,
                        col: 0,
                    });
                }

                if let Some(seq) = t.marker.seq {
                    if let Some(prev) = seq_tracker {
                        if seq != prev + 1 && seq != 1 {
                            warnings.push(format!(
                                "Sequential task gap: [{}] follows [{}] in {}",
                                seq,
                                prev,
                                path.join(" > ")
                            ));
                        }
                    }
                    seq_tracker = Some(seq);
                }

                for gate in &t.gates {
                    validate_gate_ref(&gate.name, known_validators, known_tasks, warnings);
                }

                validate_nodes(
                    &t.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::Define(d) => {
                if let Some(scope) = &d.from_scope {
                    if !scope.starts_with('@')
                        && !scope.starts_with("~/")
                        && !scope.starts_with("./")
                    {
                        warnings.push(format!(
                            "Define @{} from_scope '{}' has unusual path syntax",
                            d.entity, scope
                        ));
                    }
                }
            }

            Node::Mutate(m) => {
                let result = schemas.validate_mutation(&m.entity, &m.fields);
                for msg in result.errors {
                    errors.push(ValidationError {
                        code: kind_to_code("invalid_mutation"),
                        kind: "invalid_mutation".into(),
                        message: msg,
                        context: Some(format!("mutate:@{}", m.entity)),
                        line: 0,
                        col: 0,
                    });
                }
                for msg in result.warnings {
                    warnings.push(msg);
                }
            }

            Node::Delete(d) => {
                if !schemas.entities.contains_key(&d.entity) {
                    warnings.push(format!("Delete targets unknown entity @{}", d.entity));
                }
            }

            Node::Query(q) => {
                if !schemas.entities.contains_key(&q.entity) {
                    // Hard error: query on entity with no schema definition
                    errors.push(ValidationError {
                        code: kind_to_code("unknown_query_entity"),
                        kind: "unknown_query_entity".into(),
                        message: format!("Query references unknown entity @{}", q.entity),
                        context: Some(format!("? @{}", q.entity)),
                        line: 0,
                        col: 0,
                    });
                } else {
                    let entity_schema = &schemas.entities[&q.entity];
                    let field_names: HashSet<&str> = entity_schema
                        .fields
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect();

                    // Hard error: filter on unknown field
                    if let Some(filter) = &q.filter {
                        for field in extract_filter_fields(filter) {
                            if !field_names.contains(field.as_str()) {
                                errors.push(ValidationError {
                                    code: kind_to_code("unknown_query_field"),
                                    kind: "unknown_query_field".into(),
                                    message: format!(
                                        "Query filter on unknown field '{}' for @{}",
                                        field, q.entity
                                    ),
                                    context: Some(format!("? @{} where {}", q.entity, filter)),
                                    line: 0,
                                    col: 0,
                                });
                            }
                        }
                    }

                    // Hard error: sort on unknown field
                    if let Some(sort_field) = &q.sort {
                        let field = sort_field
                            .trim_start_matches('+')
                            .trim_start_matches('-')
                            .trim();
                        if !field_names.contains(field) {
                            errors.push(ValidationError {
                                code: kind_to_code("unknown_sort_field"),
                                kind: "unknown_sort_field".into(),
                                message: format!(
                                    "Query sort on unknown field '{}' for @{}",
                                    field, q.entity
                                ),
                                context: Some(format!("? @{} order by {}", q.entity, field)),
                                line: 0,
                                col: 0,
                            });
                        }
                    }
                }
            }

            Node::Variable(v) => {
                let scoped = format!("{}:{}", path.join(":"), v.name);
                if known_vars.contains(&scoped) && !path.is_empty() {
                    warnings.push(format!(
                        "Variable '{}' may shadow a parent variable",
                        v.name
                    ));
                }
            }

            Node::Flow(f) => {
                for edge in &f.edges {
                    for node_name in edge.from.iter().chain(edge.to.iter()) {
                        if node_name.is_empty() {
                            errors.push(ValidationError {
                                code: kind_to_code("empty_flow_node"),
                                kind: "empty_flow_node".into(),
                                message: "Flow edge references empty node".into(),
                                context: None,
                                line: 0,
                                col: 0,
                            });
                        }
                    }
                }
            }

            Node::States(s) => {
                let mut all_states: HashSet<&str> = HashSet::new();
                for edge in &s.transitions {
                    for name in edge.from.iter().chain(edge.to.iter()) {
                        all_states.insert(name.as_str());
                    }
                }
                for edge in &s.transitions {
                    for name in edge.from.iter().chain(edge.to.iter()) {
                        if name.is_empty() {
                            errors.push(ValidationError {
                                code: kind_to_code("empty_state"),
                                kind: "empty_state".into(),
                                message: "State transition references empty state".into(),
                                context: None,
                                line: 0,
                                col: 0,
                            });
                        }
                    }
                }
            }

            Node::Validate(v) => {
                let has_flow = v.children.iter().any(|c| matches!(c, Node::Flow(_)));
                let has_tasks = v.children.iter().any(|c| matches!(c, Node::Task(_)));

                if !has_tasks {
                    warnings.push(format!("Validator '{}' has no tasks", v.name));
                }
                if has_tasks && !has_flow {
                    warnings.push(format!(
                        "Validator '{}' has tasks but no flow: block",
                        v.name
                    ));
                }
                validate_nodes(
                    &v.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::Escalate(e) => {
                if !known_validators.contains(&e.target) && !known_tasks.contains(&e.target) {
                    warnings.push(format!(
                        "Escalate target '{}' not found as validator or task",
                        e.target
                    ));
                }
            }

            Node::FilesDef(f) => {
                if f.paths.is_empty() {
                    warnings.push("files: block has no paths".into());
                }
            }

            Node::PolicyDef(p) => {
                for rule in &p.rules {
                    for gate in &rule.gates {
                        validate_gate_ref(&gate.name, known_validators, known_tasks, warnings);
                    }
                }
            }

            Node::Conditional(c) => {
                validate_nodes(
                    &c.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::LatticeValidates(lv) => {
                validate_nodes(
                    &lv.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::LatticeConstraint(lc) => {
                if lc.rule.is_empty() && lc.children.is_empty() {
                    warnings.push("LatticeConstraint has no rule or children".into());
                }
                validate_nodes(
                    &lc.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::LatticeSchema(ls) => {
                validate_nodes(
                    &ls.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::LatticeFrontier(lf) => {
                validate_nodes(
                    &lf.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::UnitCell(uc) => {
                validate_nodes(
                    &uc.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            Node::Symmetry(sy) => {
                validate_nodes(
                    &sy.children,
                    schemas,
                    errors,
                    warnings,
                    path,
                    known_vars,
                    known_validators,
                    known_tasks,
                );
            }

            _ => {}
        }
    }
}

/// Extract the `kind` value from a Group's children.
/// The parser renders `kind: GATE` as a Prose child node with text "kind: GATE".
/// Also check atoms for `:kind:VALUE` syntax.
fn extract_construct_kind(group: &Group) -> Option<String> {
    // Check atoms first (`:kind:GATE` syntax)
    if let Some(atom) = group.atoms.iter().find(|a| a.name == "kind") {
        return atom.value.clone();
    }
    // Check prose children for `kind: VALUE` lines
    for child in &group.children {
        if let Node::Prose(p) = child {
            let trimmed = p.text.trim();
            if let Some(rest) = trimmed.strip_prefix("kind:") {
                let val = rest.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Collect section names that belong to a depth-1 group.
/// Sections are either children of the group OR sibling Groups with depth > 1
/// that appear after this group and before the next depth-1 group.
fn collect_section_names<'a>(
    group: &'a Group,
    nodes: &'a [Node],
    group_index: usize,
) -> Vec<&'a str> {
    let mut names: Vec<&str> = Vec::new();

    // Check children first
    for child in &group.children {
        if let Node::Group(cg) = child {
            names.push(cg.name.as_str());
        }
    }

    // Check sibling nodes that follow this group (depth > 1, before next depth-1)
    for node in nodes.iter().skip(group_index + 1) {
        if let Node::Group(sibling) = node {
            if sibling.depth <= 1 {
                break; // Reached the next top-level group
            }
            names.push(sibling.name.as_str());
        }
    }

    names
}

/// Check lattice construct kind constraints.
/// If a top-level Group has a `kind` value matching a lattice construct type,
/// warn if the expected sections are missing from its associated sections.
fn check_lattice_construct_sections(nodes: &[Node], warnings: &mut Vec<String>) {
    for (i, node) in nodes.iter().enumerate() {
        if let Node::Group(g) = node {
            if g.depth != 1 {
                continue;
            }
            let kind = extract_construct_kind(g);
            if let Some(kind_val) = kind {
                let section_names = collect_section_names(g, nodes, i);

                match kind_val.as_str() {
                    "GATE" => {
                        if !section_names.contains(&"Validates") {
                            warnings.push(format!(
                                "Gate construct '{}' is missing a '## Validates' section",
                                g.name
                            ));
                        }
                    }
                    "BOUND" => {
                        if !section_names.contains(&"Constraint") {
                            warnings.push(format!(
                                "Bound construct '{}' is missing a '## Constraint' section",
                                g.name
                            ));
                        }
                    }
                    "ARTIFACT_SCHEMA" => {
                        if !section_names.contains(&"Schema") {
                            warnings.push(format!(
                                "Artifact schema construct '{}' is missing a '## Schema' section",
                                g.name
                            ));
                        }
                    }
                    "FRONTIER" => {
                        if !section_names.contains(&"Expected Schema")
                            && !section_names.contains(&"Exploration Strategy")
                        {
                            warnings.push(format!(
                                "Frontier construct '{}' is missing '## Expected Schema' or '## Exploration Strategy' section",
                                g.name
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn collect_refs_and_deletes(
    nodes: &[Node],
    deleted: &mut HashSet<(String, String)>,
    refs_used: &mut Vec<(String, String, String)>,
) {
    for node in nodes {
        match node {
            Node::Delete(d) => {
                deleted.insert((d.entity.clone(), d.id.clone()));
            }
            Node::Mutate(m) => {
                for (field_name, field_value) in &m.fields {
                    if let Some(rest) = field_value.strip_prefix('@') {
                        if let Some((entity, id)) = rest.split_once(':') {
                            if !entity.is_empty() && !id.is_empty() {
                                let context = format!(
                                    "mutate:@{}:{} field '{}'",
                                    m.entity,
                                    m.id.as_deref().unwrap_or("?"),
                                    field_name,
                                );
                                refs_used.push((entity.to_string(), id.to_string(), context));
                            }
                        }
                    }
                }
            }
            Node::Group(g) => collect_refs_and_deletes(&g.children, deleted, refs_used),
            Node::Validate(v) => collect_refs_and_deletes(&v.children, deleted, refs_used),
            Node::Conditional(c) => collect_refs_and_deletes(&c.children, deleted, refs_used),
            Node::LatticeValidates(lv) => {
                collect_refs_and_deletes(&lv.children, deleted, refs_used)
            }
            Node::LatticeConstraint(lc) => {
                collect_refs_and_deletes(&lc.children, deleted, refs_used)
            }
            Node::LatticeSchema(ls) => collect_refs_and_deletes(&ls.children, deleted, refs_used),
            Node::LatticeFrontier(lf) => collect_refs_and_deletes(&lf.children, deleted, refs_used),
            Node::UnitCell(uc) => collect_refs_and_deletes(&uc.children, deleted, refs_used),
            Node::Symmetry(sy) => collect_refs_and_deletes(&sy.children, deleted, refs_used),
            _ => {}
        }
    }
}

fn check_referential_integrity(nodes: &[Node], warnings: &mut Vec<String>) {
    let mut deleted: HashSet<(String, String)> = HashSet::new();
    let mut refs_used: Vec<(String, String, String)> = Vec::new();

    collect_refs_and_deletes(nodes, &mut deleted, &mut refs_used);

    for (entity, id, context) in &refs_used {
        if deleted.contains(&(entity.clone(), id.clone())) {
            warnings.push(format!(
                "Reference to @{}:{} may be dangling (entity is deleted) in {}",
                entity, id, context
            ));
        }
    }
}

/// Extract field names referenced in a filter expression like "status=open and points>3".
fn extract_filter_fields(filter: &str) -> Vec<String> {
    let ops = ["!~", "~~", "~=", "!=", ">=", "<=", "=", ">", "<"];
    let mut fields = Vec::new();

    for or_clause in filter.split(" or ") {
        for and_clause in or_clause.split(" and ") {
            let clause = and_clause.trim();
            for op in ops {
                if let Some(pos) = clause.find(op) {
                    let field = clause[..pos].trim().to_string();
                    if !field.is_empty() && !fields.contains(&field) {
                        fields.push(field);
                    }
                    break;
                }
            }
        }
    }

    fields
}

fn validate_gate_ref(
    name: &str,
    known_validators: &HashSet<String>,
    known_tasks: &HashSet<String>,
    warnings: &mut Vec<String>,
) {
    let gate_name = name.split_whitespace().next().unwrap_or(name);
    if gate_name.starts_with("when")
        || gate_name.starts_with("unless")
        || gate_name.starts_with("after")
        || gate_name.starts_with("needs")
        || gate_name.starts_with("all")
        || gate_name.starts_with("any")
        || gate_name.starts_with("not")
        || gate_name.starts_with("intake")
    {
        return;
    }
    if !known_validators.contains(gate_name) && !known_tasks.contains(gate_name) {
        warnings.push(format!(
            "Gate '{{{}}}' references unknown validator/task",
            gate_name
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::schema::SchemaRegistry;

    fn validate_src(src: &str) -> ValidationResult {
        let doc = parse::parse(src).expect("parse failed");
        let mut schemas = SchemaRegistry::new();
        schemas.extract_from_doc(&doc);
        validate(&doc, &schemas)
    }

    #[test]
    fn valid_simple_doc() {
        let result = validate_src("# Project\n\n    [!] Do something");
        assert!(result.valid());
    }

    #[test]
    fn empty_task_text_error() {
        let result = validate_src("[!]  ");
        assert!(!result.valid());
        assert!(result.errors.iter().any(|e| e.kind == "empty_task"));
    }

    #[test]
    fn unknown_entity_mutation_error() {
        let src = "mutate:@Bug:b1\n    title: Crash";
        let result = validate_src(src);
        assert!(result.errors.iter().any(|e| e.kind == "invalid_mutation"));
    }

    #[test]
    fn valid_mutation_with_schema() {
        let src = "define:@Task\n    title: \"\"\n\nmutate:@Task:t1\n    title: Ship";
        let result = validate_src(src);
        let mutation_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.kind == "invalid_mutation")
            .collect();
        assert!(mutation_errors.is_empty());
    }

    #[test]
    fn unknown_field_mutation_error() {
        let src = "define:@Task\n    title: \"\"\n\nmutate:@Task:t1\n    nonexistent: foo";
        let result = validate_src(src);
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == "invalid_mutation" && e.message.contains("nonexistent")));
    }

    #[test]
    fn unknown_entity_delete_warning() {
        let result = validate_src("delete:@NonExistent:x");
        assert!(result.warnings.iter().any(|w| w.contains("unknown entity")));
    }

    #[test]
    fn unknown_entity_query_error() {
        let result = validate_src("? NonExistent where x=1");
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == "unknown_query_entity"));
    }

    #[test]
    fn dangling_ref_after_delete_warning() {
        let doc = Document {
            nodes: vec![
                Node::Delete(Delete {
                    entity: "User".to_string(),
                    id: "u1".to_string(),
                    mod_scope: None,
                    workspace_scope: None,
                }),
                Node::Mutate(Mutate {
                    entity: "Task".to_string(),
                    id: Some("t1".to_string()),
                    gate: None,
                    fields: vec![
                        ("assignee".to_string(), "@User:u1".to_string()),
                        ("title".to_string(), "Ship it".to_string()),
                    ],
                    batch: None,
                    mod_scope: None,
                    workspace_scope: None,
                }),
            ], ..Default::default()
        };
        let schemas = SchemaRegistry::new();
        let result = validate(&doc, &schemas);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("@User:u1") && w.contains("dangling")));
    }

    #[test]
    fn no_dangling_ref_without_delete() {
        let doc = Document {
            nodes: vec![Node::Mutate(Mutate {
                entity: "Task".to_string(),
                id: Some("t1".to_string()),
                gate: None,
                fields: vec![("assignee".to_string(), "@User:u1".to_string())],
                batch: None,
                mod_scope: None,
                workspace_scope: None,
            })], ..Default::default()
        };
        let schemas = SchemaRegistry::new();
        let result = validate(&doc, &schemas);
        assert!(!result.warnings.iter().any(|w| w.contains("dangling")));
    }

    #[test]
    fn flow_empty_node_error() {
        let doc = Document {
            nodes: vec![Node::Flow(Flow {
                name: None,
                edges: vec![FlowEdge {
                    from: vec!["A".to_string()],
                    to: vec!["".to_string()],
                    label: None,
                    parallel: false,
                    gate: None,
                    wait: None,
                    timeout: None,
                }],
            })], ..Default::default()
        };
        let schemas = SchemaRegistry::new();
        let result = validate(&doc, &schemas);
        assert!(result.errors.iter().any(|e| e.kind == "empty_flow_node"));
    }

    #[test]
    fn validator_no_tasks_warning() {
        let result = validate_src("validate empty:\n    Some prose content");
        assert!(result.warnings.iter().any(|w| w.contains("no tasks")));
    }

    #[test]
    fn files_def_empty_warning() {
        let doc = Document {
            nodes: vec![Node::FilesDef(FilesDef { paths: vec![] })], ..Default::default()
        };
        let schemas = SchemaRegistry::new();
        let result = validate(&doc, &schemas);
        assert!(result.warnings.iter().any(|w| w.contains("no paths")));
    }

    #[test]
    fn list_op_on_non_list_field_surfaces_warning() {
        let src = "define:@Task\n    title: \"\"\n\nmutate:@Task:t1\n    title: +[\"extra\"]";
        let result = validate_src(src);
        let mutation_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.kind == "invalid_mutation")
            .collect();
        assert!(mutation_errors.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("List operator on non-list field")));
    }

    #[test]
    fn type_mismatch_surfaces_warning() {
        let src = "define:@Task\n    count: 0\n\nmutate:@Task:t1\n    count: not-a-number";
        let result = validate_src(src);
        let mutation_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.kind == "invalid_mutation")
            .collect();
        assert!(mutation_errors.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("Type mismatch")));
    }

    #[test]
    fn query_filter_unknown_field_error() {
        let src = "define:@Task\n    title: \"\"\n    status: \"\"\n\n? Task where bogus=open";
        let result = validate_src(src);
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == "unknown_query_field" && e.message.contains("bogus")));
    }

    #[test]
    fn query_filter_known_field_ok() {
        let src = "define:@Task\n    title: \"\"\n    status: \"\"\n\n? Task where status=open";
        let result = validate_src(src);
        assert!(!result
            .errors
            .iter()
            .any(|e| e.kind == "unknown_query_field"));
    }

    #[test]
    fn extract_filter_fields_basic() {
        let fields = extract_filter_fields("status=open and points>3");
        assert!(fields.contains(&"status".to_string()));
        assert!(fields.contains(&"points".to_string()));
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn extract_filter_fields_or_clause() {
        let fields = extract_filter_fields("status=open or status=in_progress");
        assert!(fields.contains(&"status".to_string()));
        assert_eq!(fields.len(), 1);
    }

    // ── validate_gate_ref ──

    #[test]
    fn gate_ref_when_no_warning() {
        let mut warnings = Vec::new();
        validate_gate_ref(
            "when count > 0",
            &HashSet::new(),
            &HashSet::new(),
            &mut warnings,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn gate_ref_unknown_warns() {
        let mut warnings = Vec::new();
        validate_gate_ref(
            "unknown-gate",
            &HashSet::new(),
            &HashSet::new(),
            &mut warnings,
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown-gate"));
    }

    #[test]
    fn gate_ref_known_validator_no_warning() {
        let mut warnings = Vec::new();
        let validators: HashSet<String> = ["code-review".to_string()].into();
        validate_gate_ref("code-review", &validators, &HashSet::new(), &mut warnings);
        assert!(warnings.is_empty());
    }

    // ── ValidationResult::valid ──

    #[test]
    fn validation_result_valid_when_no_errors() {
        let result = ValidationResult {
            errors: vec![],
            warnings: vec!["a warning".to_string()],
        };
        assert!(result.valid());
    }

    #[test]
    fn validation_result_invalid_with_errors() {
        let result = ValidationResult {
            errors: vec![ValidationError {
                code: "E_VALIDATION_ERROR".to_string(),
                kind: "test".to_string(),
                message: "bad".to_string(),
                context: None,
                line: 0,
                col: 0,
            }],
            warnings: vec![],
        };
        assert!(!result.valid());
    }

    #[test]
    fn kind_to_code_covers_all_known_kinds() {
        assert_eq!(kind_to_code("empty_task"), "E_EMPTY_TASK");
        assert_eq!(kind_to_code("invalid_mutation"), "E_INVALID_MUTATION");
        assert_eq!(kind_to_code("unknown_query_entity"), "E_UNKNOWN_ENTITY");
        assert_eq!(kind_to_code("unknown_query_field"), "E_UNKNOWN_FIELD");
        assert_eq!(kind_to_code("empty_flow_node"), "E_EMPTY_FLOW_NODE");
        assert_eq!(kind_to_code("unknown_sort_field"), "E_UNKNOWN_SORT_FIELD");
        assert_eq!(kind_to_code("empty_state"), "E_EMPTY_STATE");
        assert_eq!(kind_to_code("anything_else"), "E_VALIDATION_ERROR");
    }
}
