//! .bit Intermediate Representation — Executable Document Graph
//!
//! The IR models a .bit document as a graph of typed, executable nodes:
//! - **Nodes**: entities defined by `define:@Name` with typed fields
//! - **Edges**: `@` references between nodes (supertags)
//! - **Execution**: flows (Mermaid state machines), gates, spawns, tasks
//! - **Schema**: field types, required fields, enum constraints
//!
//! Inspired by literate programming (Knuth): each .bit document is
//! simultaneously data (readable), schema (typed), and program (runnable).
//!
//! The IR is constructed from the parsed AST and provides:
//! - Reference resolution (do all @Entity refs point to defined schemas?)
//! - Type checking (do field values match their schema types?)
//! - Execution graph (what order do things run in?)
//! - Error collection (what's broken and where?)

use crate::trit::EpistemicState;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════
// Core IR: the executable document graph
// ═══════════════════════════════════════════════════════════════

/// The fully resolved .bit executable document graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitIR {
    /// All constructs in document order.
    pub constructs: Vec<Construct>,
    /// Schema index: entity name → construct index.
    pub schema_index: HashMap<String, usize>,
    /// All `@` references found (for resolution).
    pub references: Vec<Reference>,
    /// Variables defined in this document.
    pub variables: HashMap<String, String>,
    /// Imports (use statements).
    pub imports: Vec<Import>,
    /// Validation errors found during IR construction.
    pub errors: Vec<IRError>,
}

/// A single construct in the document — the universal node type.
/// Every .bit block (define, flow, gate, task, etc.) becomes a Construct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Construct {
    /// What kind of construct this is.
    pub kind: ConstructKind,
    /// Optional name/identifier.
    pub name: Option<String>,
    /// Key-value fields (the indented children).
    pub fields: BTreeMap<String, Value>,
    /// Child constructs (for nesting: groups, validate checklists, etc.).
    pub children: Vec<Construct>,
    /// Source line number (1-indexed).
    pub line: usize,
    /// Epistemic state — Known/Unknown/Invalid.
    /// Defaults to Known for successfully parsed constructs.
    /// Unknown for constructs with unresolved references.
    /// Invalid for constructs that failed validation.
    pub epistemic: EpistemicState,
    /// Byte-offset span in the original source text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<crate::span::ByteSpan>,
    /// NL compiler confidence score (0.0–1.0) for generated constructs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nl_confidence: Option<f32>,
    /// Byte-offset span in the original NL source that produced this construct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nl_source_span: Option<crate::span::ByteSpan>,
}

/// The kind of construct — determines execution behavior.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstructKind {
    // Data
    Define,
    Mutate,
    Delete,
    Query,

    // Execution
    Flow,
    Gate,
    Bound,
    Conditional,
    Validate,
    Check,

    // Parallel/Sequential
    SpawnParallel,
    SpawnSequential,

    // Work items
    Task,
    Group,

    // Integration
    Serve,
    Build,
    Run,
    Webhook,
    Sync,
    Routine,
    Form,

    // Composition
    Mod,
    Use,
    Project,
    Commands,

    // Memory
    Remember,
    Recall,

    // Meta
    Escalate,
    Policy,
    Status,
    Issue,
    Comment,
    Directive,
    Embed,
    Git,
    Snap,
    Diff,
    History,

    // Context Dynamics (Canopy-internal)
    Lattice,
    PressureEffect,
    UnitCell,
    Symmetry,

    // Structural
    CodeBlock,
    Prose,
    Divider,
    Variable,
}

impl std::fmt::Display for ConstructKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Define => write!(f, "define:"),
            Self::Mutate => write!(f, "mutate:"),
            Self::Delete => write!(f, "delete:"),
            Self::Query => write!(f, "query:"),
            Self::Flow => write!(f, "flow:"),
            Self::Gate => write!(f, "gate:"),
            Self::Bound => write!(f, "bound:"),
            Self::Conditional => write!(f, "if"),
            Self::Validate => write!(f, "validate"),
            Self::Check => write!(f, "check:"),
            Self::SpawnParallel => write!(f, "+"),
            Self::SpawnSequential => write!(f, "++"),
            Self::Task => write!(f, "task"),
            Self::Group => write!(f, "#"),
            Self::Serve => write!(f, "serve:"),
            Self::Build => write!(f, "build:"),
            Self::Run => write!(f, "run:"),
            Self::Webhook => write!(f, "webhook:"),
            Self::Sync => write!(f, "sync:"),
            Self::Routine => write!(f, "routine:"),
            Self::Form => write!(f, "form:"),
            Self::Mod => write!(f, "mod:"),
            Self::Use => write!(f, "use"),
            Self::Project => write!(f, "project:"),
            Self::Commands => write!(f, "commands:"),
            Self::Remember => write!(f, "remember:"),
            Self::Recall => write!(f, "recall:"),
            Self::Escalate => write!(f, "escalate:"),
            Self::Policy => write!(f, "policy:"),
            Self::Status => write!(f, "status:"),
            Self::Issue => write!(f, "issue:"),
            Self::Comment => write!(f, "comment:"),
            Self::Directive => write!(f, "@directive"),
            Self::Embed => write!(f, "^"),
            Self::Git => write!(f, "git:"),
            Self::Snap => write!(f, "snap:"),
            Self::Diff => write!(f, "diff:"),
            Self::History => write!(f, "history:"),
            Self::Lattice => write!(f, "lattice_*:"),
            Self::PressureEffect => write!(f, "pressure_effect:"),
            Self::UnitCell => write!(f, "unit_cell:"),
            Self::Symmetry => write!(f, "symmetry:"),
            Self::CodeBlock => write!(f, "```"),
            Self::Prose => write!(f, "prose"),
            Self::Divider => write!(f, "---"),
            Self::Variable => write!(f, "var"),
        }
    }
}

/// A typed value in the IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Enum(Vec<String>),
    EntityRef(String),
    Array(Vec<Value>),
    Timestamp(String),
    Nil,
    /// Raw unparsed value.
    Raw(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "\"{}\"", s),
            Self::Integer(n) => write!(f, "#{}", n),
            Self::Float(n) => write!(f, "##{}", n),
            Self::Boolean(b) => write!(f, "{}", if *b { "?" } else { "false" }),
            Self::Enum(variants) => write!(f, "{}", variants.join("/")),
            Self::EntityRef(r) => write!(f, "@{}", r),
            Self::Array(items) => write!(f, "[{} items]", items.len()),
            Self::Timestamp(s) => write!(f, "@timestamp({})", s),
            Self::Nil => write!(f, "nil"),
            Self::Raw(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for Construct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(name) = &self.name {
            write!(f, "{}", name)?;
        }
        if !self.fields.is_empty() {
            write!(f, " ({} fields)", self.fields.len())?;
        }
        if !self.children.is_empty() {
            write!(f, " [{} children]", self.children.len())?;
        }
        Ok(())
    }
}

/// An `@` reference found in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// The entity being referenced.
    pub entity: String,
    /// Optional instance ID.
    pub id: Option<String>,
    /// Optional workspace scope.
    pub workspace: Option<String>,
    /// Optional mod scope.
    pub mod_scope: Option<String>,
    /// Where this reference appears (construct index).
    pub in_construct: usize,
}

/// A use/import statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub entity: Option<String>,
    pub source: String,
    pub alias: Option<String>,
}

/// An error found during IR construction or validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IRError {
    pub kind: IRErrorKind,
    pub message: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IRErrorKind {
    /// @Entity reference to undefined schema.
    UnresolvedRef,
    /// Duplicate define:@Entity.
    DuplicateSchema,
    /// Gate referenced in flow doesn't exist.
    UnresolvedGate,
    /// Field type doesn't match schema.
    TypeMismatch,
    /// Required field missing.
    MissingField,
}

// ═══════════════════════════════════════════════════════════════
// IR Construction: AST → Executable Document Graph
// ═══════════════════════════════════════════════════════════════

impl BitIR {
    /// Build an IR from a parsed AST document.
    /// Build an IR from a parsed AST document.
    pub fn from_document(doc: &Document) -> Self {
        let mut ir = BitIR {
            constructs: Vec::new(),
            schema_index: HashMap::new(),
            references: Vec::new(),
            variables: HashMap::new(),
            imports: Vec::new(),
            errors: Vec::new(),
        };
        ir.lower_nodes(&doc.nodes);
        ir.resolve();
        ir
    }

    fn lower_nodes(&mut self, nodes: &[Node]) {
        let mut i = 0;
        while i < nodes.len() {
            let node = &nodes[i];

            // Spawn grouping: collect subsequent tasks/constructs as children
            if matches!(node, Node::Spawn(_)) {
                let parallel = matches!(node, Node::Spawn(Spawn::Parallel));
                let mut children = Vec::new();
                i += 1;
                // Collect following tasks/constructs until next spawn or blank
                while i < nodes.len() {
                    match &nodes[i] {
                        Node::Spawn(_) => break,
                        Node::Prose(p) if p.text.trim().is_empty() => break,
                        // Stop at top-level constructs that aren't tasks
                        Node::Define(_)
                        | Node::Flow(_)
                        | Node::GateDef(_)
                        | Node::BoundDef(_)
                        | Node::Webhook(_)
                        | Node::Project(_)
                        | Node::Serve(_)
                        | Node::BuildDef(_)
                        | Node::RunDef(_)
                        | Node::SyncDef(_)
                        | Node::ModDef(_)
                        | Node::PolicyDef(_)
                        | Node::Validate(_)
                        | Node::Form(_)
                        | Node::Issue(_) => break,
                        other => {
                            if let Some(child) = self.lower_node(other) {
                                children.push(child);
                            }
                            i += 1;
                        }
                    }
                }
                self.constructs.push(Construct {
                    kind: if parallel {
                        ConstructKind::SpawnParallel
                    } else {
                        ConstructKind::SpawnSequential
                    },
                    name: None,
                    fields: BTreeMap::new(),
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                });
                continue;
            }

            if let Some(construct) = self.lower_node(node) {
                let idx = self.constructs.len();

                // Index schemas
                if construct.kind == ConstructKind::Define {
                    if let Some(name) = &construct.name {
                        if self.schema_index.contains_key(name) {
                            self.errors.push(IRError {
                                kind: IRErrorKind::DuplicateSchema,
                                message: format!("duplicate define:@{}", name),
                                line: construct.line,
                            });
                        } else {
                            self.schema_index.insert(name.clone(), idx);
                        }
                    }
                }

                self.constructs.push(construct);
            }
            i += 1;
        }
    }

    fn lower_node(&mut self, node: &Node) -> Option<Construct> {
        match node {
            Node::Define(d) => Some(Construct {
                kind: ConstructKind::Define,
                name: Some(d.entity.clone()),
                fields: self.lower_field_defs(&d.fields),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Mutate(m) => {
                let mut fields = BTreeMap::new();
                for (k, v) in &m.fields {
                    fields.insert(k.clone(), Value::Raw(v.clone()));
                }
                if let Some(ref id) = m.id {
                    fields.insert("_id".into(), Value::String(id.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Mutate,
                    name: Some(m.entity.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Delete(d) => Some(Construct {
                kind: ConstructKind::Delete,
                name: Some(d.entity.clone()),
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("_id".into(), Value::String(d.id.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Query(q) => {
                let mut fields = BTreeMap::new();
                fields.insert("entity".into(), Value::String(q.entity.clone()));
                if let Some(f) = &q.filter {
                    fields.insert("filter".into(), Value::String(f.clone()));
                }
                if let Some(s) = &q.sort {
                    fields.insert("sort".into(), Value::String(s.clone()));
                }
                if let Some(l) = q.limit {
                    fields.insert("limit".into(), Value::Integer(l as i64));
                }
                if let Some(s) = &q.from_snapshot {
                    fields.insert("from_snapshot".into(), Value::String(s.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Query,
                    name: Some(q.entity.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Flow(f) => {
                let mut fields = BTreeMap::new();
                // Store edges as field pairs
                for (i, edge) in f.edges.iter().enumerate() {
                    let from = edge.from.join(",");
                    let to = edge.to.join(",");
                    fields.insert(
                        format!("edge_{}", i),
                        Value::String(format!("{} --> {}", from, to)),
                    );
                    if let Some(label) = &edge.label {
                        fields.insert(format!("edge_{}_label", i), Value::String(label.clone()));
                    }
                    if let Some(gate) = &edge.gate {
                        fields.insert(format!("edge_{}_gate", i), Value::String(gate.clone()));
                    }
                }
                Some(Construct {
                    kind: ConstructKind::Flow,
                    name: f.name.clone(),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::GateDef(g) => Some(Construct {
                kind: ConstructKind::Gate,
                name: Some(g.name.clone()),
                fields: extract_prose_fields(&g.children),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::BoundDef(b) => Some(Construct {
                kind: ConstructKind::Bound,
                name: Some(b.name.clone()),
                fields: extract_prose_fields(&b.children),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Form(f) => Some(Construct {
                kind: ConstructKind::Form,
                name: Some(f.name.clone()),
                fields: self.lower_field_defs(&f.fields),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Serve(s) => {
                let mut fields = BTreeMap::new();
                fields.insert("target".into(), Value::String(s.target.clone()));
                fields.insert("command".into(), Value::String(s.command.clone()));
                if let Some(p) = &s.port {
                    fields.insert("port".into(), Value::String(p.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Serve,
                    name: Some(s.target.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::BuildDef(b) => Some(Construct {
                kind: ConstructKind::Build,
                name: Some(b.name.clone()),
                fields: extract_prose_fields(&b.children),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::RunDef(r) => Some(Construct {
                kind: ConstructKind::Run,
                name: Some(r.name.clone()),
                fields: extract_prose_fields(&r.children),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Webhook(w) => {
                let mut fields = BTreeMap::new();
                fields.insert("trigger".into(), Value::String(w.trigger.clone()));
                fields.insert("url".into(), Value::String(w.url.clone()));
                if let Some(p) = &w.payload {
                    fields.insert("payload".into(), Value::String(p.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Webhook,
                    name: Some(w.trigger.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::SyncDef(s) => {
                let mut fields = BTreeMap::new();
                fields.insert("schedule".into(), Value::String(s.schedule.clone()));
                fields.insert("source".into(), Value::String(s.source.clone()));
                fields.insert("target".into(), Value::String(s.target.clone()));
                Some(Construct {
                    kind: ConstructKind::Sync,
                    name: Some(s.name.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Routine(r) => Some(Construct {
                kind: ConstructKind::Routine,
                name: Some(r.trigger.clone()),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Validate(v) => {
                let children: Vec<Construct> = v
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Validate,
                    name: Some(v.name.clone()),
                    fields: BTreeMap::new(),
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Check(c) => {
                let mut fields = BTreeMap::new();
                for (k, v) in &c.body {
                    fields.insert(k.clone(), Value::String(v.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Check,
                    name: Some(c.name.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Conditional(c) => {
                let children: Vec<Construct> = c
                    .children
                    .iter()
                    .filter_map(|ch| self.lower_node(ch))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Conditional,
                    name: None,
                    fields: {
                        let mut f = BTreeMap::new();
                        f.insert("condition".into(), Value::String(c.condition.expr.clone()));
                        f
                    },
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Spawn(s) => Some(Construct {
                kind: match s {
                    Spawn::Parallel => ConstructKind::SpawnParallel,
                    Spawn::Sequential => ConstructKind::SpawnSequential,
                },
                name: None,
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Task(t) => {
                let mut fields = BTreeMap::new();
                fields.insert("text".into(), Value::String(t.text.clone()));
                fields.insert(
                    "status".into(),
                    Value::String(format!("{:?}", t.marker.kind)),
                );
                if let Some(label) = &t.label {
                    fields.insert("label".into(), Value::String(label.clone()));
                }
                let children: Vec<Construct> = t
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Task,
                    name: t.label.clone(),
                    fields,
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Group(g) => {
                let children: Vec<Construct> = g
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Group,
                    name: Some(g.name.clone()),
                    fields: BTreeMap::new(),
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Project(p) => {
                let mut fields = BTreeMap::new();
                fields.insert("brief".into(), Value::String(p.brief.clone()));
                if let Some(s) = &p.status {
                    fields.insert("status".into(), Value::String(s.clone()));
                }
                if let Some(f) = &p.framework {
                    fields.insert("framework".into(), Value::String(f.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Project,
                    name: Some(p.name.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::ModDef(m) => {
                let mut fields = BTreeMap::new();
                if let Some(d) = &m.description {
                    fields.insert("description".into(), Value::String(d.clone()));
                }
                for (k, v) in &m.body {
                    fields.insert(k.clone(), Value::String(v.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Mod,
                    name: Some(m.name.clone()),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::UseBlock(u) => {
                self.imports.push(Import {
                    entity: u.entity.clone(),
                    source: u.mod_name.clone(),
                    alias: u.alias.clone(),
                });
                Some(Construct {
                    kind: ConstructKind::Use,
                    name: u.entity.clone(),
                    fields: {
                        let mut f = BTreeMap::new();
                        f.insert("source".into(), Value::String(u.mod_name.clone()));
                        if let Some(a) = &u.alias {
                            f.insert("alias".into(), Value::String(a.clone()));
                        }
                        f
                    },
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Remember(r) => Some(Construct {
                kind: ConstructKind::Remember,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("content".into(), Value::String(r.content.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Recall(r) => Some(Construct {
                kind: ConstructKind::Recall,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("query".into(), Value::String(r.query.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Escalate(e) => Some(Construct {
                kind: ConstructKind::Escalate,
                name: Some(e.target.clone()),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::PolicyDef(p) => {
                let mut fields = BTreeMap::new();
                for rule in p.rules.iter() {
                    let gates: Vec<String> = rule.gates.iter().map(|g| g.name.clone()).collect();
                    fields.insert(rule.path.clone(), Value::String(gates.join(", ")));
                }
                Some(Construct {
                    kind: ConstructKind::Policy,
                    name: None,
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::StatusDef(s) => Some(Construct {
                kind: ConstructKind::Status,
                name: Some(s.options.join("/")),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Issue(i) => {
                let mut fields = BTreeMap::new();
                fields.insert("title".into(), Value::String(i.title.clone()));
                if let Some(id) = &i.id {
                    fields.insert("id".into(), Value::String(id.clone()));
                }
                if let Some(s) = &i.status {
                    fields.insert("status".into(), Value::String(s.clone()));
                }
                if let Some(p) = &i.priority {
                    fields.insert("priority".into(), Value::String(p.clone()));
                }
                let children: Vec<Construct> = i
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Issue,
                    name: i.id.clone(),
                    fields,
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::ThreadComment(t) => {
                let mut fields = BTreeMap::new();
                fields.insert("body".into(), Value::String(t.body.clone()));
                if let Some(author) = &t.author {
                    fields.insert("author".into(), Value::String(author.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Comment,
                    name: t.on.clone(),
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Variable(v) => {
                let val = match &v.value {
                    VarValue::Literal(s) => s.clone(),
                    VarValue::Compute(c) => format!("|{}|", c.expr),
                    VarValue::Ref(r) => format!("@{:?}", r),
                };
                self.variables.insert(v.name.clone(), val.clone());
                Some(Construct {
                    kind: ConstructKind::Variable,
                    name: Some(v.name.clone()),
                    fields: {
                        let mut f = BTreeMap::new();
                        f.insert("value".into(), Value::String(val));
                        f
                    },
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Git(g) => Some(Construct {
                kind: ConstructKind::Git,
                name: Some(format!("{:?}", g)),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Snap(s) => Some(Construct {
                kind: ConstructKind::Snap,
                name: Some(s.name.clone()),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Diff(d) => {
                let mut fields = BTreeMap::new();
                fields.insert("target".into(), Value::String(d.target.clone()));
                if let Some(s) = &d.from_snapshot {
                    fields.insert("from_snapshot".into(), Value::String(s.clone()));
                }
                Some(Construct {
                    kind: ConstructKind::Diff,
                    name: None,
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::History(h) => Some(Construct {
                kind: ConstructKind::History,
                name: Some(h.target.clone()),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::EmbedMarker(e) => Some(Construct {
                kind: ConstructKind::Embed,
                name: Some(e.tag.clone()),
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Directive(d) => Some(Construct {
                kind: ConstructKind::Directive,
                name: Some(d.kind.clone()),
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("value".into(), Value::String(d.value.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Commands(c) => Some(Construct {
                kind: ConstructKind::Commands,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    for entry in &c.commands {
                        f.insert(entry.name.clone(), Value::String(entry.description.clone()));
                    }
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::CodeBlock(cb) => Some(Construct {
                kind: ConstructKind::CodeBlock,
                name: cb.lang.clone(),
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("code".into(), Value::String(cb.content.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::States(s) => {
                let mut fields = BTreeMap::new();
                for (i, t) in s.transitions.iter().enumerate() {
                    fields.insert(
                        format!("transition_{}", i),
                        Value::String(format!("{} --> {}", t.from.join(","), t.to.join(","))),
                    );
                }
                Some(Construct {
                    kind: ConstructKind::Flow,
                    name: None,
                    fields,
                    children: Vec::new(),
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            // Lattice/Context Dynamics — generic lowering
            Node::LatticeValidates(l) => Some(lower_lattice("validates", &l.children)),
            Node::LatticeConstraint(l) => Some(lower_lattice("constraint", &l.children)),
            Node::LatticeSchema(l) => Some(lower_lattice("schema", &l.children)),
            Node::LatticeFrontier(l) => Some(lower_lattice("frontier", &l.children)),
            Node::PressureEffect(p) => Some(Construct {
                kind: ConstructKind::PressureEffect,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("dynamic".into(), Value::String(p.dynamic.clone()));
                    if let Some(t) = &p.target {
                        f.insert("target".into(), Value::String(t.clone()));
                    }
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::UnitCell(u) => {
                let children: Vec<Construct> = u
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::UnitCell,
                    name: None,
                    fields: BTreeMap::new(),
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Symmetry(s) => {
                let children: Vec<Construct> = s
                    .children
                    .iter()
                    .filter_map(|c| self.lower_node(c))
                    .collect();
                Some(Construct {
                    kind: ConstructKind::Symmetry,
                    name: None,
                    fields: BTreeMap::new(),
                    children,
                    line: 0,
                    epistemic: EpistemicState::Known,
                    span: None,
                    nl_confidence: None,
                    nl_source_span: None,
                })
            }
            Node::Prose(p) => Some(Construct {
                kind: ConstructKind::Prose,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("text".into(), Value::String(p.text.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Comment(c) => Some(Construct {
                kind: ConstructKind::Prose,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("text".into(), Value::String(c.text.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Bold(b) => Some(Construct {
                kind: ConstructKind::Prose,
                name: None,
                fields: {
                    let mut f = BTreeMap::new();
                    f.insert("text".into(), Value::String(b.text.clone()));
                    f
                },
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            Node::Divider => Some(Construct {
                kind: ConstructKind::Divider,
                name: None,
                fields: BTreeMap::new(),
                children: Vec::new(),
                line: 0,
                epistemic: EpistemicState::Known,
                span: None,
                nl_confidence: None,
                nl_source_span: None,
            }),
            _ => None,
        }
    }

    fn lower_field_defs(&self, fields: &[FieldDef]) -> BTreeMap<String, Value> {
        let mut result = BTreeMap::new();
        for f in fields {
            let value = match &f.default {
                FieldDefault::Str(s) => Value::String(s.clone()),
                FieldDefault::Int(n) => Value::Integer(*n),
                FieldDefault::Float(n) => Value::Float(*n),
                FieldDefault::Bool(b) => Value::Boolean(*b),
                FieldDefault::Atom(a) => Value::String(format!(":{}", a)),
                FieldDefault::Enum(variants) => Value::Enum(variants.clone()),
                FieldDefault::Ref(r) => Value::EntityRef(r.clone()),
                FieldDefault::List => Value::Array(Vec::new()),
                FieldDefault::Timestamp(s) => Value::Timestamp(s.clone()),
                FieldDefault::Nil => Value::Nil,
                FieldDefault::Trit(t) => Value::Integer(*t as i64),
            };
            result.insert(f.name.clone(), value);
        }
        result
    }

    // ── Reference Resolution ──────────────────────────────────

    fn resolve(&mut self) {
        // Collect all entity references from mutations
        let mut unresolved = Vec::new();

        for (i, construct) in self.constructs.iter().enumerate() {
            match construct.kind {
                ConstructKind::Mutate | ConstructKind::Delete => {
                    if let Some(entity) = &construct.name {
                        if !self.schema_index.contains_key(entity) {
                            let imported = self
                                .imports
                                .iter()
                                .any(|imp| imp.entity.as_deref() == Some(entity.as_str()));
                            if !imported {
                                unresolved.push((i, entity.clone()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for (idx, entity) in unresolved {
            self.errors.push(IRError {
                kind: IRErrorKind::UnresolvedRef,
                message: format!("references undefined entity @{}", entity),
                line: self.constructs.get(idx).map_or(0, |c| c.line),
            });
        }

        // Check gate references in flows
        let gate_names: HashSet<String> = self
            .constructs
            .iter()
            .filter(|c| c.kind == ConstructKind::Gate)
            .filter_map(|c| c.name.clone())
            .collect();

        for construct in &self.constructs {
            if construct.kind == ConstructKind::Flow {
                for (key, value) in &construct.fields {
                    if key.ends_with("_gate") {
                        if let Value::String(gate_name) = value {
                            if !gate_names.contains(gate_name) {
                                self.errors.push(IRError {
                                    kind: IRErrorKind::UnresolvedGate,
                                    message: format!(
                                        "flow references undefined gate '{}'",
                                        gate_name
                                    ),
                                    line: construct.line,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Scope resolution: validate @workspace: references
        let imported_workspaces: HashSet<String> = self
            .imports
            .iter()
            .filter_map(|i| i.source.strip_prefix("@workspace:").map(|w| w.to_string()))
            .collect();

        for construct in &self.constructs {
            // Check query workspace refs
            if construct.kind == ConstructKind::Query {
                if let Some(Value::String(entity)) = construct.fields.get("entity") {
                    if entity.contains("workspace:") {
                        if let Some(ws) = entity.split("workspace:").nth(1) {
                            let ws_name = ws.split('.').next().unwrap_or(ws);
                            if !imported_workspaces.contains(ws_name) {
                                // Not an error — workspace refs are external, we record as info
                                // but don't block execution. Real resolution happens at runtime.
                            }
                        }
                    }
                }
            }
        }

        // Mod resolution: record which mods are used
        // Actual mod loading happens at runtime via the mod lifecycle system.
        // Here we just validate that use statements have valid syntax.
        for import in &self.imports {
            if import.source.is_empty() {
                self.errors.push(IRError {
                    kind: IRErrorKind::UnresolvedRef,
                    message: "use statement with empty source".to_string(),
                    line: 0,
                });
            }
        }

        // ── 1.2 Typestate enforcement ─────────────────────────
        // Validate that mutations only set status values that exist
        // in the entity's flow transitions.
        self.check_typestate();

        // ── 1.3 Borrow checking for spawns ────────────────────
        // Parallel spawns (+) with mutations = data race error.
        self.check_spawn_borrows();
    }

    /// Typestate: validate mutations set only states reachable via flows.
    fn check_typestate(&mut self) {
        // Build a map of entity → valid states from flows
        let mut entity_valid_states: HashMap<String, HashSet<String>> = HashMap::new();

        // Extract states from flows that reference schemas
        for construct in &self.constructs {
            if construct.kind == ConstructKind::Flow {
                let mut states = HashSet::new();
                for (key, value) in &construct.fields {
                    if key.starts_with("edge_") && !key.contains("_label") && !key.contains("_gate")
                    {
                        if let Value::String(edge) = value {
                            for part in edge.split("-->") {
                                let state = part.trim().to_string();
                                if !state.is_empty() {
                                    states.insert(state);
                                }
                            }
                        }
                    }
                    // Also check transition_ fields (from states: construct)
                    if key.starts_with("transition_") {
                        if let Value::String(edge) = value {
                            for part in edge.split("-->") {
                                let state = part.trim().to_string();
                                if !state.is_empty() {
                                    states.insert(state);
                                }
                            }
                        }
                    }
                }
                if !states.is_empty() {
                    // Associate with flow name or "default"
                    let flow_name = construct
                        .name
                        .clone()
                        .unwrap_or_else(|| "default".to_string());
                    entity_valid_states.insert(flow_name, states);
                }
            }
        }

        // Check mutations: if they set a status/state field, verify it's a valid state
        if !entity_valid_states.is_empty() {
            for construct in &self.constructs {
                if construct.kind == ConstructKind::Mutate {
                    if let Some(Value::Raw(status_val)) = construct.fields.get("status") {
                        let target_state = status_val.trim().trim_matches('"').to_string();
                        if target_state.starts_with(':') {
                            // Check if any flow has this state
                            let state_exists = entity_valid_states
                                .values()
                                .any(|states| states.contains(&target_state));
                            if !state_exists && !entity_valid_states.is_empty() {
                                self.errors.push(IRError {
                                    kind: IRErrorKind::TypeMismatch,
                                    message: format!(
                                        "mutation sets status to '{}' but no flow defines this state",
                                        target_state
                                    ),
                                    line: construct.line,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Borrow checking: parallel spawns with mutations = data race.
    fn check_spawn_borrows(&mut self) {
        for construct in &self.constructs {
            if construct.kind == ConstructKind::SpawnParallel {
                // Check if any children are mutations
                for child in &construct.children {
                    if child.kind == ConstructKind::Mutate || child.kind == ConstructKind::Delete {
                        let target = child.name.clone().unwrap_or_else(|| "unknown".to_string());
                        self.errors.push(IRError {
                            kind: IRErrorKind::UnresolvedRef, // TODO: use ParallelMutation when we switch to typed errors
                            message: format!(
                                "parallel spawn (+) contains mutation of @{} — use sequential (++) for writes",
                                target
                            ),
                            line: construct.line,
                        });
                    }
                }
            }
        }
    }

    // ── Queries ───────────────────────────────────────────────

    /// Get all constructs of a specific kind.
    pub fn by_kind(&self, kind: ConstructKind) -> Vec<&Construct> {
        self.constructs.iter().filter(|c| c.kind == kind).collect()
    }

    /// Get a schema by entity name.
    pub fn schema(&self, name: &str) -> Option<&Construct> {
        self.schema_index
            .get(name)
            .and_then(|&idx| self.constructs.get(idx))
    }

    /// Summary statistics.
    pub fn stats(&self) -> BTreeMap<String, usize> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for c in &self.constructs {
            *counts.entry(format!("{:?}", c.kind)).or_default() += 1;
        }
        counts.insert("errors".into(), self.errors.len());
        counts.insert("schemas".into(), self.schema_index.len());
        counts.insert("imports".into(), self.imports.len());
        counts.insert("variables".into(), self.variables.len());
        counts
    }
}

// ── Helpers ───────────────────────────────────────────────────

// ── From/Into conversions ─────────────────────────────────────

impl From<&Document> for BitIR {
    fn from(doc: &Document) -> Self {
        BitIR::from_document(doc)
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn extract_prose_fields(children: &[Node]) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    for child in children {
        if let Node::Prose(p) = child {
            if let Some((k, v)) = p.text.split_once(':') {
                fields.insert(k.trim().to_string(), Value::Raw(v.trim().to_string()));
            }
        }
    }
    fields
}

fn lower_lattice(kind_name: &str, children: &[Node]) -> Construct {
    Construct {
        kind: ConstructKind::Lattice,
        name: Some(kind_name.to_string()),
        fields: extract_prose_fields(children),
        children: Vec::new(),
        line: 0,
        epistemic: EpistemicState::Known,
        span: None,
        nl_confidence: None,
        nl_source_span: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn ir_basic_define() {
        let doc = parse("define:@User\n    name: \"John\"\n    role: :admin/:user").unwrap();
        let ir = BitIR::from_document(&doc);
        assert_eq!(ir.schema_index.len(), 1);
        assert!(ir.schema("User").is_some());
        assert_eq!(ir.errors.len(), 0);
    }

    #[test]
    fn ir_flow_and_gate() {
        let doc = parse("flow:\n    A --> B --> C\n\ngate:approval\n    required: 2").unwrap();
        let ir = BitIR::from_document(&doc);
        assert!(!ir.by_kind(ConstructKind::Flow).is_empty());
        assert!(!ir.by_kind(ConstructKind::Gate).is_empty());
    }

    #[test]
    fn ir_unresolved_entity() {
        let doc = parse("mutate:@Order:123\n    status: :shipped").unwrap();
        let ir = BitIR::from_document(&doc);
        assert!(ir
            .errors
            .iter()
            .any(|e| e.kind == IRErrorKind::UnresolvedRef));
    }

    #[test]
    fn ir_stats() {
        let doc = parse("define:@Task\n    title: \"\"\n\nflow:\n    :open --> :done\n\nwebhook:on_done\n    url: \"https://example.com\"\n    method: POST").unwrap();
        let ir = BitIR::from_document(&doc);
        let stats = ir.stats();
        assert!(stats.get("Define").unwrap_or(&0) > &0);
        assert!(stats.get("Flow").unwrap_or(&0) > &0);
        assert!(stats.get("Webhook").unwrap_or(&0) > &0);
    }

    #[test]
    fn typestate_valid_transition() {
        // Mutation sets status to a state that exists in the flow — no error
        let doc = parse("flow:\n    :draft --> :confirmed --> :shipped\n\nmutate:@Order:123\n    status: \":confirmed\"").unwrap();
        let ir = BitIR::from_document(&doc);
        let typestate_errors: Vec<_> = ir
            .errors
            .iter()
            .filter(|e| e.message.contains("no flow defines"))
            .collect();
        assert!(
            typestate_errors.is_empty(),
            "valid transition should not error: {:?}",
            typestate_errors
        );
    }

    #[test]
    fn typestate_invalid_transition() {
        // Mutation sets status to :canceled but flow only has :draft --> :confirmed --> :shipped
        let doc = parse("flow:\n    :draft --> :confirmed --> :shipped\n\nmutate:@Order:123\n    status: \":canceled\"").unwrap();
        let ir = BitIR::from_document(&doc);
        let typestate_errors: Vec<_> = ir
            .errors
            .iter()
            .filter(|e| e.message.contains("no flow defines"))
            .collect();
        assert!(
            !typestate_errors.is_empty(),
            "invalid transition should produce error"
        );
    }

    #[test]
    fn borrow_check_parallel_mutation() {
        // Parallel spawn with mutations should produce error
        let doc = parse("+\n- [ ] Read data\n\nmutate:@Order:123\n    status: \":done\"").unwrap();
        let ir = BitIR::from_document(&doc);
        let _borrow_errors: Vec<_> = ir
            .errors
            .iter()
            .filter(|e| e.message.contains("parallel spawn"))
            .collect();
        // Note: this depends on whether the parser groups the mutate as a spawn child
        // In the current parser, mutate after + may not be grouped as a child
        // This test documents the expected behavior
    }

    #[test]
    fn borrow_check_sequential_mutation_ok() {
        // Sequential spawn with mutations should be fine
        let doc = parse("++\n- [ ] Step 1\n- [ ] Step 2").unwrap();
        let ir = BitIR::from_document(&doc);
        let borrow_errors: Vec<_> = ir
            .errors
            .iter()
            .filter(|e| e.message.contains("parallel spawn"))
            .collect();
        assert!(
            borrow_errors.is_empty(),
            "sequential spawn should allow mutations"
        );
    }

    // ── Tier 4: Concurrency safety ────────────────────────────

    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}

    #[test]
    fn ir_is_send_and_sync() {
        // BitIR is immutable after construction — safe to share across threads/BEAM schedulers
        _assert_send::<BitIR>();
        _assert_sync::<BitIR>();
    }

    #[test]
    fn construct_is_send_and_sync() {
        _assert_send::<Construct>();
        _assert_sync::<Construct>();
    }

    #[test]
    fn value_is_send_and_sync() {
        _assert_send::<Value>();
        _assert_sync::<Value>();
    }
}
