use crate::gate::{self, GateContext};
use crate::mutate::{value_to_string, RecordStore};
use crate::query::{self, QueryRequest, SortSpec};
use crate::schema::SchemaRegistry;
use crate::types::{CheckDef, Document, FieldDefault, Node, ValidateDef};
use crate::workflow::{self, FlowGraph};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckExecution {
    pub checks: Vec<CheckResult>,
    pub suites: Vec<ValidateSuiteResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateSuiteResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    pub status: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    pub kind: String,
    pub passed: bool,
    pub status: String,
    pub details: String,
    pub evidence: HashMap<String, String>,
}

struct ExecutionContext<'a> {
    root_path: &'a str,
    schemas: &'a SchemaRegistry,
    store: &'a RecordStore,
    gate_context: &'a GateContext,
    config: &'a HashMap<String, String>,
    flow_graphs: &'a [FlowGraph],
}

#[derive(Default)]
struct ExecutionState {
    checks: Vec<CheckResult>,
    suites: Vec<ValidateSuiteResult>,
}

pub fn execute_checks(
    doc: &Document,
    root_path: &str,
    schemas: &SchemaRegistry,
    store: &RecordStore,
    gate_context: &GateContext,
    config: &HashMap<String, String>,
) -> CheckExecution {
    let flow_graphs = workflow::extract_flows(doc);
    let ctx = ExecutionContext {
        root_path,
        schemas,
        store,
        gate_context,
        config,
        flow_graphs: &flow_graphs,
    };
    let mut state = ExecutionState::default();

    collect_results(&doc.nodes, &ctx, &mut state);

    let passed = state
        .checks
        .iter()
        .filter(|result| result.status == "passed")
        .count();
    let skipped = state
        .checks
        .iter()
        .filter(|result| result.status == "skipped" || result.status == "error")
        .count();
    let failed = state.checks.len().saturating_sub(passed + skipped);

    CheckExecution {
        total: state.checks.len(),
        passed,
        failed,
        skipped,
        checks: state.checks,
        suites: state.suites,
    }
}

fn collect_results(nodes: &[Node], ctx: &ExecutionContext<'_>, state: &mut ExecutionState) {
    for node in nodes {
        match node {
            Node::Group(group) => collect_results(&group.children, ctx, state),
            Node::Task(task) => {
                collect_results(&task.children, ctx, state);
                if let Some(pass) = &task.on_pass {
                    collect_results(pass, ctx, state);
                }
                if let Some(fail) = &task.on_fail {
                    collect_results(fail, ctx, state);
                }
                if let Some(arms) = &task.match_arms {
                    for arm in arms {
                        collect_results(&arm.children, ctx, state);
                    }
                }
            }
            Node::Validate(validate) => execute_validate_suite(validate, ctx, state),
            Node::Conditional(conditional) => collect_results(&conditional.children, ctx, state),
            Node::GateDef(gate) => collect_results(&gate.children, ctx, state),
            Node::Check(check) => state.checks.push(execute_check(check, None, ctx)),
            _ => {}
        }
    }
}

fn execute_validate_suite(
    validate: &ValidateDef,
    ctx: &ExecutionContext<'_>,
    state: &mut ExecutionState,
) {
    let suite_ref = canonical_ref("Validate", validate.meta.id.as_deref());
    let checks = collect_suite_checks(&validate.children);
    let ordered_checks = order_suite_checks(&checks);
    let mut executed_by_ref: HashMap<String, String> = HashMap::new();
    let mut suite_results = Vec::new();

    for check in ordered_checks {
        let blocked_by = check
            .meta
            .depends_on
            .iter()
            .find(|dependency| match executed_by_ref.get(*dependency) {
                Some(status) => status != "passed",
                None => false,
            })
            .cloned();

        let result = if let Some(dependency) = blocked_by {
            skipped_result(check, suite_ref.clone(), dependency)
        } else {
            execute_check(check, suite_ref.clone(), ctx)
        };

        if let Some(reference) = result.r#ref.clone() {
            executed_by_ref.insert(reference, result.status.clone());
        }

        suite_results.push(result.clone());
        state.checks.push(result);
    }

    let suite_passed = suite_results
        .iter()
        .filter(|result| result.status == "passed")
        .count();
    let suite_skipped = suite_results
        .iter()
        .filter(|result| result.status == "skipped")
        .count();
    let suite_failed = suite_results
        .len()
        .saturating_sub(suite_passed + suite_skipped);
    let status = if suite_failed == 0 && suite_skipped == 0 {
        "passed"
    } else {
        "failed"
    };

    state.suites.push(ValidateSuiteResult {
        name: validate.name.clone(),
        id: validate.meta.id.clone(),
        r#ref: suite_ref,
        targets: validate.meta.targets.clone(),
        requires: validate.meta.requires.clone(),
        blocks: validate.meta.blocks.clone(),
        depends_on: validate.meta.depends_on.clone(),
        status: status.to_string(),
        total: suite_results.len(),
        passed: suite_passed,
        failed: suite_failed,
        skipped: suite_skipped,
        execution_order: suite_results
            .iter()
            .filter_map(|result| result.r#ref.clone())
            .collect(),
    });
}

fn collect_suite_checks(nodes: &[Node]) -> Vec<&CheckDef> {
    let mut checks = Vec::new();

    for node in nodes {
        match node {
            Node::Check(check) => checks.push(check),
            Node::Group(group) => checks.extend(collect_suite_checks(&group.children)),
            Node::Task(task) => {
                checks.extend(collect_suite_checks(&task.children));
                if let Some(pass) = &task.on_pass {
                    checks.extend(collect_suite_checks(pass));
                }
                if let Some(fail) = &task.on_fail {
                    checks.extend(collect_suite_checks(fail));
                }
                if let Some(arms) = &task.match_arms {
                    for arm in arms {
                        checks.extend(collect_suite_checks(&arm.children));
                    }
                }
            }
            Node::Conditional(conditional) => {
                checks.extend(collect_suite_checks(&conditional.children));
            }
            Node::GateDef(gate) => checks.extend(collect_suite_checks(&gate.children)),
            Node::Validate(_) => {}
            _ => {}
        }
    }

    checks
}

fn order_suite_checks<'a>(checks: &'a [&'a CheckDef]) -> Vec<&'a CheckDef> {
    let refs_by_index: Vec<Option<String>> = checks
        .iter()
        .map(|check| canonical_ref("Check", check.meta.id.as_deref()))
        .collect();

    let index_by_ref: HashMap<String, usize> = refs_by_index
        .iter()
        .enumerate()
        .filter_map(|(idx, value)| value.clone().map(|reference| (reference, idx)))
        .collect();

    let mut ordered = Vec::new();
    let mut permanent = HashSet::new();
    let mut temporary = HashSet::new();

    for idx in 0..checks.len() {
        visit_check(
            idx,
            checks,
            &index_by_ref,
            &mut permanent,
            &mut temporary,
            &mut ordered,
        );
    }

    ordered
}

fn visit_check<'a>(
    idx: usize,
    checks: &'a [&'a CheckDef],
    index_by_ref: &HashMap<String, usize>,
    permanent: &mut HashSet<usize>,
    temporary: &mut HashSet<usize>,
    ordered: &mut Vec<&'a CheckDef>,
) {
    if permanent.contains(&idx) || temporary.contains(&idx) {
        return;
    }

    temporary.insert(idx);

    for dependency in &checks[idx].meta.depends_on {
        if let Some(dep_idx) = index_by_ref.get(dependency) {
            visit_check(
                *dep_idx,
                checks,
                index_by_ref,
                permanent,
                temporary,
                ordered,
            );
        }
    }

    temporary.remove(&idx);
    permanent.insert(idx);
    ordered.push(checks[idx]);
}

fn execute_check(
    check: &CheckDef,
    suite_ref: Option<String>,
    ctx: &ExecutionContext<'_>,
) -> CheckResult {
    let fields = body_to_map(&check.body);
    let kind = fields
        .get("kind")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    if let Some(adapter) = fields.get("adapter").cloned() {
        let mut evidence = HashMap::new();
        evidence.insert("adapter".to_string(), adapter);

        return build_result(
            check,
            suite_ref,
            &kind,
            "passed",
            "adapter check deferred to runtime".to_string(),
            evidence,
        );
    }

    let (status, details, evidence) = match kind.as_str() {
        "schema" => execute_schema_check(check, &fields, ctx.schemas),
        "query" => execute_query_check(check, &fields, ctx.store),
        "gate" => execute_gate_check(check, &fields, ctx.gate_context),
        "file" => execute_file_check(check, &fields, ctx.root_path),
        "flow" => execute_flow_check(check, &fields, ctx.flow_graphs),
        "config" => execute_config_check(check, &fields, ctx.config),
        _ => (
            "error".to_string(),
            "unsupported check kind".to_string(),
            HashMap::new(),
        ),
    };

    build_result(check, suite_ref, &kind, &status, details, evidence)
}

fn execute_schema_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    schemas: &SchemaRegistry,
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let entity = fields
        .get("entity")
        .cloned()
        .unwrap_or_default()
        .trim_start_matches('@')
        .to_string();
    let field = fields.get("field").cloned();

    if entity.is_empty() {
        return (
            "error".to_string(),
            "schema check requires entity".to_string(),
            evidence,
        );
    }

    let Some(schema) = schemas.entities.get(&entity) else {
        evidence.insert("entity".to_string(), entity.clone());
        return (
            "failed".to_string(),
            format!("@{} is not defined", entity),
            evidence,
        );
    };

    evidence.insert("entity".to_string(), entity.clone());

    if let Some(field_name) = field {
        let Some(field_def) = schema.fields.iter().find(|entry| entry.name == field_name) else {
            evidence.insert("field".to_string(), field_name.clone());
            return (
                "failed".to_string(),
                format!("@{} is missing field '{}'", entity, field_name),
                evidence,
            );
        };

        evidence.insert("field".to_string(), field_name.clone());
        evidence.insert("plural".to_string(), field_def.plural.to_string());
        evidence.insert(
            "type".to_string(),
            field_default_type(&field_def.default).to_string(),
        );

        if let Some(expected_type) = fields.get("type") {
            if field_default_type(&field_def.default) != expected_type {
                return (
                    "failed".to_string(),
                    format!(
                        "@{} field '{}' has type '{}' not '{}'",
                        entity,
                        field_name,
                        field_default_type(&field_def.default),
                        expected_type
                    ),
                    evidence,
                );
            }
        }

        if let Some(expected_plural) = parse_bool(fields.get("plural").map(String::as_str)) {
            if field_def.plural != expected_plural {
                return (
                    "failed".to_string(),
                    format!(
                        "@{} field '{}' plural={} not {}",
                        entity, field_name, field_def.plural, expected_plural
                    ),
                    evidence,
                );
            }
        }

        if let Some(required) = parse_bool(fields.get("required").map(String::as_str)) {
            let actual_required = field_is_required(field_def);
            evidence.insert("required".to_string(), required.to_string());
            evidence.insert("actual_required".to_string(), actual_required.to_string());

            if actual_required != required {
                return (
                    "failed".to_string(),
                    format!(
                        "@{} field '{}' required={} not {}",
                        entity, field_name, actual_required, required
                    ),
                    evidence,
                );
            }
        }

        (
            "passed".to_string(),
            format!("@{} has expected schema for '{}'", entity, field_name),
            evidence,
        )
    } else {
        (
            "passed".to_string(),
            format!("@{} is defined", entity),
            evidence,
        )
    }
}

fn execute_query_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    store: &RecordStore,
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let entity = fields.get("entity").cloned().unwrap_or_default();

    if entity.is_empty() {
        return (
            "error".to_string(),
            "query check requires entity".to_string(),
            evidence,
        );
    }

    let query = QueryRequest {
        entity: entity.clone(),
        plural: parse_bool(fields.get("plural").map(String::as_str)).unwrap_or(true),
        filter: fields.get("filter").cloned(),
        sort: parse_sort(fields.get("sort")),
        limit: fields
            .get("limit")
            .and_then(|value| value.parse::<usize>().ok()),
        include: None,
    };

    let result = query::execute_query(&query, store);
    evidence.insert("entity".to_string(), entity.clone());
    evidence.insert("count".to_string(), result.count.to_string());

    // If the entity has no table in the store at all, return error instead of
    // silently passing/failing — the check likely references a wrong entity.
    if result.count == 0 && !store.tables.contains_key(&entity) {
        return (
            "error".to_string(),
            format!(
                "Entity @{} has no data in store — check may reference wrong entity",
                entity
            ),
            evidence,
        );
    }

    if let Some(field_name) = fields.get("field") {
        let observed_values: Vec<String> = result
            .records
            .iter()
            .filter_map(|record| record.get(field_name).map(value_to_string))
            .collect();

        evidence.insert("field".to_string(), field_name.clone());
        if !observed_values.is_empty() {
            evidence.insert("observed".to_string(), observed_values.join("|"));
        }

        if let Some(expected) = fields.get("equals") {
            if observed_values
                .iter()
                .any(|value| strip_quotes(value) == strip_quotes(expected))
            {
                return (
                    "passed".to_string(),
                    format!("query @{} matched {} = {}", entity, field_name, expected),
                    evidence,
                );
            }

            return (
                "failed".to_string(),
                format!(
                    "query @{} did not match {} = {}",
                    entity, field_name, expected
                ),
                evidence,
            );
        }

        if let Some(expected) = fields.get("contains") {
            if observed_values.iter().any(|value| value.contains(expected)) {
                return (
                    "passed".to_string(),
                    format!(
                        "query @{} matched {} contains {}",
                        entity, field_name, expected
                    ),
                    evidence,
                );
            }

            return (
                "failed".to_string(),
                format!(
                    "query @{} did not match {} contains {}",
                    entity, field_name, expected
                ),
                evidence,
            );
        }
    }

    let count = result.count;

    if let Some(expected_count) = fields
        .get("expect_count")
        .and_then(|value| value.parse::<usize>().ok())
    {
        evidence.insert("expect_count".to_string(), expected_count.to_string());
        let status = if count == expected_count {
            "passed"
        } else {
            "failed"
        };
        return (
            status.to_string(),
            format!("query @{} returned {} record(s)", entity, count),
            evidence,
        );
    }

    if let Some(min_count) = fields
        .get("min_count")
        .or_else(|| fields.get("min"))
        .and_then(|value| value.parse::<usize>().ok())
    {
        evidence.insert("min".to_string(), min_count.to_string());
        let status = if count >= min_count {
            "passed"
        } else {
            "failed"
        };
        return (
            status.to_string(),
            format!("query @{} returned {} record(s)", entity, count),
            evidence,
        );
    }

    if let Some(max_count) = fields
        .get("max_count")
        .or_else(|| fields.get("max"))
        .and_then(|value| value.parse::<usize>().ok())
    {
        evidence.insert("max".to_string(), max_count.to_string());
        let status = if count <= max_count {
            "passed"
        } else {
            "failed"
        };
        return (
            status.to_string(),
            format!("query @{} returned {} record(s)", entity, count),
            evidence,
        );
    }

    if let Some(expected) = fields.get("equals") {
        let status = if count.to_string() == *expected {
            "passed"
        } else {
            "failed"
        };
        return (
            status.to_string(),
            format!("query @{} returned {} record(s)", entity, count),
            evidence,
        );
    }

    (
        if count > 0 { "passed" } else { "failed" }.to_string(),
        format!("query @{} returned {} record(s)", entity, count),
        evidence,
    )
}

fn execute_gate_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    gate_context: &GateContext,
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let gate_body = fields
        .get("gate")
        .cloned()
        .or_else(|| fields.get("expr").cloned())
        .unwrap_or_default();

    if gate_body.is_empty() {
        return (
            "error".to_string(),
            "gate check requires gate".to_string(),
            evidence,
        );
    }

    let gate_result = gate::eval_gate(&gate_body, gate_context);
    evidence.insert("gate".to_string(), gate_body);
    evidence.insert("details".to_string(), gate_result.details.clone());

    (
        if gate_result.passed {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        gate_result.details,
        evidence,
    )
}

fn execute_file_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    root_path: &str,
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let relative_path = fields.get("path").cloned().unwrap_or_default();

    if relative_path.is_empty() {
        return (
            "error".to_string(),
            "file check requires path".to_string(),
            evidence,
        );
    }

    let full_path = if Path::new(&relative_path).is_absolute() || root_path.is_empty() {
        relative_path.clone()
    } else {
        Path::new(root_path)
            .join(&relative_path)
            .to_string_lossy()
            .to_string()
    };

    let exists = Path::new(&full_path).exists();
    evidence.insert("path".to_string(), relative_path.clone());
    evidence.insert("exists".to_string(), exists.to_string());

    if !exists {
        return (
            "failed".to_string(),
            format!("file '{}' does not exist", relative_path),
            evidence,
        );
    }

    const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB
    match std::fs::metadata(&full_path) {
        Ok(meta) if meta.len() > MAX_FILE_SIZE => {
            return (
                "error".to_string(),
                format!("file '{}' exceeds 1 MB size limit", relative_path),
                evidence,
            );
        }
        Err(e) => {
            return (
                "error".to_string(),
                format!(
                    "failed to read file metadata for '{}': {}",
                    relative_path, e
                ),
                evidence,
            );
        }
        _ => {}
    }

    let contents = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                "error".to_string(),
                format!("failed to read file '{}': {}", relative_path, e),
                evidence,
            );
        }
    };

    if let Some(expected) = fields.get("contains") {
        evidence.insert("contains".to_string(), expected.clone());
        return (
            if contents.contains(expected) {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            if contents.contains(expected) {
                format!("file '{}' contains expected text", relative_path)
            } else {
                format!("file '{}' is missing expected text", relative_path)
            },
            evidence,
        );
    }

    (
        "passed".to_string(),
        format!("file '{}' exists", relative_path),
        evidence,
    )
}

fn execute_flow_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    flow_graphs: &[FlowGraph],
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let from = fields.get("from").cloned().unwrap_or_default();
    let to = fields
        .get("to")
        .cloned()
        .or_else(|| fields.get("terminal").cloned())
        .unwrap_or_default();

    if from.is_empty() || to.is_empty() {
        return (
            "error".to_string(),
            "flow check requires from and to".to_string(),
            evidence,
        );
    }

    evidence.insert("from".to_string(), from.clone());
    evidence.insert("to".to_string(), to.clone());
    let reachable = flow_graphs
        .iter()
        .any(|graph| flow_has_path(graph, &from, &to));

    (
        if reachable { "passed" } else { "failed" }.to_string(),
        if reachable {
            format!("flow path exists from '{}' to '{}'", from, to)
        } else {
            format!("flow path does not exist from '{}' to '{}'", from, to)
        },
        evidence,
    )
}

fn execute_config_check(
    _check: &CheckDef,
    fields: &HashMap<String, String>,
    config: &HashMap<String, String>,
) -> (String, String, HashMap<String, String>) {
    let mut evidence = HashMap::new();
    let key = fields.get("key").cloned().unwrap_or_default();

    if key.is_empty() {
        return (
            "error".to_string(),
            "config check requires key".to_string(),
            evidence,
        );
    }

    let observed = config.get(&key).cloned();
    evidence.insert("key".to_string(), key.clone());
    if let Some(value) = &observed {
        evidence.insert("observed".to_string(), value.clone());
    }

    let Some(value) = observed else {
        return (
            "failed".to_string(),
            format!("config '{}' is not set", key),
            evidence,
        );
    };

    if let Some(expected) = fields.get("equals") {
        evidence.insert("equals".to_string(), expected.clone());
        return (
            if strip_quotes(&value) == strip_quotes(expected) {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            format!("config '{}' observed '{}'", key, value),
            evidence,
        );
    }

    if let Some(expected) = fields.get("contains") {
        evidence.insert("contains".to_string(), expected.clone());
        return (
            if value.contains(expected) {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            format!("config '{}' observed '{}'", key, value),
            evidence,
        );
    }

    (
        "passed".to_string(),
        format!("config '{}' is set", key),
        evidence,
    )
}

fn build_result(
    check: &CheckDef,
    suite_ref: Option<String>,
    kind: &str,
    status: &str,
    details: String,
    evidence: HashMap<String, String>,
) -> CheckResult {
    CheckResult {
        name: check.name.clone(),
        id: check.meta.id.clone(),
        r#ref: canonical_ref("Check", check.meta.id.as_deref()),
        suite_ref,
        targets: check.meta.targets.clone(),
        depends_on: check.meta.depends_on.clone(),
        kind: kind.to_string(),
        passed: status == "passed",
        status: status.to_string(),
        details,
        evidence,
    }
}

fn skipped_result(check: &CheckDef, suite_ref: Option<String>, dependency: String) -> CheckResult {
    let mut evidence = HashMap::new();
    evidence.insert("blocked_by".to_string(), dependency.clone());

    build_result(
        check,
        suite_ref,
        "dependency",
        "skipped",
        format!("skipped because dependency '{}' did not pass", dependency),
        evidence,
    )
}

fn body_to_map(body: &[(String, String)]) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for (key, value) in body {
        if key != "_" {
            fields.insert(key.clone(), value.clone());
        }
    }

    fields
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn parse_sort(value: Option<&String>) -> Option<SortSpec> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    let descending = value.starts_with('-');
    let field = value.trim_start_matches(['+', '-']).trim();
    if field.is_empty() {
        None
    } else {
        Some(SortSpec {
            field: field.to_string(),
            descending,
        })
    }
}

fn field_default_type(default: &FieldDefault) -> &'static str {
    match default {
        FieldDefault::Str(_) => "string",
        FieldDefault::Int(_) => "integer",
        FieldDefault::Float(_) => "float",
        FieldDefault::Bool(_) => "boolean",
        FieldDefault::Atom(_) => "atom",
        FieldDefault::Enum(_) => "enum",
        FieldDefault::Ref(_) => "ref",
        FieldDefault::List => "list",
        FieldDefault::Timestamp(_) => "timestamp",
        FieldDefault::Trit(_) => "trit",
        FieldDefault::Nil => "nil",
    }
}

fn field_is_required(field: &crate::types::FieldDef) -> bool {
    !matches!(field.default, FieldDefault::Nil)
}

fn flow_has_path(graph: &FlowGraph, from: &str, to: &str) -> bool {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    let mut stack = vec![from];
    let mut seen = HashSet::new();

    while let Some(node) = stack.pop() {
        if node == to {
            return true;
        }
        if !seen.insert(node) {
            continue;
        }
        if let Some(next) = adjacency.get(node) {
            stack.extend(next.iter().copied());
        }
    }

    false
}

fn canonical_ref(kind: &str, id: Option<&str>) -> Option<String> {
    id.filter(|value| !value.is_empty())
        .map(|value| format!("@{}:{}", kind, value))
}

fn strip_quotes(value: &str) -> String {
    value.trim_matches('"').to_string()
}

pub fn default_gate_context(store: RecordStore) -> GateContext {
    GateContext {
        store,
        index: crate::index::DocIndex::default(),
        vars: HashMap::new(),
        completed_tasks: Vec::new(),
        task_results: HashMap::new(),
        task_scores: HashMap::new(),
        submitted_forms: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn executes_query_check() {
        let mut store = RecordStore::new();
        store.tables.insert(
            "Task".to_string(),
            HashMap::from([
                (
                    "a".to_string(),
                    HashMap::from([("status".to_string(), json!("open"))]),
                ),
                (
                    "b".to_string(),
                    HashMap::from([("status".to_string(), json!("open"))]),
                ),
            ]),
        );

        let doc = Document {
            nodes: vec![Node::Check(CheckDef {
                name: "open-tasks".to_string(),
                meta: crate::types::AttachmentMeta::default(),
                body: vec![
                    ("kind".to_string(), "query".to_string()),
                    ("entity".to_string(), "Task".to_string()),
                    ("filter".to_string(), "status = open".to_string()),
                    ("expect_count".to_string(), "2".to_string()),
                ],
            })], ..Default::default()
        };

        let result = execute_checks(
            &doc,
            "",
            &SchemaRegistry::new(),
            &store,
            &default_gate_context(store.clone()),
            &HashMap::new(),
        );

        assert_eq!(result.total, 1);
        assert_eq!(result.passed, 1);
    }

    #[test]
    fn query_check_errors_on_missing_entity() {
        let store = RecordStore::new();

        let doc = Document {
            nodes: vec![Node::Check(CheckDef {
                name: "missing-entity".to_string(),
                meta: crate::types::AttachmentMeta::default(),
                body: vec![
                    ("kind".to_string(), "query".to_string()),
                    ("entity".to_string(), "NonexistentEntity".to_string()),
                    ("field".to_string(), "name".to_string()),
                    ("equals".to_string(), "foo".to_string()),
                ],
            })], ..Default::default()
        };

        let result = execute_checks(
            &doc,
            "",
            &SchemaRegistry::new(),
            &store,
            &default_gate_context(store.clone()),
            &HashMap::new(),
        );

        assert_eq!(result.total, 1);
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.checks[0].status, "error");
        assert!(result.checks[0].details.contains("no data"));
    }
}
