use crate::types::*;

/// Format a parsed `.bit` `Document` into canonical source text.
///
/// Properties guaranteed:
/// - Idempotent: `format(parse(format(s))) == format(parse(s))`
/// - 2-space indentation throughout
/// - Trailing newline on output
/// - No trailing whitespace on any line
/// - At most one blank line between top-level block constructs
pub fn format(doc: &Document) -> String {
    let mut buf = Formatter::new();
    buf.emit_nodes(&doc.nodes, 0);
    buf.finish()
}

// ── Internal formatter ───────────────────────────────────────────

struct Formatter {
    lines: Vec<String>,
}

impl Formatter {
    fn new() -> Self {
        Formatter { lines: Vec::new() }
    }

    fn finish(mut self) -> String {
        // Strip trailing blank lines before adding the single trailing newline.
        while self.lines.last().is_some_and(|l| l.is_empty()) {
            self.lines.pop();
        }
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }

    /// Emit one line at `depth` indent levels (2 spaces each).
    /// Trailing whitespace is stripped from `text` before writing.
    fn line(&mut self, depth: usize, text: &str) {
        let text = text.trim_end();
        if text.is_empty() {
            self.lines.push(String::new());
        } else {
            let indent = "  ".repeat(depth);
            self.lines.push(format!("{}{}", indent, text));
        }
    }

    /// Insert a blank line if the last line is not already blank.
    fn blank_if_needed(&mut self) {
        if self.lines.last().is_none_or(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
    }

    // ── Node list ────────────────────────────────────────────────

    fn emit_nodes(&mut self, nodes: &[Node], depth: usize) {
        let mut prev_was_block = false;
        for node in nodes {
            let is_block = is_block_node(node);
            // Separate block-level constructs at the top level with a blank line.
            if depth == 0 && is_block && prev_was_block {
                self.blank_if_needed();
            }
            self.emit_node(node, depth);
            prev_was_block = is_block;
        }
    }

    fn emit_node(&mut self, node: &Node, depth: usize) {
        match node {
            Node::Group(g) => self.emit_group(g, depth),
            Node::Task(t) => self.emit_task(t, depth),
            Node::Prose(p) => self.line(depth, &p.text),
            Node::Comment(c) => self.line(depth, &format!("// {}", c.text)),
            Node::Spawn(s) => match s {
                Spawn::Parallel => self.line(depth, "+"),
                Spawn::Sequential => self.line(depth, "++"),
            },
            Node::Divider => self.line(depth, "---"),
            Node::Define(d) => self.emit_define(d, depth),
            Node::Mutate(m) => self.emit_mutate(m, depth),
            Node::Delete(d) => self.emit_delete(d, depth),
            Node::Query(q) => self.emit_query(q, depth),
            Node::Variable(v) => self.emit_variable(v, depth),
            Node::Flow(f) => self.emit_flow(f, depth),
            Node::States(s) => self.emit_states(s, depth),
            Node::Validate(v) => self.emit_validate(v, depth),
            Node::Check(c) => self.emit_check(c, depth),
            Node::Form(f) => self.emit_form(f, depth),
            Node::ModDef(m) => self.emit_mod_def(m, depth),
            Node::ModInvoke(m) => self.emit_mod_invoke(m, depth),
            Node::Git(g) => self.emit_git(g, depth),
            Node::Conditional(c) => self.emit_conditional(c, depth),
            Node::Snap(s) => self.line(depth, &format!("snap: \"{}\"", s.name)),
            Node::Diff(d) => self.emit_diff(d, depth),
            Node::History(h) => self.emit_history(h, depth),
            Node::StatusDef(s) => self.emit_status_def(s, depth),
            Node::Routine(r) => self.emit_routine(r, depth),
            Node::Bold(b) => self.line(depth, &format!("**{}**", b.text)),
            Node::Webhook(w) => self.emit_webhook(w, depth),
            Node::UseBlock(u) => self.emit_use_block(u, depth),
            Node::Remember(r) => self.line(depth, &format!("remember: \"{}\"", r.content)),
            Node::Recall(r) => self.line(depth, &format!("recall: \"{}\"", r.query)),
            Node::EmbedMarker(e) => self.line(depth, &format!("^{}", e.tag)),
            Node::FilesDef(f) => self.emit_files_def(f, depth),
            Node::PolicyDef(p) => self.emit_policy_def(p, depth),
            Node::Escalate(e) => self.line(depth, &format!("escalate: {}", e.target)),
            Node::SyncDef(s) => self.emit_sync_def(s, depth),
            Node::EntityDef(e) => self.emit_entity_def(e, depth),
            Node::MetricDef(m) => self.emit_metric_def(m, depth),
            Node::GateDef(g) => self.emit_gate_def(g, depth),
            Node::LatticeValidates(lv) => self.emit_lattice_validates(lv, depth),
            Node::LatticeConstraint(lc) => self.emit_lattice_constraint(lc, depth),
            Node::LatticeSchema(ls) => self.emit_lattice_schema(ls, depth),
            Node::LatticeFrontier(lf) => self.emit_lattice_frontier(lf, depth),
            Node::PressureEffect(pe) => self.emit_pressure_effect(pe, depth),
            Node::UnitCell(uc) => self.emit_unit_cell(uc, depth),
            Node::Symmetry(sym) => self.emit_symmetry(sym, depth),
            Node::CodeBlock(cb) => self.emit_code_block(cb, depth),
            Node::Serve(sd) => self.emit_serve_def(sd, depth),
            Node::Issue(i) => self.emit_issue(i, depth),
            Node::ThreadComment(tc) => self.emit_thread_comment(tc, depth),
            Node::Commands(cd) => self.emit_commands_def(cd, depth),
            Node::Project(p) => self.emit_project_def(p, depth),
            Node::ProjectScope(ps) => self.emit_project_scope(ps, depth),
            Node::BoundDef(b) => {
                self.line(depth, &format!("bound:{}", b.name));
                for child in &b.children {
                    self.emit_node(child, depth + 1);
                }
            }
            Node::BuildDef(b) => {
                self.line(depth, &format!("build:{}", b.name));
                for child in &b.children {
                    self.emit_node(child, depth + 1);
                }
            }
            Node::RunDef(r) => {
                self.line(depth, &format!("run:{}", r.name));
                for child in &r.children {
                    self.emit_node(child, depth + 1);
                }
            }
            Node::Directive(d) => {
                self.line(depth, &format!("@{} {}", d.kind, d.value));
            }
        }
    }

    // ── Group ────────────────────────────────────────────────────

    fn emit_group(&mut self, g: &Group, depth: usize) {
        let hashes = "#".repeat(g.depth as usize);
        let mut header = format!("{} {}", hashes, g.name);
        for atom in &g.atoms {
            header.push(' ');
            header.push_str(&format_atom(atom));
        }
        for gate in &g.gates {
            header.push(' ');
            header.push_str(&format_gate(gate));
        }
        self.line(depth, &header);
        self.emit_nodes(&g.children, depth + 1);
    }

    // ── Task ─────────────────────────────────────────────────────

    fn emit_task(&mut self, t: &Task, depth: usize) {
        // TaskMarker in types.rs does NOT have a `label` field — label is on Task.
        let marker_str = format_full_task_marker(&t.marker, &t.label);
        let mut line_text = format!("{} {}", marker_str, t.text.trim());
        for gate in &t.gates {
            line_text.push(' ');
            line_text.push_str(&format_gate(gate));
        }
        if let Some(closes) = t.closes {
            line_text.push_str(&format!(" git:closes #{}", closes));
        }
        self.line(depth, &line_text);

        if let Some(dep) = &t.depends {
            self.line(depth + 1, &format!("depends: {}", dep));
        }
        if let Some(val) = &t.validate {
            self.line(depth + 1, &format!("validate: {}", val));
        }
        if let Some(status) = &t.status {
            self.line(depth + 1, &format!("status: {}", status));
        }

        self.emit_nodes(&t.children, depth + 1);

        if let Some(pass_nodes) = &t.on_pass {
            self.line(depth + 1, "on_pass:");
            self.emit_nodes(pass_nodes, depth + 2);
        }
        if let Some(fail_nodes) = &t.on_fail {
            self.line(depth + 1, "on_fail:");
            self.emit_nodes(fail_nodes, depth + 2);
        }
        if let Some(arms) = &t.match_arms {
            for arm in arms {
                self.line(depth + 1, &format!("match {}:", arm.pattern));
                self.emit_nodes(&arm.children, depth + 2);
            }
        }
    }

    // ── Define ───────────────────────────────────────────────────

    fn emit_define(&mut self, d: &Define, depth: usize) {
        let entity_str = format_entity_ref(
            &d.entity,
            d.mod_scope.as_deref(),
            d.workspace_scope.as_deref(),
        );
        let mut header = format!("define:{}", entity_str);
        for atom in &d.atoms {
            header.push(' ');
            header.push_str(&format_atom(atom));
        }
        if let Some(from) = &d.from_scope {
            header.push_str(&format!(" from @{}", from));
        }
        self.line(depth, &header);
        for field in &d.fields {
            self.line(depth + 1, &format_field_def(field));
        }
    }

    // ── Mutate ───────────────────────────────────────────────────

    fn emit_mutate(&mut self, m: &Mutate, depth: usize) {
        let entity_str = format_entity_ref(
            &m.entity,
            m.mod_scope.as_deref(),
            m.workspace_scope.as_deref(),
        );
        let mut header = format!("mutate:{}", entity_str);
        if let Some(id) = &m.id {
            header.push(':');
            header.push_str(id);
        }
        if let Some(gate) = &m.gate {
            header.push(' ');
            header.push_str(&format_gate(gate));
        }
        self.line(depth, &header);
        if let Some(batch) = &m.batch {
            for record in batch {
                self.line(depth + 1, &format!("- id: {}", record.id));
                for (k, v) in &record.fields {
                    self.line(depth + 2, &format!("{}: {}", k, v));
                }
            }
        } else {
            for (k, v) in &m.fields {
                self.line(depth + 1, &format!("{}: {}", k, v));
            }
        }
    }

    // ── Delete ───────────────────────────────────────────────────

    fn emit_delete(&mut self, d: &Delete, depth: usize) {
        let entity_str = format_entity_ref(
            &d.entity,
            d.mod_scope.as_deref(),
            d.workspace_scope.as_deref(),
        );
        self.line(depth, &format!("delete:{}:{}", entity_str, d.id));
    }

    // ── Query ────────────────────────────────────────────────────

    fn emit_query(&mut self, q: &Query, depth: usize) {
        let entity_str = format_entity_ref(
            &q.entity,
            q.mod_scope.as_deref(),
            q.workspace_scope.as_deref(),
        );
        let plural_s = if q.plural { "s" } else { "" };
        self.line(depth, &format!("query:{}{}", entity_str, plural_s));
        if let Some(filter) = &q.filter {
            self.line(depth + 1, &format!("filter: {}", filter));
        }
        if let Some(sort) = &q.sort {
            self.line(depth + 1, &format!("sort: {}", sort));
        }
        if let Some(limit) = q.limit {
            self.line(depth + 1, &format!("limit: {}", limit));
        }
        if let Some(include) = &q.include {
            self.line(depth + 1, &format!("include: {}", include.join(", ")));
        }
        if let Some(snap) = &q.from_snapshot {
            self.line(depth + 1, &format!("from_snapshot: {}", snap));
        }
    }

    // ── Variable ─────────────────────────────────────────────────

    fn emit_variable(&mut self, v: &Variable, depth: usize) {
        let rhs = match &v.value {
            VarValue::Literal(s) => s.clone(),
            VarValue::Compute(c) => {
                if c.live {
                    format!("||{}||", c.expr)
                } else {
                    format!("|{}|", c.expr)
                }
            }
            VarValue::Ref(r) => format_ref(r),
        };
        self.line(depth, &format!("{} = {}", v.name, rhs));
    }

    // ── Flow / States ────────────────────────────────────────────

    fn emit_flow(&mut self, f: &Flow, depth: usize) {
        self.line(depth, "flow:");
        for edge in &f.edges {
            self.line(depth + 1, &format_flow_edge(edge));
        }
    }

    fn emit_states(&mut self, s: &StatesDef, depth: usize) {
        self.line(depth, "states:");
        for edge in &s.transitions {
            self.line(depth + 1, &format_flow_edge(edge));
        }
    }

    // ── Validate ─────────────────────────────────────────────────

    fn emit_validate(&mut self, v: &ValidateDef, depth: usize) {
        self.line(depth, &format!("validate {}:", v.name));
        emit_attachment_meta_lines(self, &v.meta, depth + 1);
        self.emit_nodes(&v.children, depth + 1);
    }

    // ── Check ────────────────────────────────────────────────────

    fn emit_check(&mut self, c: &CheckDef, depth: usize) {
        self.line(depth, &format!("check: {}", c.name));
        emit_attachment_meta_lines(self, &c.meta, depth + 1);
        for (k, v) in &c.body {
            self.line(depth + 1, &format!("{}: {}", k, v));
        }
    }

    // ── Form ─────────────────────────────────────────────────────

    fn emit_form(&mut self, f: &FormDef, depth: usize) {
        self.line(depth, &format!("form: {}", f.name));
        if let Some(sv) = f.schema_version {
            self.line(depth + 1, &format!("schema_version: {}", sv));
        }
        if let Some(ul) = &f.ui_layout {
            self.line(depth + 1, &format!("ui_layout: {}", ul));
        }
        for page in &f.ui_pages {
            self.line(depth + 1, &format!("ui_page: {}", page));
        }
        if let Some(canon) = &f.storage.canonical {
            self.line(depth + 1, &format!("storage_canonical: {}", canon));
        }
        if let Some(ent) = &f.storage.entity {
            self.line(depth + 1, &format!("storage_entity: {}", ent));
        }
        if let Some(duck) = &f.storage.duckdb {
            self.line(depth + 1, &format!("storage_duckdb: {}", duck));
        }
        for proj in &f.projections {
            self.line(
                depth + 1,
                &format!("projection_{}: {}", proj.target, proj.mapping),
            );
        }
        for field in &f.fields {
            self.line(depth + 1, &format_field_def(field));
        }
    }

    // ── Mod def / invoke ─────────────────────────────────────────

    fn emit_mod_def(&mut self, m: &ModDef, depth: usize) {
        let version_suffix = if m.versioned { "@versioned" } else { "" };
        self.line(depth, &format!("mod:${}{}", m.name, version_suffix));
        if let Some(kind) = &m.kind {
            self.line(depth + 1, &format!("kind: {}", kind));
        }
        if let Some(desc) = &m.description {
            self.line(depth + 1, &format!("description: {}", desc));
        }
        if let Some(triggers) = &m.trigger {
            self.line(depth + 1, &format!("trigger: {}", triggers.join(", ")));
        }
        for (k, v) in &m.body {
            self.line(depth + 1, &format!("{}: {}", k, v));
        }
    }

    fn emit_mod_invoke(&mut self, m: &ModInvoke, depth: usize) {
        let s = match (&m.method, &m.args) {
            (Some(method), Some(args)) => format!("${}.{}({})", m.name, method, args),
            (Some(method), None) => format!("${}.{}", m.name, method),
            (None, Some(args)) => format!("${}({})", m.name, args),
            (None, None) => format!("${}", m.name),
        };
        self.line(depth, &s);
    }

    // ── Git ──────────────────────────────────────────────────────

    fn emit_git(&mut self, g: &GitOp, depth: usize) {
        self.line(depth, &format!("git:{} {}", g.verb, g.args));
        for (k, v) in &g.body {
            self.line(depth + 1, &format!("{}: {}", k, v));
        }
    }

    // ── Conditional ──────────────────────────────────────────────

    fn emit_conditional(&mut self, c: &Conditional, depth: usize) {
        let expr_str = if c.condition.live {
            format!("||{}||", c.condition.expr)
        } else {
            format!("|{}|", c.condition.expr)
        };
        self.line(depth, &format!("if {}:", expr_str));
        self.emit_nodes(&c.children, depth + 1);
    }

    // ── Diff / History ───────────────────────────────────────────

    fn emit_diff(&mut self, d: &Diff, depth: usize) {
        match &d.from_snapshot {
            Some(snap) => self.line(depth, &format!("diff:{} from snap:{}", d.target, snap)),
            None => self.line(depth, &format!("diff:{}", d.target)),
        }
    }

    fn emit_history(&mut self, h: &HistoryOp, depth: usize) {
        match h.limit {
            Some(limit) => self.line(depth, &format!("history:{} limit:{}", h.target, limit)),
            None => self.line(depth, &format!("history:{}", h.target)),
        }
    }

    // ── Status ───────────────────────────────────────────────────

    fn emit_status_def(&mut self, s: &StatusDef, depth: usize) {
        self.line(depth, &format!("status: {}", s.options.join(" / ")));
    }

    // ── Routine ──────────────────────────────────────────────────

    fn emit_routine(&mut self, r: &Routine, depth: usize) {
        if r.expr.is_empty() {
            self.line(depth, &format!("routine: {}", r.trigger));
        } else {
            self.line(depth, &format!("routine: {} | {}", r.trigger, r.expr));
        }
    }

    // ── Webhook ──────────────────────────────────────────────────

    fn emit_webhook(&mut self, w: &Webhook, depth: usize) {
        self.line(depth, &format!("webhook: {} {}", w.trigger, w.url));
        if let Some(payload) = &w.payload {
            self.line(depth + 1, &format!("payload: {}", payload));
        }
    }

    // ── Use block ────────────────────────────────────────────────

    fn emit_use_block(&mut self, u: &UseBlock, depth: usize) {
        let header = if let Some(entity) = &u.entity {
            let mut s = format!("use @{} from {}", entity, u.mod_name);
            if let Some(alias) = &u.alias {
                s.push_str(&format!(" as @{}", alias));
            }
            s
        } else {
            format!("use {}", u.mod_name)
        };
        self.line(depth, &header);
        for (k, v) in &u.config {
            self.line(depth + 1, &format!("{}: {}", k, v));
        }
    }

    // ── Files / Policy ───────────────────────────────────────────

    fn emit_files_def(&mut self, f: &FilesDef, depth: usize) {
        self.line(depth, "files:");
        for path in &f.paths {
            self.line(depth + 1, &format!("@{}", path));
        }
    }

    fn emit_policy_def(&mut self, p: &PolicyDef, depth: usize) {
        self.line(depth, "policy:");
        for rule in &p.rules {
            let gates_str: String = rule
                .gates
                .iter()
                .map(|g| format!(" {}", format_gate(g)))
                .collect();
            self.line(depth + 1, &format!("@{}{}", rule.path, gates_str));
        }
    }

    // ── Sync ─────────────────────────────────────────────────────

    fn emit_sync_def(&mut self, s: &SyncDef, depth: usize) {
        self.line(depth, &format!("sync: {}", s.name));
        self.line(depth + 1, &format!("class: {}", s.class));
        self.line(depth + 1, &format!("source: {}", s.source));
        self.line(depth + 1, &format!("identity: {}", s.identity));
        self.line(depth + 1, &format!("mode: {}", s.mode));
        self.line(depth + 1, &format!("target: {}", s.target));
        self.line(depth + 1, &format!("schedule: {}", s.schedule));
        self.line(depth + 1, &format!("scope: {}", s.scope));
    }

    // ── Entity / Metric ──────────────────────────────────────────

    fn emit_entity_def(&mut self, e: &EntityDef, depth: usize) {
        self.line(depth, &format!("## Entity: {}", e.name));
        self.line(depth + 1, &format!("source: {}", e.source));
        self.line(depth + 1, &format!("namespace: {}", e.namespace));
        self.line(depth + 1, &format!("identity: {}", e.identity));
        self.line(depth + 1, "fields:");
        for field in &e.fields {
            self.line(depth + 2, &format!("{}: {}", field.name, field.field_type));
        }
    }

    fn emit_metric_def(&mut self, m: &MetricDef, depth: usize) {
        self.line(depth, &format!("## Metric: {}", m.name));
        if let Some(source) = &m.source {
            self.line(depth + 1, &format!("source: {}", source));
        }
        if let Some(grain) = &m.grain {
            self.line(depth + 1, &format!("grain: {}", grain));
        }
        if !m.dimensions.is_empty() {
            self.line(
                depth + 1,
                &format!("dimensions: {}", m.dimensions.join(", ")),
            );
        }
        self.line(depth + 1, &format!("formula: {}", m.formula));
        if m.cross_source {
            self.line(depth + 1, "cross_source: true");
        }
    }

    // ── Gate def ─────────────────────────────────────────────────

    fn emit_gate_def(&mut self, g: &GateDef, depth: usize) {
        self.line(depth, &format!("gate: {}", g.name));
        self.emit_nodes(&g.children, depth + 1);
    }

    // ── Lattice constructs ───────────────────────────────────────

    fn emit_lattice_validates(&mut self, lv: &LatticeValidatesDef, depth: usize) {
        self.line(depth, "lattice_validates:");
        for art in &lv.artifacts {
            self.line(depth + 1, &format!("artifact: {}", art.artifact));
            if let Some(schema) = &art.schema {
                self.line(depth + 2, &format!("schema: {}", schema));
            }
            for check in &art.checks {
                let req = if check.required { "true" } else { "false" };
                let mut s = format!("check: {} required: {}", check.field, req);
                if let Some(min) = check.min_items {
                    s.push_str(&format!(" min_items: {}", min));
                }
                self.line(depth + 2, &s);
            }
        }
        self.emit_nodes(&lv.children, depth + 1);
    }

    fn emit_lattice_constraint(&mut self, lc: &LatticeConstraintDef, depth: usize) {
        let suffix = lc.constraint_type.as_deref().unwrap_or("");
        if suffix.is_empty() {
            self.line(depth, "lattice_constraint:");
        } else {
            self.line(depth, &format!("lattice_constraint: {}", suffix));
        }
        self.line(depth + 1, &format!("rule: {}", lc.rule));
        if !lc.applies_to.is_empty() {
            self.line(
                depth + 1,
                &format!("applies_to: {}", lc.applies_to.join(", ")),
            );
        }
        self.emit_nodes(&lc.children, depth + 1);
    }

    fn emit_lattice_schema(&mut self, ls: &LatticeSchemaDef, depth: usize) {
        self.line(depth, "lattice_schema:");
        for field in &ls.fields {
            self.line(depth + 1, &format!("name: {}", field.name));
            self.line(depth + 2, &format!("type: {}", field.field_type));
            if field.required {
                self.line(depth + 2, "required: true");
            }
        }
        self.emit_nodes(&ls.children, depth + 1);
    }

    fn emit_lattice_frontier(&mut self, lf: &LatticeFrontierDef, depth: usize) {
        self.line(depth, "lattice_frontier:");
        if let Some(es) = &lf.expected_schema {
            self.line(depth + 1, &format!("expected_schema: {}", es));
        }
        for mf in &lf.missing_fields {
            self.line(depth + 1, &format!("missing_field: {}", mf));
        }
        for strategy in &lf.exploration_strategy {
            self.line(depth + 1, &format!("strategy: {}", strategy));
        }
        self.emit_nodes(&lf.children, depth + 1);
    }

    fn emit_pressure_effect(&mut self, pe: &PressureEffectDef, depth: usize) {
        self.line(depth, "pressure_effect:");
        self.line(depth + 1, &format!("dynamic: {}", pe.dynamic));
        if let Some(target) = &pe.target {
            self.line(depth + 1, &format!("target: {}", target));
        }
    }

    fn emit_unit_cell(&mut self, uc: &UnitCellDef, depth: usize) {
        self.line(depth, "unit_cell:");
        self.emit_nodes(&uc.children, depth + 1);
    }

    fn emit_symmetry(&mut self, sym: &SymmetryDef, depth: usize) {
        self.line(depth, "symmetry:");
        self.emit_nodes(&sym.children, depth + 1);
    }

    // ── Code block ───────────────────────────────────────────────

    fn emit_code_block(&mut self, cb: &CodeBlock, depth: usize) {
        let fence = match &cb.lang {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };
        self.line(depth, &fence);
        for content_line in cb.content.lines() {
            self.line(depth, content_line);
        }
        self.line(depth, "```");
    }

    // ── Serve ────────────────────────────────────────────────────

    fn emit_serve_def(&mut self, s: &ServeDef, depth: usize) {
        self.line(depth, &format!("serve:{} | {}", s.target, s.command));
        if let Some(port) = &s.port {
            self.line(depth + 1, &format!("port: {}", port));
        }
        if let Some(open) = &s.open {
            self.line(depth + 1, &format!("open: {}", open));
        }
    }

    // ── Issue ────────────────────────────────────────────────────

    fn emit_issue(&mut self, i: &IssueDef, depth: usize) {
        let mut header = format!("issue: {}", i.title);
        for gate in &i.gates {
            header.push(' ');
            header.push_str(&format_gate(gate));
        }
        self.line(depth, &header);
        if let Some(id) = &i.id {
            self.line(depth + 1, &format!("id: {}", id));
        }
        if let Some(on) = &i.on {
            self.line(depth + 1, &format!("on: {}", on));
        }
        if let Some(status) = &i.status {
            self.line(depth + 1, &format!("status: {}", status));
        }
        if let Some(priority) = &i.priority {
            self.line(depth + 1, &format!("priority: {}", priority));
        }
        if let Some(assignee) = &i.assignee {
            self.line(depth + 1, &format!("assignee: {}", assignee));
        }
        if !i.labels.is_empty() {
            let labels: Vec<String> = i.labels.iter().map(|l| format!(":{}", l)).collect();
            self.line(depth + 1, &format!("labels: [{}]", labels.join(", ")));
        }
        if let Some(est) = i.estimate {
            self.line(depth + 1, &format!("estimate: {}", est));
        }
        if let Some(milestone) = &i.milestone {
            self.line(depth + 1, &format!("milestone: \"{}\"", milestone));
        }
        if let Some(due) = &i.due_date {
            self.line(depth + 1, &format!("due_date: |{}|", due));
        }
        if let Some(desc) = &i.description {
            let desc_lines: Vec<&str> = desc.lines().collect();
            if desc_lines.len() > 1 {
                self.line(depth + 1, "description: |");
                for dl in &desc_lines {
                    self.line(depth + 2, dl);
                }
            } else {
                self.line(depth + 1, &format!("description: \"{}\"", desc));
            }
        }
        self.emit_nodes(&i.children, depth + 1);
    }

    // ── ThreadComment ────────────────────────────────────────────

    fn emit_thread_comment(&mut self, tc: &ThreadComment, depth: usize) {
        let mut header = "comment:".to_string();
        for gate in &tc.gates {
            header.push(' ');
            header.push_str(&format_gate(gate));
        }
        self.line(depth, &header);
        if let Some(on) = &tc.on {
            self.line(depth + 1, &format!("on: {}", on));
        }
        if let Some(author) = &tc.author {
            self.line(depth + 1, &format!("author: {}", author));
        }
        let body_lines: Vec<&str> = tc.body.lines().collect();
        if body_lines.len() > 1 {
            self.line(depth + 1, "body: |");
            for bl in &body_lines {
                self.line(depth + 2, bl);
            }
        } else {
            self.line(depth + 1, &format!("body: \"{}\"", tc.body));
        }
        if !tc.reactions.is_empty() {
            let rxns: Vec<String> = tc.reactions.iter().map(|r| format!(":{}", r)).collect();
            self.line(depth + 1, &format!("reactions: [{}]", rxns.join(", ")));
        }
        if let Some(created) = &tc.created_at {
            self.line(depth + 1, &format!("created_at: |{}|", created));
        }
        self.emit_nodes(&tc.children, depth + 1);
    }

    // ── Commands ─────────────────────────────────────────────────

    fn emit_commands_def(&mut self, cd: &CommandsDef, depth: usize) {
        self.line(depth, "commands:");
        for cmd in &cd.commands {
            self.line(depth + 1, &format!("/{}", cmd.name));
            self.line(depth + 2, &format!("description: {}", cmd.description));
            if !cmd.params.is_empty() {
                self.line(depth + 2, &format!("params: {}", cmd.params.join(", ")));
            }
            self.line(depth + 2, &format!("prompt: {}", cmd.prompt));
        }
    }

    // ── Project ──────────────────────────────────────────────────

    fn emit_project_def(&mut self, p: &ProjectDef, depth: usize) {
        self.line(depth, &format!("project: {}", p.name));
        self.line(depth + 1, &format!("brief: {}", p.brief));
        if let Some(hb) = &p.heartbeat {
            self.line(depth + 1, &format!("heartbeat: {}", hb));
        }
        if let Some(fw) = &p.framework {
            self.line(depth + 1, &format!("framework: {}", fw));
        }
        if let Some(st) = &p.status {
            self.line(depth + 1, &format!("status: {}", st));
        }
        if let Some(fitness) = p.fitness {
            self.line(depth + 1, &format!("fitness: {}", fitness));
        }
        if let Some(pressure) = p.pressure {
            self.line(depth + 1, &format!("pressure: {}", pressure));
        }
        if let Some(phase) = &p.phase {
            self.line(depth + 1, &format!("phase: {}", phase));
        }
        if let Some(inhibited) = &p.inhibited_until {
            self.line(depth + 1, &format!("inhibited_until: {}", inhibited));
        }
        if let Some(completion) = &p.completion {
            self.line(depth + 1, &format!("completion: {}", completion));
        }
        if let Some(kpi) = &p.kpi {
            self.line(depth + 1, &format!("kpi: {}", kpi));
        }
        if let Some(routine) = &p.routine {
            self.line(depth + 1, &format!("routine: {}", routine));
        }
        if let Some(cmds) = &p.commands {
            self.emit_commands_def(cmds, depth + 1);
        }
        if let Some(serve) = &p.serve {
            self.emit_serve_def(serve, depth + 1);
        }
    }

    fn emit_project_scope(&mut self, ps: &ProjectScope, depth: usize) {
        self.line(depth, &format!("%{}", ps.name));
        self.emit_nodes(&ps.children, depth + 1);
    }
}

// ── Free helpers ────────────────────────────────────────────────

/// Returns true for node kinds that are multi-line / block-level constructs
/// and should be separated by blank lines at the top level.
fn is_block_node(node: &Node) -> bool {
    matches!(
        node,
        Node::Group(_)
            | Node::Define(_)
            | Node::Mutate(_)
            | Node::Query(_)
            | Node::Flow(_)
            | Node::States(_)
            | Node::Validate(_)
            | Node::Check(_)
            | Node::Form(_)
            | Node::ModDef(_)
            | Node::Git(_)
            | Node::Conditional(_)
            | Node::Issue(_)
            | Node::ThreadComment(_)
            | Node::Commands(_)
            | Node::Project(_)
            | Node::ProjectScope(_)
            | Node::GateDef(_)
            | Node::SyncDef(_)
            | Node::EntityDef(_)
            | Node::MetricDef(_)
            | Node::LatticeValidates(_)
            | Node::LatticeConstraint(_)
            | Node::LatticeSchema(_)
            | Node::LatticeFrontier(_)
            | Node::UnitCell(_)
            | Node::Symmetry(_)
            | Node::CodeBlock(_)
            | Node::UseBlock(_)
            | Node::FilesDef(_)
            | Node::PolicyDef(_)
    )
}

/// Format a complete task marker string.
///
/// `label` is taken from `Task.label` (not from `TaskMarker`, which has no
/// label field in types.rs).
fn format_full_task_marker(marker: &TaskMarker, label: &Option<String>) -> String {
    let prefix = match &marker.prefix {
        TaskPrefix::None => String::new(),
        TaskPrefix::Parallel => "+".to_string(),
        TaskPrefix::ParallelSubtask => "++".to_string(),
        TaskPrefix::Subtask(n) => "-".repeat(*n as usize),
    };

    let inside = if let Some(seq) = marker.seq {
        match marker.kind {
            TaskKind::Required => format!("{}!", seq),
            TaskKind::Optional => format!("{}o", seq),
            _ => format!("{}", seq),
        }
    } else {
        match label {
            Some(lbl) => match marker.kind {
                TaskKind::Required => format!("{}!", lbl),
                TaskKind::Optional => format!("{}o", lbl),
                TaskKind::Open if marker.priority == Priority::Decision => {
                    format!("{}?", lbl)
                }
                _ => lbl.clone(),
            },
            None => match marker.kind {
                TaskKind::Required => "!".to_string(),
                TaskKind::Optional => "o".to_string(),
                TaskKind::Completed => "x".to_string(),
                TaskKind::Open => " ".to_string(),
            },
        }
    };

    format!("{}[{}]", prefix, inside)
}

fn format_gate(g: &Gate) -> String {
    match &g.body {
        Some(body) => format!("{{{} {}}}", g.name, body),
        None => format!("{{{}}}", g.name),
    }
}

fn format_atom(a: &Atom) -> String {
    match &a.value {
        Some(v) => format!(":{}:{}", a.name, v),
        None => format!(":{}", a.name),
    }
}

fn format_ref(r: &Ref) -> String {
    let mut s = String::new();
    if let Some(ws) = &r.workspace_scope {
        s.push_str(&format!("@workspace:{}/", ws));
    }
    if let Some(ms) = &r.mod_scope {
        s.push_str(&format!("${}.", ms));
    }
    s.push('@');
    s.push_str(&r.path.join("."));
    if r.plural {
        s.push('s');
    }
    s
}

/// Format the entity reference part that goes after a `define:`, `mutate:`,
/// `delete:`, or `query:` keyword.
///
/// The parser strips the leading `@` (or `$Mod.@`) when it stores the entity
/// name, so we must restore it here.  The canonical form is:
///   - `@Entity`                   (no scope)
///   - `$Mod.@Entity`              (mod-scoped)
///   - `@workspace:name.@Entity`   (workspace-scoped)
fn format_entity_ref(
    entity: &str,
    mod_scope: Option<&str>,
    workspace_scope: Option<&str>,
) -> String {
    let mut s = String::new();
    if let Some(ws) = workspace_scope {
        s.push_str(&format!("@workspace:{}.@", ws));
    } else if let Some(ms) = mod_scope {
        s.push_str(&format!("${}.", ms));
        s.push('@');
    } else {
        s.push('@');
    }
    s.push_str(entity);
    s
}

fn format_field_def(f: &FieldDef) -> String {
    let plural = if f.plural { "s" } else { "" };
    let default_str = match &f.default {
        FieldDefault::Str(s) => format!("\"{}\"", s),
        FieldDefault::Int(i) => i.to_string(),
        FieldDefault::Float(fl) => fl.to_string(),
        FieldDefault::Bool(b) => b.to_string(),
        FieldDefault::Atom(a) => format!(":{}", a),
        FieldDefault::Enum(variants) => {
            let vs: Vec<String> = variants.iter().map(|v| format!(":{}", v)).collect();
            format!("[{}]", vs.join(" | "))
        }
        FieldDefault::Ref(r) => format!("@{}", r),
        FieldDefault::List => "[]".to_string(),
        FieldDefault::Timestamp(t) => format!("timestamp({})", t),
        FieldDefault::Nil => "nil".to_string(),
        FieldDefault::Trit(t) => format!("~{}", t),
    };
    format!("{}{}: {}", f.name, plural, default_str)
}

fn format_flow_edge(e: &FlowEdge) -> String {
    let from = if e.from.len() == 1 {
        e.from[0].clone()
    } else {
        format!("[{}]", e.from.join(", "))
    };
    let arrow = if e.parallel { "||>" } else { "->" };
    let to = if e.to.len() == 1 {
        e.to[0].clone()
    } else {
        format!("[{}]", e.to.join(", "))
    };

    let mut s = format!("{} {} {}", from, arrow, to);
    if let Some(label) = &e.label {
        s.push_str(&format!(" \"{}\"", label));
    }
    if let Some(gate) = &e.gate {
        s.push_str(&format!(" gate:{}", gate));
    }
    if let Some(wait) = &e.wait {
        s.push_str(&format!(" wait:{}", wait));
    }
    if let Some(timeout) = &e.timeout {
        s.push_str(&format!(" timeout:{}", timeout));
    }
    s
}

fn emit_attachment_meta_lines(buf: &mut Formatter, meta: &AttachmentMeta, depth: usize) {
    if let Some(id) = &meta.id {
        buf.line(depth, &format!("id: {}", id));
    }
    if !meta.targets.is_empty() {
        buf.line(depth, &format!("targets: {}", meta.targets.join(", ")));
    }
    if !meta.requires.is_empty() {
        buf.line(depth, &format!("requires: {}", meta.requires.join(", ")));
    }
    if !meta.blocks.is_empty() {
        buf.line(depth, &format!("blocks: {}", meta.blocks.join(", ")));
    }
    if !meta.depends_on.is_empty() {
        buf.line(
            depth,
            &format!("depends_on: {}", meta.depends_on.join(", ")),
        );
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    /// Assert that formatting is idempotent:
    ///   format(parse(s)) == format(parse(format(parse(s))))
    fn assert_idempotent(src: &str) {
        let doc1 = parse(src).expect("first parse failed");
        let formatted1 = format(&doc1);
        let doc2 = parse(&formatted1).expect("second parse (of formatted output) failed");
        let formatted2 = format(&doc2);
        assert_eq!(
            formatted1, formatted2,
            "Format is not idempotent.\n--- First pass ---\n{}\n--- Second pass ---\n{}",
            formatted1, formatted2
        );
    }

    #[test]
    fn test_idempotent_tasks_and_groups() {
        assert_idempotent(
            r#"
# My Project

[!] Required task
[ ] Open task
[x] Completed task
-[ ] Subtask
+[!] Parallel required
"#,
        );
    }

    #[test]
    fn test_idempotent_define_and_mutate() {
        assert_idempotent(
            r#"
define:@User
  name: "anonymous"
  age: 0
  active: true

mutate:@User:123
  name: "Alice"
  active: false
"#,
        );
    }

    #[test]
    fn test_idempotent_flow_and_validate() {
        assert_idempotent(
            r#"
flow:
  start -> review
  review -> done

validate MyCheck:
  id: check-1
  targets: alpha, beta
  [ ] verify something
"#,
        );
    }

    #[test]
    fn test_idempotent_code_block() {
        assert_idempotent(
            r#"
# Section

```rust
fn main() {
    println!("hello");
}
```
"#,
        );
    }

    #[test]
    fn test_trailing_newline() {
        let src = "[ ] hello";
        let doc = parse(src).expect("parse failed");
        let out = format(&doc);
        assert!(out.ends_with('\n'), "output must end with a newline");
    }

    #[test]
    fn test_no_trailing_whitespace() {
        let src = "[ ] task with trailing spaces   \n# Group  \n  [ ] child  ";
        let doc = parse(src).expect("parse failed");
        let out = format(&doc);
        for line in out.lines() {
            assert_eq!(
                line,
                line.trim_end(),
                "line has trailing whitespace: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_idempotent_comment_and_divider() {
        assert_idempotent(
            r#"
// This is a comment
---
[ ] a task
// another comment
"#,
        );
    }

    #[test]
    fn test_idempotent_variable() {
        assert_idempotent("count = 42\nname = hello world");
    }

    #[test]
    fn test_idempotent_prose() {
        assert_idempotent("Some prose line here.\nAnother prose line.");
    }
}
