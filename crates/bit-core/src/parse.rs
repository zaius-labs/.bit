use crate::types::*;
use std::collections::HashMap;

const MAX_DEPTH: usize = 128;

pub fn parse(source: &str) -> Result<Document, ParseError> {
    let normalized = source.replace('\t', "    ");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut ctx = ParseCtx::new(&lines);
    let raw_nodes = ctx.parse_block(0, 0);
    if let Some(err) = ctx.first_error.take() {
        return Err(err);
    }
    let nodes = canonicalize_nodes(raw_nodes)?;
    Ok(Document { nodes, ..Default::default() })
}

struct ParseCtx<'a> {
    lines: &'a [&'a str],
    pos: usize,
    /// First fatal error encountered during parsing.  parse() drains this after
    /// parse_block returns so that the Result<Document, ParseError> contract is
    /// preserved without changing parse_block / parse_line signatures (Task 3
    /// will migrate those signatures to carry Results directly).
    first_error: Option<ParseError>,
}

impl<'a> ParseCtx<'a> {
    fn new(lines: &'a [&'a str]) -> Self {
        Self {
            lines,
            pos: 0,
            first_error: None,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }

    fn advance(&mut self) -> &'a str {
        // Use .get() instead of direct indexing: a panic here crashes the BEAM VM
        // scheduler thread (clean NIFs), killing co-located Erlang processes.
        // Callers that call advance() without a prior peek() receive "" and the
        // subsequent peek() returns None, terminating parse loops gracefully.
        match self.lines.get(self.pos) {
            Some(line) => {
                self.pos += 1;
                line
            }
            None => "",
        }
    }

    fn current_line(&self) -> usize {
        self.pos + 1 // 1-indexed for error messages
    }

    fn indent_of(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    fn parse_block(&mut self, min_indent: usize, nesting: usize) -> Vec<Node> {
        if nesting >= MAX_DEPTH {
            return Vec::new();
        }

        let mut nodes = Vec::new();

        while let Some(raw) = self.peek() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.advance();
                continue;
            }

            let indent = Self::indent_of(raw);
            if indent < min_indent {
                break;
            }

            if let Some(node) = self.parse_line(nesting) {
                nodes.push(node);
            }
        }

        nodes
    }

    fn parse_line(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.peek()?;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            self.advance();
            return None;
        }

        let stripped = strip_comment(trimmed);

        // Code block (fenced with ```)
        if stripped.starts_with("```") {
            return self.parse_code_block();
        }

        // Comment-only line
        if trimmed.starts_with("//") {
            self.advance();
            return Some(Node::Comment(Comment {
                text: trimmed.trim_start_matches("//").trim().to_string(),
            }));
        }

        // Divider
        if stripped == "---" || (stripped.starts_with("---") && stripped.chars().all(|c| c == '-'))
        {
            self.advance();
            return Some(Node::Divider);
        }

        // Spawn
        if stripped == "+" {
            self.advance();
            return Some(Node::Spawn(Spawn::Parallel));
        }
        if stripped == "++" {
            self.advance();
            return Some(Node::Spawn(Spawn::Sequential));
        }

        // Entity definition: ## Entity: Name
        if let Some(name) = stripped
            .strip_prefix("## Entity:")
            .or_else(|| stripped.strip_prefix("## Entity :"))
        {
            return self.parse_entity_block(name.trim().to_string());
        }

        // Metric definition: ## Metric: Name
        if let Some(name) = stripped
            .strip_prefix("## Metric:")
            .or_else(|| stripped.strip_prefix("## Metric :"))
        {
            return self.parse_metric_block(name.trim().to_string());
        }

        // Group: # ## ### etc
        if stripped.starts_with('#') && !stripped.starts_with("#(") {
            return self.parse_group(nesting);
        }

        // Define
        if stripped.starts_with("define:@") || stripped.starts_with("define:$") {
            return self.parse_define();
        }

        // Mutate
        if stripped.starts_with("mutate:@") || stripped.starts_with("mutate:$") {
            return self.parse_mutate();
        }

        // Delete
        if stripped.starts_with("delete:@") || stripped.starts_with("delete:$") {
            self.advance();
            let rest = stripped.trim_start_matches("delete:");
            let (mod_scope, workspace_scope, entity_rest) = extract_entity_scope(rest);
            let parts: Vec<&str> = entity_rest.splitn(2, ':').collect();
            let entity = parts[0].to_string();
            let id = parts.get(1).unwrap_or(&"").to_string();
            return Some(Node::Delete(Delete {
                entity,
                id,
                mod_scope,
                workspace_scope,
            }));
        }

        // Query block
        if stripped.starts_with("query:") {
            return self.parse_query_block();
        }

        // Inline query
        if stripped.starts_with("? ") {
            return self.parse_inline_query();
        }

        // Flow
        if stripped == "flow:" {
            return self.parse_flow();
        }

        // States
        if stripped == "states:" {
            return self.parse_states();
        }

        // Validate
        if stripped.starts_with("validate ") && stripped.ends_with(':') {
            return self.parse_validate(nesting);
        }

        // Check
        if stripped.starts_with("check:") && !stripped.starts_with("check:@") {
            return self.parse_check();
        }

        // Form
        if stripped.starts_with("form:") && !stripped.starts_with("form:install") {
            return self.parse_form();
        }

        // Mod definition
        if stripped.starts_with("mod:$") {
            return self.parse_mod_def();
        }

        // Mod install/publish/update/remove/list/registry
        if let Some(after_mod) = stripped.strip_prefix("mod:") {
            self.advance();
            let (method, args) = if let Some(idx) = after_mod.find(' ') {
                (
                    after_mod[..idx].to_string(),
                    Some(after_mod[idx + 1..].trim().to_string()),
                )
            } else {
                (after_mod.to_string(), None)
            };
            return Some(Node::ModInvoke(ModInvoke {
                name: args.clone().unwrap_or_default(),
                method: Some(method),
                args,
            }));
        }

        // Project definition
        if stripped.starts_with("project:") {
            return self.parse_project();
        }

        // Commands definition
        if stripped == "commands:" || stripped.starts_with("commands:") {
            return self.parse_commands();
        }

        // Serve definition
        if stripped.starts_with("serve:") {
            return self.parse_serve();
        }

        // Sync definition (top-level keyword)
        if stripped.starts_with("sync:") {
            return self.parse_sync();
        }

        // Git operations
        if stripped.starts_with("git:") {
            return self.parse_git();
        }

        // Snap
        if stripped.starts_with("snap:") {
            self.advance();
            let name = stripped
                .trim_start_matches("snap:")
                .trim()
                .trim_matches('"')
                .to_string();
            return Some(Node::Snap(Snap { name }));
        }

        // Diff (with optional temporal scoping: diff:@Entity:id from snap:version)
        if stripped.starts_with("diff:") {
            self.advance();
            let rest = stripped.trim_start_matches("diff:").trim();
            let (target, from_snapshot) = if let Some(snap_idx) = rest.find(" from snap:") {
                (
                    rest[..snap_idx].trim().to_string(),
                    Some(rest[snap_idx + 11..].trim().to_string()),
                )
            } else {
                (rest.to_string(), None)
            };
            return Some(Node::Diff(Diff {
                target,
                from_snapshot,
            }));
        }

        // History
        if stripped.starts_with("history:") {
            self.advance();
            let rest = stripped.trim_start_matches("history:").trim();
            let (target, limit) = if let Some(idx) = rest.find("limit:") {
                let t = rest[..idx].trim().to_string();
                let l = rest[idx + 6..].trim().parse().ok();
                (t, l)
            } else {
                (rest.to_string(), None)
            };
            return Some(Node::History(HistoryOp { target, limit }));
        }

        // Status definition
        if stripped.starts_with("status:") && stripped.contains('/') {
            self.advance();
            let raw_opts = stripped.trim_start_matches("status:").trim();
            let options = raw_opts
                .split('/')
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect();
            return Some(Node::StatusDef(StatusDef { options }));
        }

        // Conditional
        if stripped.starts_with("if ") && stripped.ends_with(':') {
            return self.parse_conditional(nesting);
        }

        // Bold line (standalone)
        if let Some(inner) = stripped
            .strip_prefix("**")
            .and_then(|s| s.strip_suffix("**"))
        {
            if !inner.is_empty() {
                self.advance();
                return Some(Node::Bold(Bold {
                    text: inner.to_string(),
                }));
            }
        }

        // Webhook
        if stripped.starts_with("webhook:") {
            return self.parse_webhook();
        }

        // Remember
        if stripped.starts_with("remember:") {
            self.advance();
            let content = stripped
                .trim_start_matches("remember:")
                .trim()
                .trim_matches('"')
                .to_string();
            return Some(Node::Remember(Remember { content }));
        }

        // Recall
        if stripped.starts_with("recall:") {
            self.advance();
            let query = stripped
                .trim_start_matches("recall:")
                .trim()
                .trim_matches('"')
                .to_string();
            return Some(Node::Recall(RecallOp { query }));
        }

        // Embed marker: ^tag_name
        if stripped.starts_with('^') && stripped.len() > 1 {
            self.advance();
            let tag = stripped[1..].trim().to_string();
            return Some(Node::EmbedMarker(EmbedMarker { tag }));
        }

        // Files definition
        if stripped.starts_with("files:") {
            return self.parse_files_def();
        }

        // Policy definition
        if stripped.starts_with("policy:") {
            return self.parse_policy_def();
        }

        // Gate definition: gate:name
        if stripped.starts_with("gate:") && !stripped.starts_with("gate:name") {
            return self.parse_gate_def();
        }

        // Escalate
        if stripped.starts_with("escalate:") {
            self.advance();
            let target = stripped.trim_start_matches("escalate:").trim().to_string();
            return Some(Node::Escalate(Escalate { target }));
        }

        // Use block (mod or scope path)
        if stripped.starts_with("use $") || stripped.starts_with("use @") {
            return self.parse_use_block();
        }

        // Project scope: %ProjectName on its own line
        if stripped.starts_with('%') && stripped.len() > 1 {
            return self.parse_project_scope(nesting);
        }

        // Mod direct invocation: $Name.method(...)
        if stripped.starts_with('$') {
            self.advance();
            return self.parse_mod_invocation(stripped);
        }

        // Issue definition
        if stripped.starts_with("issue:") {
            return self.parse_issue(nesting);
        }

        // Threaded comment block
        if stripped.starts_with("comment:") {
            return self.parse_thread_comment(nesting);
        }

        // Task markers
        if is_task_line(stripped) {
            return self.parse_task(nesting);
        }

        // Variable: name = value
        if let Some(var) = try_parse_variable(stripped) {
            self.advance();
            return Some(Node::Variable(var));
        }

        // Routine
        if stripped.starts_with("routine:") {
            self.advance();
            let rest = stripped.trim_start_matches("routine:").trim();
            let (trigger, expr) = if let Some(pipe_start) = rest.find('|') {
                let t = rest[..pipe_start].trim_end_matches(':').trim().to_string();
                let e = extract_pipe_expr(&rest[pipe_start..]);
                (t, e)
            } else {
                (rest.to_string(), String::new())
            };
            return Some(Node::Routine(Routine { trigger, expr }));
        }

        // Lattice constructs
        if stripped.starts_with("lattice_validates:") {
            return self.parse_lattice_validates();
        }
        if stripped.starts_with("lattice_constraint:") {
            return self.parse_lattice_constraint();
        }
        if stripped.starts_with("lattice_schema:") {
            return self.parse_lattice_schema();
        }
        if stripped.starts_with("lattice_frontier:") {
            return self.parse_lattice_frontier();
        }
        if stripped.starts_with("pressure_effect:") {
            return self.parse_pressure_effect();
        }
        if stripped.starts_with("unit_cell:") {
            return self.parse_unit_cell(nesting);
        }
        if stripped.starts_with("symmetry:") {
            return self.parse_symmetry(nesting);
        }

        // Fallback: prose
        self.advance();
        Some(Node::Prose(Prose {
            text: trimmed.to_string(),
            inline: parse_inline(stripped),
        }))
    }

    fn parse_code_block(&mut self) -> Option<Node> {
        let opening = self.advance();
        let trimmed = opening.trim();
        let lang_str = trimmed.trim_start_matches('`');
        let lang = if lang_str.is_empty() {
            None
        } else {
            Some(lang_str.to_string())
        };

        let mut content = String::new();
        while let Some(line) = self.peek() {
            if line.trim().starts_with("```") {
                self.advance(); // consume closing fence
                break;
            }
            content.push_str(self.advance());
            content.push('\n');
        }

        // Remove trailing newline
        if content.ends_with('\n') {
            content.pop();
        }

        Some(Node::CodeBlock(CodeBlock { lang, content }))
    }

    fn parse_group(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);

        let group_depth = stripped.chars().take_while(|&c| c == '#').count() as u8;
        let rest = stripped[group_depth as usize..].trim();

        let gates = extract_gates(rest);
        let clean = remove_gates(rest);
        let (name, atoms) = parse_name_and_atoms(&clean);

        let group_indent = Self::indent_of(raw);
        let child_indent = group_indent + 1;
        let children = self.parse_block(child_indent, nesting + 1);

        Some(Node::Group(Group {
            depth: group_depth,
            name,
            atoms,
            gates,
            children,
        }))
    }

    fn parse_task(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);

        let (marker, rest) = parse_task_marker(stripped)?;

        let gates = extract_gates(rest);
        let clean = remove_gates(rest);
        let (text, closes) = extract_closes(&clean);
        let text = text.trim().to_string();
        let inline = parse_inline(&text);

        let label = marker.label.clone();
        let task_marker = marker.marker;

        let child_indent = indent + 1;
        let all_children = self.parse_block(child_indent, nesting + 1);

        let (all_children, depends, validate, task_status) = extract_task_fields(all_children);

        let (children, on_pass, on_fail, match_arms) = partition_task_children(all_children);

        Some(Node::Task(Task {
            marker: task_marker,
            label,
            text,
            inline,
            gates,
            children,
            on_pass,
            on_fail,
            match_arms,
            closes,
            depends,
            validate,
            status: task_status,
        }))
    }

    fn parse_define(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let rest = stripped.trim_start_matches("define:");
        let (mod_scope, workspace_scope, entity_rest) = extract_entity_scope(rest);

        let (entity, from_scope) = if let Some(from_idx) = entity_rest.find(" from @") {
            let ent_part = &entity_rest[..from_idx];
            let path = entity_rest[from_idx + 6..].trim().to_string();
            let (name_part, _) = parse_name_and_atoms(ent_part);
            (name_part, Some(path))
        } else {
            let (name_part, _) = parse_name_and_atoms(entity_rest);
            (name_part, None)
        };

        let (_, atoms) =
            parse_name_and_atoms(entity_rest.split(" from ").next().unwrap_or(entity_rest));

        let base_indent = Self::indent_of(raw);
        let mut fields = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let field_str = strip_comment(next_trimmed);
            if let Some(field) = parse_field_def(field_str) {
                fields.push(field);
            }
        }

        Some(Node::Define(Define {
            entity,
            atoms,
            fields,
            from_scope,
            mod_scope,
            workspace_scope,
        }))
    }

    fn parse_mutate(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let rest = stripped.trim_start_matches("mutate:");
        let (mod_scope, workspace_scope, entity_rest) = extract_entity_scope(rest);

        let gate = extract_gates(entity_rest).into_iter().next();
        let clean = remove_gates(entity_rest);
        let parts: Vec<&str> = clean.splitn(2, ':').collect();
        let entity = parts[0].to_string();
        let id = parts
            .get(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let base_indent = Self::indent_of(raw);
        let mut raw_lines: Vec<String> = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            raw_lines.push(strip_comment(next_trimmed).to_string());
        }

        if id.is_none() && !raw_lines.is_empty() && is_batch_format(&raw_lines) {
            let batch = Some(parse_batch_records(&raw_lines));
            return Some(Node::Mutate(Mutate {
                entity,
                id,
                gate,
                fields: Vec::new(),
                batch,
                mod_scope,
                workspace_scope,
            }));
        }

        let fields = raw_lines
            .iter()
            .filter_map(|l| {
                l.split_once(':')
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            })
            .collect();

        Some(Node::Mutate(Mutate {
            entity,
            id,
            gate,
            fields,
            batch: None,
            mod_scope,
            workspace_scope,
        }))
    }

    fn parse_query_block(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut body_lines = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            body_lines.push(strip_comment(next_trimmed).to_string());
        }

        parse_query_from_lines(&body_lines).map(Node::Query)
    }

    fn parse_inline_query(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let body = stripped.trim_start_matches("? ").trim();
        parse_query_from_lines(&[body.to_string()]).map(Node::Query)
    }

    fn parse_flow(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut edges = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let edge_str = strip_comment(next_trimmed);
            edges.extend(parse_flow_edges(edge_str));
        }

        Some(Node::Flow(Flow { name: None, edges }))
    }

    fn parse_states(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut transitions = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let edge_str = strip_comment(next_trimmed);
            transitions.extend(parse_flow_edges(edge_str));
        }

        Some(Node::States(StatesDef { transitions }))
    }

    fn parse_validate(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = trimmed
            .trim_start_matches("validate ")
            .trim_end_matches(':')
            .trim()
            .to_string();
        let base_indent = Self::indent_of(raw);
        let children = self.parse_block(base_indent + 1, nesting + 1);

        let meta_keys: std::collections::HashSet<&str> =
            ["id", "targets", "requires", "blocks", "depends_on"]
                .iter()
                .copied()
                .collect();
        let mut meta = AttachmentMeta::default();
        let mut filtered_children = Vec::new();

        for child in children {
            if let Node::Prose(ref p) = child {
                let text = p.text.trim();
                if let Some((key, value)) = text.split_once(':') {
                    let key = key.trim();
                    let value = value.trim().to_string();
                    if meta_keys.contains(key) {
                        match key {
                            "id" => meta.id = Some(value),
                            "targets" => {
                                meta.targets = value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            }
                            "requires" => {
                                meta.requires = value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            }
                            "blocks" => {
                                meta.blocks = value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            }
                            "depends_on" => {
                                meta.depends_on = value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            }
                            _ => {}
                        }
                        continue;
                    }
                }
            }
            filtered_children.push(child);
        }

        Some(Node::Validate(ValidateDef {
            name,
            meta,
            children: filtered_children,
        }))
    }

    fn parse_check(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let name = stripped.trim_start_matches("check:").trim().to_string();

        let base_indent = Self::indent_of(raw);
        let mut raw_lines: Vec<String> = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            raw_lines.push(strip_comment(next_trimmed).to_string());
        }

        let meta_keys: std::collections::HashSet<&str> =
            ["id", "targets", "requires", "blocks", "depends_on"]
                .iter()
                .copied()
                .collect();
        let mut meta = AttachmentMeta::default();
        let mut body = Vec::new();

        for line in &raw_lines {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();
                if meta_keys.contains(key) {
                    match key {
                        "id" => meta.id = Some(value),
                        "targets" => {
                            meta.targets = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        }
                        "requires" => {
                            meta.requires = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        }
                        "blocks" => {
                            meta.blocks = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        }
                        "depends_on" => {
                            meta.depends_on = value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        }
                        _ => {}
                    }
                } else {
                    body.push((key.to_string(), value));
                }
            }
        }

        Some(Node::Check(CheckDef { name, meta, body }))
    }

    fn parse_gate_def(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = trimmed.trim_start_matches("gate:").trim().to_string();
        let base_indent = Self::indent_of(raw);
        let children = self.parse_block(base_indent + 1, 1);

        Some(Node::GateDef(GateDef { name, children }))
    }

    fn parse_form(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = strip_comment(trimmed)
            .trim_start_matches("form:")
            .trim()
            .to_string();
        let base_indent = Self::indent_of(raw);
        let mut schema_version = None;
        let mut ui_layout = None;
        let mut ui_pages = Vec::new();
        let mut storage = FormStorageDef::default();
        let mut projections = Vec::new();
        let mut fields = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let field_str = strip_comment(next_trimmed);
            if let Some((key, value)) = field_str.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "schema_version" => {
                        schema_version = parse_form_u32(value);
                        continue;
                    }
                    "ui_layout" => {
                        ui_layout = Some(parse_form_text(value));
                        continue;
                    }
                    "ui_page" => {
                        ui_pages.push(parse_form_text(value));
                        continue;
                    }
                    "storage_canonical" => {
                        storage.canonical = Some(parse_form_text(value));
                        continue;
                    }
                    "storage_entity" => {
                        storage.entity = Some(parse_form_text(value));
                        continue;
                    }
                    "storage_duckdb" => {
                        storage.duckdb = Some(parse_form_text(value));
                        continue;
                    }
                    "projection_entity" => {
                        projections.push(FormProjectionDef {
                            target: "entity".to_string(),
                            mapping: parse_form_text(value),
                        });
                        continue;
                    }
                    "projection_duckdb" => {
                        projections.push(FormProjectionDef {
                            target: "duckdb".to_string(),
                            mapping: parse_form_text(value),
                        });
                        continue;
                    }
                    _ => {}
                }
            }

            if let Some(field) = parse_field_def(field_str) {
                fields.push(field);
            }
        }

        Some(Node::Form(FormDef {
            name,
            schema_version,
            ui_layout,
            ui_pages,
            storage,
            projections,
            fields,
        }))
    }

    fn parse_project(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = strip_comment(trimmed)
            .trim_start_matches("project:")
            .trim()
            .to_string();

        let base_indent = Self::indent_of(raw);
        let mut brief = String::new();
        let mut heartbeat = None;
        let mut framework = None;
        let mut status = None;
        let mut commands = None;
        let mut serve = None;
        let mut fitness = None;
        let mut pressure = None;
        let mut phase = None;
        let mut inhibited_until = None;
        let mut completion = None;
        let mut kpi = None;
        let mut routine = None;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();

            if next_trimmed.is_empty() {
                self.advance();
                // Check if next non-empty line is still indented under this project
                if let Some(after_blank) = self.peek() {
                    if Self::indent_of(after_blank) <= base_indent && !after_blank.trim().is_empty()
                    {
                        break;
                    }
                } else {
                    break;
                }
                continue;
            }

            if next_indent <= base_indent {
                break;
            }

            // Check for sub-blocks that need delegation
            if next_trimmed == "commands:" || next_trimmed.starts_with("commands:") {
                // parse_commands advances past the "commands:" line itself
                if let Some(Node::Commands(c)) = self.parse_commands() {
                    commands = Some(c);
                }
                continue;
            }

            if next_trimmed.starts_with("serve:") {
                if let Some(Node::Serve(s)) = self.parse_serve() {
                    serve = Some(s);
                }
                continue;
            }

            // Regular key: value field
            self.advance();
            let field_str = strip_comment(next_trimmed);
            if let Some((k, v)) = field_str.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "brief" => brief = val.to_string(),
                    "heartbeat" => heartbeat = Some(val.to_string()),
                    "framework" => framework = Some(val.to_string()),
                    "status" => status = Some(val.to_string()),
                    "fitness" => fitness = val.parse::<f64>().ok(),
                    "pressure" => pressure = val.parse::<f64>().ok(),
                    "phase" => phase = Some(val.to_string()),
                    "inhibited_until" => inhibited_until = Some(val.to_string()),
                    "completion" => completion = Some(val.to_string()),
                    "kpi" => kpi = Some(val.to_string()),
                    "routine" => routine = Some(val.to_string()),
                    _ => {}
                }
            }
        }

        Some(Node::Project(ProjectDef {
            name,
            brief,
            heartbeat,
            framework,
            status,
            commands,
            serve,
            fitness,
            pressure,
            phase,
            inhibited_until,
            completion,
            kpi,
            routine,
        }))
    }

    fn parse_project_scope(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = trimmed[1..].trim().to_string(); // skip '%'
        let base_indent = Self::indent_of(raw);

        // Collect indented children; stop on dedent or `---` divider
        let mut children = Vec::new();
        while let Some(next_raw) = self.peek() {
            let next_trimmed = next_raw.trim();

            if next_trimmed.is_empty() {
                self.advance();
                // Check if next non-empty line is still indented under this scope
                if let Some(after_blank) = self.peek() {
                    if Self::indent_of(after_blank) <= base_indent && !after_blank.trim().is_empty()
                    {
                        break;
                    }
                } else {
                    break;
                }
                continue;
            }

            // Divider terminates the scope
            if next_trimmed == "---"
                || (next_trimmed.starts_with("---") && next_trimmed.chars().all(|c| c == '-'))
            {
                self.advance(); // consume the divider
                break;
            }

            let next_indent = Self::indent_of(next_raw);
            if next_indent <= base_indent {
                break;
            }

            if let Some(node) = self.parse_line(nesting + 1) {
                children.push(node);
            }
        }

        Some(Node::ProjectScope(ProjectScope { name, children }))
    }

    fn parse_serve(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let rest = strip_comment(trimmed)
            .trim_start_matches("serve:")
            .to_string();

        // Parse serve:target|command| pattern
        let (target, command) = if let Some(pipe_start) = rest.find('|') {
            let t = rest[..pipe_start].trim().to_string();
            let after_pipe = &rest[pipe_start + 1..];
            let cmd = if let Some(pipe_end) = after_pipe.find('|') {
                after_pipe[..pipe_end].trim().to_string()
            } else {
                after_pipe.trim().to_string()
            };
            (t, cmd)
        } else {
            (rest.trim().to_string(), String::new())
        };

        let base_indent = Self::indent_of(raw);
        let mut port = None;
        let mut open = None;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let field_str = strip_comment(next_trimmed);
            if let Some((k, v)) = field_str.split_once(':') {
                match k.trim() {
                    "port" => port = Some(v.trim().to_string()),
                    "open" => open = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }

        Some(Node::Serve(ServeDef {
            target,
            command,
            port,
            open,
        }))
    }

    fn parse_commands(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut commands: Vec<CommandEntry> = Vec::new();

        // Outer level: each /name line starts a new command
        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();

            if next_trimmed.is_empty() {
                // Blank line: if we already have commands, break out
                if !commands.is_empty() {
                    self.advance();
                    break;
                }
                self.advance();
                continue;
            }

            // Must be indented deeper than `commands:` line
            if next_indent <= base_indent {
                break;
            }

            // Expect a /name line at the command level
            if !next_trimmed.starts_with('/') {
                break;
            }

            self.advance();
            let cmd_name = next_trimmed.trim_start_matches('/').trim().to_string();
            let cmd_indent = next_indent;

            let mut description = String::new();
            let mut params: Vec<String> = Vec::new();
            let mut prompt = String::new();

            // Inner level: properties of this command
            while let Some(prop_raw) = self.peek() {
                let prop_indent = Self::indent_of(prop_raw);
                let prop_trimmed = prop_raw.trim();

                if prop_trimmed.is_empty() {
                    // Blank line might end command or separate commands
                    break;
                }

                // Must be indented deeper than the /name line
                if prop_indent <= cmd_indent {
                    break;
                }

                self.advance();
                let field_str = strip_comment(prop_trimmed);

                if let Some((k, v)) = field_str.split_once(':') {
                    let key = k.trim();
                    let val = v.trim();
                    match key {
                        "description" => {
                            description = val.to_string();
                        }
                        "params" => {
                            params = val
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        "prompt" => {
                            if val == "|" {
                                // Multiline prompt: collect indented lines
                                let mut prompt_lines: Vec<String> = Vec::new();
                                while let Some(ml_raw) = self.peek() {
                                    let ml_indent = Self::indent_of(ml_raw);
                                    let ml_trimmed = ml_raw.trim();
                                    if ml_trimmed.is_empty() || ml_indent <= prop_indent {
                                        break;
                                    }
                                    self.advance();
                                    prompt_lines.push(ml_trimmed.to_string());
                                }
                                prompt = prompt_lines.join("\n");
                            } else {
                                prompt = val.to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }

            commands.push(CommandEntry {
                name: cmd_name,
                description,
                params,
                prompt,
            });
        }

        Some(Node::Commands(CommandsDef { commands }))
    }

    fn parse_sync(&mut self) -> Option<Node> {
        let error_line = self.current_line();
        let raw = self.advance();
        let trimmed = raw.trim();
        let name = strip_comment(trimmed)
            .trim_start_matches("sync:")
            .trim()
            .to_string();

        if name.is_empty() && self.first_error.is_none() {
            self.first_error = Some(ParseError {
                code: "E_SYNC_MISSING_NAME".to_string(),
                kind: "parse_error".to_string(),
                message: format!("sync: declaration on line {} has no name", error_line),
                context: None,
                line: error_line,
                col: 0,
            });
            return None;
        }

        let base_indent = Self::indent_of(raw);
        let mut field_lines = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let field_str = strip_comment(next_trimmed);
            if let Some((k, v)) = field_str.split_once(':') {
                field_lines.push((k.trim().to_string(), v.trim().to_string()));
            }
        }

        let get = |key: &str| -> String {
            field_lines
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };

        Some(Node::SyncDef(SyncDef {
            name,
            class: get("class"),
            source: get("source"),
            identity: get("identity"),
            mode: get("mode"),
            target: get("target"),
            schedule: get("schedule"),
            scope: get("scope"),
        }))
    }

    fn parse_issue(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let title = stripped.trim_start_matches("issue:").trim().to_string();
        let base_indent = Self::indent_of(raw);

        // Parse gates from header line
        let gates = extract_gates(trimmed);

        let mut fields: Vec<(String, String)> = Vec::new();
        let mut children: Vec<Node> = Vec::new();
        let mut description_lines: Vec<String> = Vec::new();
        let mut in_multiline_desc = false;
        let mut multiline_indent = 0usize;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();

            if next_trimmed.is_empty() {
                self.advance();
                if in_multiline_desc {
                    description_lines.push(String::new());
                }
                continue;
            }
            if next_indent <= base_indent {
                break;
            }

            // Handle multiline description continuation
            if in_multiline_desc {
                if next_indent > multiline_indent {
                    self.advance();
                    description_lines.push(next_trimmed.to_string());
                    continue;
                } else {
                    in_multiline_desc = false;
                }
            }

            let field_str = strip_comment(next_trimmed);

            // Check if it's a child block (comment: or issue:)
            if field_str.starts_with("comment:") || field_str.starts_with("issue:") {
                let child_nodes = self.parse_block(next_indent, nesting + 1);
                children.extend(child_nodes);
                continue;
            }

            self.advance();

            if let Some((k, v)) = field_str.split_once(':') {
                let key = k.trim();
                let val = v.trim();

                if key == "description" && val == "|" {
                    in_multiline_desc = true;
                    multiline_indent = next_indent;
                    continue;
                }

                fields.push((key.to_string(), val.to_string()));
            }
        }

        // Extract fields
        let get = |key: &str| -> Option<String> {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };

        let labels = get("labels")
            .map(|v| {
                v.trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().trim_start_matches(':').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let estimate = get("estimate").and_then(|v| v.parse::<f64>().ok());

        let description = if !description_lines.is_empty() {
            // Trim trailing empty lines
            while description_lines.last().is_some_and(|l| l.is_empty()) {
                description_lines.pop();
            }
            Some(description_lines.join("\n"))
        } else {
            get("description").map(|v| v.trim_matches('"').to_string())
        };

        Some(Node::Issue(IssueDef {
            title,
            id: get("id").map(|v| {
                v.trim_start_matches(':')
                    .trim_matches('"')
                    .trim_matches('|')
                    .to_string()
            }),
            on: get("on"),
            status: get("status").map(|v| v.trim_start_matches(':').to_string()),
            priority: get("priority").map(|v| v.trim_start_matches(':').to_string()),
            assignee: get("assignee"),
            labels,
            estimate,
            milestone: get("milestone").map(|v| v.trim_matches('"').to_string()),
            due_date: get("due_date").map(|v| v.trim_matches('|').to_string()),
            description,
            gates,
            children,
        }))
    }

    fn parse_thread_comment(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let base_indent = Self::indent_of(raw);

        // Parse gates from header line
        let gates = extract_gates(trimmed);

        let mut fields: Vec<(String, String)> = Vec::new();
        let mut children: Vec<Node> = Vec::new();
        let mut body_lines: Vec<String> = Vec::new();
        let mut in_multiline_body = false;
        let mut multiline_indent = 0usize;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();

            if next_trimmed.is_empty() {
                self.advance();
                if in_multiline_body {
                    body_lines.push(String::new());
                }
                continue;
            }
            if next_indent <= base_indent {
                break;
            }

            // Handle multiline body continuation
            if in_multiline_body {
                if next_indent > multiline_indent {
                    self.advance();
                    body_lines.push(next_trimmed.to_string());
                    continue;
                } else {
                    in_multiline_body = false;
                }
            }

            let field_str = strip_comment(next_trimmed);

            // Check if it's a nested comment block (reply)
            if field_str.starts_with("comment:") {
                let child_nodes = self.parse_block(next_indent, nesting + 1);
                children.extend(child_nodes);
                continue;
            }

            self.advance();

            if let Some((k, v)) = field_str.split_once(':') {
                let key = k.trim();
                let val = v.trim();

                if key == "body" && val == "|" {
                    in_multiline_body = true;
                    multiline_indent = next_indent;
                    continue;
                }

                fields.push((key.to_string(), val.to_string()));
            }
        }

        // Extract fields
        let get = |key: &str| -> Option<String> {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };

        let reactions = get("reactions")
            .map(|v| {
                v.trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().trim_start_matches(':').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let body = if !body_lines.is_empty() {
            // Trim trailing empty lines
            while body_lines.last().is_some_and(|l| l.is_empty()) {
                body_lines.pop();
            }
            body_lines.join("\n")
        } else {
            get("body")
                .map(|v| v.trim_matches('"').to_string())
                .unwrap_or_default()
        };

        Some(Node::ThreadComment(ThreadComment {
            on: get("on"),
            author: get("author"),
            body,
            reactions,
            created_at: get("created_at").map(|v| v.trim_matches('|').to_string()),
            gates,
            children,
        }))
    }

    fn parse_entity_block(&mut self, name: String) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut source = String::new();
        let mut namespace = String::new();
        let mut identity = String::new();
        let mut fields: Vec<EntityField> = Vec::new();
        let mut in_fields = false;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line = strip_comment(next_trimmed);

            if line == "fields:" {
                in_fields = true;
                continue;
            }

            if in_fields {
                // Parse field line: "- name: type"
                let field_str = line.trim_start_matches('-').trim();
                if let Some((fname, ftype)) = field_str.split_once(':') {
                    fields.push(EntityField {
                        name: fname.trim().to_string(),
                        field_type: ftype.trim().to_string(),
                    });
                }
            } else if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let val = v.trim().to_string();
                match key {
                    "source" => source = val,
                    "namespace" => namespace = val,
                    "identity" => identity = val,
                    _ => {}
                }
            }
        }

        Some(Node::EntityDef(EntityDef {
            name,
            source,
            namespace,
            identity,
            fields,
        }))
    }

    fn parse_metric_block(&mut self, name: String) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut source: Option<String> = None;
        let mut grain: Option<String> = None;
        let mut dimensions: Vec<String> = Vec::new();
        let mut formula = String::new();
        let mut cross_source = false;
        let mut in_formula = false;

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                if in_formula {
                    formula.push('\n');
                    self.advance();
                    continue;
                }
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();

            if in_formula {
                // Accumulate formula lines
                if !formula.is_empty() {
                    formula.push('\n');
                }
                formula.push_str(next_trimmed);
                continue;
            }

            let line = strip_comment(next_trimmed);

            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "source" => source = Some(val.to_string()),
                    "grain" => grain = Some(val.to_string()),
                    "cross_source" => cross_source = val == "true",
                    "dimensions" => {
                        // Parse [dim1, dim2, ...]
                        let inner = val.trim_start_matches('[').trim_end_matches(']');
                        dimensions = inner
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    "formula" => {
                        if val == "|" || val.is_empty() {
                            in_formula = true;
                        } else {
                            formula = val.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Trim trailing whitespace from formula
        let formula = formula.trim().to_string();

        Some(Node::MetricDef(MetricDef {
            name,
            source,
            grain,
            dimensions,
            formula,
            cross_source,
        }))
    }

    fn parse_mod_def(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let raw_name = stripped.trim_start_matches("mod:$").trim();
        let versioned = raw_name.contains(":v") || raw_name.ends_with(" :v");
        let name = raw_name
            .replace(" :v", "")
            .replace(":v", "")
            .trim()
            .to_string();
        let base_indent = Self::indent_of(raw);
        let mut body = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line_str = strip_comment(next_trimmed);
            if let Some((k, v)) = line_str.split_once(':') {
                let key = k.trim().to_string();
                let val = v.trim().to_string();
                if val == "\"\"\"" {
                    // Triple-quote multiline string
                    let mut multiline = String::new();
                    while let Some(ml_raw) = self.peek() {
                        let ml_trimmed = ml_raw.trim();
                        if ml_trimmed == "\"\"\"" {
                            self.advance();
                            break;
                        }
                        if !multiline.is_empty() {
                            multiline.push('\n');
                        }
                        multiline.push_str(ml_trimmed);
                        self.advance();
                    }
                    body.push((key, multiline));
                } else {
                    body.push((key, val));
                }
            } else {
                body.push(("_".to_string(), line_str.to_string()));
            }
        }

        Some(Node::ModDef(ModDef {
            name,
            kind: body
                .iter()
                .find(|(k, _)| k == "kind")
                .map(|(_, v)| v.clone()),
            description: body
                .iter()
                .find(|(k, _)| k == "description")
                .map(|(_, v)| v.trim_matches('"').to_string()),
            trigger: body.iter().find(|(k, _)| k == "trigger").map(|(_, v)| {
                v.split('/')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect()
            }),
            body,
            versioned,
        }))
    }

    fn parse_git(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let rest = stripped.trim_start_matches("git:").trim();

        let (verb, args) = rest
            .split_once(char::is_whitespace)
            .map(|(v, a)| (v.to_string(), a.to_string()))
            .unwrap_or_else(|| (rest.to_string(), String::new()));

        let base_indent = Self::indent_of(raw);
        let mut body = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line_str = strip_comment(next_trimmed);
            if let Some((k, v)) = line_str.split_once(':') {
                body.push((k.trim().to_string(), v.trim().to_string()));
            }
        }

        Some(Node::Git(GitOp { verb, args, body }))
    }

    fn parse_conditional(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let cond_str = stripped
            .trim_start_matches("if ")
            .trim_end_matches(':')
            .trim();

        let (expr, live) = if cond_str.starts_with("||") && cond_str.ends_with("||") {
            (cond_str[2..cond_str.len() - 2].to_string(), true)
        } else if cond_str.starts_with('|') && cond_str.ends_with('|') {
            (cond_str[1..cond_str.len() - 1].to_string(), false)
        } else {
            (cond_str.to_string(), false)
        };

        let base_indent = Self::indent_of(raw);
        let children = self.parse_block(base_indent + 1, nesting + 1);

        Some(Node::Conditional(Conditional {
            condition: Compute { expr, live },
            children,
        }))
    }

    fn parse_webhook(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let rest = stripped.trim_start_matches("webhook:").trim();

        let trigger = if let Some(after_on) = rest
            .find("on:")
            .map(|i| &rest[i..])
            .and_then(|s| s.strip_prefix("on:"))
        {
            extract_pipe_expr(after_on)
        } else {
            String::new()
        };

        let url = if let Some(after_url) = rest
            .find("url:")
            .map(|i| &rest[i..])
            .and_then(|s| s.strip_prefix("url:"))
        {
            after_url
                .trim()
                .trim_matches('"')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        let payload = rest
            .find("payload:")
            .map(|i| &rest[i..])
            .and_then(|s| s.strip_prefix("payload:"))
            .map(|after_payload| after_payload.trim().trim_matches('"').to_string());

        Some(Node::Webhook(Webhook {
            trigger,
            url,
            payload,
        }))
    }

    fn parse_use_block(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let stripped = strip_comment(trimmed);
        let rest = stripped.trim_start_matches("use ").trim();

        // Parse the various use forms:
        //   use $ModName                              → mod_name=$ModName
        //   use @Entity from $Mod                     → entity=Entity, from_mod=Mod
        //   use @Entity from $Mod as @Alias           → entity=Entity, from_mod=Mod, alias=Alias
        //   use @Entity from @workspace:name          → entity=Entity, from_workspace=name
        //   use @Entity from @workspace:name as @Alias → entity=Entity, from_workspace=name, alias=Alias
        let mut entity = None;
        let mut from_mod = None;
        let mut from_workspace = None;
        let mut alias = None;
        let mod_name;

        if rest.starts_with('@') {
            // use @Entity from ...
            let parts: Vec<&str> = rest.splitn(2, " from ").collect();
            let entity_name = parts[0].trim().trim_start_matches('@').to_string();
            entity = Some(entity_name);

            if parts.len() > 1 {
                let from_rest = parts[1].trim();
                // Check for "as @Alias" at the end
                let (source, als) = if let Some(as_idx) = from_rest.find(" as @") {
                    (
                        from_rest[..as_idx].trim(),
                        Some(from_rest[as_idx + 5..].trim().to_string()),
                    )
                } else {
                    (from_rest, None)
                };
                alias = als;

                if source.starts_with('$') {
                    from_mod = Some(source.trim_start_matches('$').to_string());
                    mod_name = source.to_string();
                } else if let Some(stripped) = source.strip_prefix("@workspace:") {
                    from_workspace = Some(stripped.to_string());
                    mod_name = source.to_string();
                } else {
                    mod_name = source.to_string();
                }
            } else {
                mod_name = rest.to_string();
            }
        } else {
            // use $ModName (simple form)
            mod_name = rest.split_whitespace().next().unwrap_or("").to_string();
        }

        let base_indent = Self::indent_of(raw);
        let mut config = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line_str = strip_comment(next_trimmed);
            if let Some((k, v)) = line_str.split_once(':') {
                config.push((k.trim().to_string(), v.trim().to_string()));
            }
        }

        Some(Node::UseBlock(UseBlock {
            mod_name,
            config,
            entity,
            from_mod,
            from_workspace,
            alias,
        }))
    }

    fn parse_files_def(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut paths = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line = strip_comment(next_trimmed);
            if let Some(rest) = line.strip_prefix('@') {
                paths.push(rest.trim().to_string());
            } else {
                paths.push(line.to_string());
            }
        }

        Some(Node::FilesDef(FilesDef { paths }))
    }

    fn parse_policy_def(&mut self) -> Option<Node> {
        let raw = self.advance();
        let base_indent = Self::indent_of(raw);
        let mut rules = Vec::new();

        while let Some(next_raw) = self.peek() {
            let next_indent = Self::indent_of(next_raw);
            let next_trimmed = next_raw.trim();
            if next_trimmed.is_empty() {
                self.advance();
                continue;
            }
            if next_indent <= base_indent {
                break;
            }
            self.advance();
            let line = strip_comment(next_trimmed);
            if let Some(rest) = line.strip_prefix('@') {
                if let Some(colon) = rest.find(':') {
                    let path = rest[..colon].trim().to_string();
                    let gates_str = rest[colon + 1..].trim();
                    let gates = extract_gates(gates_str);
                    rules.push(PolicyRule { path, gates });
                } else {
                    rules.push(PolicyRule {
                        path: rest.trim().to_string(),
                        gates: Vec::new(),
                    });
                }
            }
        }

        Some(Node::PolicyDef(PolicyDef { rules }))
    }

    fn parse_mod_invocation(&mut self, text: &str) -> Option<Node> {
        let rest = text.trim_start_matches('$');
        if let Some(paren) = rest.find('(') {
            let prefix = &rest[..paren];
            let args = &rest[paren..];
            if let Some(dot) = prefix.find('.') {
                Some(Node::ModInvoke(ModInvoke {
                    name: prefix[..dot].to_string(),
                    method: Some(prefix[dot + 1..].to_string()),
                    args: Some(args.trim_matches(|c| c == '(' || c == ')').to_string()),
                }))
            } else {
                Some(Node::ModInvoke(ModInvoke {
                    name: prefix.to_string(),
                    method: None,
                    args: Some(args.trim_matches(|c| c == '(' || c == ')').to_string()),
                }))
            }
        } else if let Some(dot) = rest.find('.') {
            Some(Node::ModInvoke(ModInvoke {
                name: rest[..dot].to_string(),
                method: Some(rest[dot + 1..].to_string()),
                args: None,
            }))
        } else {
            Some(Node::ModInvoke(ModInvoke {
                name: rest.to_string(),
                method: None,
                args: None,
            }))
        }
    }

    // ── Lattice construct parsers ────────────────────────────────

    fn parse_lattice_validates(&mut self) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, 0);

        let mut artifacts = Vec::new();
        let mut remaining = Vec::new();

        for child in children {
            match &child {
                Node::Prose(p) => {
                    let text = p.text.trim();
                    if let Some(art_name) = text
                        .strip_prefix("- artifact:")
                        .or_else(|| text.strip_prefix("artifact:"))
                    {
                        let art_name = art_name.trim().to_string();
                        artifacts.push(LatticeArtifactRef {
                            artifact: art_name,
                            schema: None,
                            checks: Vec::new(),
                        });
                    } else if let Some(schema_val) = text.strip_prefix("schema:") {
                        // Attach schema to the last artifact if present
                        if let Some(last) = artifacts.last_mut() {
                            last.schema = Some(schema_val.trim().to_string());
                        } else {
                            remaining.push(child);
                        }
                    } else {
                        remaining.push(child);
                    }
                }
                _ => remaining.push(child),
            }
        }

        Some(Node::LatticeValidates(LatticeValidatesDef {
            artifacts,
            children: remaining,
        }))
    }

    fn parse_lattice_constraint(&mut self) -> Option<Node> {
        let raw = self.advance();
        let trimmed = raw.trim();
        let constraint_type = {
            let rest = trimmed.trim_start_matches("lattice_constraint:").trim();
            if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            }
        };
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, 0);

        let mut rule = String::new();
        let mut applies_to = Vec::new();
        let mut remaining = Vec::new();

        for child in children {
            if let Node::Prose(ref p) = child {
                let text = p.text.trim();
                if let Some(val) = text.strip_prefix("rule:") {
                    rule = val.trim().to_string();
                } else if let Some(val) = text.strip_prefix("applies_to:") {
                    applies_to = val
                        .trim()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                } else {
                    remaining.push(child);
                }
            } else {
                remaining.push(child);
            }
        }

        Some(Node::LatticeConstraint(LatticeConstraintDef {
            constraint_type,
            rule,
            applies_to,
            children: remaining,
        }))
    }

    fn parse_lattice_schema(&mut self) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, 0);

        let mut fields = Vec::new();
        let mut remaining = Vec::new();

        // Collect field definitions from consecutive prose lines
        let mut pending_name: Option<String> = None;
        let mut pending_type: Option<String> = None;
        let mut pending_required = false;

        for child in children {
            if let Node::Prose(ref p) = child {
                let text = p.text.trim();
                if let Some(val) = text.strip_prefix("name:") {
                    // Flush any pending field
                    if let Some(name) = pending_name.take() {
                        fields.push(LatticeSchemaField {
                            name,
                            field_type: pending_type.take().unwrap_or_default(),
                            required: pending_required,
                        });
                        pending_required = false;
                    }
                    pending_name = Some(val.trim().to_string());
                } else if let Some(val) = text.strip_prefix("type:") {
                    pending_type = Some(val.trim().to_string());
                } else if let Some(val) = text.strip_prefix("required:") {
                    pending_required = val.trim() == "true";
                } else {
                    remaining.push(child);
                }
            } else {
                remaining.push(child);
            }
        }
        // Flush last pending field
        if let Some(name) = pending_name.take() {
            fields.push(LatticeSchemaField {
                name,
                field_type: pending_type.take().unwrap_or_default(),
                required: pending_required,
            });
        }

        Some(Node::LatticeSchema(LatticeSchemaDef {
            fields,
            children: remaining,
        }))
    }

    fn parse_lattice_frontier(&mut self) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, 0);

        let mut expected_schema = None;
        let mut missing_fields = Vec::new();
        let mut exploration_strategy = Vec::new();
        let mut remaining = Vec::new();

        for child in children {
            if let Node::Prose(ref p) = child {
                let text = p.text.trim();
                if let Some(val) = text.strip_prefix("expected_schema:") {
                    expected_schema = Some(val.trim().to_string());
                } else if let Some(val) = text.strip_prefix("missing_field:") {
                    missing_fields.push(val.trim().to_string());
                } else if let Some(val) = text.strip_prefix("strategy:") {
                    exploration_strategy.push(val.trim().to_string());
                } else {
                    remaining.push(child);
                }
            } else {
                remaining.push(child);
            }
        }

        Some(Node::LatticeFrontier(LatticeFrontierDef {
            expected_schema,
            missing_fields,
            exploration_strategy,
            children: remaining,
        }))
    }

    fn parse_pressure_effect(&mut self) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, 0);

        let mut dynamic = String::new();
        let mut target = None;

        for child in &children {
            if let Node::Prose(p) = child {
                let text = p.text.trim();
                if let Some(val) = text.strip_prefix("dynamic:") {
                    dynamic = val.trim().to_string();
                } else if let Some(val) = text.strip_prefix("target:") {
                    target = Some(val.trim().to_string());
                }
            }
        }

        Some(Node::PressureEffect(PressureEffectDef { dynamic, target }))
    }

    fn parse_unit_cell(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, nesting + 1);
        Some(Node::UnitCell(UnitCellDef { children }))
    }

    fn parse_symmetry(&mut self, nesting: usize) -> Option<Node> {
        let raw = self.advance();
        let indent = Self::indent_of(raw);
        let children = self.parse_block(indent + 4, nesting + 1);
        Some(Node::Symmetry(SymmetryDef { children }))
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn canonicalize_nodes(nodes: Vec<Node>) -> Result<Vec<Node>, ParseError> {
    nodes.into_iter().map(canonicalize_node).collect()
}

fn canonicalize_node(node: Node) -> Result<Node, ParseError> {
    match node {
        Node::Group(mut group) => {
            group.children = canonicalize_nodes(group.children)?;

            if group.name == "Sync" {
                group.children = canonicalize_sync_children(group.children)?;
            }

            // Canonicalize lattice section headers into proper lattice nodes
            if group.depth == 2 {
                match group.name.as_str() {
                    "Validates" => return Ok(canonicalize_validates(group)),
                    "Constraint" => return Ok(canonicalize_constraint(group)),
                    "Schema" => return Ok(canonicalize_schema(group)),
                    "Frontier" => return Ok(canonicalize_frontier(group)),
                    "Pressure Effect" => return Ok(canonicalize_pressure_effect(group)),
                    "Unit Cell" => return Ok(canonicalize_unit_cell(group)),
                    "Symmetry" => return Ok(canonicalize_symmetry(group)),
                    _ => {}
                }
            }

            Ok(Node::Group(group))
        }
        Node::Task(mut task) => {
            task.children = canonicalize_nodes(task.children)?;
            task.on_pass = canonicalize_optional_nodes(task.on_pass)?;
            task.on_fail = canonicalize_optional_nodes(task.on_fail)?;

            if let Some(arms) = task.match_arms.as_mut() {
                for arm in arms {
                    arm.children = canonicalize_nodes(std::mem::take(&mut arm.children))?;
                }
            }

            Ok(Node::Task(task))
        }
        Node::Validate(mut validate) => {
            validate.children = canonicalize_nodes(validate.children)?;
            Ok(Node::Validate(validate))
        }
        Node::Check(c) => Ok(Node::Check(c)),
        Node::Conditional(mut conditional) => {
            conditional.children = canonicalize_nodes(conditional.children)?;
            Ok(Node::Conditional(conditional))
        }
        Node::GateDef(mut gate) => {
            gate.children = canonicalize_nodes(gate.children)?;
            Ok(Node::GateDef(gate))
        }
        Node::Issue(mut issue) => {
            issue.children = canonicalize_nodes(issue.children)?;
            Ok(Node::Issue(issue))
        }
        Node::ThreadComment(mut tc) => {
            tc.children = canonicalize_nodes(tc.children)?;
            Ok(Node::ThreadComment(tc))
        }
        Node::ProjectScope(mut ps) => {
            ps.children = canonicalize_nodes(ps.children)?;
            Ok(Node::ProjectScope(ps))
        }
        Node::LatticeValidates(mut lv) => {
            lv.children = canonicalize_nodes(lv.children)?;
            Ok(Node::LatticeValidates(lv))
        }
        Node::LatticeConstraint(mut lc) => {
            lc.children = canonicalize_nodes(lc.children)?;
            Ok(Node::LatticeConstraint(lc))
        }
        Node::LatticeSchema(mut ls) => {
            ls.children = canonicalize_nodes(ls.children)?;
            Ok(Node::LatticeSchema(ls))
        }
        Node::LatticeFrontier(mut lf) => {
            lf.children = canonicalize_nodes(lf.children)?;
            Ok(Node::LatticeFrontier(lf))
        }
        Node::UnitCell(mut uc) => {
            uc.children = canonicalize_nodes(uc.children)?;
            Ok(Node::UnitCell(uc))
        }
        Node::Symmetry(mut sym) => {
            sym.children = canonicalize_nodes(sym.children)?;
            Ok(Node::Symmetry(sym))
        }
        other => Ok(other),
    }
}

fn canonicalize_optional_nodes(nodes: Option<Vec<Node>>) -> Result<Option<Vec<Node>>, ParseError> {
    nodes.map(canonicalize_nodes).transpose()
}

fn canonicalize_sync_children(children: Vec<Node>) -> Result<Vec<Node>, ParseError> {
    children
        .into_iter()
        .map(|child| match child {
            Node::Group(group) => parse_sync_def(group).map(Node::SyncDef),
            other => Ok(other),
        })
        .collect()
}

fn parse_sync_def(group: Group) -> Result<SyncDef, ParseError> {
    let fields = parse_sync_fields(&group.children);
    let class = fields.get("class").cloned().unwrap_or_default();

    if !class.is_empty() && !matches!(class.as_str(), "canon" | "ops" | "data") {
        return Err(ParseError {
            code: "E_SYNC_INVALID_CLASS".to_string(),
            kind: "parse_error".to_string(),
            message: format!(
                "Invalid sync class '{}' in sync declaration '{}'; expected canon, ops, or data",
                class, group.name
            ),
            context: Some(group.name),
            line: 0,
            col: 0,
        });
    }

    Ok(SyncDef {
        name: group.name,
        class,
        source: fields.get("source").cloned().unwrap_or_default(),
        identity: fields.get("identity").cloned().unwrap_or_default(),
        mode: fields.get("mode").cloned().unwrap_or_default(),
        target: fields.get("target").cloned().unwrap_or_default(),
        schedule: fields.get("schedule").cloned().unwrap_or_default(),
        scope: fields.get("scope").cloned().unwrap_or_default(),
    })
}

fn parse_sync_fields(children: &[Node]) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for child in children {
        if let Node::Prose(prose) = child {
            if let Some((key, value)) = prose.text.split_once(':') {
                fields.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    fields
}

// ── Lattice section-header canonicalization helpers ──────────────

/// Extract key-value pairs from prose children for lattice canonicalization.
fn extract_lattice_fields(children: &[Node]) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for child in children {
        if let Node::Prose(prose) = child {
            if let Some((key, value)) = prose.text.trim().split_once(':') {
                fields.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    fields
}

fn canonicalize_validates(group: Group) -> Node {
    let mut artifacts = Vec::new();
    let mut remaining = Vec::new();

    for child in group.children {
        if let Node::Prose(ref p) = child {
            let text = p.text.trim();
            if let Some(art_name) = text
                .strip_prefix("- artifact:")
                .or_else(|| text.strip_prefix("artifact:"))
            {
                let art_name = art_name.trim().to_string();
                artifacts.push(LatticeArtifactRef {
                    artifact: art_name,
                    schema: None,
                    checks: Vec::new(),
                });
            } else if let Some(schema_val) = text.strip_prefix("schema:") {
                if let Some(last) = artifacts.last_mut() {
                    last.schema = Some(schema_val.trim().to_string());
                } else {
                    remaining.push(child);
                }
            } else {
                remaining.push(child);
            }
        } else {
            remaining.push(child);
        }
    }

    Node::LatticeValidates(LatticeValidatesDef {
        artifacts,
        children: remaining,
    })
}

fn canonicalize_constraint(group: Group) -> Node {
    let fields = extract_lattice_fields(&group.children);
    let applies_to = fields
        .get("applies_to")
        .map(|s| {
            s.split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut remaining = Vec::new();
    for child in group.children {
        if let Node::Prose(ref p) = child {
            let text = p.text.trim();
            if text.starts_with("rule:") || text.starts_with("applies_to:") {
                continue;
            }
            remaining.push(child);
        } else {
            remaining.push(child);
        }
    }

    Node::LatticeConstraint(LatticeConstraintDef {
        constraint_type: None,
        rule: fields.get("rule").cloned().unwrap_or_default(),
        applies_to,
        children: remaining,
    })
}

fn canonicalize_schema(group: Group) -> Node {
    let mut fields = Vec::new();
    let mut remaining = Vec::new();
    let mut pending_name: Option<String> = None;
    let mut pending_type: Option<String> = None;
    let mut pending_required = false;

    for child in group.children {
        if let Node::Prose(ref p) = child {
            let text = p.text.trim();
            if let Some(val) = text.strip_prefix("name:") {
                if let Some(name) = pending_name.take() {
                    fields.push(LatticeSchemaField {
                        name,
                        field_type: pending_type.take().unwrap_or_default(),
                        required: pending_required,
                    });
                    pending_required = false;
                }
                pending_name = Some(val.trim().to_string());
            } else if let Some(val) = text.strip_prefix("type:") {
                pending_type = Some(val.trim().to_string());
            } else if let Some(val) = text.strip_prefix("required:") {
                pending_required = val.trim() == "true";
            } else {
                remaining.push(child);
            }
        } else {
            remaining.push(child);
        }
    }
    if let Some(name) = pending_name.take() {
        fields.push(LatticeSchemaField {
            name,
            field_type: pending_type.take().unwrap_or_default(),
            required: pending_required,
        });
    }

    Node::LatticeSchema(LatticeSchemaDef {
        fields,
        children: remaining,
    })
}

fn canonicalize_frontier(group: Group) -> Node {
    let mut expected_schema = None;
    let mut missing_fields = Vec::new();
    let mut exploration_strategy = Vec::new();
    let mut remaining = Vec::new();

    for child in group.children {
        if let Node::Prose(ref p) = child {
            let text = p.text.trim();
            if let Some(val) = text.strip_prefix("expected_schema:") {
                expected_schema = Some(val.trim().to_string());
            } else if let Some(val) = text.strip_prefix("missing_field:") {
                missing_fields.push(val.trim().to_string());
            } else if let Some(val) = text.strip_prefix("strategy:") {
                exploration_strategy.push(val.trim().to_string());
            } else {
                remaining.push(child);
            }
        } else {
            remaining.push(child);
        }
    }

    Node::LatticeFrontier(LatticeFrontierDef {
        expected_schema,
        missing_fields,
        exploration_strategy,
        children: remaining,
    })
}

fn canonicalize_pressure_effect(group: Group) -> Node {
    let fields = extract_lattice_fields(&group.children);
    Node::PressureEffect(PressureEffectDef {
        dynamic: fields.get("dynamic").cloned().unwrap_or_default(),
        target: fields.get("target").cloned(),
    })
}

fn canonicalize_unit_cell(group: Group) -> Node {
    Node::UnitCell(UnitCellDef {
        children: group.children,
    })
}

fn canonicalize_symmetry(group: Group) -> Node {
    Node::Symmetry(SymmetryDef {
        children: group.children,
    })
}

fn strip_comment(s: &str) -> &str {
    if let Some(idx) = find_comment_start(s) {
        s[..idx].trim_end()
    } else {
        s
    }
}

fn find_comment_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_string = false;
    let mut in_pipe = 0u8;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => {
                i += 2; // skip escaped character
                continue;
            }
            b'"' => in_string = !in_string,
            b'|' if !in_string => {
                if in_pipe > 0 {
                    in_pipe -= 1;
                } else {
                    in_pipe += 1;
                }
            }
            b'/' if !in_string && in_pipe == 0 => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_task_line(trimmed: &str) -> bool {
    // Patterns: [!] [o] [ ] [x] [1] [2!] [3o] +[!] -[!] --[o] ++[!]
    let s = trimmed.trim_start_matches('+').trim_start_matches('-');
    s.starts_with('[') && s.contains(']')
}

struct ParsedMarker {
    marker: TaskMarker,
    label: Option<String>,
}

fn parse_task_marker(text: &str) -> Option<(ParsedMarker, &str)> {
    let mut s = text;
    let mut prefix = TaskPrefix::None;

    // Count prefix +/- for parallel/subtask
    if let Some(rest) = s.strip_prefix("++") {
        if rest.starts_with('[') {
            prefix = TaskPrefix::ParallelSubtask;
            s = rest;
        }
    }
    if prefix == TaskPrefix::None {
        if let Some(rest) = s.strip_prefix('+') {
            if rest.starts_with('[') {
                prefix = TaskPrefix::Parallel;
                s = rest;
            }
        }
    }
    if prefix == TaskPrefix::None {
        let dash_count = s.chars().take_while(|&c| c == '-').count();
        if dash_count > 0 && s.len() > dash_count && s.as_bytes()[dash_count] == b'[' {
            prefix = TaskPrefix::Subtask(dash_count as u8);
            s = &s[dash_count..];
        }
    }

    if !s.starts_with('[') {
        return None;
    }

    let bracket_end = s.find(']')?;
    let inside = &s[1..bracket_end];
    let rest = &s[bracket_end + 1..];

    let (label, kind, priority, seq) = parse_bracket_content(inside);

    Some((
        ParsedMarker {
            marker: TaskMarker {
                kind,
                priority,
                prefix,
                seq,
            },
            label,
        },
        rest.trim_start(),
    ))
}

fn parse_bracket_content(inside: &str) -> (Option<String>, TaskKind, Priority, Option<u32>) {
    let trimmed = inside.trim();

    if trimmed.is_empty() || trimmed == " " {
        return (None, TaskKind::Open, Priority::None, None);
    }

    match trimmed {
        "!" => return (None, TaskKind::Required, Priority::Required, None),
        "o" => return (None, TaskKind::Optional, Priority::Optional, None),
        "x" => return (None, TaskKind::Completed, Priority::None, None),
        _ => {}
    }

    // Check for label + modifier: [A!] [setup!] [C?]
    let last = trimmed.as_bytes().last().copied();
    let has_bang = last == Some(b'!');
    let has_o = last == Some(b'o') && trimmed.len() > 1;
    let has_q = last == Some(b'?');

    // Pure numeric: [1] [2] [3]
    if let Ok(n) = trimmed.parse::<u32>() {
        return (None, TaskKind::Open, Priority::None, Some(n));
    }

    // Numeric + modifier: [2!] [3o]
    if trimmed.len() >= 2 {
        // Use char boundary to avoid panic on multi-byte UTF-8
        let last_char_start = trimmed
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        let prefix_str = &trimmed[..last_char_start];
        if let Ok(n) = prefix_str.parse::<u32>() {
            if has_bang {
                return (None, TaskKind::Required, Priority::Required, Some(n));
            }
            if has_o {
                return (None, TaskKind::Optional, Priority::Optional, Some(n));
            }
        }
    }

    // Label + modifier: [A!] [build!] [D?]
    if has_bang {
        let label = trimmed[..trimmed.len() - 1].to_string();
        return (Some(label), TaskKind::Required, Priority::Required, None);
    }
    if has_q {
        let label = trimmed[..trimmed.len() - 1].to_string();
        return (Some(label), TaskKind::Open, Priority::Decision, None);
    }
    if has_o
        && trimmed.len() > 1
        && !trimmed[..trimmed.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        let label = trimmed[..trimmed.len() - 1].to_string();
        return (Some(label), TaskKind::Optional, Priority::Optional, None);
    }

    // Just a label: [A]
    (
        Some(trimmed.to_string()),
        TaskKind::Open,
        Priority::None,
        None,
    )
}

fn extract_gates(text: &str) -> Vec<Gate> {
    let mut gates = Vec::new();
    let mut s = text;

    while let Some(start) = s.find('{') {
        if let Some(end) = find_matching_brace(s, start) {
            let inner = &s[start + 1..end];
            let name = inner.split_whitespace().next().unwrap_or(inner).to_string();
            let body = if inner.len() > name.len() {
                Some(inner[name.len()..].trim().to_string())
            } else {
                None
            };
            gates.push(Gate { name, body });
            s = &s[end + 1..];
        } else {
            break;
        }
    }

    gates
}

fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_closes(text: &str) -> (String, Option<u32>) {
    if let Some(pos) = text.find("git:closes #") {
        let after = &text[pos + 12..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num_str.parse::<u32>() {
            let end = pos + 12 + num_str.len();
            let cleaned = format!("{}{}", &text[..pos], &text[end..]);
            return (cleaned, Some(n));
        }
    }
    (text.to_string(), None)
}

fn remove_gates(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find('{') {
        if let Some(end) = find_matching_brace(&result, start) {
            result = format!("{}{}", &result[..start], &result[end + 1..]);
        } else {
            break;
        }
    }
    result
}

fn parse_name_and_atoms(text: &str) -> (String, Vec<Atom>) {
    let mut name_parts = Vec::new();
    let mut atoms = Vec::new();

    for word in text.split_whitespace() {
        if let Some(atom_text) = word.strip_prefix(':') {
            if let Some((k, v)) = atom_text.split_once(':') {
                atoms.push(Atom {
                    name: k.to_string(),
                    value: Some(v.to_string()),
                });
            } else {
                atoms.push(Atom {
                    name: atom_text.to_string(),
                    value: None,
                });
            }
        } else if !word.starts_with("on:")
            && !word.starts_with("routine:")
            && !word.starts_with("due:")
            && !word.starts_with("deadline:")
            && !word.starts_with("timebox:")
            && !word.starts_with("available:")
            && !word.starts_with("blackout:")
        {
            name_parts.push(word);
        } else {
            // Time/trigger atoms with values
            if let Some((k, v)) = word.split_once(':') {
                atoms.push(Atom {
                    name: k.to_string(),
                    value: Some(v.to_string()),
                });
            }
        }
    }

    let name = name_parts.join(" ");
    (name, atoms)
}

fn parse_inline(text: &str) -> Vec<Inline> {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '@' if i == 0
                || chars[i - 1].is_whitespace()
                || chars[i - 1] == '('
                || chars[i - 1] == ':' =>
            {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let (r, consumed) = parse_ref_at(&chars[i..]);
                spans.push(Inline::Ref(r));
                i += consumed;
            }
            '$' => {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let (name, consumed) = scan_identifier(&chars[i + 1..]);
                // Check for $Mod.@Entity pattern — produces a mod-scoped Ref
                let after = i + 1 + consumed;
                if name.ends_with('.') && after < chars.len() && chars[after] == '@' {
                    let mod_name = name.trim_end_matches('.').to_string();
                    let (mut r, ref_consumed) = parse_ref_at(&chars[after..]);
                    r.mod_scope = Some(mod_name);
                    spans.push(Inline::Ref(r));
                    i = after + ref_consumed;
                } else {
                    let clean_name = name.trim_end_matches('.').to_string();
                    spans.push(Inline::ModCall {
                        name: clean_name,
                        args: None,
                    });
                    i += 1 + consumed;
                }
            }
            '%' if i + 1 < chars.len()
                && chars[i + 1].is_alphanumeric()
                && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(') =>
            {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                // Consume project name greedily: alphanum, spaces, hyphens, underscores
                // Stop at inline markers or end of text; trim trailing whitespace
                let mut name = String::new();
                let mut j = i + 1;
                while j < chars.len() {
                    match chars[j] {
                        '@' | '$' | '#' | '|' | '{' | '*' | '%' => break,
                        c => {
                            name.push(c);
                            j += 1;
                        }
                    }
                }
                let name = name.trim_end().to_string();
                // Adjust consumed to not eat trailing whitespace that was trimmed
                let consumed = 1 + name.len(); // '%' + name chars (no trailing space)
                spans.push(Inline::ProjectRef { name });
                i += consumed;
            }
            '#' if i + 1 < chars.len() && chars[i + 1].is_alphanumeric() => {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let (name, consumed) = scan_identifier(&chars[i + 1..]);
                spans.push(Inline::Channel { name });
                i += 1 + consumed;
            }
            '|' if i + 1 < chars.len() && chars[i + 1] == '|' => {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let end = find_double_pipe_end(&chars, i + 2);
                let expr: String = chars[i + 2..end].iter().collect();
                spans.push(Inline::Compute(Compute { expr, live: true }));
                i = end + 2;
            }
            '|' => {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let end = find_single_pipe_end(&chars, i + 1);
                let expr: String = chars[i + 1..end].iter().collect();
                spans.push(Inline::Compute(Compute { expr, live: false }));
                i = end + 1;
            }
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                if !current_text.is_empty() {
                    spans.push(Inline::Text {
                        value: std::mem::take(&mut current_text),
                    });
                }
                let start = i + 2;
                let end = find_bold_end(&chars, start);
                let inner: String = chars[start..end].iter().collect();
                spans.push(Inline::Bold { value: inner });
                i = end + 2;
            }
            _ => {
                current_text.push(chars[i]);
                i += 1;
            }
        }
    }

    if !current_text.is_empty() {
        spans.push(Inline::Text {
            value: current_text,
        });
    }

    spans
}

fn parse_ref_at(chars: &[char]) -> (Ref, usize) {
    let mut path = Vec::new();
    let mut current = String::new();
    let mut i = 1; // skip @
    let mut plural = false;
    let mut workspace_scope = None;

    // Check for @workspace:name.@Entity pattern
    let remaining: String = chars[i..].iter().collect();
    if remaining.starts_with("workspace:") {
        if let Some(dot_at) = remaining.find(".@") {
            let ws_name = &remaining[10..dot_at]; // after "workspace:", before ".@"
            workspace_scope = Some(ws_name.to_string());
            i += dot_at + 2; // skip past "workspace:name.@"
                             // Now parse the entity part normally
        }
    }

    while i < chars.len() {
        match chars[i] {
            ':' => {
                if !current.is_empty() {
                    path.push(std::mem::take(&mut current));
                }
                i += 1;
            }
            '(' if i + 2 < chars.len() && chars[i + 1] == 's' && chars[i + 2] == ')' => {
                plural = true;
                if !current.is_empty() {
                    path.push(std::mem::take(&mut current));
                }
                i += 3;
            }
            c if c.is_alphanumeric()
                || c == '_'
                || c == '-'
                || c == '.'
                || c == '/'
                || c == '~' =>
            {
                current.push(c);
                i += 1;
            }
            _ => break,
        }
    }

    if !current.is_empty() {
        path.push(current);
    }

    if path.is_empty() {
        path.push(String::new());
    }

    (
        Ref {
            path,
            plural,
            mod_scope: None,
            workspace_scope,
        },
        i,
    )
}

fn scan_identifier(chars: &[char]) -> (String, usize) {
    let mut name = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.' {
            name.push(c);
            i += 1;
        } else {
            break;
        }
    }
    (name, i)
}

fn find_single_pipe_end(chars: &[char], start: usize) -> usize {
    for i in start..chars.len() {
        if chars[i] == '|' && !(i + 1 < chars.len() && chars[i + 1] == '|') {
            return i;
        }
    }
    chars.len()
}

fn find_double_pipe_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '|' && chars[i + 1] == '|' {
            return i;
        }
        i += 1;
    }
    chars.len()
}

fn find_bold_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return i;
        }
        i += 1;
    }
    chars.len()
}

fn extract_pipe_expr(s: &str) -> String {
    if let Some(start) = s.find('|') {
        let rest = &s[start + 1..];
        if let Some(end) = rest.find('|') {
            return rest[..end].to_string();
        }
    }
    s.to_string()
}

fn parse_field_def(line: &str) -> Option<FieldDef> {
    let (name_part, value_part) = line.split_once(':')?;
    let raw_name = name_part.trim();
    let value = value_part.trim();

    let (name, plural) = if raw_name.contains("(s)") {
        (raw_name.replace("(s)", ""), true)
    } else {
        (raw_name.to_string(), false)
    };

    let default = parse_field_default(value);
    Some(FieldDef {
        name,
        plural,
        default,
    })
}

fn parse_form_text(value: &str) -> String {
    let trimmed = value.trim();

    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_form_u32(value: &str) -> Option<u32> {
    parse_form_text(value).parse::<u32>().ok()
}

/// Extracts entity scoping from text after a construct keyword (define:, mutate:, delete:).
///
/// Handles three forms:
///   - `@Entity...`                     → (None, None, "Entity...")
///   - `$Mod.@Entity...`               → (Some("Mod"), None, "Entity...")
///   - `@workspace:name.@Entity...`    → (None, Some("name"), "Entity...")
fn extract_entity_scope(text: &str) -> (Option<String>, Option<String>, &str) {
    // $Mod.@Entity pattern
    if text.starts_with('$') {
        if let Some(dot_at) = text.find(".@") {
            let mod_name = &text[1..dot_at];
            let rest = &text[dot_at + 2..]; // after .@
            return (Some(mod_name.to_string()), None, rest);
        }
    }
    // @workspace:name.@Entity pattern
    if let Some(after_ws) = text.strip_prefix("@workspace:") {
        // after "@workspace:"
        if let Some(dot_at) = after_ws.find(".@") {
            let ws_name = &after_ws[..dot_at];
            let rest = &after_ws[dot_at + 2..]; // after .@
            return (None, Some(ws_name.to_string()), rest);
        }
    }
    // @Entity (local, no scoping)
    if let Some(stripped) = text.strip_prefix('@') {
        return (None, None, stripped);
    }
    (None, None, text)
}

fn parse_field_default(value: &str) -> FieldDefault {
    let v = value.trim();

    if v == "nil" {
        return FieldDefault::Nil;
    }
    if v == "true" || v == "false" {
        return FieldDefault::Bool(v == "true");
    }
    if v == "[]" {
        return FieldDefault::List;
    }
    if v.starts_with('"') && v.ends_with('"') {
        return FieldDefault::Str(v[1..v.len() - 1].to_string());
    }
    if v.starts_with('|') && v.ends_with('|') {
        return FieldDefault::Timestamp(v[1..v.len() - 1].to_string());
    }
    if let Some(rest) = v.strip_prefix('@') {
        return FieldDefault::Ref(rest.to_string());
    }
    if v.contains('/') && v.starts_with(':') {
        let opts: Vec<String> = v
            .split('/')
            .map(|s| s.trim().trim_start_matches(':').to_string())
            .collect();
        return FieldDefault::Enum(opts);
    }
    if let Some(rest) = v.strip_prefix(':') {
        return FieldDefault::Atom(rest.to_string());
    }
    if let Ok(n) = v.parse::<i64>() {
        return FieldDefault::Int(n);
    }
    if let Ok(f) = v.parse::<f64>() {
        return FieldDefault::Float(f);
    }

    FieldDefault::Str(v.to_string())
}

fn try_parse_variable(line: &str) -> Option<Variable> {
    // Must contain = but not be a define/mutate/flow/etc
    if line.starts_with('#')
        || line.starts_with('[')
        || line.starts_with('+')
        || line.starts_with('-')
        || line.starts_with("//")
        || line.starts_with("define:")
        || line.starts_with("mutate:")
        || line.starts_with("query:")
        || line.starts_with("flow:")
        || line.starts_with("states:")
        || line.starts_with("status:")
        || line.starts_with("validate")
        || line.starts_with("form:")
        || line.starts_with("mod:")
        || line.starts_with("git:")
        || line.starts_with("if ")
        || line.starts_with("**")
        || line.starts_with("$")
        || line.starts_with('@')
        || line.starts_with("snap:")
        || line.starts_with("diff:")
        || line.starts_with("history:")
        || line.starts_with("routine:")
        || line.starts_with("use ")
        || line.starts_with("webhook:")
        || line.starts_with("delete:")
        || line.starts_with("? ")
        || line.starts_with("remember:")
        || line.starts_with("recall:")
        || line.starts_with('^')
        || line.starts_with("files:")
        || line.starts_with("policy:")
        || line.starts_with("escalate:")
        || line.starts_with("gate:")
        || line.starts_with("check:")
    {
        return None;
    }

    let eq_pos = line.find('=')?;
    if eq_pos == 0 {
        return None;
    }

    // Avoid matching == or != or ~= etc
    let before = line.as_bytes()[eq_pos.saturating_sub(1)];
    if before == b'!' || before == b'~' || before == b'<' || before == b'>' {
        return None;
    }
    if eq_pos + 1 < line.len() && line.as_bytes()[eq_pos + 1] == b'=' {
        return None;
    }

    let name = line[..eq_pos].trim().to_string();
    let raw_value = line[eq_pos + 1..].trim();

    // Reject if name contains spaces (probably prose)
    if name.contains(' ') && !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    let value = if raw_value.starts_with("||") && raw_value.ends_with("||") {
        VarValue::Compute(Compute {
            expr: raw_value[2..raw_value.len() - 2].to_string(),
            live: true,
        })
    } else if raw_value.starts_with('|') && raw_value.ends_with('|') {
        VarValue::Compute(Compute {
            expr: raw_value[1..raw_value.len() - 1].to_string(),
            live: false,
        })
    } else if raw_value.starts_with('@') {
        let chars: Vec<char> = raw_value.chars().collect();
        let (r, _) = parse_ref_at(&chars);
        VarValue::Ref(r)
    } else {
        VarValue::Literal(raw_value.to_string())
    };

    Some(Variable { name, value })
}

fn parse_query_from_lines(lines: &[String]) -> Option<Query> {
    if lines.is_empty() {
        return None;
    }

    let first = &lines[0];

    // Extract temporal scoping: "from snap:version" suffix (before where clause)
    let (pre_snap, from_snapshot) = if let Some(snap_idx) = first.find(" from snap:") {
        let snap_version = first[snap_idx + 11..].trim();
        // The snap version might be followed by a where clause
        let (snap_val, remaining) = if let Some(where_pos) = snap_version.find(" where ") {
            (
                snap_version[..where_pos].trim().to_string(),
                Some(format!(
                    "{} where {}",
                    &first[..snap_idx],
                    &snap_version[where_pos + 7..]
                )),
            )
        } else {
            (snap_version.to_string(), None)
        };
        (
            remaining.unwrap_or_else(|| first[..snap_idx].to_string()),
            Some(snap_val),
        )
    } else {
        (first.to_string(), None)
    };

    // Extract entity scoping: $Mod.@Entity or @workspace:name.@Entity
    let (mod_scope, workspace_scope, scoped_text) = extract_entity_scope(pre_snap.trim());
    let scoped_str = scoped_text.to_string();

    let (entity_part, filter) = if let Some(where_pos) = scoped_str.find(" where ") {
        (
            &scoped_str[..where_pos],
            Some(scoped_str[where_pos + 7..].to_string()),
        )
    } else {
        (scoped_str.as_str(), None)
    };

    let (entity, plural) = if entity_part.contains("(s)") {
        (entity_part.replace("(s)", "").trim().to_string(), true)
    } else {
        (entity_part.trim().to_string(), false)
    };

    let mut sort = None;
    let mut limit = None;
    let mut include = None;

    for line in &lines[1..] {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("sort:") {
            sort = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("limit:") {
            limit = rest.trim().parse().ok();
        } else if let Some(rest) = trimmed.strip_prefix("include:") {
            include = Some(rest.split(',').map(|s| s.trim().to_string()).collect());
        }
    }

    Some(Query {
        entity,
        plural,
        filter,
        sort,
        limit,
        include,
        mod_scope,
        workspace_scope,
        from_snapshot,
    })
}

fn parse_flow_edges(line: &str) -> Vec<FlowEdge> {
    let mut edges = Vec::new();

    // Split on --> variants: +--> , --label--> , --timeout:...--> , -->
    let segments: Vec<&str> = line.split("-->").collect();

    for i in 0..segments.len().saturating_sub(1) {
        let from_part = segments[i].trim();
        let to_part = segments[i + 1].trim();

        let parallel = from_part.ends_with('+') || from_part.ends_with("+-");
        let from_clean = from_part
            .trim_end_matches('+')
            .trim_end_matches('-')
            .trim_end_matches("+-");

        let (label, gate, wait, timeout) = parse_edge_modifiers(from_part);

        let from: Vec<String> = if from_clean.is_empty() && i > 0 {
            Vec::new()
        } else {
            from_clean
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let to_clean = to_part.split("-->").next().unwrap_or(to_part).trim();
        let to: Vec<String> = to_clean
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !from.is_empty() || !to.is_empty() {
            edges.push(FlowEdge {
                from,
                to,
                label,
                parallel,
                gate,
                wait,
                timeout,
            });
        }
    }

    edges
}

fn parse_edge_modifiers(
    segment: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let trimmed = segment.trim();

    // Check for --label-- pattern (both dashes present)
    if let Some(last_dash_dash) = trimmed.rfind("--") {
        let before = &trimmed[..last_dash_dash];
        if let Some(start_dash) = before.rfind("--") {
            let inner = &trimmed[start_dash + 2..last_dash_dash];
            if !inner.is_empty() {
                return classify_edge_modifier(inner);
            }
        }
    }

    // Trailing -- consumed by --> split: look for single -- prefix
    if let Some(dash_pos) = trimmed.rfind("--") {
        let inner = trimmed[dash_pos + 2..].trim();
        if !inner.is_empty() && dash_pos > 0 {
            return classify_edge_modifier(inner);
        }
    }

    (None, None, None, None)
}

fn classify_edge_modifier(
    inner: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if inner.starts_with("timeout:") {
        let expr = inner.trim_start_matches("timeout:");
        return (None, None, None, Some(extract_pipe_expr(expr)));
    }
    if inner.starts_with("wait:") {
        let expr = inner.trim_start_matches("wait:");
        return (None, None, Some(extract_pipe_expr(expr)), None);
    }
    if inner.starts_with('{') {
        return (None, Some(inner.to_string()), None, None);
    }
    (Some(inner.to_string()), None, None, None)
}

/// Extract pressure-field metadata fields (depends:, validate:, status:) from
/// task children. Returns the remaining children plus extracted optional values.
fn extract_task_fields(
    children: Vec<Node>,
) -> (Vec<Node>, Option<String>, Option<String>, Option<String>) {
    let mut remaining = Vec::new();
    let mut depends = None;
    let mut validate = None;
    let mut status = None;

    for node in children {
        let mut consumed = false;
        if let Node::Prose(ref p) = node {
            let t = p.text.trim();
            if let Some((k, v)) = t.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "depends" if !val.is_empty() => {
                        depends = Some(val.to_string());
                        consumed = true;
                    }
                    "validate" if !val.is_empty() => {
                        validate = Some(val.to_string());
                        consumed = true;
                    }
                    "status" if !val.is_empty() => {
                        status = Some(val.to_string());
                        consumed = true;
                    }
                    _ => {}
                }
            }
        }
        if !consumed {
            remaining.push(node);
        }
    }

    (remaining, depends, validate, status)
}

#[allow(clippy::type_complexity)]
fn partition_task_children(
    children: Vec<Node>,
) -> (
    Vec<Node>,
    Option<Vec<Node>>,
    Option<Vec<Node>>,
    Option<Vec<MatchArm>>,
) {
    let mut regular = Vec::new();
    let mut on_pass: Option<Vec<Node>> = None;
    let mut on_fail: Option<Vec<Node>> = None;
    let mut match_arms: Option<Vec<MatchArm>> = None;

    let mut current_section: Option<&str> = None;
    let mut pass_nodes = Vec::new();
    let mut fail_nodes = Vec::new();
    let mut arms = Vec::new();
    let mut current_arm_pattern: Option<String> = None;
    let mut current_arm_children = Vec::new();

    for node in children {
        let section_keyword = match &node {
            Node::Prose(p) if p.text == "on_pass:" || p.text == "on pass:" => Some("pass"),
            Node::Prose(p) if p.text == "on_fail:" || p.text == "on fail:" => Some("fail"),
            Node::Prose(p) if p.text == "match:" => Some("match"),
            _ => None,
        };

        if let Some(kw) = section_keyword {
            if current_section == Some("match") {
                if let Some(pat) = current_arm_pattern.take() {
                    arms.push(MatchArm {
                        pattern: pat,
                        children: std::mem::take(&mut current_arm_children),
                    });
                }
            }
            current_section = Some(kw);
            continue;
        }

        match current_section {
            Some("pass") => pass_nodes.push(node),
            Some("fail") => fail_nodes.push(node),
            Some("match") => {
                if let Node::Prose(ref p) = node {
                    if p.text.ends_with(':') && !p.text.contains(' ') {
                        if let Some(pat) = current_arm_pattern.take() {
                            arms.push(MatchArm {
                                pattern: pat,
                                children: std::mem::take(&mut current_arm_children),
                            });
                        }
                        current_arm_pattern = Some(p.text.trim_end_matches(':').to_string());
                        continue;
                    }
                }
                current_arm_children.push(node);
            }
            _ => regular.push(node),
        }
    }

    if current_section == Some("match") {
        if let Some(pat) = current_arm_pattern.take() {
            arms.push(MatchArm {
                pattern: pat,
                children: current_arm_children,
            });
        }
    }

    if !pass_nodes.is_empty() {
        on_pass = Some(pass_nodes);
    }
    if !fail_nodes.is_empty() {
        on_fail = Some(fail_nodes);
    }
    if !arms.is_empty() {
        match_arms = Some(arms);
    }

    (regular, on_pass, on_fail, match_arms)
}

fn is_batch_format(lines: &[String]) -> bool {
    lines.iter().any(|l| {
        let parts: Vec<&str> = l.splitn(3, ':').collect();
        if parts.len() < 3 {
            return false;
        }
        let id_part = parts[0].trim();
        !id_part.is_empty()
            && !id_part.contains('"')
            && !id_part.contains(' ')
            && id_part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    })
}

fn parse_batch_records(lines: &[String]) -> Vec<BatchRecord> {
    let mut records: Vec<BatchRecord> = Vec::new();

    for line in lines {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let id = parts[0].trim().to_string();
            let field = parts[1].trim().to_string();
            let val = parts[2].trim().to_string();

            if let Some(rec) = records.iter_mut().find(|r| r.id == id) {
                rec.fields.push((field, val));
            } else {
                records.push(BatchRecord {
                    id,
                    fields: vec![(field, val)],
                });
            }
        } else if let Some((k, v)) = line.split_once(':') {
            if let Some(rec) = records.last_mut() {
                rec.fields
                    .push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }

    records
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(s: &str) -> Document {
        parse(s).expect("parse failed")
    }

    fn first_node(doc: &Document) -> &Node {
        &doc.nodes[0]
    }

    // ── Group parsing ──

    #[test]
    fn parse_simple_group() {
        let doc = parse_ok("# My Group");
        match first_node(&doc) {
            Node::Group(g) => {
                assert_eq!(g.name, "My Group");
                assert_eq!(g.depth, 1);
            }
            _ => panic!("expected Group"),
        }
    }

    #[test]
    fn parse_group_with_child_task() {
        let doc = parse_ok("# Parent\n\n    [!] Child task");
        if let Node::Group(g) = first_node(&doc) {
            assert_eq!(g.name, "Parent");
            assert!(g.children.iter().any(|c| matches!(c, Node::Task(_))));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn parse_group_with_atoms() {
        let doc = parse_ok("# Project :status :assignee");
        if let Node::Group(g) = first_node(&doc) {
            assert_eq!(g.name, "Project");
            let atom_names: Vec<&str> = g.atoms.iter().map(|a| a.name.as_str()).collect();
            assert!(atom_names.contains(&"status"));
            assert!(atom_names.contains(&"assignee"));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn parse_group_with_gates() {
        let doc = parse_ok("# Releases :status {release-gate}");
        if let Node::Group(g) = first_node(&doc) {
            assert_eq!(g.gates.len(), 1);
            assert_eq!(g.gates[0].name, "release-gate");
        } else {
            panic!("expected Group");
        }
    }

    // ── Task parsing ──

    #[test]
    fn parse_required_task() {
        let doc = parse_ok("[!] Do the thing");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.kind, TaskKind::Required);
            assert_eq!(t.text, "Do the thing");
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_optional_task() {
        let doc = parse_ok("[o] Maybe do this");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.kind, TaskKind::Optional);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_completed_task() {
        let doc = parse_ok("[x] Already done");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.kind, TaskKind::Completed);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_sequential_task() {
        let doc = parse_ok("[1] First thing");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.seq, Some(1));
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_decision_task() {
        let doc = parse_ok("[?] Is this right?");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.priority, Priority::Decision);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_parallel_task() {
        let doc = parse_ok("+[!] Parallel task");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.marker.prefix, TaskPrefix::Parallel);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_labeled_task() {
        let doc = parse_ok("[A!] Named task");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.label, Some("A".to_string()));
            assert_eq!(t.marker.kind, TaskKind::Required);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_task_with_gates() {
        let doc = parse_ok("[!] Ship it {code-review} {security-scan}");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.gates.len(), 2);
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_task_with_closes() {
        let doc = parse_ok("[!] Fix the bug git:closes #42");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.closes, Some(42));
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_task_on_pass_on_fail() {
        let src = "[!] Check {review}\n    on_pass:\n        [!] Continue\n    on_fail:\n        [!] Retry";
        let doc = parse_ok(src);
        if let Node::Task(t) = first_node(&doc) {
            assert!(t.on_pass.is_some());
            assert!(t.on_fail.is_some());
            assert_eq!(t.on_pass.as_ref().unwrap().len(), 1);
            assert_eq!(t.on_fail.as_ref().unwrap().len(), 1);
        } else {
            panic!("expected Task");
        }
    }

    // ── Define parsing ──

    #[test]
    fn parse_define() {
        let src = "define:@Task\n    title: \"Untitled\"\n    state: :todo/:done";
        let doc = parse_ok(src);
        if let Node::Define(d) = first_node(&doc) {
            assert_eq!(d.entity, "Task");
            assert_eq!(d.fields.len(), 2);
            assert_eq!(d.fields[0].name, "title");
        } else {
            panic!("expected Define");
        }
    }

    #[test]
    fn parse_define_with_inheritance() {
        let src = "define:@Task from @~/canon.bit\n    custom: nil";
        let doc = parse_ok(src);
        if let Node::Define(d) = first_node(&doc) {
            assert!(d.from_scope.is_some());
        } else {
            panic!("expected Define");
        }
    }

    // ── Mutate/Delete parsing ──

    #[test]
    fn parse_mutate() {
        let src = "mutate:@Task:ship-api\n    title: \"Ship the API\"\n    state: :todo";
        let doc = parse_ok(src);
        if let Node::Mutate(m) = first_node(&doc) {
            assert_eq!(m.entity, "Task");
            assert_eq!(m.id, Some("ship-api".to_string()));
            assert_eq!(m.fields.len(), 2);
        } else {
            panic!("expected Mutate");
        }
    }

    #[test]
    fn parse_delete() {
        let doc = parse_ok("delete:@Task:old-one");
        if let Node::Delete(d) = first_node(&doc) {
            assert_eq!(d.entity, "Task");
            assert_eq!(d.id, "old-one");
        } else {
            panic!("expected Delete");
        }
    }

    // ── Query parsing ──

    #[test]
    fn parse_inline_query() {
        let doc = parse_ok("? Task(s) where state = :done");
        if let Node::Query(q) = first_node(&doc) {
            assert_eq!(q.entity, "Task");
            assert!(q.plural);
            assert!(q.filter.is_some());
        } else {
            panic!("expected Query");
        }
    }

    // ── Flow parsing ──

    #[test]
    fn parse_flow() {
        let src = "flow:\n    A --> B --> C";
        let doc = parse_ok(src);
        if let Node::Flow(f) = first_node(&doc) {
            assert!(!f.edges.is_empty());
        } else {
            panic!("expected Flow");
        }
    }

    // ── States parsing ──

    #[test]
    fn parse_states() {
        let src = "states:\n    :draft --> :review --> :approved";
        let doc = parse_ok(src);
        if let Node::States(s) = first_node(&doc) {
            assert!(!s.transitions.is_empty());
        } else {
            panic!("expected States");
        }
    }

    // ── Mod parsing ──

    #[test]
    fn parse_mod_def() {
        let src = "mod:$Summarizer\n    kind: :guide\n    description: \"Summarize\"";
        let doc = parse_ok(src);
        if let Node::ModDef(m) = first_node(&doc) {
            assert_eq!(m.name, "Summarizer");
        } else {
            panic!("expected ModDef");
        }
    }

    #[test]
    fn parse_mod_install() {
        let doc = parse_ok("mod:install $ArXiv");
        if let Node::ModInvoke(m) = first_node(&doc) {
            assert_eq!(m.method, Some("install".to_string()));
        } else {
            panic!("expected ModInvoke");
        }
    }

    // ── Validate parsing ──

    #[test]
    fn parse_validate() {
        let src = "validate code-review:\n    [!] Check correctness";
        let doc = parse_ok(src);
        if let Node::Validate(v) = first_node(&doc) {
            assert_eq!(v.name, "code-review");
            assert!(!v.children.is_empty());
        } else {
            panic!("expected Validate");
        }
    }

    #[test]
    fn parse_validate_with_id() {
        let src = "validate task-contract:\n    id: task-contract\n\n    [!] Check correctness";
        let doc = parse(src).expect("validate with id");
        if let Node::Validate(v) = &doc.nodes[0] {
            assert_eq!(v.meta.id, Some("task-contract".to_string()));
            assert!(!v
                .children
                .iter()
                .any(|n| matches!(n, Node::Prose(p) if p.text.contains("id:"))));
        } else {
            panic!("expected Validate");
        }
    }

    // ── Check parsing ──

    #[test]
    fn parse_check_block() {
        let src = "check: my-check\n    kind: query\n    entity: Task\n    expect_count: 1";
        let doc = parse(src).expect("check block should parse");
        if let Node::Check(c) = &doc.nodes[0] {
            assert_eq!(c.name, "my-check");
            assert_eq!(c.body.len(), 3);
            assert_eq!(c.body[0], ("kind".to_string(), "query".to_string()));
            assert_eq!(c.body[1], ("entity".to_string(), "Task".to_string()));
            assert_eq!(c.body[2], ("expect_count".to_string(), "1".to_string()));
        } else {
            panic!("expected Check, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn parse_check_with_id() {
        let src = "check: my-check\n    id: my-check\n    kind: query\n    entity: Task";
        let doc = parse(src).expect("check with id");
        if let Node::Check(c) = &doc.nodes[0] {
            assert_eq!(c.name, "my-check");
            assert_eq!(c.meta.id, Some("my-check".to_string()));
            assert!(!c.body.iter().any(|(k, _)| k == "id"));
        } else {
            panic!("expected Check");
        }
    }

    #[test]
    fn parse_check_with_depends_on() {
        let src = "check: second\n    id: second\n    depends_on: @Check:first\n    kind: query\n    entity: Task";
        let doc = parse(src).expect("check with depends_on");
        if let Node::Check(c) = &doc.nodes[0] {
            assert_eq!(c.meta.depends_on, vec!["@Check:first"]);
            assert!(!c.body.iter().any(|(k, _)| k == "depends_on"));
        } else {
            panic!("expected Check");
        }
    }

    #[test]
    fn parse_check_inside_validate() {
        let src = "validate task-contract:\n    check: task-count\n        kind: query\n        entity: Task\n        expect_count: 1";
        let doc = parse(src).expect("validate with check child");
        if let Node::Validate(v) = &doc.nodes[0] {
            assert_eq!(v.name, "task-contract");
            assert_eq!(v.children.len(), 1);
            if let Node::Check(c) = &v.children[0] {
                assert_eq!(c.name, "task-count");
            } else {
                panic!("expected Check child, got {:?}", v.children[0]);
            }
        } else {
            panic!("expected Validate");
        }
    }

    // ── Form parsing ──

    #[test]
    fn parse_form() {
        let src = "form:intake\n    schema_version: 2\n    ui_layout: \"wizard\"\n    ui_page: \"company\"\n    ui_page: \"goals\"\n    storage_canonical: \"gix\"\n    storage_entity: \"@Brief\"\n    storage_duckdb: \"brief_submissions\"\n    projection_entity: \"company=company,budget=budget\"\n    projection_duckdb: \"company=company,budget=budget\"\n    company: \"\"\n    budget: 0";
        let doc = parse_ok(src);
        if let Node::Form(f) = first_node(&doc) {
            assert_eq!(f.name, "intake");
            assert_eq!(f.fields.len(), 2);
            assert_eq!(f.schema_version, Some(2));
            assert_eq!(f.ui_layout.as_deref(), Some("wizard"));
            assert_eq!(f.ui_pages, vec!["company", "goals"]);
            assert_eq!(f.storage.canonical.as_deref(), Some("gix"));
            assert_eq!(f.storage.entity.as_deref(), Some("@Brief"));
            assert_eq!(f.storage.duckdb.as_deref(), Some("brief_submissions"));
            assert_eq!(f.projections.len(), 2);
        } else {
            panic!("expected Form");
        }
    }

    // ── Sync parsing (new keyword) ──

    #[test]
    fn parse_sync_keyword() {
        let src = "sync:customers\n    class: canon\n    source: @project:crm\n    identity: email";
        let doc = parse_ok(src);
        if let Node::SyncDef(s) = first_node(&doc) {
            assert_eq!(s.name, "customers");
            assert_eq!(s.class, "canon");
            assert_eq!(s.source, "@project:crm");
            assert_eq!(s.identity, "email");
        } else {
            panic!("expected SyncDef");
        }
    }

    // ── Gate def parsing ──

    #[test]
    fn parse_gate_def() {
        let src = "gate:production-ready\n    {when: |count Task where state = :blocked| = 0}";
        let doc = parse_ok(src);
        if let Node::GateDef(g) = first_node(&doc) {
            assert_eq!(g.name, "production-ready");
        } else {
            panic!("expected GateDef");
        }
    }

    // ── Conditional parsing ──

    #[test]
    fn parse_conditional() {
        let src = "if |budget > 0|:\n    [!] Proceed";
        let doc = parse_ok(src);
        if let Node::Conditional(c) = first_node(&doc) {
            assert!(!c.children.is_empty());
        } else {
            panic!("expected Conditional");
        }
    }

    // ── Variable parsing ──

    #[test]
    fn parse_variable() {
        let doc = parse_ok("target = 500000");
        if let Node::Variable(v) = first_node(&doc) {
            assert_eq!(v.name, "target");
        } else {
            panic!("expected Variable");
        }
    }

    // ── Misc nodes ──

    #[test]
    fn parse_divider() {
        let doc = parse_ok("---");
        assert!(matches!(first_node(&doc), Node::Divider));
    }

    #[test]
    fn parse_spawn_parallel() {
        let doc = parse_ok("+");
        assert!(matches!(first_node(&doc), Node::Spawn(Spawn::Parallel)));
    }

    #[test]
    fn parse_spawn_sequential() {
        let doc = parse_ok("++");
        assert!(matches!(first_node(&doc), Node::Spawn(Spawn::Sequential)));
    }

    #[test]
    fn parse_bold() {
        let doc = parse_ok("**This is important**");
        if let Node::Bold(b) = first_node(&doc) {
            assert_eq!(b.text, "This is important");
        } else {
            panic!("expected Bold");
        }
    }

    #[test]
    fn parse_comment_stripped() {
        let doc = parse_ok("[!] Do something // this is a comment");
        if let Node::Task(t) = first_node(&doc) {
            assert!(!t.text.contains("//"));
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_snap() {
        let doc = parse_ok("snap: \"before refactor\"");
        if let Node::Snap(s) = first_node(&doc) {
            assert_eq!(s.name, "before refactor");
        } else {
            panic!("expected Snap");
        }
    }

    #[test]
    fn parse_remember() {
        let doc = parse_ok("remember: \"decided REST over GraphQL\"");
        if let Node::Remember(r) = first_node(&doc) {
            assert!(r.content.contains("REST"));
        } else {
            panic!("expected Remember");
        }
    }

    #[test]
    fn parse_prose() {
        let doc = parse_ok("This is some free text");
        assert!(matches!(first_node(&doc), Node::Prose(_)));
    }

    #[test]
    fn parse_webhook() {
        let src = "webhook: on:|mutate:@Task| url: \"https://example.com\"";
        let doc = parse_ok(src);
        assert!(matches!(first_node(&doc), Node::Webhook(_)));
    }

    #[test]
    fn parse_status_def() {
        let doc = parse_ok("status: \"Working\"/\"Done\"/\"Blocked\"");
        if let Node::StatusDef(s) = first_node(&doc) {
            assert_eq!(s.options.len(), 3);
        } else {
            panic!("expected StatusDef");
        }
    }

    #[test]
    fn parse_git_sync() {
        let doc = parse_ok("git:sync");
        assert!(matches!(first_node(&doc), Node::Git(_)));
    }

    #[test]
    fn parse_embed_marker() {
        let doc = parse_ok("^customer_feedback");
        assert!(matches!(first_node(&doc), Node::EmbedMarker(_)));
    }

    #[test]
    fn parse_policy() {
        let src = "policy:\n    @./src/: {code-review} {lint}";
        let doc = parse_ok(src);
        assert!(matches!(first_node(&doc), Node::PolicyDef(_)));
    }

    // ── Comment stripping inside strings ──

    #[test]
    fn comment_inside_quoted_string_preserved() {
        let doc = parse_ok("[!] Visit url: \"https://example.com/path\"");
        if let Node::Task(t) = first_node(&doc) {
            assert!(
                t.text.contains("https://example.com/path"),
                "URL was truncated: {}",
                t.text
            );
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn comment_inside_field_value_preserved() {
        let src = "define:@Config\n    url: \"https://api.example.com/v1\"";
        let doc = parse_ok(src);
        if let Node::Define(d) = first_node(&doc) {
            let url_field = d
                .fields
                .iter()
                .find(|f| f.name == "url")
                .expect("url field missing");
            match &url_field.default {
                FieldDefault::Str(s) => assert_eq!(s, "https://api.example.com/v1"),
                other => panic!("expected Str, got {:?}", other),
            }
        } else {
            panic!("expected Define");
        }
    }

    #[test]
    fn comment_after_quoted_string_stripped() {
        let doc = parse_ok("[!] Use \"this library\" // but check license first");
        if let Node::Task(t) = first_node(&doc) {
            assert!(t.text.contains("this library"), "quoted text lost");
            assert!(
                !t.text.contains("check license"),
                "comment should be stripped"
            );
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn escaped_quote_in_string_handled() {
        let doc = parse_ok("[!] Say \"hello \\\"world\\\"\" // greeting");
        if let Node::Task(t) = first_node(&doc) {
            assert!(t.text.contains("hello"), "text lost");
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn tab_indent_treated_as_spaces() {
        let doc = parse_ok("# Parent\n\t[!] Child task");
        if let Node::Group(g) = first_node(&doc) {
            assert!(
                !g.children.is_empty(),
                "tab-indented child not parsed as child"
            );
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn mixed_tabs_and_spaces_consistent() {
        let src = "# Parent\n\t[!] Tab child\n    [o] Space child";
        let doc = parse_ok(src);
        if let Node::Group(g) = first_node(&doc) {
            let task_count = g
                .children
                .iter()
                .filter(|c| matches!(c, Node::Task(_)))
                .count();
            assert_eq!(
                task_count, 2,
                "both tab and space children should be parsed"
            );
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn ref_with_simple_path() {
        let doc = parse_ok("[!] Contact @user:alice for info");
        if let Node::Task(t) = first_node(&doc) {
            let refs: Vec<&Inline> = t
                .inline
                .iter()
                .filter(|i| matches!(i, Inline::Ref(_)))
                .collect();
            assert_eq!(refs.len(), 1);
            if let Inline::Ref(r) = refs[0] {
                assert_eq!(r.path, vec!["user", "alice"]);
            }
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn at_sign_mid_word_not_ref() {
        let doc = parse_ok("[!] Email alice@domain.com for details");
        if let Node::Task(t) = first_node(&doc) {
            let refs: Vec<&Inline> = t
                .inline
                .iter()
                .filter(|i| matches!(i, Inline::Ref(_)))
                .collect();
            // alice@domain.com — the @ is mid-word, should NOT start a ref
            assert_eq!(
                refs.len(),
                0,
                "@ mid-word should not start a ref, got {} refs",
                refs.len()
            );
        } else {
            panic!("expected Task");
        }
    }

    // ── Task 6: Spawn separator whitespace handling ──

    #[test]
    fn spawn_with_leading_whitespace_standalone() {
        // Standalone indented spawn parses correctly
        let doc = parse_ok("  +");
        assert!(
            matches!(first_node(&doc), Node::Spawn(Spawn::Parallel)),
            "indented + should parse as Spawn::Parallel"
        );
    }

    #[test]
    fn sequential_spawn_with_whitespace_standalone() {
        // Standalone indented sequential spawn parses correctly
        let doc = parse_ok("  ++");
        assert!(
            matches!(first_node(&doc), Node::Spawn(Spawn::Sequential)),
            "indented ++ should parse as Spawn::Sequential"
        );
    }

    #[test]
    fn spawn_between_groups_at_same_level() {
        // Spawn at group level (no indent) separates groups correctly
        let doc = parse_ok("# Group A\n\n+\n\n# Group B");
        let spawn_count = doc
            .nodes
            .iter()
            .filter(|n| matches!(n, Node::Spawn(_)))
            .count();
        assert_eq!(
            spawn_count, 1,
            "top-level + between groups should be a Spawn"
        );
    }

    #[test]
    fn indented_spawn_consumed_as_group_child() {
        // Indented spawn inside a group becomes a child, not a top-level node
        let doc = parse_ok("# Group A\n\n  +\n\n# Group B");
        // The indented + is consumed as a child of Group A
        let top_spawns = doc
            .nodes
            .iter()
            .filter(|n| matches!(n, Node::Spawn(_)))
            .count();
        assert_eq!(
            top_spawns, 0,
            "indented + should be consumed by Group A, not top-level"
        );
    }

    // ── Task 7: Status vs variable disambiguation ──

    #[test]
    fn status_with_slash_is_status_def() {
        let doc = parse_ok("status: \"Done\"/\"In Progress\"/\"Not Started\"");
        assert!(
            matches!(first_node(&doc), Node::StatusDef(_)),
            "slash-separated should be StatusDef"
        );
    }

    #[test]
    fn status_without_slash_is_not_status_def() {
        let doc = parse_ok("status = \"Done\"");
        assert!(
            matches!(first_node(&doc), Node::Variable(_)),
            "single value with = should be Variable"
        );
    }

    #[test]
    fn status_colon_no_slash_is_not_status_def() {
        let doc = parse_ok("status: Done");
        assert!(
            !matches!(first_node(&doc), Node::StatusDef(_)),
            "no slash means not a StatusDef"
        );
    }

    // ── Task 4: Scientific notation in numeric parsing ──

    #[test]
    fn field_default_scientific_notation_integer_style() {
        let d = parse_field_default("1e10");
        assert!(
            matches!(d, FieldDefault::Float(_)),
            "1e10 should parse as Float, got {:?}",
            d
        );
        if let FieldDefault::Float(f) = d {
            assert!((f - 1e10).abs() < 1.0, "1e10 value wrong: {}", f);
        }
    }

    #[test]
    fn field_default_scientific_notation_negative_exponent() {
        let d = parse_field_default("1.5e-3");
        assert!(
            matches!(d, FieldDefault::Float(_)),
            "1.5e-3 should parse as Float, got {:?}",
            d
        );
        if let FieldDefault::Float(f) = d {
            assert!((f - 1.5e-3).abs() < 1e-10, "1.5e-3 value wrong: {}", f);
        }
    }

    #[test]
    fn field_default_scientific_notation_positive_exponent() {
        let d = parse_field_default("2.5E+6");
        if let FieldDefault::Float(f) = d {
            assert!((f - 2.5e6).abs() < 1.0);
        } else {
            panic!("2.5E+6 should parse as Float, got {:?}", d);
        }
    }

    // ── Task 5: Nested brace matching in gates ──

    #[test]
    fn nested_gates_compound() {
        let gates = extract_gates("{all: {lint} {tests}}");
        assert_eq!(
            gates.len(),
            1,
            "compound gate should be one gate, got {}",
            gates.len()
        );
        assert_eq!(gates[0].name, "all:");
        let body = gates[0].body.as_ref().expect("should have body");
        assert!(
            body.contains("{lint}"),
            "body should contain nested {{lint}}, got: {}",
            body
        );
        assert!(
            body.contains("{tests}"),
            "body should contain nested {{tests}}, got: {}",
            body
        );
    }

    #[test]
    fn nested_gates_deeply_nested() {
        let gates = extract_gates("{outer: {mid: {inner}}}");
        assert_eq!(gates.len(), 1, "deeply nested should be one gate");
        let body = gates[0].body.as_ref().unwrap();
        assert!(body.contains("{mid:"), "body should contain mid gate");
    }

    #[test]
    fn sequential_gates_not_nested() {
        let gates = extract_gates("{lint} {tests} {security}");
        assert_eq!(
            gates.len(),
            3,
            "three sequential gates expected, got {}",
            gates.len()
        );
    }

    // ── Task 8: Code blocks ──

    #[test]
    fn code_block_with_language() {
        let src = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let doc = parse_ok(src);
        if let Node::CodeBlock(cb) = first_node(&doc) {
            assert_eq!(cb.lang, Some("rust".to_string()));
            assert!(cb.content.contains("fn main()"));
            assert!(cb.content.contains("println!"));
        } else {
            panic!("expected CodeBlock, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn code_block_without_language() {
        let src = "```\nsome raw text\n```";
        let doc = parse_ok(src);
        if let Node::CodeBlock(cb) = first_node(&doc) {
            assert_eq!(cb.lang, None);
            assert_eq!(cb.content, "some raw text");
        } else {
            panic!("expected CodeBlock, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn code_block_content_not_parsed() {
        let src = "```\n[!] This is NOT a task\n# Not a group\ndefine:@NotReal\n```";
        let doc = parse_ok(src);
        assert_eq!(doc.nodes.len(), 1, "code block should be single node");
        if let Node::CodeBlock(cb) = first_node(&doc) {
            assert!(cb.content.contains("[!] This is NOT a task"));
            assert!(cb.content.contains("# Not a group"));
        } else {
            panic!("expected CodeBlock");
        }
    }

    // ── Task 16: Triple-quote strings ──

    #[test]
    fn triple_quote_string_in_mod_def() {
        let src = "mod:$Summarizer\n    kind: :guide\n    prompt: \"\"\"\n    You are a summarizer.\n    Be concise.\n    \"\"\"";
        let doc = parse_ok(src);
        if let Node::ModDef(m) = first_node(&doc) {
            let prompt = m.body.iter().find(|(k, _)| k == "prompt");
            assert!(prompt.is_some(), "should have prompt field");
            let val = &prompt.unwrap().1;
            assert!(
                val.contains("You are a summarizer"),
                "prompt content missing: {}",
                val
            );
            assert!(
                val.contains("Be concise"),
                "prompt second line missing: {}",
                val
            );
        } else {
            panic!("expected ModDef");
        }
    }

    #[test]
    fn triple_quote_empty() {
        let src = "mod:$Empty\n    prompt: \"\"\"\n    \"\"\"";
        let doc = parse_ok(src);
        if let Node::ModDef(m) = first_node(&doc) {
            let prompt = m.body.iter().find(|(k, _)| k == "prompt");
            assert!(prompt.is_some());
            assert_eq!(prompt.unwrap().1, "");
        } else {
            panic!("expected ModDef");
        }
    }

    // ── Pressure-field fields ──

    #[test]
    fn project_pressure_field_fields() {
        let src = "\
project: Alpha
    brief: Build the thing
    heartbeat: 5m
    fitness: 0.85
    pressure: 1.2
    phase: active
    inhibited_until: 2026-04-01
    completion: 75%
    kpi: deploy-frequency
    routine: daily-standup";
        let doc = parse_ok(src);
        if let Node::Project(p) = first_node(&doc) {
            assert_eq!(p.name, "Alpha");
            assert_eq!(p.brief, "Build the thing");
            assert_eq!(p.heartbeat, Some("5m".to_string()));
            assert!((p.fitness.unwrap() - 0.85).abs() < f64::EPSILON);
            assert!((p.pressure.unwrap() - 1.2).abs() < f64::EPSILON);
            assert_eq!(p.phase, Some("active".to_string()));
            assert_eq!(p.inhibited_until, Some("2026-04-01".to_string()));
            assert_eq!(p.completion, Some("75%".to_string()));
            assert_eq!(p.kpi, Some("deploy-frequency".to_string()));
            assert_eq!(p.routine, Some("daily-standup".to_string()));
        } else {
            panic!("expected ProjectDef, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn project_without_pressure_fields_still_parses() {
        let src = "\
project: Legacy
    brief: Old project
    status: active";
        let doc = parse_ok(src);
        if let Node::Project(p) = first_node(&doc) {
            assert_eq!(p.name, "Legacy");
            assert_eq!(p.fitness, None);
            assert_eq!(p.pressure, None);
            assert_eq!(p.phase, None);
            assert_eq!(p.inhibited_until, None);
            assert_eq!(p.completion, None);
            assert_eq!(p.kpi, None);
            assert_eq!(p.routine, None);
        } else {
            panic!("expected ProjectDef");
        }
    }

    #[test]
    fn task_pressure_field_fields() {
        let src = "\
[!] Implement feature
    depends: design-phase
    validate: tests-pass
    status: in-progress";
        let doc = parse_ok(src);
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.text, "Implement feature");
            assert_eq!(t.depends, Some("design-phase".to_string()));
            assert_eq!(t.validate, Some("tests-pass".to_string()));
            assert_eq!(t.status, Some("in-progress".to_string()));
        } else {
            panic!("expected Task, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn task_without_pressure_fields_still_parses() {
        let doc = parse_ok("[!] Simple task");
        if let Node::Task(t) = first_node(&doc) {
            assert_eq!(t.text, "Simple task");
            assert_eq!(t.depends, None);
            assert_eq!(t.validate, None);
            assert_eq!(t.status, None);
        } else {
            panic!("expected Task");
        }
    }

    // ── Lattice construct parsing ──

    #[test]
    fn parse_lattice_validates_colon() {
        let doc = parse_ok(
            "lattice_validates:schema_check\n    artifact: contract\n    schema: contract-v1\n",
        );
        assert!(
            matches!(&doc.nodes[0], Node::LatticeValidates(_)),
            "Expected LatticeValidates, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn parse_lattice_constraint_colon() {
        let doc = parse_ok(
            "lattice_constraint:budget_cap\n    rule: max 100000\n    applies_to: project\n",
        );
        assert!(
            matches!(&doc.nodes[0], Node::LatticeConstraint(_)),
            "Expected LatticeConstraint, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn parse_lattice_schema_colon() {
        let doc = parse_ok(
            "lattice_schema:workflow_v2\n    name: title\n    type: string\n    required: true\n",
        );
        assert!(
            matches!(&doc.nodes[0], Node::LatticeSchema(_)),
            "Expected LatticeSchema, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn parse_lattice_frontier_colon() {
        let doc = parse_ok("lattice_frontier:exploration\n    expected_schema: contract-v1\n");
        assert!(
            matches!(&doc.nodes[0], Node::LatticeFrontier(_)),
            "Expected LatticeFrontier, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn parse_pressure_effect_colon() {
        let doc = parse_ok("pressure_effect:deadline\n    dynamic: decay\n    target: upstream\n");
        match &doc.nodes[0] {
            Node::PressureEffect(pe) => {
                assert_eq!(pe.dynamic, "decay");
                assert_eq!(pe.target.as_deref(), Some("upstream"));
            }
            other => panic!("Expected PressureEffect, got {:?}", other),
        }
    }

    #[test]
    fn parse_unit_cell_colon() {
        let doc = parse_ok("unit_cell:sprint\n    duration: 14d\n    capacity: 40\n");
        assert!(
            matches!(&doc.nodes[0], Node::UnitCell(_)),
            "Expected UnitCell, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn parse_symmetry_colon() {
        let doc = parse_ok("symmetry:role_parity\n    repeat: indefinite\n    measure: effort\n");
        assert!(
            matches!(&doc.nodes[0], Node::Symmetry(_)),
            "Expected Symmetry, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn canonicalize_validates_section_header() {
        let doc = parse_ok("## Validates\n    artifact: contract\n");
        assert!(
            matches!(&doc.nodes[0], Node::LatticeValidates(_)),
            "## Validates should canonicalize to LatticeValidates, got {:?}",
            &doc.nodes[0]
        );
    }

    #[test]
    fn canonicalize_constraint_section_header() {
        let doc = parse_ok("## Constraint\n    rule: Narrative only\n");
        assert!(
            matches!(&doc.nodes[0], Node::LatticeConstraint(_)),
            "## Constraint should canonicalize to LatticeConstraint, got {:?}",
            &doc.nodes[0]
        );
    }
}

#[cfg(test)]
mod corpus_tests {
    use super::*;
    use crate::render::render;

    /// Helper: parse, render, re-parse — second render must match first
    fn assert_round_trip_stable(src: &str, label: &str) {
        let doc1 = parse(src).unwrap_or_else(|e| panic!("{}: parse failed: {}", label, e));
        let r1 = render(&doc1);
        let doc2 = parse(&r1).unwrap_or_else(|e| panic!("{}: re-parse failed: {}", label, e));
        let r2 = render(&doc2);
        assert_eq!(r1, r2, "{}: round-trip unstable", label);
    }

    #[test]
    fn corpus_mod_manifest() {
        let src = "\
# Linear [ACTIVE]
  id: linear
  kind: TOOL
  version: 0.2.0
  summary: Manage Linear issues, projects, cycles, labels, and users
  scope: agent
  trigger: intent=use_linear

## Tags
  integration, project-management

## Sources
  repo: https://github.com/linear/linear
  docs: https://linear.app/developers

## Runtime
  type: node
  entry: tools/index.js";
        let doc = parse(src).expect("mod manifest should parse");
        let groups: Vec<&Group> = doc
            .nodes
            .iter()
            .filter_map(|n| {
                if let Node::Group(g) = n {
                    Some(g)
                } else {
                    None
                }
            })
            .collect();
        assert!(!groups.is_empty(), "should have at least one group");
        assert!(
            groups[0].name.contains("Linear"),
            "first group should be Linear"
        );
    }

    #[test]
    fn corpus_define_mutate_query() {
        let src = "\
define:@Task
    title: \"Untitled\"
    state: :todo/:in_progress/:done
    priority: 0
    tags: []

mutate:@Task:ship-api
    title: \"Ship the API\"
    state: :todo

? Task(s) where state = :done";
        let doc = parse(src).expect("define/mutate/query should parse");
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Define(_))));
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Mutate(_))));
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Query(_))));
    }

    #[test]
    fn corpus_flow_and_states() {
        let src = "\
flow:
    design --> implement --> test --> deploy

states:
    :draft --> :review --> :approved --> :published";
        let doc = parse(src).expect("flow/states should parse");
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Flow(_))));
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::States(_))));
    }

    #[test]
    fn corpus_task_hierarchy_with_gates() {
        let src = "\
# Release v2.0

    [!] Ship API {code-review} {security-scan}
        on_pass:
            [!] Deploy to production
        on_fail:
            [!] Fix security issues

    [o] Write blog post
    [x] Update changelog";
        let doc = parse(src).expect("task hierarchy should parse");
        if let Node::Group(g) = &doc.nodes[0] {
            assert_eq!(g.name, "Release v2.0");
            let tasks: Vec<&Task> = g
                .children
                .iter()
                .filter_map(|n| if let Node::Task(t) = n { Some(t) } else { None })
                .collect();
            assert!(tasks.len() >= 3, "should have 3+ tasks");
            assert!(tasks[0].on_pass.is_some(), "first task should have on_pass");
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn corpus_code_block_in_context() {
        let src = "\
# Setup Guide

    [!] Configure environment

    ```bash
    export API_KEY=xyz
    echo \"done\"
    ```

    [!] Run tests";
        let doc = parse(src).expect("code block in context should parse");
        let has_code = doc.nodes.iter().any(|n| {
            if let Node::Group(g) = n {
                g.children.iter().any(|c| matches!(c, Node::CodeBlock(_)))
            } else {
                matches!(n, Node::CodeBlock(_))
            }
        });
        assert!(has_code, "should contain a code block");
    }

    #[test]
    fn corpus_mixed_document_round_trip() {
        let src = "\
# Project Alpha

    [!] Design API
    [o] Write docs

---

# Project Beta

    target = 500000
    [x] Launch MVP";
        assert_round_trip_stable(src, "mixed document");
    }

    #[test]
    fn corpus_validate_and_form() {
        let src = "\
validate code-review:
    [!] Check correctness
    [!] Check style

form:intake
    schema_version: 2
    ui_layout: \"wizard\"
    ui_page: \"company\"
    company: \"\"
    budget: 0";
        let doc = parse(src).expect("validate/form should parse");
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Validate(_))));
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Form(_))));
    }

    #[test]
    fn corpus_conditional_and_webhook() {
        let src = "\
if |budget > 0|:
    [!] Proceed with purchase

webhook: on:|mutate:@Task| url: \"https://example.com/hook\"";
        let doc = parse(src).expect("conditional/webhook should parse");
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Conditional(_))));
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Webhook(_))));
    }
}

#[cfg(test)]
mod real_world_stress_tests {
    use super::*;

    #[test]
    fn parse_all_mod_files() {
        let mods_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("mods");

        if !mods_dir.exists() {
            eprintln!(
                "Skipping real-world test: mods dir not found at {:?}",
                mods_dir
            );
            return;
        }

        let mut count = 0;
        let mut errors = Vec::new();

        for entry in walkdir(mods_dir.to_str().unwrap()) {
            if entry.ends_with(".bit") {
                match std::fs::read_to_string(&entry) {
                    Ok(content) => {
                        if let Err(e) = parse(&content) {
                            errors.push(format!("{}: {}", entry, e));
                        }
                        count += 1;
                    }
                    Err(e) => {
                        errors.push(format!("{}: read error: {}", entry, e));
                    }
                }
            }
        }

        assert!(count > 0, "Should find .bit files in mods directory");
        assert!(
            errors.is_empty(),
            "Parse errors in {} of {} files:\n{}",
            errors.len(),
            count,
            errors.join("\n")
        );
        eprintln!("Successfully parsed {} real .bit files", count);
    }

    /// Simple recursive directory walker
    fn walkdir(dir: &str) -> Vec<String> {
        let mut result = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(walkdir(path.to_str().unwrap()));
                } else if let Some(s) = path.to_str() {
                    result.push(s.to_string());
                }
            }
        }
        result
    }

    #[test]
    fn parse_bit_syntax_doc() {
        let syntax_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("bit-syntax.md");

        if !syntax_path.exists() {
            eprintln!("Skipping: bit-syntax.md not found");
            return;
        }

        // Extract code blocks that look like .bit from the markdown
        let content = std::fs::read_to_string(&syntax_path).unwrap();
        let mut in_bit_block = false;
        let mut block = String::new();
        let mut blocks_parsed = 0;

        for line in content.lines() {
            if line.trim() == "```" || line.trim().starts_with("```bit") {
                if in_bit_block {
                    // End of block — try to parse it
                    let _ = parse(&block);
                    blocks_parsed += 1;
                    block.clear();
                }
                in_bit_block = !in_bit_block;
                continue;
            }
            if in_bit_block {
                block.push_str(line);
                block.push('\n');
            }
        }

        // bit-syntax.md IS valid .bit (comments as //), try parsing the whole thing
        let _ = parse(&content);
        eprintln!(
            "Parsed bit-syntax.md ({} code blocks extracted)",
            blocks_parsed
        );
    }
}

#[cfg(test)]
mod scoping_tests {
    use super::*;

    #[test]
    fn parse_mod_scoped_define() {
        let src = "define:$GoogleWorkspace.@Spreadsheet\n    title: \"\"";
        let doc = parse(src).expect("mod-scoped define should parse");
        if let Node::Define(d) = &doc.nodes[0] {
            assert_eq!(d.entity, "Spreadsheet");
            assert_eq!(d.mod_scope.as_deref(), Some("GoogleWorkspace"));
            assert!(d.workspace_scope.is_none());
        } else {
            panic!("expected Define node");
        }
    }

    #[test]
    fn parse_workspace_scoped_mutate() {
        let src = "mutate:@workspace:sales-crm.@Client:acme\n    tier: \"enterprise\"";
        let doc = parse(src).expect("workspace-scoped mutate should parse");
        if let Node::Mutate(m) = &doc.nodes[0] {
            assert_eq!(m.entity, "Client");
            assert_eq!(m.id.as_deref(), Some("acme"));
            assert_eq!(m.workspace_scope.as_deref(), Some("sales-crm"));
            assert!(m.mod_scope.is_none());
        } else {
            panic!("expected Mutate node");
        }
    }

    #[test]
    fn parse_mod_scoped_delete() {
        let src = "delete:$Excel.@Spreadsheet:old-report";
        let doc = parse(src).expect("mod-scoped delete should parse");
        if let Node::Delete(d) = &doc.nodes[0] {
            assert_eq!(d.entity, "Spreadsheet");
            assert_eq!(d.id, "old-report");
            assert_eq!(d.mod_scope.as_deref(), Some("Excel"));
        } else {
            panic!("expected Delete node");
        }
    }

    #[test]
    fn parse_temporal_query() {
        let src = "? @Client from snap:v2.1.0 where tier = \"premium\"";
        let doc = parse(src).expect("temporal query should parse");
        if let Node::Query(q) = &doc.nodes[0] {
            assert_eq!(q.from_snapshot.as_deref(), Some("v2.1.0"));
            assert!(q.filter.is_some());
        } else {
            panic!("expected Query node");
        }
    }

    #[test]
    fn parse_temporal_diff() {
        let src = "diff:@Client:acme-corp from snap:v2.1.0";
        let doc = parse(src).expect("temporal diff should parse");
        if let Node::Diff(d) = &doc.nodes[0] {
            assert_eq!(d.target, "@Client:acme-corp");
            assert_eq!(d.from_snapshot.as_deref(), Some("v2.1.0"));
        } else {
            panic!("expected Diff node");
        }
    }

    #[test]
    fn parse_use_entity_from_mod() {
        let src = "use @Spreadsheet from $GoogleWorkspace";
        let doc = parse(src).expect("use entity from mod should parse");
        if let Node::UseBlock(u) = &doc.nodes[0] {
            assert_eq!(u.entity.as_deref(), Some("Spreadsheet"));
            assert_eq!(u.from_mod.as_deref(), Some("GoogleWorkspace"));
            assert!(u.alias.is_none());
        } else {
            panic!("expected UseBlock node");
        }
    }

    #[test]
    fn parse_use_entity_from_mod_with_alias() {
        let src = "use @Spreadsheet from $Excel as @ExcelSheet";
        let doc = parse(src).expect("use with alias should parse");
        if let Node::UseBlock(u) = &doc.nodes[0] {
            assert_eq!(u.entity.as_deref(), Some("Spreadsheet"));
            assert_eq!(u.from_mod.as_deref(), Some("Excel"));
            assert_eq!(u.alias.as_deref(), Some("ExcelSheet"));
        } else {
            panic!("expected UseBlock node");
        }
    }

    #[test]
    fn parse_use_entity_from_workspace() {
        let src = "use @Client from @workspace:sales-crm as @CRMClient";
        let doc = parse(src).expect("use from workspace with alias should parse");
        if let Node::UseBlock(u) = &doc.nodes[0] {
            assert_eq!(u.entity.as_deref(), Some("Client"));
            assert_eq!(u.from_workspace.as_deref(), Some("sales-crm"));
            assert_eq!(u.alias.as_deref(), Some("CRMClient"));
        } else {
            panic!("expected UseBlock node");
        }
    }

    #[test]
    fn parse_inline_mod_scoped_ref() {
        let src = "[ ] Update $GoogleWorkspace.@Spreadsheet:budget";
        let doc = parse(src).expect("inline mod-scoped ref should parse");
        if let Node::Task(t) = &doc.nodes[0] {
            let has_mod_ref = t.inline.iter().any(|span| {
                if let Inline::Ref(r) = span {
                    r.mod_scope.as_deref() == Some("GoogleWorkspace")
                        && r.path.first().map(|s| s.as_str()) == Some("Spreadsheet")
                } else {
                    false
                }
            });
            assert!(has_mod_ref, "should contain mod-scoped ref");
        } else {
            panic!("expected Task node");
        }
    }

    #[test]
    fn parse_inline_workspace_scoped_ref() {
        let src = "[ ] Check @workspace:sales-crm.@Client:acme status";
        let doc = parse(src).expect("inline workspace-scoped ref should parse");
        if let Node::Task(t) = &doc.nodes[0] {
            let has_ws_ref = t.inline.iter().any(|span| {
                if let Inline::Ref(r) = span {
                    r.workspace_scope.as_deref() == Some("sales-crm")
                        && r.path.first().map(|s| s.as_str()) == Some("Client")
                } else {
                    false
                }
            });
            assert!(has_ws_ref, "should contain workspace-scoped ref");
        } else {
            panic!("expected Task node");
        }
    }

    #[test]
    fn local_entity_unchanged() {
        let src = "define:@Task\n    title: \"\"";
        let doc = parse(src).expect("local define should still work");
        if let Node::Define(d) = &doc.nodes[0] {
            assert_eq!(d.entity, "Task");
            assert!(d.mod_scope.is_none());
            assert!(d.workspace_scope.is_none());
        } else {
            panic!("expected Define node");
        }
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;

    /// Parser must not panic on any input — return Ok or Err, never crash
    fn must_not_panic(input: &str) {
        let _ = parse(input);
    }

    #[test]
    fn empty_input() {
        must_not_panic("");
    }

    #[test]
    fn only_whitespace() {
        must_not_panic("   \n\n  \t  \n");
    }

    #[test]
    fn only_newlines() {
        must_not_panic("\n\n\n\n\n\n\n\n\n\n");
    }

    #[test]
    fn deeply_nested_groups() {
        let src = (1..=50)
            .map(|i| format!("{} Group{}", "#".repeat(i), i))
            .collect::<Vec<_>>()
            .join("\n");
        must_not_panic(&src);
    }

    #[test]
    fn unclosed_code_block() {
        must_not_panic("```rust\nfn main() {\n// never closed");
    }

    #[test]
    fn unclosed_braces() {
        must_not_panic("[!] Task {gate_that_never_closes");
        must_not_panic("{{{{{");
        must_not_panic("[!] Task }}}}}");
    }

    #[test]
    fn unclosed_quotes() {
        must_not_panic("[!] Task with \"unclosed quote");
        must_not_panic("title: \"never ends");
    }

    #[test]
    fn null_bytes() {
        must_not_panic("# Group\0with null\n[!] Task\0too");
    }

    #[test]
    fn unicode_extremes() {
        must_not_panic("# 🎉 Unicode Group 日本語\n[!] Задача с кириллицей\n[o] عربي");
        must_not_panic("# \u{FEFF}BOM at start");
        must_not_panic("# \u{200B}Zero-width space");
    }

    #[test]
    fn very_long_line() {
        let long = "a".repeat(100_000);
        must_not_panic(&format!("[!] {}", long));
    }

    #[test]
    #[ignore] // Stack overflow with 100 nested levels — needs iterative indent parser
    fn very_deep_indentation() {
        let src = (0..100)
            .map(|i| format!("{}{}", "    ".repeat(i), "[!] Task"))
            .collect::<Vec<_>>()
            .join("\n");
        must_not_panic(&src);
    }

    #[test]
    fn all_special_chars_in_task() {
        must_not_panic("[!] @#$%^&*(){}[]|\\<>?/~`!+=-_");
    }

    #[test]
    fn repeated_keywords() {
        must_not_panic("define:define:define:@Task\ndefine:@define:@define:");
        must_not_panic("mutate:mutate:\ndelete:delete:");
    }

    #[test]
    fn pipe_without_close() {
        must_not_panic("[!] Compute |never closed");
        must_not_panic("[!] Double ||also not closed");
    }

    #[test]
    fn malformed_markers() {
        must_not_panic("[");
        must_not_panic("[]");
        must_not_panic("[!!");
        must_not_panic("[!!!!!]");
        must_not_panic("[999999]");
    }

    #[test]
    fn line_with_only_special_chars() {
        must_not_panic("@");
        must_not_panic("$");
        must_not_panic("#");
        must_not_panic("{");
        must_not_panic("}");
        must_not_panic("|");
        must_not_panic("```");
    }

    #[test]
    fn crlf_line_endings() {
        must_not_panic("# Group\r\n[!] Task\r\n[x] Done\r\n");
    }

    #[test]
    fn mixed_line_endings() {
        must_not_panic("# A\n[!] B\r\n[o] C\r[x] D");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::render::render;
    use proptest::prelude::*;

    /// Generate valid .bit group lines
    fn arb_group() -> impl Strategy<Value = String> {
        ("[A-Za-z][A-Za-z0-9 ]{0,20}", 1..4u8).prop_map(|(name, depth)| {
            let hashes = "#".repeat(depth as usize);
            format!("{} {}", hashes, name.trim())
        })
    }

    /// Generate valid .bit task lines
    fn arb_task() -> impl Strategy<Value = String> {
        (
            "[A-Za-z][A-Za-z0-9 ]{0,30}",
            prop_oneof!["[!]", "[o]", "[x]", "[ ]"],
        )
            .prop_map(|(text, marker)| format!("{} {}", marker, text.trim()))
    }

    /// Generate valid variable lines
    fn arb_variable() -> impl Strategy<Value = String> {
        ("[a-z][a-z_]{0,10}", "[0-9]{1,5}")
            .prop_map(|(name, val)| format!("{} = {}", name.trim(), val.trim()))
    }

    /// Generate a valid .bit document from a mix of node types
    fn arb_document() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof![arb_group(), arb_task(), arb_variable(),], 1..8)
            .prop_map(|lines| lines.join("\n"))
    }

    proptest! {
        #[test]
        fn parse_does_not_panic(s in ".*") {
            // Parser should never panic on arbitrary input
            let _ = parse(&s);
        }

        #[test]
        fn round_trip_stable(src in arb_document()) {
            // parse → render → parse should produce same structure
            if let Ok(doc1) = parse(&src) {
                let rendered = render(&doc1);
                if let Ok(doc2) = parse(&rendered) {
                    let rendered2 = render(&doc2);
                    prop_assert_eq!(&rendered, &rendered2,
                        "Round-trip not stable");
                }
            }
        }

        #[test]
        fn groups_preserve_name(name in "[A-Za-z][A-Za-z0-9]{0,15}") {
            let src = format!("# {}", name);
            let doc = parse(&src).unwrap();
            if let Node::Group(g) = &doc.nodes[0] {
                prop_assert_eq!(&g.name, &name);
            }
        }

        #[test]
        fn tasks_preserve_text(text in "[A-Za-z][A-Za-z0-9 ]{0,25}") {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() { return Ok(()); }
            let src = format!("[!] {}", trimmed);
            let doc = parse(&src).unwrap();
            if let Node::Task(t) = &doc.nodes[0] {
                prop_assert_eq!(&t.text, &trimmed);
            }
        }
    }
}

#[cfg(test)]
mod entity_metric_tests {
    use super::*;

    fn parse_ok(s: &str) -> Document {
        parse(s).expect("parse failed")
    }

    fn first_node(doc: &Document) -> &Node {
        &doc.nodes[0]
    }

    #[test]
    fn parse_entity_def() {
        let src = "## Entity: Charge\n  source: stripe.charges\n  namespace: stripe\n  identity: id\n  fields:\n    - amount: integer\n    - currency: string\n    - status: enum(succeeded, pending, failed)";
        let doc = parse_ok(src);
        if let Node::EntityDef(e) = first_node(&doc) {
            assert_eq!(e.name, "Charge");
            assert_eq!(e.source, "stripe.charges");
            assert_eq!(e.namespace, "stripe");
            assert_eq!(e.identity, "id");
            assert_eq!(e.fields.len(), 3);
            assert_eq!(e.fields[0].name, "amount");
            assert_eq!(e.fields[0].field_type, "integer");
            assert_eq!(e.fields[1].name, "currency");
            assert_eq!(e.fields[1].field_type, "string");
            assert_eq!(e.fields[2].name, "status");
            assert_eq!(e.fields[2].field_type, "enum(succeeded, pending, failed)");
        } else {
            panic!("expected EntityDef, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn parse_metric_def() {
        let src = "## Metric: MRR\n  source: stripe\n  grain: monthly\n  dimensions: [plan, currency]\n  formula: |\n    SELECT SUM(amount) / 100.0 AS mrr\n    FROM stripe.subscriptions\n    WHERE status = 'active'";
        let doc = parse_ok(src);
        if let Node::MetricDef(m) = first_node(&doc) {
            assert_eq!(m.name, "MRR");
            assert_eq!(m.source, Some("stripe".to_string()));
            assert_eq!(m.grain, Some("monthly".to_string()));
            assert_eq!(m.dimensions, vec!["plan", "currency"]);
            assert!(m.formula.contains("SELECT SUM(amount)"));
            assert!(m.formula.contains("WHERE status"));
            assert!(!m.cross_source);
        } else {
            panic!("expected MetricDef, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn parse_cross_source_metric() {
        let src = "## Metric: CAC\n  cross_source: true\n  grain: monthly\n  formula: |\n    SELECT m.month, m.total_spend / NULLIF(s.new_customers, 0) AS cac\n    FROM ($Meta.monthly_spend) m\n    JOIN ($Stripe.new_customers_by_month) s ON m.month = s.month";
        let doc = parse_ok(src);
        if let Node::MetricDef(m) = first_node(&doc) {
            assert_eq!(m.name, "CAC");
            assert!(m.cross_source);
            assert!(m.source.is_none());
            assert_eq!(m.grain, Some("monthly".to_string()));
            assert!(m.formula.contains("NULLIF"));
            assert!(m.formula.contains("$Meta.monthly_spend"));
        } else {
            panic!("expected MetricDef, got {:?}", first_node(&doc));
        }
    }

    #[test]
    fn parse_entity_with_five_fields() {
        let src = "## Entity: Charge\n  source: stripe.charges\n  namespace: stripe\n  identity: id\n  fields:\n    - amount: integer\n    - currency: string\n    - status: enum(succeeded, pending, failed)\n    - customer_id: string\n    - created_at: timestamp";
        let doc = parse_ok(src);
        if let Node::EntityDef(e) = first_node(&doc) {
            assert_eq!(e.fields.len(), 5);
            assert_eq!(e.fields[3].name, "customer_id");
            assert_eq!(e.fields[4].name, "created_at");
            assert_eq!(e.fields[4].field_type, "timestamp");
        } else {
            panic!("expected EntityDef");
        }
    }

    #[test]
    fn parse_metric_inline_formula() {
        let src = "## Metric: Simple\n  source: test\n  formula: SELECT COUNT(*) FROM users";
        let doc = parse_ok(src);
        if let Node::MetricDef(m) = first_node(&doc) {
            assert_eq!(m.name, "Simple");
            assert_eq!(m.formula, "SELECT COUNT(*) FROM users");
        } else {
            panic!("expected MetricDef");
        }
    }

    // ── Issue parsing ──

    #[test]
    fn parse_basic_issue() {
        let doc = parse_ok("issue: Fix auth bug");
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Issue(issue) = first_node(&doc) {
            assert_eq!(issue.title, "Fix auth bug");
            assert!(issue.id.is_none());
            assert!(issue.children.is_empty());
        } else {
            panic!("Expected Issue node");
        }
    }

    #[test]
    fn parse_issue_with_all_fields() {
        let src = "issue: Fix auth token expiry\n    id: ISS-42\n    on: @Entity:AuthService\n    status: :open\n    priority: :high\n    assignee: @User:alice\n    labels: [:bug, :security]\n    estimate: 3\n    milestone: \"v2.1\"\n    due_date: |2026-03-15|\n    description: \"Tokens expire mid-request\"";
        let doc = parse_ok(src);
        if let Node::Issue(issue) = first_node(&doc) {
            assert_eq!(issue.title, "Fix auth token expiry");
            assert_eq!(issue.id.as_deref(), Some("ISS-42"));
            assert_eq!(issue.on.as_deref(), Some("@Entity:AuthService"));
            assert_eq!(issue.status.as_deref(), Some("open"));
            assert_eq!(issue.priority.as_deref(), Some("high"));
            assert_eq!(issue.assignee.as_deref(), Some("@User:alice"));
            assert_eq!(issue.labels, vec!["bug", "security"]);
            assert_eq!(issue.estimate, Some(3.0));
            assert_eq!(issue.milestone.as_deref(), Some("v2.1"));
            assert_eq!(issue.due_date.as_deref(), Some("2026-03-15"));
            assert_eq!(
                issue.description.as_deref(),
                Some("Tokens expire mid-request")
            );
        } else {
            panic!("Expected Issue node");
        }
    }

    #[test]
    fn parse_issue_with_multiline_description() {
        let src = "issue: Fix auth\n    description: |\n        Line one\n        Line two";
        let doc = parse_ok(src);
        if let Node::Issue(issue) = first_node(&doc) {
            assert_eq!(issue.description.as_deref(), Some("Line one\nLine two"));
        } else {
            panic!("Expected Issue node");
        }
    }

    #[test]
    fn parse_issue_with_nested_comment() {
        let src = "issue: Fix auth\n    status: :open\n    comment:\n        author: @User:bob\n        body: \"Reproduced on staging\"";
        let doc = parse_ok(src);
        if let Node::Issue(issue) = first_node(&doc) {
            assert_eq!(issue.status.as_deref(), Some("open"));
            assert_eq!(issue.children.len(), 1);
            if let Node::ThreadComment(tc) = &issue.children[0] {
                assert_eq!(tc.author.as_deref(), Some("@User:bob"));
                assert_eq!(tc.body, "Reproduced on staging");
            } else {
                panic!("Expected ThreadComment child");
            }
        } else {
            panic!("Expected Issue node");
        }
    }

    // ── ThreadComment parsing ──

    #[test]
    fn parse_basic_thread_comment() {
        let src = "comment:\n    body: \"Hello world\"";
        let doc = parse_ok(src);
        if let Node::ThreadComment(tc) = first_node(&doc) {
            assert_eq!(tc.body, "Hello world");
            assert!(tc.on.is_none());
        } else {
            panic!("Expected ThreadComment node");
        }
    }

    #[test]
    fn parse_thread_comment_with_all_fields() {
        let src = "comment:\n    on: @Task:fix-auth\n    author: @User:alice\n    body: \"Great work\"\n    reactions: [:thumbsup, :heart]\n    created_at: |2026-03-08|";
        let doc = parse_ok(src);
        if let Node::ThreadComment(tc) = first_node(&doc) {
            assert_eq!(tc.on.as_deref(), Some("@Task:fix-auth"));
            assert_eq!(tc.author.as_deref(), Some("@User:alice"));
            assert_eq!(tc.body, "Great work");
            assert_eq!(tc.reactions, vec!["thumbsup", "heart"]);
            assert_eq!(tc.created_at.as_deref(), Some("2026-03-08"));
        } else {
            panic!("Expected ThreadComment node");
        }
    }

    #[test]
    fn parse_nested_comment_replies() {
        let src = "comment:\n    author: @User:alice\n    body: \"Top level\"\n    comment:\n        author: @User:bob\n        body: \"Reply\"\n        comment:\n            author: @User:alice\n            body: \"Reply to reply\"";
        let doc = parse_ok(src);
        if let Node::ThreadComment(tc) = first_node(&doc) {
            assert_eq!(tc.body, "Top level");
            assert_eq!(tc.children.len(), 1);
            if let Node::ThreadComment(reply) = &tc.children[0] {
                assert_eq!(reply.body, "Reply");
                assert_eq!(reply.children.len(), 1);
                if let Node::ThreadComment(reply2) = &reply.children[0] {
                    assert_eq!(reply2.body, "Reply to reply");
                } else {
                    panic!("Expected nested ThreadComment");
                }
            } else {
                panic!("Expected ThreadComment reply");
            }
        } else {
            panic!("Expected ThreadComment node");
        }
    }

    #[test]
    fn parse_comment_with_multiline_body() {
        let src = "comment:\n    author: @User:alice\n    body: |\n        First line\n        Second line";
        let doc = parse_ok(src);
        if let Node::ThreadComment(tc) = first_node(&doc) {
            assert_eq!(tc.body, "First line\nSecond line");
        } else {
            panic!("Expected ThreadComment node");
        }
    }

    // ── Lattice round-trip tests ────────────────────────────────────

    #[test]
    fn roundtrip_lattice_validates() {
        let doc = parse_ok("lattice_validates:\n    artifact: contract\n");
        let rendered = crate::render::render(&doc);
        let doc2 = parse_ok(&rendered);
        assert!(
            matches!(&doc2.nodes[0], Node::LatticeValidates(_)),
            "Round-trip failed. Rendered: {}",
            rendered
        );
    }

    #[test]
    fn roundtrip_lattice_constraint() {
        let doc = parse_ok("lattice_constraint:\n    rule: max 100000\n");
        let rendered = crate::render::render(&doc);
        let doc2 = parse_ok(&rendered);
        assert!(
            matches!(&doc2.nodes[0], Node::LatticeConstraint(_)),
            "Round-trip failed. Rendered: {}",
            rendered
        );
    }

    #[test]
    fn roundtrip_pressure_effect() {
        let doc = parse_ok("pressure_effect:\n    dynamic: decay\n    target: upstream\n");
        let rendered = crate::render::render(&doc);
        let doc2 = parse_ok(&rendered);
        match &doc2.nodes[0] {
            Node::PressureEffect(pe) => {
                assert_eq!(pe.dynamic, "decay");
                assert_eq!(pe.target.as_deref(), Some("upstream"));
            }
            other => panic!("Round-trip failed, got {:?}. Rendered: {}", other, rendered),
        }
    }

    #[test]
    fn roundtrip_unit_cell() {
        let doc = parse_ok("unit_cell:\n    duration: 14d\n");
        let rendered = crate::render::render(&doc);
        let doc2 = parse_ok(&rendered);
        assert!(
            matches!(&doc2.nodes[0], Node::UnitCell(_)),
            "Round-trip failed. Rendered: {}",
            rendered
        );
    }

    #[test]
    fn roundtrip_symmetry() {
        let doc = parse_ok("symmetry:\n    repeat: indefinite\n");
        let rendered = crate::render::render(&doc);
        let doc2 = parse_ok(&rendered);
        assert!(
            matches!(&doc2.nodes[0], Node::Symmetry(_)),
            "Round-trip failed. Rendered: {}",
            rendered
        );
    }

    #[test]
    fn parse_error_has_line_number() {
        // sync: with no name should fail with a ParseError that has line > 0
        let src = "sync:\n    schedule: \"0 * * * *\"";
        let result = parse(src);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.line > 0, "expected line > 0, got {}", err.line);
        assert!(!err.code.is_empty(), "expected non-empty code");
    }
}
