use crate::types::*;

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    for node in &doc.nodes {
        render_node(node, 0, &mut out);
    }
    out.trim_end().to_string()
}

fn render_node(node: &Node, depth: usize, out: &mut String) {
    let indent = "    ".repeat(depth);

    match node {
        Node::Group(g) => {
            let hashes = "#".repeat(g.depth as usize);
            out.push_str(&format!("{}{} {}", indent, hashes, g.name));
            for atom in &g.atoms {
                render_atom(atom, out);
            }
            out.push('\n');
            out.push('\n');
            for child in &g.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::Task(t) => {
            out.push_str(&indent);
            render_task_marker(&t.marker, &t.label, out);
            out.push(' ');
            out.push_str(&t.text);
            for gate in &t.gates {
                out.push_str(&format!(" {{{}}}", gate.name));
            }
            out.push('\n');
            if let Some(dep) = &t.depends {
                out.push_str(&format!("{}    depends: {}\n", indent, dep));
            }
            if let Some(val) = &t.validate {
                out.push_str(&format!("{}    validate: {}\n", indent, val));
            }
            if let Some(st) = &t.status {
                out.push_str(&format!("{}    status: {}\n", indent, st));
            }
            for child in &t.children {
                render_node(child, depth + 1, out);
            }
            if let Some(pass) = &t.on_pass {
                out.push_str(&format!("{}    on_pass:\n", indent));
                for child in pass {
                    render_node(child, depth + 2, out);
                }
            }
            if let Some(fail) = &t.on_fail {
                out.push_str(&format!("{}    on_fail:\n", indent));
                for child in fail {
                    render_node(child, depth + 2, out);
                }
            }
            if let Some(arms) = &t.match_arms {
                out.push_str(&format!("{}    match:\n", indent));
                for arm in arms {
                    out.push_str(&format!("{}        {}:\n", indent, arm.pattern));
                    for child in &arm.children {
                        render_node(child, depth + 3, out);
                    }
                }
            }
        }

        Node::Prose(p) => {
            out.push_str(&indent);
            out.push_str(&p.text);
            out.push('\n');
        }

        Node::Comment(c) => {
            out.push_str(&indent);
            out.push_str("// ");
            out.push_str(&c.text);
            out.push('\n');
        }

        Node::Spawn(s) => {
            out.push('\n');
            match s {
                Spawn::Parallel => out.push_str("+\n"),
                Spawn::Sequential => out.push_str("++\n"),
            }
            out.push('\n');
        }

        Node::Divider => {
            out.push_str("---\n\n");
        }

        Node::Define(d) => {
            out.push_str(&format!("{}define:", indent));
            if let Some(ms) = &d.mod_scope {
                out.push_str(&format!("${}.@{}", ms, d.entity));
            } else if let Some(ws) = &d.workspace_scope {
                out.push_str(&format!("@workspace:{}.@{}", ws, d.entity));
            } else {
                out.push_str(&format!("@{}", d.entity));
            }
            if let Some(scope) = &d.from_scope {
                out.push_str(&format!(" from {}", scope));
            }
            for atom in &d.atoms {
                render_atom(atom, out);
            }
            out.push('\n');
            for field in &d.fields {
                out.push_str(&format!("{}    ", indent));
                render_field_def(field, out);
                out.push('\n');
            }
            out.push('\n');
        }

        Node::Mutate(m) => {
            out.push_str(&format!("{}mutate:", indent));
            if let Some(ms) = &m.mod_scope {
                out.push_str(&format!("${}.@{}", ms, m.entity));
            } else if let Some(ws) = &m.workspace_scope {
                out.push_str(&format!("@workspace:{}.@{}", ws, m.entity));
            } else {
                out.push_str(&format!("@{}", m.entity));
            }
            if let Some(id) = &m.id {
                out.push(':');
                out.push_str(id);
            }
            out.push('\n');
            if let Some(batch) = &m.batch {
                for rec in batch {
                    for (k, v) in &rec.fields {
                        out.push_str(&format!("{}    {}:{}:{}\n", indent, rec.id, k, v));
                    }
                }
            } else {
                for (k, v) in &m.fields {
                    out.push_str(&format!("{}    {}: {}\n", indent, k, v));
                }
            }
            out.push('\n');
        }

        Node::Delete(d) => {
            out.push_str(&format!("{}delete:", indent));
            if let Some(ms) = &d.mod_scope {
                out.push_str(&format!("${}.@{}:{}\n", ms, d.entity, d.id));
            } else if let Some(ws) = &d.workspace_scope {
                out.push_str(&format!("@workspace:{}.@{}:{}\n", ws, d.entity, d.id));
            } else {
                out.push_str(&format!("@{}:{}\n", d.entity, d.id));
            }
        }

        Node::Query(q) => {
            out.push_str(&format!("{}? ", indent));
            if let Some(ms) = &q.mod_scope {
                out.push_str(&format!("${}.@{}", ms, q.entity));
            } else if let Some(ws) = &q.workspace_scope {
                out.push_str(&format!("@workspace:{}.@{}", ws, q.entity));
            } else {
                out.push_str(&q.entity);
            }
            if q.plural {
                out.push_str("(s)");
            }
            if let Some(snap) = &q.from_snapshot {
                out.push_str(&format!(" from snap:{}", snap));
            }
            if let Some(f) = &q.filter {
                out.push_str(&format!(" where {}", f));
            }
            out.push('\n');
            if let Some(s) = &q.sort {
                out.push_str(&format!("{}    sort: {}\n", indent, s));
            }
            if let Some(l) = &q.limit {
                out.push_str(&format!("{}    limit: {}\n", indent, l));
            }
            if let Some(inc) = &q.include {
                out.push_str(&format!("{}    include: {}\n", indent, inc.join(", ")));
            }
        }

        Node::Variable(v) => {
            out.push_str(&format!("{}{} = ", indent, v.name));
            match &v.value {
                VarValue::Literal(s) => out.push_str(s),
                VarValue::Compute(c) => {
                    if c.live {
                        out.push_str(&format!("||{}||", c.expr));
                    } else {
                        out.push_str(&format!("|{}|", c.expr));
                    }
                }
                VarValue::Ref(r) => {
                    out.push('@');
                    out.push_str(&r.path.join(":"));
                }
            }
            out.push('\n');
        }

        Node::Flow(f) => {
            out.push_str(&format!("{}flow:\n", indent));
            for edge in &f.edges {
                render_flow_edge(edge, depth + 1, out);
            }
            out.push('\n');
        }

        Node::States(s) => {
            out.push_str(&format!("{}states:\n", indent));
            for edge in &s.transitions {
                render_flow_edge(edge, depth + 1, out);
            }
            out.push('\n');
        }

        Node::Validate(v) => {
            out.push_str(&format!("{}validate {}:\n", indent, v.name));
            for child in &v.children {
                render_node(child, depth + 1, out);
            }
            out.push('\n');
        }

        Node::Check(c) => {
            out.push_str(&format!("{}check: {}\n", indent, c.name));
            for (key, value) in &c.body {
                out.push_str(&format!("{}    {}: {}\n", indent, key, value));
            }
            out.push('\n');
        }

        Node::Form(f) => {
            out.push_str(&format!("{}form:{}\n", indent, f.name));
            if let Some(schema_version) = f.schema_version {
                out.push_str(&format!(
                    "{}    schema_version: {}\n",
                    indent, schema_version
                ));
            }
            if let Some(layout) = &f.ui_layout {
                out.push_str(&format!("{}    ui_layout: \"{}\"\n", indent, layout));
            }
            for page in &f.ui_pages {
                out.push_str(&format!("{}    ui_page: \"{}\"\n", indent, page));
            }
            if let Some(canonical) = &f.storage.canonical {
                out.push_str(&format!(
                    "{}    storage_canonical: \"{}\"\n",
                    indent, canonical
                ));
            }
            if let Some(entity) = &f.storage.entity {
                out.push_str(&format!("{}    storage_entity: \"{}\"\n", indent, entity));
            }
            if let Some(duckdb) = &f.storage.duckdb {
                out.push_str(&format!("{}    storage_duckdb: \"{}\"\n", indent, duckdb));
            }
            for projection in &f.projections {
                out.push_str(&format!(
                    "{}    projection_{}: \"{}\"\n",
                    indent, projection.target, projection.mapping
                ));
            }
            for field in &f.fields {
                out.push_str(&format!("{}    ", indent));
                render_field_def(field, out);
                out.push('\n');
            }
            out.push('\n');
        }

        Node::ModDef(m) => {
            out.push_str(&format!("{}mod:${}\n", indent, m.name));
            for (k, v) in &m.body {
                if k == "_" {
                    out.push_str(&format!("{}    {}\n", indent, v));
                } else {
                    out.push_str(&format!("{}    {}: {}\n", indent, k, v));
                }
            }
            out.push('\n');
        }

        Node::ModInvoke(m) => {
            out.push_str(&format!("{}${}", indent, m.name));
            if let Some(method) = &m.method {
                out.push('.');
                out.push_str(method);
            }
            if let Some(args) = &m.args {
                out.push_str(&format!("({})", args));
            }
            out.push('\n');
        }

        Node::Git(g) => {
            out.push_str(&format!("{}git:{}", indent, g.verb));
            if !g.args.is_empty() {
                out.push(' ');
                out.push_str(&g.args);
            }
            out.push('\n');
            for (k, v) in &g.body {
                out.push_str(&format!("{}    {}: {}\n", indent, k, v));
            }
        }

        Node::Conditional(c) => {
            if c.condition.live {
                out.push_str(&format!("{}if ||{}||:\n", indent, c.condition.expr));
            } else {
                out.push_str(&format!("{}if |{}|:\n", indent, c.condition.expr));
            }
            for child in &c.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::Snap(s) => {
            out.push_str(&format!("{}snap: \"{}\"\n", indent, s.name));
        }

        Node::Diff(d) => {
            out.push_str(&format!("{}diff: {}", indent, d.target));
            if let Some(snap) = &d.from_snapshot {
                out.push_str(&format!(" from snap:{}", snap));
            }
            out.push('\n');
        }

        Node::History(h) => {
            out.push_str(&format!("{}history: {}", indent, h.target));
            if let Some(l) = h.limit {
                out.push_str(&format!(" limit: {}", l));
            }
            out.push('\n');
        }

        Node::StatusDef(s) => {
            let opts: Vec<String> = s.options.iter().map(|o| format!("\"{}\"", o)).collect();
            out.push_str(&format!("{}status: {}\n", indent, opts.join("/")));
        }

        Node::Routine(r) => {
            out.push_str(&format!("{}routine:{}:|{}|\n", indent, r.trigger, r.expr));
        }

        Node::Bold(b) => {
            out.push_str(&format!("{}**{}**\n", indent, b.text));
        }

        Node::Webhook(w) => {
            out.push_str(&format!(
                "{}webhook: on:|{}| url: \"{}\"",
                indent, w.trigger, w.url
            ));
            if let Some(p) = &w.payload {
                out.push_str(&format!(" payload: \"{}\"", p));
            }
            out.push('\n');
        }

        Node::UseBlock(u) => {
            out.push_str(&format!("{}use {} config:\n", indent, u.mod_name));
            for (k, v) in &u.config {
                out.push_str(&format!("{}    {}: {}\n", indent, k, v));
            }
        }

        Node::Remember(r) => {
            out.push_str(&format!("{}remember: \"{}\"\n", indent, r.content));
        }

        Node::Recall(r) => {
            out.push_str(&format!("{}recall: \"{}\"\n", indent, r.query));
        }

        Node::EmbedMarker(e) => {
            out.push_str(&format!("{}^{}\n", indent, e.tag));
        }

        Node::FilesDef(f) => {
            out.push_str(&format!("{}files:\n", indent));
            for p in &f.paths {
                out.push_str(&format!("{}    @{}\n", indent, p));
            }
        }

        Node::PolicyDef(p) => {
            out.push_str(&format!("{}policy:\n", indent));
            for rule in &p.rules {
                out.push_str(&format!("{}    @{}", indent, rule.path));
                if !rule.gates.is_empty() {
                    out.push_str(": ");
                    for g in &rule.gates {
                        out.push('{');
                        out.push_str(&g.name);
                        if let Some(b) = &g.body {
                            out.push(' ');
                            out.push_str(b);
                        }
                        out.push('}');
                    }
                }
                out.push('\n');
            }
        }

        Node::Escalate(e) => {
            out.push_str(&format!("{}escalate: {}\n", indent, e.target));
        }

        Node::SyncDef(s) => {
            out.push_str(&format!("{}sync:{}\n", indent, s.name));
            if !s.class.is_empty() {
                out.push_str(&format!("{}    class: {}\n", indent, s.class));
            }
            if !s.source.is_empty() {
                out.push_str(&format!("{}    source: {}\n", indent, s.source));
            }
            if !s.identity.is_empty() {
                out.push_str(&format!("{}    identity: {}\n", indent, s.identity));
            }
        }

        Node::GateDef(g) => {
            out.push_str(&format!("{}gate:{}\n", indent, g.name));
            for child in &g.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::CodeBlock(cb) => {
            out.push_str(&indent);
            out.push_str("```");
            if let Some(lang) = &cb.lang {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(&cb.content);
            out.push('\n');
            out.push_str(&indent);
            out.push_str("```\n");
        }

        Node::Serve(s) => {
            out.push_str(&format!("{}serve:{}|{}|\n", indent, s.target, s.command));
            if let Some(p) = &s.port {
                out.push_str(&format!("{}  port: {}\n", indent, p));
            }
            if let Some(o) = &s.open {
                out.push_str(&format!("{}  open: {}\n", indent, o));
            }
        }

        Node::EntityDef(e) => {
            out.push_str(&format!("{}## Entity: {}\n", indent, e.name));
            out.push_str(&format!("{}  source: {}\n", indent, e.source));
            out.push_str(&format!("{}  namespace: {}\n", indent, e.namespace));
            out.push_str(&format!("{}  identity: {}\n", indent, e.identity));
            if !e.fields.is_empty() {
                out.push_str(&format!("{}  fields:\n", indent));
                for f in &e.fields {
                    out.push_str(&format!("{}    - {}: {}\n", indent, f.name, f.field_type));
                }
            }
        }

        Node::MetricDef(m) => {
            out.push_str(&format!("{}## Metric: {}\n", indent, m.name));
            if let Some(src) = &m.source {
                out.push_str(&format!("{}  source: {}\n", indent, src));
            }
            if m.cross_source {
                out.push_str(&format!("{}  cross_source: true\n", indent));
            }
            if let Some(grain) = &m.grain {
                out.push_str(&format!("{}  grain: {}\n", indent, grain));
            }
            if !m.dimensions.is_empty() {
                out.push_str(&format!(
                    "{}  dimensions: [{}]\n",
                    indent,
                    m.dimensions.join(", ")
                ));
            }
            if !m.formula.is_empty() {
                if m.formula.contains('\n') {
                    out.push_str(&format!("{}  formula: |\n", indent));
                    for line in m.formula.lines() {
                        out.push_str(&format!("{}    {}\n", indent, line));
                    }
                } else {
                    out.push_str(&format!("{}  formula: {}\n", indent, m.formula));
                }
            }
        }

        Node::Commands(c) => {
            out.push_str(&format!("{}commands:\n", indent));
            for cmd in &c.commands {
                out.push_str(&format!("{}  /{}\n", indent, cmd.name));
                out.push_str(&format!("{}    description: {}\n", indent, cmd.description));
                if !cmd.params.is_empty() {
                    out.push_str(&format!(
                        "{}    params: {}\n",
                        indent,
                        cmd.params.join(", ")
                    ));
                }
                if !cmd.prompt.is_empty() {
                    if cmd.prompt.contains('\n') {
                        out.push_str(&format!("{}    prompt: |\n", indent));
                        for line in cmd.prompt.lines() {
                            out.push_str(&format!("{}      {}\n", indent, line));
                        }
                    } else {
                        out.push_str(&format!("{}    prompt: {}\n", indent, cmd.prompt));
                    }
                }
            }
        }

        Node::ProjectScope(ps) => {
            out.push_str(&format!("{}%{}\n", indent, ps.name));
            for child in &ps.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::Project(p) => {
            out.push_str(&format!("{}project:{}\n", indent, p.name));
            out.push_str(&format!("{}  brief: {}\n", indent, p.brief));
            if let Some(h) = &p.heartbeat {
                out.push_str(&format!("{}  heartbeat: {}\n", indent, h));
            }
            if let Some(f) = &p.framework {
                out.push_str(&format!("{}  framework: {}\n", indent, f));
            }
            if let Some(s) = &p.status {
                out.push_str(&format!("{}  status: {}\n", indent, s));
            }
            if let Some(c) = &p.commands {
                // Render as an indented commands sub-block
                render_node(&Node::Commands(c.clone()), depth + 1, out);
            }
            if let Some(s) = &p.serve {
                // Render as an indented serve sub-block
                render_node(&Node::Serve(s.clone()), depth + 1, out);
            }
            if let Some(f) = &p.fitness {
                out.push_str(&format!("{}  fitness: {}\n", indent, f));
            }
            if let Some(pr) = &p.pressure {
                out.push_str(&format!("{}  pressure: {}\n", indent, pr));
            }
            if let Some(ph) = &p.phase {
                out.push_str(&format!("{}  phase: {}\n", indent, ph));
            }
            if let Some(inh) = &p.inhibited_until {
                out.push_str(&format!("{}  inhibited_until: {}\n", indent, inh));
            }
            if let Some(comp) = &p.completion {
                out.push_str(&format!("{}  completion: {}\n", indent, comp));
            }
            if let Some(k) = &p.kpi {
                out.push_str(&format!("{}  kpi: {}\n", indent, k));
            }
            if let Some(r) = &p.routine {
                out.push_str(&format!("{}  routine: {}\n", indent, r));
            }
        }

        Node::Issue(issue) => {
            out.push_str(&format!("{}issue: {}", indent, issue.title));
            for gate in &issue.gates {
                out.push_str(&format!(" {{{}}}", gate.name));
            }
            out.push('\n');
            if let Some(id) = &issue.id {
                out.push_str(&format!("{}    id: {}\n", indent, id));
            }
            if let Some(on) = &issue.on {
                out.push_str(&format!("{}    on: {}\n", indent, on));
            }
            if let Some(status) = &issue.status {
                out.push_str(&format!("{}    status: :{}\n", indent, status));
            }
            if let Some(priority) = &issue.priority {
                out.push_str(&format!("{}    priority: :{}\n", indent, priority));
            }
            if let Some(assignee) = &issue.assignee {
                out.push_str(&format!("{}    assignee: {}\n", indent, assignee));
            }
            if !issue.labels.is_empty() {
                let label_strs: Vec<String> =
                    issue.labels.iter().map(|l| format!(":{}", l)).collect();
                out.push_str(&format!(
                    "{}    labels: [{}]\n",
                    indent,
                    label_strs.join(", ")
                ));
            }
            if let Some(estimate) = &issue.estimate {
                out.push_str(&format!("{}    estimate: {}\n", indent, estimate));
            }
            if let Some(milestone) = &issue.milestone {
                out.push_str(&format!("{}    milestone: \"{}\"\n", indent, milestone));
            }
            if let Some(due_date) = &issue.due_date {
                out.push_str(&format!("{}    due_date: |{}|\n", indent, due_date));
            }
            if let Some(desc) = &issue.description {
                if desc.contains('\n') {
                    out.push_str(&format!("{}    description: |\n", indent));
                    for line in desc.lines() {
                        out.push_str(&format!("{}        {}\n", indent, line));
                    }
                } else {
                    out.push_str(&format!("{}    description: \"{}\"\n", indent, desc));
                }
            }
            for child in &issue.children {
                render_node(child, depth + 1, out);
            }
            out.push('\n');
        }

        Node::LatticeValidates(lv) => {
            out.push_str(&format!("{}lattice_validates:\n", indent));
            for art in &lv.artifacts {
                out.push_str(&format!("{}    artifact: {}\n", indent, art.artifact));
                if let Some(schema) = &art.schema {
                    out.push_str(&format!("{}    schema: {}\n", indent, schema));
                }
                for check in &art.checks {
                    out.push_str(&format!("{}    field: {}\n", indent, check.field));
                    if check.required {
                        out.push_str(&format!("{}    required: true\n", indent));
                    }
                    if let Some(min) = check.min_items {
                        out.push_str(&format!("{}    min_items: {}\n", indent, min));
                    }
                }
            }
            for child in &lv.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::LatticeConstraint(lc) => {
            out.push_str(&format!("{}lattice_constraint:\n", indent));
            if let Some(ct) = &lc.constraint_type {
                out.push_str(&format!("{}    type: {}\n", indent, ct));
            }
            if !lc.rule.is_empty() {
                out.push_str(&format!("{}    rule: {}\n", indent, lc.rule));
            }
            for at in &lc.applies_to {
                out.push_str(&format!("{}    applies_to: {}\n", indent, at));
            }
            for child in &lc.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::LatticeSchema(ls) => {
            out.push_str(&format!("{}lattice_schema:\n", indent));
            for field in &ls.fields {
                out.push_str(&format!("{}    name: {}\n", indent, field.name));
                out.push_str(&format!("{}    type: {}\n", indent, field.field_type));
                if field.required {
                    out.push_str(&format!("{}    required: true\n", indent));
                }
            }
            for child in &ls.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::LatticeFrontier(lf) => {
            out.push_str(&format!("{}lattice_frontier:\n", indent));
            if let Some(schema) = &lf.expected_schema {
                out.push_str(&format!("{}    expected_schema: {}\n", indent, schema));
            }
            for mf in &lf.missing_fields {
                out.push_str(&format!("{}    missing_field: {}\n", indent, mf));
            }
            for es in &lf.exploration_strategy {
                out.push_str(&format!("{}    strategy: {}\n", indent, es));
            }
            for child in &lf.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::PressureEffect(pe) => {
            out.push_str(&format!("{}pressure_effect:\n", indent));
            if !pe.dynamic.is_empty() {
                out.push_str(&format!("{}    dynamic: {}\n", indent, pe.dynamic));
            }
            if let Some(target) = &pe.target {
                out.push_str(&format!("{}    target: {}\n", indent, target));
            }
        }

        Node::UnitCell(uc) => {
            out.push_str(&format!("{}unit_cell:\n", indent));
            for child in &uc.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::Symmetry(sy) => {
            out.push_str(&format!("{}symmetry:\n", indent));
            for child in &sy.children {
                render_node(child, depth + 1, out);
            }
        }

        Node::ThreadComment(tc) => {
            out.push_str(&format!("{}comment:", indent));
            for gate in &tc.gates {
                out.push_str(&format!(" {{{}}}", gate.name));
            }
            out.push('\n');
            if let Some(on) = &tc.on {
                out.push_str(&format!("{}    on: {}\n", indent, on));
            }
            if let Some(author) = &tc.author {
                out.push_str(&format!("{}    author: {}\n", indent, author));
            }
            if tc.body.contains('\n') {
                out.push_str(&format!("{}    body: |\n", indent));
                for line in tc.body.lines() {
                    out.push_str(&format!("{}        {}\n", indent, line));
                }
            } else {
                out.push_str(&format!("{}    body: \"{}\"\n", indent, tc.body));
            }
            if !tc.reactions.is_empty() {
                let reaction_strs: Vec<String> =
                    tc.reactions.iter().map(|r| format!(":{}", r)).collect();
                out.push_str(&format!(
                    "{}    reactions: [{}]\n",
                    indent,
                    reaction_strs.join(", ")
                ));
            }
            if let Some(created_at) = &tc.created_at {
                out.push_str(&format!("{}    created_at: |{}|\n", indent, created_at));
            }
            for child in &tc.children {
                render_node(child, depth + 1, out);
            }
            out.push('\n');
        }

        Node::BoundDef(b) => {
            out.push_str(&format!("{}bound:{}\n", indent, b.name));
            for child in &b.children {
                render_node(child, depth + 1, out);
            }
        }
        Node::BuildDef(b) => {
            out.push_str(&format!("{}build:{}\n", indent, b.name));
            for child in &b.children {
                render_node(child, depth + 1, out);
            }
        }
        Node::RunDef(r) => {
            out.push_str(&format!("{}run:{}\n", indent, r.name));
            for child in &r.children {
                render_node(child, depth + 1, out);
            }
        }
        Node::Directive(d) => {
            out.push_str(&format!("{}@{} {}\n", indent, d.kind, d.value));
        }
    }
}

fn render_atom(atom: &Atom, out: &mut String) {
    out.push_str(&format!(" :{}", atom.name));
    if let Some(v) = &atom.value {
        out.push(':');
        out.push_str(v);
    }
}

fn render_task_marker(marker: &TaskMarker, label: &Option<String>, out: &mut String) {
    match &marker.prefix {
        TaskPrefix::Parallel => out.push('+'),
        TaskPrefix::Subtask(n) => {
            for _ in 0..*n {
                out.push('-');
            }
        }
        TaskPrefix::ParallelSubtask => out.push_str("++"),
        TaskPrefix::None => {}
    }

    out.push('[');
    if let Some(lbl) = label {
        out.push_str(lbl);
    }
    if let Some(seq) = marker.seq {
        out.push_str(&seq.to_string());
    }
    match (&marker.kind, &marker.priority) {
        (TaskKind::Required, _) => out.push('!'),
        (TaskKind::Optional, _) => out.push('o'),
        (TaskKind::Completed, _) => out.push('x'),
        (TaskKind::Open, _) => {
            if label.is_none() && marker.seq.is_none() {
                out.push(' ');
            }
        }
    }
    out.push(']');
}

fn render_field_def(field: &FieldDef, out: &mut String) {
    out.push_str(&field.name);
    if field.plural {
        out.push_str("(s)");
    }
    out.push_str(": ");
    match &field.default {
        FieldDefault::Str(s) => out.push_str(&format!("\"{}\"", s)),
        FieldDefault::Int(n) => out.push_str(&n.to_string()),
        FieldDefault::Float(f) => out.push_str(&f.to_string()),
        FieldDefault::Bool(b) => out.push_str(&b.to_string()),
        FieldDefault::Atom(a) => out.push_str(&format!(":{}", a)),
        FieldDefault::Enum(opts) => {
            let parts: Vec<String> = opts.iter().map(|o| format!(":{}", o)).collect();
            out.push_str(&parts.join("/"));
        }
        FieldDefault::Ref(r) => out.push_str(&format!("@{}", r)),
        FieldDefault::List => out.push_str("[]"),
        FieldDefault::Timestamp(t) => out.push_str(&format!("|{}|", t)),
        FieldDefault::Nil => out.push_str("nil"),
        FieldDefault::Trit(t) => out.push_str(&format!("~{}", t)),
    }
}

fn render_flow_edge(edge: &FlowEdge, depth: usize, out: &mut String) {
    let indent = "    ".repeat(depth);
    let from = edge.from.join(", ");
    let to = edge.to.join(", ");

    out.push_str(&indent);
    out.push_str(&from);
    out.push(' ');

    if let Some(t) = &edge.timeout {
        out.push_str(&format!("--timeout:|{}|-->", t));
    } else if let Some(w) = &edge.wait {
        out.push_str(&format!("--wait:|{}|-->", w));
    } else if let Some(g) = &edge.gate {
        out.push_str(&format!("--{}-->", g));
    } else if let Some(label) = &edge.label {
        out.push_str(&format!("--{}-->", label));
    } else if edge.parallel {
        out.push_str("+-->");
    } else {
        out.push_str("-->");
    }

    out.push(' ');
    out.push_str(&to);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn roundtrip(src: &str) -> String {
        let doc = parse::parse(src).expect("parse failed");
        render(&doc)
    }

    #[test]
    fn render_group() {
        let doc = Document {
            nodes: vec![Node::Group(Group {
                depth: 1,
                name: "My Project".to_string(),
                atoms: vec![],
                gates: vec![],
                children: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("# My Project"));
    }

    #[test]
    fn render_task() {
        let doc = Document {
            nodes: vec![Node::Task(Task {
                marker: TaskMarker {
                    kind: TaskKind::Required,
                    priority: Priority::Required,
                    prefix: TaskPrefix::None,
                    seq: None,
                },
                label: None,
                text: "Build API".to_string(),
                inline: vec![],
                gates: vec![],
                children: vec![],
                on_pass: None,
                on_fail: None,
                match_arms: None,
                closes: None,
                depends: None,
                validate: None,
                status: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("[!] Build API"));
    }

    #[test]
    fn render_optional_task() {
        let doc = Document {
            nodes: vec![Node::Task(Task {
                marker: TaskMarker {
                    kind: TaskKind::Optional,
                    priority: Priority::Optional,
                    prefix: TaskPrefix::None,
                    seq: None,
                },
                label: None,
                text: "Nice to have".to_string(),
                inline: vec![],
                gates: vec![],
                children: vec![],
                on_pass: None,
                on_fail: None,
                match_arms: None,
                closes: None,
                depends: None,
                validate: None,
                status: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("[o] Nice to have"));
    }

    #[test]
    fn render_completed_task() {
        let doc = Document {
            nodes: vec![Node::Task(Task {
                marker: TaskMarker {
                    kind: TaskKind::Completed,
                    priority: Priority::None,
                    prefix: TaskPrefix::None,
                    seq: None,
                },
                label: None,
                text: "Done thing".to_string(),
                inline: vec![],
                gates: vec![],
                children: vec![],
                on_pass: None,
                on_fail: None,
                match_arms: None,
                closes: None,
                depends: None,
                validate: None,
                status: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("[x] Done thing"));
    }

    #[test]
    fn render_prose() {
        let doc = Document {
            nodes: vec![Node::Prose(Prose {
                text: "Hello world".to_string(),
                inline: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn render_comment() {
        let doc = Document {
            nodes: vec![Node::Comment(Comment {
                text: "A note".to_string(),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("// A note"));
    }

    #[test]
    fn render_divider() {
        let doc = Document {
            nodes: vec![Node::Divider], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("---"));
    }

    #[test]
    fn render_define() {
        let doc = Document {
            nodes: vec![Node::Define(Define {
                entity: "Task".to_string(),
                atoms: vec![],
                fields: vec![FieldDef {
                    name: "title".to_string(),
                    plural: false,
                    default: FieldDefault::Str("".to_string()),
                }],
                from_scope: None,
                mod_scope: None,
                workspace_scope: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("define:@Task"));
        assert!(result.contains("title: \"\""));
    }

    #[test]
    fn render_mutate() {
        let doc = Document {
            nodes: vec![Node::Mutate(Mutate {
                entity: "Task".to_string(),
                id: Some("t1".to_string()),
                gate: None,
                fields: vec![("title".to_string(), "Ship it".to_string())],
                batch: None,
                mod_scope: None,
                workspace_scope: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("mutate:@Task:t1"));
        assert!(result.contains("title: Ship it"));
    }

    #[test]
    fn render_delete() {
        let doc = Document {
            nodes: vec![Node::Delete(Delete {
                entity: "Task".to_string(),
                id: "old".to_string(),
                mod_scope: None,
                workspace_scope: None,
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("delete:@Task:old"));
    }

    #[test]
    fn render_variable_literal() {
        let doc = Document {
            nodes: vec![Node::Variable(Variable {
                name: "target".to_string(),
                value: VarValue::Literal("100".to_string()),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("target = 100"));
    }

    #[test]
    fn render_remember() {
        let doc = Document {
            nodes: vec![Node::Remember(Remember {
                content: "important fact".to_string(),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("remember: \"important fact\""));
    }

    #[test]
    fn render_recall() {
        let doc = Document {
            nodes: vec![Node::Recall(RecallOp {
                query: "find that fact".to_string(),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("recall: \"find that fact\""));
    }

    #[test]
    fn render_spawn_parallel() {
        let doc = Document {
            nodes: vec![Node::Spawn(Spawn::Parallel)], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("+"));
    }

    #[test]
    fn render_bold() {
        let doc = Document {
            nodes: vec![Node::Bold(Bold {
                text: "Important".to_string(),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("**Important**"));
    }

    #[test]
    fn render_code_block() {
        let doc = Document {
            nodes: vec![Node::CodeBlock(CodeBlock {
                lang: Some("python".to_string()),
                content: "print('hello')".to_string(),
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("```python"));
        assert!(result.contains("print('hello')"));
        assert!(result.contains("```"));
    }

    #[test]
    fn render_field_def_plural() {
        let mut out = String::new();
        render_field_def(
            &FieldDef {
                name: "tag".to_string(),
                plural: true,
                default: FieldDefault::List,
            },
            &mut out,
        );
        assert!(out.contains("tag(s): []"));
    }

    #[test]
    fn render_field_def_enum() {
        let mut out = String::new();
        render_field_def(
            &FieldDef {
                name: "state".to_string(),
                plural: false,
                default: FieldDefault::Enum(vec!["todo".to_string(), "done".to_string()]),
            },
            &mut out,
        );
        assert!(out.contains(":todo/:done"));
    }

    #[test]
    fn roundtrip_group() {
        let result = roundtrip("# My Group");
        assert!(result.contains("# My Group"));
    }

    #[test]
    fn roundtrip_define() {
        let src = "define:@Task\n    title: \"\"";
        let result = roundtrip(src);
        assert!(result.contains("define:@Task"));
        assert!(result.contains("title: \"\""));
    }

    #[test]
    fn roundtrip_mutate() {
        let src = "mutate:@Task:t1\n    title: Ship it";
        let result = roundtrip(src);
        assert!(result.contains("mutate:@Task:t1"));
        assert!(result.contains("title: Ship it"));
    }

    // ── Issue rendering ──

    #[test]
    fn render_basic_issue() {
        let doc = Document {
            nodes: vec![Node::Issue(IssueDef {
                title: "Fix auth bug".to_string(),
                id: None,
                on: None,
                status: Some("open".to_string()),
                priority: None,
                assignee: None,
                labels: vec![],
                estimate: None,
                milestone: None,
                due_date: None,
                description: None,
                gates: vec![],
                children: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("issue: Fix auth bug"));
        assert!(result.contains("status: :open"));
    }

    #[test]
    fn render_issue_with_labels() {
        let doc = Document {
            nodes: vec![Node::Issue(IssueDef {
                title: "Bug".to_string(),
                id: None,
                on: None,
                status: None,
                priority: None,
                assignee: None,
                labels: vec!["bug".to_string(), "urgent".to_string()],
                estimate: None,
                milestone: None,
                due_date: None,
                description: None,
                gates: vec![],
                children: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("labels: [:bug, :urgent]"));
    }

    // ── ThreadComment rendering ──

    #[test]
    fn render_basic_thread_comment() {
        let doc = Document {
            nodes: vec![Node::ThreadComment(ThreadComment {
                on: Some("@Task:t1".to_string()),
                author: Some("@User:alice".to_string()),
                body: "Looks good".to_string(),
                reactions: vec![],
                created_at: None,
                gates: vec![],
                children: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("comment:"));
        assert!(result.contains("on: @Task:t1"));
        assert!(result.contains("author: @User:alice"));
        assert!(result.contains("body: \"Looks good\""));
    }

    #[test]
    fn render_thread_comment_with_reactions() {
        let doc = Document {
            nodes: vec![Node::ThreadComment(ThreadComment {
                on: None,
                author: None,
                body: "Nice".to_string(),
                reactions: vec!["thumbsup".to_string(), "heart".to_string()],
                created_at: None,
                gates: vec![],
                children: vec![],
            })], ..Default::default()
        };
        let result = render(&doc);
        assert!(result.contains("reactions: [:thumbsup, :heart]"));
    }

    // ── Issue & ThreadComment roundtrips ──

    #[test]
    fn roundtrip_issue() {
        let src = "issue: Fix auth bug\n    status: :open\n    priority: :high";
        let result = roundtrip(src);
        assert!(result.contains("issue: Fix auth bug"));
        assert!(result.contains("status: :open"));
        assert!(result.contains("priority: :high"));
    }

    #[test]
    fn roundtrip_thread_comment() {
        let src = "comment:\n    author: @User:alice\n    body: \"Hello world\"";
        let result = roundtrip(src);
        assert!(result.contains("comment:"));
        assert!(result.contains("author: @User:alice"));
        assert!(result.contains("body: \"Hello world\""));
    }
}
