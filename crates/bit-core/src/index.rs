use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocIndex {
    pub groups: Vec<GroupEntry>,
    pub tasks: Vec<TaskEntry>,
    pub refs: Vec<RefEntry>,
    pub mods: Vec<ModEntry>,
    pub flows: Vec<FlowEntry>,
    pub validators: Vec<String>,
    pub forms: Vec<String>,
    pub variables: HashMap<String, String>,
    pub remembers: Vec<String>,
    pub recalls: Vec<String>,
    pub embed_markers: Vec<String>,
    pub file_scopes: Vec<FileScopeEntry>,
    pub policies: Vec<PolicyEntry>,
    pub escalations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupEntry {
    pub name: String,
    pub depth: u8,
    pub path: Vec<String>,
    pub atoms: Vec<String>,
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub text: String,
    pub label: Option<String>,
    pub kind: String,
    pub group_path: Vec<String>,
    pub assignee: Option<String>,
    pub gates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefEntry {
    pub path: Vec<String>,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub name: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEntry {
    pub edges: Vec<(Vec<String>, Vec<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileScopeEntry {
    pub paths: Vec<String>,
    pub group_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub rules: Vec<(String, Vec<String>)>,
    pub group_path: Vec<String>,
}

impl DocIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(doc: &Document) -> Self {
        let mut idx = Self::new();
        idx.index_nodes(&doc.nodes, &[], &mut None);
        idx.resolve_assignment_inheritance();
        idx
    }

    fn index_nodes(
        &mut self,
        nodes: &[Node],
        parent_path: &[String],
        parent_assignee: &mut Option<String>,
    ) {
        for node in nodes {
            match node {
                Node::Group(g) => {
                    let mut path = parent_path.to_vec();
                    path.push(g.name.clone());

                    let group_assignee = extract_group_assignee(&g.atoms);

                    self.groups.push(GroupEntry {
                        name: g.name.clone(),
                        depth: g.depth,
                        path: path.clone(),
                        atoms: g.atoms.iter().map(|a| a.name.clone()).collect(),
                        assignee: group_assignee.clone(),
                    });

                    let mut effective_assignee = group_assignee.or_else(|| parent_assignee.clone());
                    self.index_nodes(&g.children, &path, &mut effective_assignee);
                }

                Node::Task(t) => {
                    let direct_assignee = extract_assignee(&t.inline);
                    let assignee = direct_assignee.or_else(|| parent_assignee.clone());
                    let kind = format!("{:?}", t.marker.kind);

                    self.tasks.push(TaskEntry {
                        text: t.text.clone(),
                        label: t.label.clone(),
                        kind,
                        group_path: parent_path.to_vec(),
                        assignee,
                        gates: t.gates.iter().map(|g| g.name.clone()).collect(),
                    });

                    self.index_nodes(&t.children, parent_path, parent_assignee);
                }

                Node::Prose(p) => {
                    self.extract_refs(&p.inline, &p.text);
                }

                Node::ModDef(m) => {
                    self.mods.push(ModEntry {
                        name: m.name.clone(),
                        kind: m.kind.clone(),
                    });
                }

                Node::ModInvoke(m) => {
                    self.mods.push(ModEntry {
                        name: m.name.clone(),
                        kind: None,
                    });
                }

                Node::Flow(f) => {
                    self.flows.push(FlowEntry {
                        edges: f
                            .edges
                            .iter()
                            .map(|e| (e.from.clone(), e.to.clone()))
                            .collect(),
                    });
                }

                Node::Validate(v) => {
                    self.validators.push(v.name.clone());
                    self.index_nodes(&v.children, parent_path, parent_assignee);
                }

                Node::Check(_) => {}

                Node::Form(f) => {
                    self.forms.push(f.name.clone());
                }

                Node::Variable(v) => {
                    let val = match &v.value {
                        VarValue::Literal(s) => s.clone(),
                        VarValue::Compute(c) => {
                            if c.live {
                                format!("||{}||", c.expr)
                            } else {
                                format!("|{}|", c.expr)
                            }
                        }
                        VarValue::Ref(r) => format!("@{}", r.path.join(":")),
                    };
                    let scoped_name = if parent_path.is_empty() {
                        v.name.clone()
                    } else {
                        format!("{}:{}", parent_path.join(":"), v.name)
                    };
                    self.variables.insert(scoped_name, val.clone());
                    self.variables.insert(v.name.clone(), val);
                }

                Node::Conditional(c) => {
                    self.index_nodes(&c.children, parent_path, parent_assignee);
                }

                Node::Remember(r) => {
                    self.remembers.push(r.content.clone());
                }

                Node::Recall(r) => {
                    self.recalls.push(r.query.clone());
                }

                Node::EmbedMarker(e) => {
                    self.embed_markers.push(e.tag.clone());
                }

                Node::FilesDef(f) => {
                    self.file_scopes.push(FileScopeEntry {
                        paths: f.paths.clone(),
                        group_path: parent_path.to_vec(),
                    });
                }

                Node::PolicyDef(p) => {
                    let rules = p
                        .rules
                        .iter()
                        .map(|r| {
                            (
                                r.path.clone(),
                                r.gates.iter().map(|g| g.name.clone()).collect(),
                            )
                        })
                        .collect();
                    self.policies.push(PolicyEntry {
                        rules,
                        group_path: parent_path.to_vec(),
                    });
                }

                Node::Escalate(e) => {
                    self.escalations.push(e.target.clone());
                }

                _ => {}
            }
        }
    }

    fn resolve_assignment_inheritance(&mut self) {
        let group_assignees: HashMap<Vec<String>, String> = self
            .groups
            .iter()
            .filter_map(|g| g.assignee.as_ref().map(|a| (g.path.clone(), a.clone())))
            .collect();

        for task in &mut self.tasks {
            if task.assignee.is_some() {
                continue;
            }
            let mut path = task.group_path.clone();
            while !path.is_empty() {
                if let Some(a) = group_assignees.get(&path) {
                    task.assignee = Some(a.clone());
                    break;
                }
                path.pop();
            }
        }
    }

    fn extract_refs(&mut self, inlines: &[Inline], context: &str) {
        for inline in inlines {
            if let Inline::Ref(r) = inline {
                self.refs.push(RefEntry {
                    path: r.path.clone(),
                    context: context.to_string(),
                });
            }
        }
    }
}

fn extract_assignee(inlines: &[Inline]) -> Option<String> {
    for inline in inlines {
        if let Inline::Ref(r) = inline {
            if r.path.first().is_some_and(|p| p == "agent" || p == "me") || r.path.len() >= 2 {
                return Some(format!("@{}", r.path.join(":")));
            }
        }
        if let Inline::Atom(a) = inline {
            if a.name.starts_with('@') {
                return Some(a.name.clone());
            }
        }
    }
    None
}

fn extract_group_assignee(atoms: &[Atom]) -> Option<String> {
    for atom in atoms {
        if atom.name.starts_with('@') {
            return Some(atom.name.clone());
        }
        if let Some(val) = &atom.value {
            if val.starts_with('@') {
                return Some(val.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn build_from_src(src: &str) -> DocIndex {
        let doc = parse::parse(src).expect("parse failed");
        DocIndex::build(&doc)
    }

    #[test]
    fn index_groups() {
        let idx = build_from_src("# Alpha\n\n## Beta");
        assert_eq!(idx.groups.len(), 2);
        assert_eq!(idx.groups[0].name, "Alpha");
        assert_eq!(idx.groups[0].depth, 1);
        assert_eq!(idx.groups[1].name, "Beta");
        assert_eq!(idx.groups[1].depth, 2);
    }

    #[test]
    fn index_tasks() {
        let idx = build_from_src("# Project\n\n    [!] Ship it\n    [o] Nice to have");
        assert_eq!(idx.tasks.len(), 2);
        assert_eq!(idx.tasks[0].text, "Ship it");
        assert_eq!(idx.tasks[0].kind, "Required");
        assert_eq!(idx.tasks[1].kind, "Optional");
    }

    #[test]
    fn index_task_in_group_path() {
        let idx = build_from_src("# Alpha\n\n    [!] Do thing");
        assert_eq!(idx.tasks[0].group_path, vec!["Alpha"]);
    }

    #[test]
    fn index_mod_def() {
        let idx = build_from_src("mod:$Summarizer\n    kind: :guide");
        assert_eq!(idx.mods.len(), 1);
        assert_eq!(idx.mods[0].name, "Summarizer");
    }

    #[test]
    fn index_flow() {
        let idx = build_from_src("flow:\n    A --> B --> C");
        assert_eq!(idx.flows.len(), 1);
        assert!(!idx.flows[0].edges.is_empty());
    }

    #[test]
    fn index_validators() {
        let idx = build_from_src("validate code-review:\n    [!] Check");
        assert_eq!(idx.validators, vec!["code-review"]);
    }

    #[test]
    fn index_forms() {
        let idx = build_from_src("form:intake\n    company: \"\"\n    budget: 0");
        assert_eq!(idx.forms, vec!["intake"]);
    }

    #[test]
    fn index_variables() {
        let idx = build_from_src("target = 500000");
        assert!(idx.variables.contains_key("target"));
        assert_eq!(idx.variables["target"], "500000");
    }

    #[test]
    fn index_remembers() {
        let idx = build_from_src("remember: \"important fact\"");
        assert_eq!(idx.remembers, vec!["important fact"]);
    }

    #[test]
    fn index_recalls() {
        let idx = build_from_src("recall: \"find something\"");
        assert_eq!(idx.recalls, vec!["find something"]);
    }

    #[test]
    fn index_embed_markers() {
        let idx = build_from_src("^my_tag");
        assert_eq!(idx.embed_markers, vec!["my_tag"]);
    }

    #[test]
    fn index_escalations() {
        let idx = build_from_src("escalate: manager");
        assert_eq!(idx.escalations, vec!["manager"]);
    }

    #[test]
    fn index_empty_doc() {
        let idx = build_from_src("");
        assert!(idx.groups.is_empty());
        assert!(idx.tasks.is_empty());
    }

    // ── extract_assignee ──

    #[test]
    fn extract_assignee_from_ref() {
        let inlines = vec![Inline::Ref(Ref {
            path: vec!["agent".to_string()],
            plural: false,
            mod_scope: None,
            workspace_scope: None,
        })];
        assert_eq!(extract_assignee(&inlines), Some("@agent".to_string()));
    }

    #[test]
    fn extract_assignee_from_multi_part_ref() {
        let inlines = vec![Inline::Ref(Ref {
            path: vec!["team".to_string(), "alice".to_string()],
            plural: false,
            mod_scope: None,
            workspace_scope: None,
        })];
        assert_eq!(extract_assignee(&inlines), Some("@team:alice".to_string()));
    }

    #[test]
    fn extract_assignee_none() {
        let inlines = vec![Inline::Text {
            value: "just text".to_string(),
        }];
        assert!(extract_assignee(&inlines).is_none());
    }

    // ── extract_group_assignee ──

    #[test]
    fn group_assignee_from_atom_name() {
        let atoms = vec![Atom {
            name: "@alice".to_string(),
            value: None,
        }];
        assert_eq!(extract_group_assignee(&atoms), Some("@alice".to_string()));
    }

    #[test]
    fn group_assignee_from_atom_value() {
        let atoms = vec![Atom {
            name: "assigned".to_string(),
            value: Some("@bob".to_string()),
        }];
        assert_eq!(extract_group_assignee(&atoms), Some("@bob".to_string()));
    }

    #[test]
    fn group_assignee_none() {
        let atoms = vec![Atom {
            name: "status".to_string(),
            value: None,
        }];
        assert!(extract_group_assignee(&atoms).is_none());
    }

    // ── assignment inheritance ──

    #[test]
    fn assignment_inheritance() {
        let src = "# Team :@alice\n\n    [!] Unassigned task";
        let idx = build_from_src(src);
        // The task should inherit the group's assignee
        assert_eq!(idx.tasks[0].assignee, Some("@alice".to_string()));
    }
}
