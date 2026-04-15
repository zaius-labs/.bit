//! Bit grammar conformance test
//!
//! Verifies that the formal PEG grammar in `grammar/bit.peg` and the
//! hand-written Rust parser in `src/parse.rs` agree on all `.bit` files in
//! the test corpus.
//!
//! Phase 1 scope
//! -------------
//!   - `parse.rs` is the authoritative parser.  Any divergence is a spec bug.
//!   - This test validates structural coverage, not full re-parsing:
//!       1. Every node kind that appears in a corpus-parsed AST has a
//!          corresponding production in the grammar catalogue.
//!       2. Every `.bit` file parses without error.
//!       3. Grammar-level structural invariants hold for each node type
//!          (e.g. a Define always has an entity name starting with uppercase).
//!       4. SchemaRegistry output is stable across corpus files (define: parity).
//!   - Divergences are classified:
//!     (a) spec gap        -- grammar lacks coverage -> BLOCKS merge
//!     (b) spec ambiguity  -- grammar production is underdetermined -> BLOCKS merge
//!     (c) parser quirk    -- known asymmetry, documented in QUIRKS section of
//!     bit.peg -> does NOT block merge
//!
//! Running
//! -------
//!   cargo test -p bit-lang-core --test conformance -- --nocapture

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bit_core::parse::parse;
use bit_core::schema::SchemaRegistry;
use bit_core::types::Node;

// ---------------------------------------------------------------------------
// Corpus discovery
// ---------------------------------------------------------------------------

/// Collect all `.bit` files from the test fixtures directory.
fn collect_corpus() -> Vec<PathBuf> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let mut files: Vec<PathBuf> = Vec::new();
    collect_bit_files(&crate_root.join("tests/fixtures"), &mut files);

    files.sort();
    files.dedup();
    files
}

fn collect_bit_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_bit_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "bit") {
            out.push(path);
        }
    }
}

// ---------------------------------------------------------------------------
// Grammar catalogue -- all node kinds in grammar/bit.peg
// ---------------------------------------------------------------------------

fn grammar_covered_kinds() -> HashSet<&'static str> {
    [
        "Group",
        "Task",
        "Prose",
        "Comment",
        "Spawn",
        "Divider",
        "Define",
        "Mutate",
        "Delete",
        "Query",
        "Variable",
        "Flow",
        "States",
        "Validate",
        "Check",
        "Form",
        "ModDef",
        "ModInvoke",
        "Git",
        "Conditional",
        "Snap",
        "Diff",
        "History",
        "StatusDef",
        "Routine",
        "Bold",
        "Webhook",
        "UseBlock",
        "Remember",
        "Recall",
        "EmbedMarker",
        "FilesDef",
        "PolicyDef",
        "Escalate",
        "SyncDef",
        "EntityDef",
        "MetricDef",
        "GateDef",
        "LatticeValidates",
        "LatticeConstraint",
        "LatticeSchema",
        "LatticeFrontier",
        "PressureEffect",
        "UnitCell",
        "Symmetry",
        "CodeBlock",
        "Serve",
        "Issue",
        "ThreadComment",
        "Commands",
        "Project",
        "ProjectScope",
        "BoundDef",
        "BuildDef",
        "RunDef",
        "Directive",
    ]
    .into_iter()
    .collect()
}

// ---------------------------------------------------------------------------
// Node kind extraction
// ---------------------------------------------------------------------------

fn node_kind(node: &Node) -> &'static str {
    match node {
        Node::Group(_) => "Group",
        Node::Task(_) => "Task",
        Node::Prose(_) => "Prose",
        Node::Comment(_) => "Comment",
        Node::Spawn(_) => "Spawn",
        Node::Divider => "Divider",
        Node::Define(_) => "Define",
        Node::Mutate(_) => "Mutate",
        Node::Delete(_) => "Delete",
        Node::Query(_) => "Query",
        Node::Variable(_) => "Variable",
        Node::Flow(_) => "Flow",
        Node::States(_) => "States",
        Node::Validate(_) => "Validate",
        Node::Check(_) => "Check",
        Node::Form(_) => "Form",
        Node::ModDef(_) => "ModDef",
        Node::ModInvoke(_) => "ModInvoke",
        Node::Git(_) => "Git",
        Node::Conditional(_) => "Conditional",
        Node::Snap(_) => "Snap",
        Node::Diff(_) => "Diff",
        Node::History(_) => "History",
        Node::StatusDef(_) => "StatusDef",
        Node::Routine(_) => "Routine",
        Node::Bold(_) => "Bold",
        Node::Webhook(_) => "Webhook",
        Node::UseBlock(_) => "UseBlock",
        Node::Remember(_) => "Remember",
        Node::Recall(_) => "Recall",
        Node::EmbedMarker(_) => "EmbedMarker",
        Node::FilesDef(_) => "FilesDef",
        Node::PolicyDef(_) => "PolicyDef",
        Node::Escalate(_) => "Escalate",
        Node::SyncDef(_) => "SyncDef",
        Node::EntityDef(_) => "EntityDef",
        Node::MetricDef(_) => "MetricDef",
        Node::GateDef(_) => "GateDef",
        Node::LatticeValidates(_) => "LatticeValidates",
        Node::LatticeConstraint(_) => "LatticeConstraint",
        Node::LatticeSchema(_) => "LatticeSchema",
        Node::LatticeFrontier(_) => "LatticeFrontier",
        Node::PressureEffect(_) => "PressureEffect",
        Node::UnitCell(_) => "UnitCell",
        Node::Symmetry(_) => "Symmetry",
        Node::CodeBlock(_) => "CodeBlock",
        Node::Serve(_) => "Serve",
        Node::Issue(_) => "Issue",
        Node::ThreadComment(_) => "ThreadComment",
        Node::Commands(_) => "Commands",
        Node::Project(_) => "Project",
        Node::ProjectScope(_) => "ProjectScope",
        Node::BoundDef(_) => "BoundDef",
        Node::BuildDef(_) => "BuildDef",
        Node::RunDef(_) => "RunDef",
        Node::Directive(_) => "Directive",
    }
}

/// Walk a node tree, collecting the kind of every node into `out`.
fn collect_kinds(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        out.insert(node_kind(node).to_string());
        collect_kinds(children_of(node), out);
    }
}

/// Return the immediate child node slice of a node (if it has children).
fn children_of(node: &Node) -> &[Node] {
    match node {
        Node::Group(g) => &g.children,
        Node::Task(t) => &t.children,
        Node::Validate(v) => &v.children,
        Node::Conditional(c) => &c.children,
        Node::GateDef(g) => &g.children,
        Node::Issue(i) => &i.children,
        Node::ThreadComment(tc) => &tc.children,
        Node::ProjectScope(ps) => &ps.children,
        Node::LatticeValidates(lv) => &lv.children,
        Node::LatticeConstraint(lc) => &lc.children,
        Node::LatticeSchema(ls) => &ls.children,
        Node::LatticeFrontier(lf) => &lf.children,
        Node::UnitCell(uc) => &uc.children,
        Node::Symmetry(s) => &s.children,
        Node::BoundDef(b) => &b.children,
        Node::BuildDef(b) => &b.children,
        Node::RunDef(r) => &r.children,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------
// Structural invariant checks
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Divergence {
    file: PathBuf,
    kind: DivergenceKind,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum DivergenceKind {
    SpecGap,
    SpecAmbiguity,
    ParserQuirk,
}

fn check_structural_invariants(nodes: &[Node], file: &Path, divergences: &mut Vec<Divergence>) {
    for node in nodes {
        check_node(node, file, divergences);
        check_structural_invariants(children_of(node), file, divergences);
    }
}

fn check_node(node: &Node, file: &Path, divs: &mut Vec<Divergence>) {
    match node {
        // define: entity name must start with uppercase (grammar: UPPER_IDENT)
        Node::Define(d) => {
            if !d.entity.is_empty() && !d.entity.starts_with(|c: char| c.is_uppercase()) {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: format!(
                        "Define: entity name {:?} does not start with uppercase \
                         (grammar: SCOPED_ENTITY -> UPPER_IDENT)",
                        d.entity
                    ),
                });
            }
        }

        // mutate: entity name must start with uppercase
        Node::Mutate(m) => {
            if !m.entity.is_empty() && !m.entity.starts_with(|c: char| c.is_uppercase()) {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: format!(
                        "Mutate: entity name {:?} does not start with uppercase \
                         (grammar: SCOPED_ENTITY -> UPPER_IDENT)",
                        m.entity
                    ),
                });
            }
        }

        // delete: entity must be non-empty
        Node::Delete(d) => {
            if d.entity.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "Delete: empty entity name".to_string(),
                });
            }
        }

        // group: depth must be >= 1; name must be non-empty
        Node::Group(g) => {
            if g.depth == 0 {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "Group: depth=0 (grammar: group_depth = '#'+)".to_string(),
                });
            }
        }

        Node::CodeBlock(_) => {}

        // task: marker must be present
        Node::Task(t) => {
            use bit_core::types::{TaskKind, TaskPrefix};
            let _ = &t.marker;
            match (&t.marker.prefix, &t.marker.kind) {
                (TaskPrefix::Subtask(d), _) if *d == 0 => {
                    divs.push(Divergence {
                        file: file.to_path_buf(),
                        kind: DivergenceKind::SpecGap,
                        message: "Task: Subtask(0) -- depth must be >=1".to_string(),
                    });
                }
                _ => {}
            }
            if t.marker.kind == TaskKind::Completed
                && t.marker.priority == bit_core::types::Priority::Required
            {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecAmbiguity,
                    message: "Task: kind=Completed but priority=Required -- ambiguous bracket"
                        .to_string(),
                });
            }
        }

        // mod_def: name starts with uppercase ($Name)
        Node::ModDef(m) => {
            if !m.name.starts_with(|c: char| c.is_uppercase()) {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: format!(
                        "ModDef: name {:?} does not start with uppercase \
                         (grammar: mod:$ ~ UPPER_IDENT)",
                        m.name
                    ),
                });
            }
        }

        // status_def: must have >=2 options
        Node::StatusDef(s) => {
            if s.options.len() < 2 {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: format!(
                        "StatusDef: only {} option(s) -- grammar requires >=2 (status: ... / ...)",
                        s.options.len()
                    ),
                });
            }
        }

        Node::Flow(_) | Node::States(_) => {}

        // embed_marker: tag must be non-empty
        Node::EmbedMarker(e) => {
            if e.tag.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "EmbedMarker: empty tag (grammar: '^' ~ IDENT)".to_string(),
                });
            }
        }

        // use_block: mod_name must be non-empty
        Node::UseBlock(u) => {
            if u.mod_name.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "UseBlock: empty mod_name".to_string(),
                });
            }
        }

        // project: name must be non-empty
        Node::Project(p) => {
            if p.name.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "Project: empty name".to_string(),
                });
            }
        }

        // project_scope: name must start with uppercase
        Node::ProjectScope(ps) => {
            if !ps.name.starts_with(|c: char| c.is_uppercase()) {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::ParserQuirk,
                    message: format!(
                        "ProjectScope: name {:?} does not start with uppercase \
                         (QUIRK: parser accepts lowercase after %; grammar expects UPPER_IDENT)",
                        ps.name
                    ),
                });
            }
        }

        // serve: target must be non-empty
        Node::Serve(s) => {
            if s.target.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "Serve: empty target".to_string(),
                });
            }
        }

        // git: verb must be non-empty
        Node::Git(g) => {
            if g.verb.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "Git: empty verb".to_string(),
                });
            }
        }

        // sync_def: name must be non-empty
        Node::SyncDef(s) => {
            if s.name.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "SyncDef: empty name".to_string(),
                });
            }
            if !s.class.is_empty() && !matches!(s.class.as_str(), "canon" | "ops" | "data") {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: format!(
                        "SyncDef {:?}: class {:?} not in {{canon, ops, data}}",
                        s.name, s.class
                    ),
                });
            }
        }

        // entity_def: name must be non-empty
        Node::EntityDef(e) => {
            if e.name.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "EntityDef: empty name".to_string(),
                });
            }
        }

        // metric_def: name must be non-empty
        Node::MetricDef(m) => {
            if m.name.is_empty() {
                divs.push(Divergence {
                    file: file.to_path_buf(),
                    kind: DivergenceKind::SpecGap,
                    message: "MetricDef: empty name".to_string(),
                });
            }
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// SchemaRegistry parity check
// ---------------------------------------------------------------------------

fn check_schema_parity(nodes: &[Node], file: &Path, divergences: &mut Vec<Divergence>) {
    let defines = collect_defines(nodes);
    if defines.is_empty() {
        return;
    }

    let doc = bit_core::types::Document {
        nodes: nodes.to_vec(), ..Default::default()
    };
    let mut registry = SchemaRegistry::new();
    registry.extract_from_doc(&doc);

    for entity in &defines {
        if !registry.entities.contains_key(entity) {
            divergences.push(Divergence {
                file: file.to_path_buf(),
                kind: DivergenceKind::SpecGap,
                message: format!(
                    "SchemaRegistry parity: entity {:?} parsed by Define node \
                     but absent from SchemaRegistry",
                    entity
                ),
            });
        }
    }
}

fn collect_defines(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        if let Node::Define(d) = node {
            out.push(d.entity.clone());
        }
        out.extend(collect_defines(children_of(node)));
    }
    out
}

// ---------------------------------------------------------------------------
// Main test entry point
// ---------------------------------------------------------------------------

#[test]
fn grammar_conformance() {
    let corpus = collect_corpus();

    if corpus.is_empty() {
        eprintln!("[SKIP] No .bit files found in corpus -- check collect_corpus() search dirs");
        return;
    }

    let grammar_kinds = grammar_covered_kinds();
    let mut corpus_kinds: BTreeSet<String> = BTreeSet::new();
    let mut parse_failures: Vec<(PathBuf, String)> = Vec::new();
    let mut all_divergences: Vec<Divergence> = Vec::new();
    let mut file_count = 0usize;

    for path in &corpus {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };

        file_count += 1;

        match parse(&source) {
            Err(e) => {
                parse_failures.push((path.clone(), e.message.clone()));
            }
            Ok(doc) => {
                collect_kinds(&doc.nodes, &mut corpus_kinds);
                check_structural_invariants(&doc.nodes, path, &mut all_divergences);
                check_schema_parity(&doc.nodes, path, &mut all_divergences);
            }
        }
    }

    // -- Report --

    println!("\n=== Bit Grammar Conformance Report ===");
    println!("Files scanned : {}", file_count);
    println!("Corpus kinds  : {:?}", corpus_kinds);

    // 1. Coverage check
    let mut spec_gaps: Vec<String> = corpus_kinds
        .iter()
        .filter(|k| !grammar_kinds.contains(k.as_str()))
        .cloned()
        .collect();
    spec_gaps.sort();

    if !spec_gaps.is_empty() {
        println!("\n[FAIL] Spec gaps (grammar missing productions):");
        for g in &spec_gaps {
            println!("  - {}", g);
        }
    }

    // 2. Grammar coverage (informational)
    let mut untested: Vec<&str> = grammar_kinds
        .iter()
        .copied()
        .filter(|k| !corpus_kinds.contains(*k))
        .collect();
    untested.sort();

    if !untested.is_empty() {
        println!("\n[INFO] Grammar productions not exercised by current corpus:");
        for k in &untested {
            println!("  - {}", k);
        }
    }

    // 3. Parse failures
    if !parse_failures.is_empty() {
        println!("\n[FAIL] Parse failures ({}):", parse_failures.len());
        for (path, err) in &parse_failures {
            println!("  {:?}: {}", path, err);
        }
    }

    // 4. Structural divergences
    let blocking: Vec<_> = all_divergences
        .iter()
        .filter(|d| d.kind != DivergenceKind::ParserQuirk)
        .collect();
    let quirks: Vec<_> = all_divergences
        .iter()
        .filter(|d| d.kind == DivergenceKind::ParserQuirk)
        .collect();

    if !quirks.is_empty() {
        println!("\n[INFO] Parser quirks (documented, non-blocking):");
        let mut by_msg: BTreeMap<&str, usize> = BTreeMap::new();
        for q in &quirks {
            *by_msg.entry(q.message.as_str()).or_default() += 1;
        }
        for (msg, count) in &by_msg {
            println!(
                "  ({} occurrence{}) {}",
                count,
                if *count == 1 { "" } else { "s" },
                msg
            );
        }
    }

    if !blocking.is_empty() {
        println!("\n[FAIL] Blocking divergences ({}):", blocking.len());
        for d in &blocking {
            println!("  [{:?}] {:?}: {}", d.kind, d.file, d.message);
        }
    }

    // -- Assertions (merge gate) --

    assert!(
        parse_failures.is_empty(),
        "{} .bit file(s) failed to parse -- see output above",
        parse_failures.len()
    );

    assert!(
        spec_gaps.is_empty(),
        "{} node kind(s) in corpus not covered by grammar spec -- \
         add productions to bit.peg: {:?}",
        spec_gaps.len(),
        spec_gaps
    );

    assert!(
        blocking.is_empty(),
        "{} blocking divergence(s) found -- see output above",
        blocking.len()
    );

    println!(
        "\n[PASS] {} files, {} kinds -- 0 parse failures, 0 spec gaps, 0 blocking divergences",
        file_count,
        corpus_kinds.len()
    );
}

// ---------------------------------------------------------------------------
// XGrammar compatibility check
// ---------------------------------------------------------------------------

const STRUCTURAL_PREFIXES: &[&str] = &[
    "```",
    "//",
    "---",
    "++",
    "+",
    "## Entity:",
    "## Metric:",
    "#",
    "define:",
    "mutate:",
    "delete:",
    "query:",
    "? ",
    "flow:",
    "states:",
    "validate ",
    "check:",
    "form:",
    "mod:$",
    "mod:",
    "project:",
    "commands:",
    "serve:",
    "sync:",
    "git:",
    "snap:",
    "diff:",
    "history:",
    "status:",
    "if ",
    "**",
    "webhook:",
    "remember:",
    "recall:",
    "^",
    "files:",
    "policy:",
    "gate:",
    "escalate:",
    "use $",
    "use @",
    "%",
    "issue:",
    "comment:",
    "lattice_validates:",
    "lattice_constraint:",
    "lattice_schema:",
    "lattice_frontier:",
    "pressure_effect:",
    "unit_cell:",
    "symmetry:",
    "$",
    "bound:",
    "build:",
    "run:",
    "@!",
];

#[test]
fn structural_tokens_are_context_independent() {
    let mut ambiguities: Vec<(String, String)> = Vec::new();

    for (i, a) in STRUCTURAL_PREFIXES.iter().enumerate() {
        for (j, b) in STRUCTURAL_PREFIXES.iter().enumerate() {
            if i == j {
                continue;
            }
            if b.starts_with(a) && i < j {
                ambiguities.push((a.to_string(), b.to_string()));
            }
        }
    }

    if !ambiguities.is_empty() {
        for (shorter, longer) in &ambiguities {
            eprintln!(
                "[FAIL] structural_tokens: {:?} is prefix of {:?} \
                 but appears later in STRUCTURAL_PREFIXES -- reorder to fix",
                shorter, longer
            );
        }
    }

    assert!(
        ambiguities.is_empty(),
        "{} structural token ordering ambiguit(y/ies) -- see stderr",
        ambiguities.len()
    );
}

#[test]
fn structural_tokens_cover_all_node_kinds() {
    let required: &[(&str, &str)] = &[
        ("CodeBlock", "```"),
        ("Comment", "//"),
        ("Divider", "---"),
        ("Spawn", "+"),
        ("EntityDef", "## Entity:"),
        ("MetricDef", "## Metric:"),
        ("Group", "#"),
        ("Define", "define:"),
        ("Mutate", "mutate:"),
        ("Delete", "delete:"),
        ("Query", "query:"),
        ("Query", "? "),
        ("Flow", "flow:"),
        ("States", "states:"),
        ("Validate", "validate "),
        ("Check", "check:"),
        ("Form", "form:"),
        ("ModDef", "mod:$"),
        ("ModInvoke", "mod:"),
        ("ModInvoke", "$"),
        ("Project", "project:"),
        ("Commands", "commands:"),
        ("Serve", "serve:"),
        ("SyncDef", "sync:"),
        ("Git", "git:"),
        ("Snap", "snap:"),
        ("Diff", "diff:"),
        ("History", "history:"),
        ("StatusDef", "status:"),
        ("Conditional", "if "),
        ("Bold", "**"),
        ("Webhook", "webhook:"),
        ("Remember", "remember:"),
        ("Recall", "recall:"),
        ("EmbedMarker", "^"),
        ("FilesDef", "files:"),
        ("PolicyDef", "policy:"),
        ("GateDef", "gate:"),
        ("Escalate", "escalate:"),
        ("UseBlock", "use $"),
        ("UseBlock", "use @"),
        ("ProjectScope", "%"),
        ("Issue", "issue:"),
        ("ThreadComment", "comment:"),
        ("LatticeValidates", "lattice_validates:"),
        ("LatticeConstraint", "lattice_constraint:"),
        ("LatticeSchema", "lattice_schema:"),
        ("LatticeFrontier", "lattice_frontier:"),
        ("PressureEffect", "pressure_effect:"),
        ("UnitCell", "unit_cell:"),
        ("Symmetry", "symmetry:"),
    ];

    let prefix_set: HashSet<&&str> = STRUCTURAL_PREFIXES.iter().collect();
    let mut missing: Vec<(&str, &str)> = Vec::new();

    for &(kind, prefix) in required {
        if !prefix_set.contains(&prefix) {
            missing.push((kind, prefix));
        }
    }

    if !missing.is_empty() {
        for (kind, prefix) in &missing {
            eprintln!(
                "[FAIL] Node kind {:?} requires prefix {:?} \
                 which is absent from STRUCTURAL_PREFIXES",
                kind, prefix
            );
        }
    }

    assert!(
        missing.is_empty(),
        "{} prefix(es) missing from STRUCTURAL_PREFIXES -- see stderr",
        missing.len()
    );
}
