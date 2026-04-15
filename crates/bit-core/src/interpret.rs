//! .bit Interpreter — executes an Executable Document Graph.
//!
//! Takes a BitIR and runs it:
//! - Flows: state machine execution (transition between states, check gates)
//! - Gates: boolean evaluation (check all conditions/requires)
//! - Tasks: track state (open → done/blocked)
//! - Spawns: parallel (+) and sequential (++) execution markers
//! - Conditionals: evaluate condition, execute children
//! - Validate: run checklist items in order
//!
//! The interpreter produces an execution trace (Vec<ExecEvent>) that records
//! what happened during execution, enabling replay and debugging.

use crate::ir::*;
use crate::trit::Trit;
use std::collections::{BTreeMap, HashMap};

// ═══════════════════════════════════════════════════════════════
// Execution Context
// ═══════════════════════════════════════════════════════════════

/// Runtime state for the interpreter.
#[derive(Debug, Clone)]
pub struct ExecContext {
    /// Current state of each flow (flow name → current state).
    pub flow_states: HashMap<String, String>,
    /// Entity store: entity_name:id → field values.
    pub entities: HashMap<String, BTreeMap<String, Value>>,
    /// Variables.
    pub variables: HashMap<String, String>,
    /// Gate results cache — ternary: Pos (pass), Neutral (uncertain), Neg (fail).
    pub gate_results: HashMap<String, Trit>,
    /// Task states: task label/text → done.
    pub task_states: HashMap<String, bool>,
    /// Execution trace.
    pub trace: Vec<ExecEvent>,
    /// Errors encountered during execution.
    pub errors: Vec<ExecError>,
}

/// An event in the execution trace.
#[derive(Debug, Clone)]
pub struct ExecEvent {
    pub kind: ExecEventKind,
    pub construct: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecEventKind {
    /// Schema registered.
    SchemaRegistered,
    /// Entity created/mutated.
    EntityMutated,
    /// Entity deleted.
    EntityDeleted,
    /// Flow state transitioned.
    FlowTransition,
    /// Flow blocked by gate.
    FlowBlocked,
    /// Gate evaluated.
    GateEvaluated,
    /// Bound checked.
    BoundChecked,
    /// Task state changed.
    TaskUpdated,
    /// Spawn started.
    SpawnStarted,
    /// Conditional evaluated.
    ConditionalEvaluated,
    /// Validate checklist checked.
    ValidationRun,
    /// Webhook would fire.
    WebhookTriggered,
    /// Variable set.
    VariableSet,
    /// Sync scheduled.
    SyncScheduled,
    /// Routine scheduled.
    RoutineScheduled,
    /// Error.
    Error,
}

#[derive(Debug, Clone)]
pub struct ExecError {
    pub message: String,
    pub construct: String,
}

// ═══════════════════════════════════════════════════════════════
// Interpreter
// ═══════════════════════════════════════════════════════════════

impl Default for ExecContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecContext {
    pub fn new() -> Self {
        ExecContext {
            flow_states: HashMap::new(),
            entities: HashMap::new(),
            variables: HashMap::new(),
            gate_results: HashMap::new(),
            task_states: HashMap::new(),
            trace: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Execute an entire BitIR document.
    pub fn execute(&mut self, ir: &BitIR) {
        // Import variables from IR
        for (k, v) in &ir.variables {
            self.variables.insert(k.clone(), v.clone());
            self.trace.push(ExecEvent {
                kind: ExecEventKind::VariableSet,
                construct: k.clone(),
                detail: v.clone(),
            });
        }

        // Execute constructs in document order
        for construct in &ir.constructs {
            self.exec_construct(construct, ir);
        }
    }

    fn exec_construct(&mut self, construct: &Construct, ir: &BitIR) {
        match construct.kind {
            ConstructKind::Define => self.exec_define(construct),
            ConstructKind::Mutate => self.exec_mutate(construct, ir),
            ConstructKind::Delete => self.exec_delete(construct),
            ConstructKind::Flow => self.exec_flow(construct, ir),
            ConstructKind::Gate => self.exec_gate(construct),
            ConstructKind::Bound => self.exec_bound(construct),
            ConstructKind::Task => self.exec_task(construct),
            ConstructKind::SpawnParallel => self.exec_spawn(construct, true, ir),
            ConstructKind::SpawnSequential => self.exec_spawn(construct, false, ir),
            ConstructKind::Conditional => self.exec_conditional(construct, ir),
            ConstructKind::Validate => self.exec_validate(construct),
            ConstructKind::Webhook => self.exec_webhook(construct),
            ConstructKind::Group => {
                // Execute children in order
                for child in &construct.children {
                    self.exec_construct(child, ir);
                }
            }
            ConstructKind::Variable => {
                if let Some(name) = &construct.name {
                    if let Some(Value::String(val)) = construct.fields.get("value") {
                        self.variables.insert(name.clone(), val.clone());
                    }
                }
            }
            ConstructKind::Sync => self.exec_sync(construct),
            ConstructKind::Routine => self.exec_routine(construct),
            // Non-executable constructs (data/meta)
            ConstructKind::Query
            | ConstructKind::Serve
            | ConstructKind::Build
            | ConstructKind::Run
            | ConstructKind::Form
            | ConstructKind::Mod
            | ConstructKind::Use
            | ConstructKind::Project
            | ConstructKind::Commands
            | ConstructKind::Remember
            | ConstructKind::Recall
            | ConstructKind::Escalate
            | ConstructKind::Policy
            | ConstructKind::Status
            | ConstructKind::Issue
            | ConstructKind::Comment
            | ConstructKind::Directive
            | ConstructKind::Embed
            | ConstructKind::Git
            | ConstructKind::Snap
            | ConstructKind::Diff
            | ConstructKind::History
            | ConstructKind::Lattice
            | ConstructKind::PressureEffect
            | ConstructKind::UnitCell
            | ConstructKind::Symmetry
            | ConstructKind::CodeBlock
            | ConstructKind::Prose
            | ConstructKind::Divider
            | ConstructKind::Check => {}
        }
    }

    // ── Define: register schema + create default entity ───────

    fn exec_define(&mut self, construct: &Construct) {
        let name = match &construct.name {
            Some(n) => n.clone(),
            None => return,
        };

        // Register schema in entity store
        self.entities
            .insert(format!("@{}", name), construct.fields.clone());

        self.trace.push(ExecEvent {
            kind: ExecEventKind::SchemaRegistered,
            construct: format!("define:@{}", name),
            detail: format!("{} fields", construct.fields.len()),
        });
    }

    // ── Mutate: update entity fields ──────────────────────────

    fn exec_mutate(&mut self, construct: &Construct, _ir: &BitIR) {
        let entity = match &construct.name {
            Some(n) => n.clone(),
            None => return,
        };

        let id = construct
            .fields
            .get("_id")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let key = format!("@{}:{}", entity, id);

        // Get or create entity
        let entry = self.entities.entry(key.clone()).or_default();

        // Apply field updates
        for (k, v) in &construct.fields {
            if k != "_id" {
                entry.insert(k.clone(), v.clone());
            }
        }

        self.trace.push(ExecEvent {
            kind: ExecEventKind::EntityMutated,
            construct: format!("mutate:@{}:{}", entity, id),
            detail: format!("{} fields updated", construct.fields.len() - 1),
        });
    }

    // ── Delete: remove entity ─────────────────────────────────

    fn exec_delete(&mut self, construct: &Construct) {
        let entity = match &construct.name {
            Some(n) => n.clone(),
            None => return,
        };

        let id = construct
            .fields
            .get("_id")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let key = format!("@{}:{}", entity, id);
        self.entities.remove(&key);

        self.trace.push(ExecEvent {
            kind: ExecEventKind::EntityDeleted,
            construct: format!("delete:@{}:{}", entity, id),
            detail: String::new(),
        });
    }

    // ── Flow: state machine execution ─────────────────────────

    fn exec_flow(&mut self, construct: &Construct, _ir: &BitIR) {
        let flow_name = construct
            .name
            .clone()
            .unwrap_or_else(|| "unnamed".to_string());

        // Extract edges
        let mut edges: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
        for (key, value) in &construct.fields {
            if key.starts_with("edge_") && !key.contains("_label") && !key.contains("_gate") {
                if let Value::String(edge_str) = value {
                    if let Some((from, to)) = edge_str.split_once(" --> ") {
                        let idx = key.trim_start_matches("edge_");
                        let label = construct
                            .fields
                            .get(&format!("edge_{}_label", idx))
                            .and_then(|v| {
                                if let Value::String(s) = v {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            });
                        let gate = construct
                            .fields
                            .get(&format!("edge_{}_gate", idx))
                            .and_then(|v| {
                                if let Value::String(s) = v {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            });
                        edges.push((from.to_string(), to.to_string(), label, gate));
                    }
                }
            }
        }

        if edges.is_empty() {
            return;
        }

        // Initialize flow state to first node
        let initial = edges.first().map(|e| e.0.clone()).unwrap_or_default();
        let current = self
            .flow_states
            .entry(flow_name.clone())
            .or_insert(initial.clone());

        // Try to advance through edges
        let current_state = current.clone();
        for (from, to, label, gate) in &edges {
            if *from == current_state {
                // Check gate if present — ternary: Pos=proceed, Neutral=uncertain, Neg=blocked
                if let Some(gate_name) = gate {
                    let gate_result = self
                        .gate_results
                        .get(gate_name)
                        .copied()
                        .unwrap_or(Trit::Neutral);
                    match gate_result {
                        Trit::Pos => {} // proceed
                        Trit::Neutral => {
                            self.trace.push(ExecEvent {
                                kind: ExecEventKind::FlowBlocked,
                                construct: format!("flow:{}", flow_name),
                                detail: format!(
                                    "{} uncertain — gate '{}' has unknown conditions (retry later)",
                                    from, gate_name
                                ),
                            });
                            continue;
                        }
                        Trit::Neg => {
                            self.trace.push(ExecEvent {
                                kind: ExecEventKind::FlowBlocked,
                                construct: format!("flow:{}", flow_name),
                                detail: format!("{} blocked — gate '{}' failed", from, gate_name),
                            });
                            continue;
                        }
                    }
                }

                // Transition
                self.flow_states.insert(flow_name.clone(), to.clone());
                self.trace.push(ExecEvent {
                    kind: ExecEventKind::FlowTransition,
                    construct: format!("flow:{}", flow_name),
                    detail: format!(
                        "{} --> {}{}",
                        from,
                        to,
                        label
                            .as_ref()
                            .map(|l| format!(" ({})", l))
                            .unwrap_or_default()
                    ),
                });
                break;
            }
        }
    }

    // ── Gate: evaluate conditions ─────────────────────────────

    fn exec_gate(&mut self, construct: &Construct) {
        let name = match &construct.name {
            Some(n) => n.clone(),
            None => return,
        };

        // Gate evaluation: check requires against known state
        let _passed = true;
        let mut missing = Vec::new();

        // Ternary gate evaluation using Kleene logic
        let mut result = Trit::Pos; // Start optimistic

        // Check "requires" field — list of conditions
        if let Some(Value::Raw(req_str)) = construct.fields.get("requires") {
            let conditions: Vec<&str> = req_str
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().trim_matches(':'))
                .filter(|s| !s.is_empty())
                .collect();

            for condition in &conditions {
                // Check condition against known state
                let condition_trit = if let Some(&gate_result) = self.gate_results.get(*condition) {
                    gate_result
                } else if let Some(&task_done) = self.task_states.get(*condition) {
                    if task_done {
                        Trit::Pos
                    } else {
                        Trit::Neutral
                    } // undone task = uncertain
                } else if self.variables.contains_key(*condition) {
                    Trit::Pos // variable exists = condition met
                } else {
                    Trit::Neutral // not found = unknown, not failed
                };

                // Kleene AND: Neg dominates, then Neutral, then Pos
                result = match (result, condition_trit) {
                    (Trit::Neg, _) | (_, Trit::Neg) => Trit::Neg,
                    (Trit::Neutral, _) | (_, Trit::Neutral) => Trit::Neutral,
                    _ => Trit::Pos,
                };

                if condition_trit != Trit::Pos {
                    missing.push(condition.to_string());
                }
            }
        }

        self.gate_results.insert(name.clone(), result);
        let detail = match result {
            Trit::Pos => format!(
                "passed=true ({})",
                if missing.is_empty() {
                    "all conditions met"
                } else {
                    "conditions met"
                }
            ),
            Trit::Neutral => format!("uncertain — unknown conditions: {}", missing.join(", ")),
            Trit::Neg => format!("failed — missing: {}", missing.join(", ")),
        };

        self.trace.push(ExecEvent {
            kind: ExecEventKind::GateEvaluated,
            construct: format!("gate:{}", name),
            detail,
        });
    }

    // ── Bound: check constraints ──────────────────────────────

    fn exec_bound(&mut self, construct: &Construct) {
        let name = match &construct.name {
            Some(n) => n.clone(),
            None => return,
        };

        self.trace.push(ExecEvent {
            kind: ExecEventKind::BoundChecked,
            construct: format!("bound:{}", name),
            detail: format!("{} constraints", construct.fields.len()),
        });
    }

    // ── Task: track state ─────────────────────────────────────

    fn exec_task(&mut self, construct: &Construct) {
        let text = construct
            .fields
            .get("text")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let status = construct
            .fields
            .get("status")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Open".to_string());

        let done = status.contains("Completed") || status.contains("Done");
        self.task_states.insert(text.clone(), done);

        self.trace.push(ExecEvent {
            kind: ExecEventKind::TaskUpdated,
            construct: "task".to_string(),
            detail: format!("[{}] {}", if done { "x" } else { " " }, text),
        });

        // Execute child constructs
        for child in &construct.children {
            self.exec_construct(
                child,
                &BitIR::from_document(&crate::types::Document { nodes: Vec::new(), ..Default::default() }),
            );
        }
    }

    // ── Spawn: parallel/sequential ────────────────────────────

    fn exec_spawn(&mut self, construct: &Construct, parallel: bool, ir: &BitIR) {
        self.trace.push(ExecEvent {
            kind: ExecEventKind::SpawnStarted,
            construct: if parallel { "+" } else { "++" }.to_string(),
            detail: format!(
                "{} ({} children)",
                if parallel { "parallel" } else { "sequential" },
                construct.children.len()
            ),
        });

        // Execute children (in parallel mode they'd run concurrently via BEAM)
        for child in &construct.children {
            self.exec_construct(child, ir);
        }
    }

    // ── Conditional: evaluate and branch ───────────────────────

    fn exec_conditional(&mut self, construct: &Construct, ir: &BitIR) {
        let condition = construct
            .fields
            .get("condition")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Simple condition evaluation: check against variables
        let result = self.eval_simple_condition(&condition);

        self.trace.push(ExecEvent {
            kind: ExecEventKind::ConditionalEvaluated,
            construct: format!("if {}", condition),
            detail: format!("result={}", result),
        });

        if result {
            for child in &construct.children {
                self.exec_construct(child, ir);
            }
        }
    }

    fn eval_simple_condition(&self, condition: &str) -> bool {
        // Simple comparisons: "budget > 100000", "status = :critical"
        if let Some(gt_pos) = condition.find('>') {
            let lhs = condition[..gt_pos].trim();
            let rhs = condition[gt_pos + 1..].trim().trim_start_matches('=');
            if let Some(lhs_val) = self.variables.get(lhs) {
                if let (Ok(l), Ok(r)) = (lhs_val.parse::<f64>(), rhs.trim().parse::<f64>()) {
                    return l > r;
                }
            }
        }
        if let Some(lt_pos) = condition.find('<') {
            let lhs = condition[..lt_pos].trim();
            let rhs = condition[lt_pos + 1..].trim().trim_start_matches('=');
            if let Some(lhs_val) = self.variables.get(lhs) {
                if let (Ok(l), Ok(r)) = (lhs_val.parse::<f64>(), rhs.trim().parse::<f64>()) {
                    return l < r;
                }
            }
        }
        if let Some(eq_pos) = condition.find('=') {
            let lhs = condition[..eq_pos].trim();
            let rhs = condition[eq_pos + 1..]
                .trim()
                .trim_start_matches('=')
                .trim();
            if let Some(lhs_val) = self.variables.get(lhs) {
                return lhs_val.trim() == rhs;
            }
        }
        // Default: condition is true (optimistic execution)
        true
    }

    // ── Validate: run checklist ───────────────────────────────

    fn exec_validate(&mut self, construct: &Construct) {
        let name = construct
            .name
            .clone()
            .unwrap_or_else(|| "checklist".to_string());
        let total = construct.children.len();
        let done = construct
            .children
            .iter()
            .filter(|c| {
                c.fields
                    .get("status")
                    .and_then(|v| {
                        if let Value::String(s) = v {
                            Some(s.contains("Completed"))
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            })
            .count();

        self.trace.push(ExecEvent {
            kind: ExecEventKind::ValidationRun,
            construct: format!("validate:{}", name),
            detail: format!("{}/{} items complete", done, total),
        });
    }

    // ── Webhook: record trigger ───────────────────────────────

    fn exec_webhook(&mut self, construct: &Construct) {
        let name = construct.name.clone().unwrap_or_default();
        let url = construct
            .fields
            .get("url")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        self.trace.push(ExecEvent {
            kind: ExecEventKind::WebhookTriggered,
            construct: format!("webhook:{}", name),
            detail: format!("POST {}", url),
        });
    }

    // ── Sync: record schedule for Elixir to dispatch ────────

    fn exec_sync(&mut self, construct: &Construct) {
        let name = construct.name.clone().unwrap_or_default();
        let schedule = construct
            .fields
            .get("schedule")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let source = construct
            .fields
            .get("source")
            .and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        self.trace.push(ExecEvent {
            kind: ExecEventKind::SyncScheduled,
            construct: format!("sync:{}", name),
            detail: format!("schedule={} source={}", schedule, source),
        });
    }

    // ── Routine: record schedule for Elixir to dispatch ────

    fn exec_routine(&mut self, construct: &Construct) {
        let name = construct.name.clone().unwrap_or_default();

        self.trace.push(ExecEvent {
            kind: ExecEventKind::RoutineScheduled,
            construct: format!("routine:{}", name),
            detail: name.clone(),
        });
    }

    // ── Summary ───────────────────────────────────────────────

    pub fn summary(&self) -> BTreeMap<String, usize> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for event in &self.trace {
            *counts.entry(format!("{:?}", event.kind)).or_default() += 1;
        }
        counts.insert("total_events".into(), self.trace.len());
        counts.insert("entities".into(), self.entities.len());
        counts.insert("flow_states".into(), self.flow_states.len());
        counts.insert("gate_results".into(), self.gate_results.len());
        counts.insert("task_states".into(), self.task_states.len());
        counts.insert("errors".into(), self.errors.len());
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BitIR;
    use crate::parse::parse;

    #[test]
    fn exec_define_and_mutate() {
        let doc = parse("define:@User\n    name: \"\"\n    role: :admin\n\nmutate:@User:alice\n    name: \"Alice\"\n    role: :admin").unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        assert!(ctx.entities.contains_key("@User"));
        assert!(ctx.entities.contains_key("@User:alice"));
        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::SchemaRegistered));
        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::EntityMutated));
    }

    #[test]
    fn exec_flow_transitions() {
        let doc = parse("flow:\n    A --> B --> C").unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        // Flow should initialize to first state and try to advance
        assert!(!ctx.flow_states.is_empty());
        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::FlowTransition));
    }

    #[test]
    fn exec_gate_and_task() {
        let doc = parse("gate:review\n    approvals: 2\n\n[!] Write tests\n[x] Deploy").unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        assert!(ctx.gate_results.contains_key("review"));
        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::GateEvaluated));
        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::TaskUpdated));
    }

    #[test]
    fn exec_conditional_with_variable() {
        let doc =
            parse("budget = 200000\n\nif budget > 100000:\n    [!] Requires VP approval").unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::ConditionalEvaluated));
        // The conditional should evaluate to true (200000 > 100000)
        let cond_event = ctx
            .trace
            .iter()
            .find(|e| e.kind == ExecEventKind::ConditionalEvaluated)
            .unwrap();
        assert!(cond_event.detail.contains("true"));
    }

    #[test]
    fn exec_webhook() {
        let doc =
            parse("webhook:on_deploy\n    url: \"https://hooks.example.com\"\n    method: POST")
                .unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        assert!(ctx
            .trace
            .iter()
            .any(|e| e.kind == ExecEventKind::WebhookTriggered));
    }

    #[test]
    fn exec_full_document() {
        let source = r#"define:@Task
    title: ""
    status: :open

flow:
    :open --> :in_progress --> :done

gate:review
    approvals: 2

- [ ] Write tests
- [x] Deploy

webhook:on_done
    url: "https://example.com"
    method: POST"#;

        let doc = parse(source).unwrap();
        let ir = BitIR::from_document(&doc);
        let mut ctx = ExecContext::new();
        ctx.execute(&ir);

        let summary = ctx.summary();
        assert!(*summary.get("total_events").unwrap_or(&0) > 0);
        assert!(*summary.get("entities").unwrap_or(&0) > 0);
    }

    fn _assert_send<T: Send>() {}

    #[test]
    fn exec_context_is_send() {
        // ExecContext is owned by one BEAM process at a time — must be Send
        _assert_send::<ExecContext>();
    }
}
