use crate::span::ByteSpan;
use serde::{Deserialize, Serialize};

/// The root of a parsed .bit file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    pub nodes: Vec<Node>,
    /// Byte-offset spans for each top-level node (same indexing as `nodes`).
    /// Populated by span-aware parsers; empty for legacy callers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_spans: Vec<Option<ByteSpan>>,
}

/// Every line/block in a .bit file becomes a Node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Node {
    Group(Group),
    Task(Task),
    Prose(Prose),
    Comment(Comment),
    Spawn(Spawn),
    Divider,
    Define(Define),
    Mutate(Mutate),
    Delete(Delete),
    Query(Query),
    Variable(Variable),
    Flow(Flow),
    States(StatesDef),
    Validate(ValidateDef),
    Check(CheckDef),
    Form(FormDef),
    ModDef(ModDef),
    ModInvoke(ModInvoke),
    Git(GitOp),
    Conditional(Conditional),
    Snap(Snap),
    Diff(Diff),
    History(HistoryOp),
    StatusDef(StatusDef),
    Routine(Routine),
    Bold(Bold),
    Webhook(Webhook),
    UseBlock(UseBlock),
    Remember(Remember),
    Recall(RecallOp),
    EmbedMarker(EmbedMarker),
    FilesDef(FilesDef),
    PolicyDef(PolicyDef),
    Escalate(Escalate),
    SyncDef(SyncDef),
    EntityDef(EntityDef),
    MetricDef(MetricDef),
    GateDef(GateDef),
    LatticeValidates(LatticeValidatesDef),
    LatticeConstraint(LatticeConstraintDef),
    LatticeSchema(LatticeSchemaDef),
    LatticeFrontier(LatticeFrontierDef),
    PressureEffect(PressureEffectDef),
    UnitCell(UnitCellDef),
    Symmetry(SymmetryDef),
    CodeBlock(CodeBlock),
    Serve(ServeDef),
    Issue(IssueDef),
    ThreadComment(ThreadComment),
    Commands(CommandsDef),
    Project(ProjectDef),
    ProjectScope(ProjectScope),
    BoundDef(BoundDef),
    BuildDef(BuildDef),
    RunDef(RunDef),
    Directive(DirectiveDef),
}

// ── Groups ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub depth: u8,
    pub name: String,
    pub atoms: Vec<Atom>,
    pub gates: Vec<Gate>,
    pub children: Vec<Node>,
}

// ── Tasks ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub marker: TaskMarker,
    pub label: Option<String>,
    pub text: String,
    pub inline: Vec<Inline>,
    pub gates: Vec<Gate>,
    pub children: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_pass: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_fail: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_arms: Option<Vec<MatchArm>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMarker {
    pub kind: TaskKind,
    pub priority: Priority,
    pub prefix: TaskPrefix,
    pub seq: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskKind {
    Open,
    Required,
    Optional,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    None,
    Required,
    Optional,
    Decision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskPrefix {
    None,
    Parallel,
    Subtask(u8),
    ParallelSubtask,
}

// ── Inline spans ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Inline {
    Text { value: String },
    Ref(Ref),
    Channel { name: String },
    ModCall { name: String, args: Option<String> },
    Compute(Compute),
    Bold { value: String },
    Atom(Atom),
    Gate(Gate),
    Time(Time),
    ProjectRef { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ref {
    pub path: Vec<String>,
    pub plural: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atom {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compute {
    pub expr: String,
    pub live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Time {
    pub constraint: String,
    pub expr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub name: String,
    pub body: Option<String>,
}

// ── Spawns / Dividers ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Spawn {
    Parallel,
    Sequential,
}

// ── Data Model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Define {
    pub entity: String,
    pub atoms: Vec<Atom>,
    pub fields: Vec<FieldDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub plural: bool,
    pub default: FieldDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldDefault {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Atom(String),
    Enum(Vec<String>),
    Ref(String),
    List,
    Timestamp(String),
    Nil,
    Trit(i8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutate {
    pub entity: String,
    pub id: Option<String>,
    pub gate: Option<Gate>,
    pub fields: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<Vec<BatchRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRecord {
    pub id: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delete {
    pub entity: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub entity: String,
    pub plural: bool,
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<u32>,
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: VarValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VarValue {
    Literal(String),
    Compute(Compute),
    Ref(Ref),
}

// ── Orchestration ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub name: Option<String>,
    pub edges: Vec<FlowEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdge {
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub label: Option<String>,
    pub parallel: bool,
    pub gate: Option<String>,
    pub wait: Option<String>,
    pub timeout: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatesDef {
    pub transitions: Vec<FlowEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateDef {
    pub name: String,
    #[serde(default)]
    pub meta: AttachmentMeta,
    pub children: Vec<Node>,
}

/// Shared metadata for Check and Validate nodes: id, targets, depends_on, etc.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttachmentMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckDef {
    pub name: String,
    #[serde(default)]
    pub meta: AttachmentMeta,
    pub body: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_layout: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_pages: Vec<String>,
    #[serde(default)]
    pub storage: FormStorageDef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projections: Vec<FormProjectionDef>,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FormStorageDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duckdb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormProjectionDef {
    pub target: String,
    pub mapping: String,
}

// ── Mods ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDef {
    pub name: String,
    pub kind: Option<String>,
    pub description: Option<String>,
    pub trigger: Option<Vec<String>>,
    pub body: Vec<(String, String)>,
    pub versioned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInvoke {
    pub name: String,
    pub method: Option<String>,
    pub args: Option<String>,
}

// ── Git / Collaboration ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOp {
    pub verb: String,
    pub args: String,
    pub body: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snap {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryOp {
    pub target: String,
    pub limit: Option<u32>,
}

// ── Comments & Issues ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDef {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub gates: Vec<Gate>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadComment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub gates: Vec<Gate>,
    pub children: Vec<Node>,
}

// ── Memory / Semantic ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remember {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallOp {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedMarker {
    pub tag: String,
}

// ── Scoping / Policy ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesDef {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDef {
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub path: String,
    pub gates: Vec<Gate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalate {
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDef {
    pub name: String,
    pub class: String,
    pub source: String,
    pub identity: String,
    pub mode: String,
    pub target: String,
    pub schedule: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeDef {
    pub target: String,  // dev, build, test, etc.
    pub command: String, // the shell command inside pipes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<String>, // browser, canvas, none
}

// ── Projects ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDef {
    pub name: String,
    pub brief: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandsDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serve: Option<ServeDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inhibited_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kpi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
}

/// A `%ProjectName` block that scopes its indented children to a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectScope {
    pub name: String,
    pub children: Vec<Node>,
}

// ── Commands ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsDef {
    pub commands: Vec<CommandEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEntry {
    pub name: String, // without the /
    pub description: String,
    pub params: Vec<String>,
    pub prompt: String, // template with {param} slots
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub name: String,
    pub source: String,
    pub namespace: String,
    pub identity: String,
    pub fields: Vec<EntityField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityField {
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDef {
    pub name: String,
    pub source: Option<String>,
    pub grain: Option<String>,
    pub dimensions: Vec<String>,
    pub formula: String,
    pub cross_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDef {
    pub name: String,
    pub children: Vec<Node>,
}

// ── Lattice Constructs ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeValidatesDef {
    pub artifacts: Vec<LatticeArtifactRef>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeArtifactRef {
    pub artifact: String,
    pub schema: Option<String>,
    pub checks: Vec<LatticeCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeCheck {
    pub field: String,
    pub required: bool,
    pub min_items: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeConstraintDef {
    pub constraint_type: Option<String>,
    pub rule: String,
    pub applies_to: Vec<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeSchemaDef {
    pub fields: Vec<LatticeSchemaField>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeSchemaField {
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeFrontierDef {
    pub expected_schema: Option<String>,
    pub missing_fields: Vec<String>,
    pub exploration_strategy: Vec<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureEffectDef {
    pub dynamic: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCellDef {
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymmetryDef {
    pub children: Vec<Node>,
}

// ── Code Blocks ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub lang: Option<String>,
    pub content: String,
}

// ── Errors ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub code: String, // machine-readable: E_INVALID_SYNTAX, E_MAX_DEPTH, E_SYNC_MISSING_NAME, etc.
    pub kind: String, // keep for backwards compat
    pub message: String,
    pub context: Option<String>,
    pub line: usize, // 1-indexed; 0 if unknown
    pub col: usize,  // 1-indexed; 0 if unknown
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ParseError {}

// ── Other primitives ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prose {
    pub text: String,
    pub inline: Vec<Inline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDef {
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub trigger: String,
    pub expr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bold {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conditional {
    pub condition: Compute,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub trigger: String,
    pub url: String,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseBlock {
    pub mod_name: String,
    pub config: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_mod: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

// ── Forward-declared constructs (used by ir.rs) ─────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundDef {
    pub name: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildDef {
    pub name: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDef {
    pub name: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectiveDef {
    pub kind: String,
    pub value: String,
}
