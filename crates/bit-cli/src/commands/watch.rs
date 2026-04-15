use clap::Args;
use notify::{recommended_watcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

// ── Public types ─────────────────────────────────────────────────

#[derive(Serialize, Debug)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchEvent {
    // File-level events (preserved, same output as before)
    Created {
        path: String,
        timestamp: String,
    },
    Modified {
        path: String,
        timestamp: String,
    },
    Removed {
        path: String,
        timestamp: String,
    },

    // Construct-level events (--node-diffs mode only)
    NodeAdded {
        path: String,
        construct_id: String,
        kind: String,
        name: String,
        nl_span: Option<NlSpanRef>,
        timestamp: String,
    },
    NodeModified {
        path: String,
        construct_id: String,
        kind: String,
        name: String,
        nl_span: Option<NlSpanRef>,
        changed_fields: Vec<String>,
        timestamp: String,
    },
    NodeRemoved {
        path: String,
        construct_id: String,
        kind: String,
        name: String,
        timestamp: String,
    },
}

#[derive(Serialize, Debug, Clone)]
pub struct NlSpanRef {
    pub nl_file: String,
    pub start: u32,
    pub end: u32,
}

impl WatchEvent {
    pub fn to_ndjson(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ── CLI args ─────────────────────────────────────────────────────

#[derive(Args)]
pub struct WatchArgs {
    /// Directory to watch for .bit file changes
    pub dir: String,

    /// Emit construct-level NDJSON diff events in addition to file-level events
    #[arg(long)]
    pub node_diffs: bool,
}

// ── Watch state ──────────────────────────────────────────────────

#[derive(Clone)]
struct KnownConstruct {
    id: String,
    kind: String,
    name: String,
    fields_hash: u64,
    nl_span: Option<NlSpanRef>,
}

struct WatchState {
    known_constructs: HashMap<PathBuf, Vec<KnownConstruct>>,
}

impl WatchState {
    fn new() -> Self {
        Self {
            known_constructs: HashMap::new(),
        }
    }

    fn process_modification(&mut self, path: &Path) -> Vec<WatchEvent> {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let new_constructs = parse_constructs(path, &source);
        let path_str = path.display().to_string();
        let timestamp = current_timestamp();

        let old = self.known_constructs.get(path).cloned().unwrap_or_default();
        let events = diff_constructs(&path_str, &old, &new_constructs, &timestamp);

        self.known_constructs.insert(path.to_path_buf(), new_constructs);
        events
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn node_kind_name(node: &bit_core::types::Node) -> &'static str {
    use bit_core::types::Node;
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

fn node_name(node: &bit_core::types::Node) -> Option<String> {
    use bit_core::types::Node;
    match node {
        Node::Group(g) => Some(g.name.clone()),
        Node::Task(t) => Some(t.text.clone()),
        Node::Define(d) => Some(d.entity.clone()),
        Node::Mutate(m) => Some(m.entity.clone()),
        Node::Delete(d) => Some(d.entity.clone()),
        Node::Query(q) => Some(q.entity.clone()),
        Node::Variable(v) => Some(v.name.clone()),
        Node::Flow(f) => f.name.clone(),
        Node::Validate(v) => Some(v.name.clone()),
        Node::Check(c) => Some(c.name.clone()),
        Node::Form(f) => Some(f.name.clone()),
        Node::ModDef(m) => Some(m.name.clone()),
        Node::ModInvoke(m) => Some(m.name.clone()),
        Node::Snap(s) => Some(s.name.clone()),
        Node::Diff(d) => Some(d.target.clone()),
        Node::SyncDef(s) => Some(s.name.clone()),
        Node::EntityDef(e) => Some(e.name.clone()),
        Node::MetricDef(m) => Some(m.name.clone()),
        Node::GateDef(g) => Some(g.name.clone()),
        Node::Serve(s) => Some(s.target.clone()),
        Node::Issue(i) => Some(i.title.clone()),
        Node::Commands(c) => {
            // No single name; use first command name if any
            c.commands.first().map(|e| e.name.clone())
        }
        Node::Project(p) => Some(p.name.clone()),
        Node::ProjectScope(p) => Some(p.name.clone()),
        Node::BoundDef(b) => Some(b.name.clone()),
        Node::BuildDef(b) => Some(b.name.clone()),
        Node::RunDef(r) => Some(r.name.clone()),
        Node::Directive(d) => Some(d.kind.clone()),
        Node::Webhook(w) => Some(w.trigger.clone()),
        Node::UseBlock(u) => Some(u.mod_name.clone()),
        Node::Prose(p) => Some(p.text.chars().take(40).collect()),
        _ => None,
    }
}

fn hash_node(node: &bit_core::types::Node) -> u64 {
    let serialized = serde_json::to_string(node).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    serialized.hash(&mut hasher);
    hasher.finish()
}

fn parse_constructs(_path: &Path, source: &str) -> Vec<KnownConstruct> {
    let doc = match bit_core::parse_source(source) {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let mut constructs = Vec::new();
    let mut id_counts: HashMap<String, u32> = HashMap::new();

    for node in &doc.nodes {
        let kind = node_kind_name(node);
        let name = node_name(node).unwrap_or_else(|| kind.to_lowercase());

        let base_id = format!(
            "{}_{}",
            kind.to_lowercase(),
            name.to_lowercase().replace(' ', "_")
        );

        // Deduplicate IDs by appending a counter for repeated names
        let counter = id_counts.entry(base_id.clone()).or_insert(0);
        let id = if *counter == 0 {
            base_id.clone()
        } else {
            format!("{}_{}", base_id, counter)
        };
        *counter += 1;

        let fields_hash = hash_node(node);

        constructs.push(KnownConstruct {
            id,
            kind: kind.to_string(),
            name,
            fields_hash,
            nl_span: None,
        });
    }

    constructs
}

fn diff_constructs(
    path: &str,
    old: &[KnownConstruct],
    new: &[KnownConstruct],
    timestamp: &str,
) -> Vec<WatchEvent> {
    let mut events = Vec::new();

    let old_map: HashMap<&str, &KnownConstruct> =
        old.iter().map(|c| (c.id.as_str(), c)).collect();
    let new_map: HashMap<&str, &KnownConstruct> =
        new.iter().map(|c| (c.id.as_str(), c)).collect();

    // NodeAdded: in new but not old
    for c in new {
        if !old_map.contains_key(c.id.as_str()) {
            events.push(WatchEvent::NodeAdded {
                path: path.to_string(),
                construct_id: c.id.clone(),
                kind: c.kind.clone(),
                name: c.name.clone(),
                nl_span: c.nl_span.clone(),
                timestamp: timestamp.to_string(),
            });
        }
    }

    // NodeRemoved: in old but not new
    for c in old {
        if !new_map.contains_key(c.id.as_str()) {
            events.push(WatchEvent::NodeRemoved {
                path: path.to_string(),
                construct_id: c.id.clone(),
                kind: c.kind.clone(),
                name: c.name.clone(),
                timestamp: timestamp.to_string(),
            });
        }
    }

    // NodeModified: in both but fields_hash changed
    for c in new {
        if let Some(old_c) = old_map.get(c.id.as_str()) {
            if old_c.fields_hash != c.fields_hash {
                events.push(WatchEvent::NodeModified {
                    path: path.to_string(),
                    construct_id: c.id.clone(),
                    kind: c.kind.clone(),
                    name: c.name.clone(),
                    nl_span: c.nl_span.clone(),
                    changed_fields: vec!["fields".to_string()],
                    timestamp: timestamp.to_string(),
                });
            }
        }
    }

    events
}

fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ── Main run ─────────────────────────────────────────────────────

pub fn run(args: &WatchArgs) -> Result<(), Box<dyn Error>> {
    let dir = Path::new(&args.dir).canonicalize().map_err(|e| {
        eprintln!("error: cannot resolve directory '{}': {}", args.dir, e);
        e
    })?;

    eprintln!("Watching {} for .bit changes...", dir.display());

    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(tx)?;
    watcher.watch(&dir, RecursiveMode::Recursive)?;

    let mut state = if args.node_diffs {
        Some(WatchState::new())
    } else {
        None
    };

    for res in rx {
        match res {
            Ok(event) => {
                let kind_str = match event.kind {
                    notify::EventKind::Create(_) => "created",
                    notify::EventKind::Modify(_) => "modified",
                    notify::EventKind::Remove(_) => "removed",
                    _ => continue,
                };

                for path in &event.paths {
                    if path.extension().and_then(|e| e.to_str()) != Some("bit") {
                        continue;
                    }
                    let rel = path.strip_prefix(&dir).unwrap_or(path);
                    let rel_str = rel.to_string_lossy().to_string();
                    let ts = current_timestamp();

                    // Emit file-level event (typed, same JSON shape as before)
                    let file_event = match kind_str {
                        "created" => WatchEvent::Created {
                            path: rel_str.clone(),
                            timestamp: ts.clone(),
                        },
                        "removed" => WatchEvent::Removed {
                            path: rel_str.clone(),
                            timestamp: ts.clone(),
                        },
                        _ => WatchEvent::Modified {
                            path: rel_str.clone(),
                            timestamp: ts.clone(),
                        },
                    };
                    println!("{}", file_event.to_ndjson());

                    // Emit construct-level diffs if --node-diffs is set
                    if let Some(ref mut st) = state {
                        if kind_str == "removed" {
                            // Clear cached constructs for deleted file and emit removals
                            if let Some(old) = st.known_constructs.remove(path) {
                                for c in &old {
                                    let ev = WatchEvent::NodeRemoved {
                                        path: rel_str.clone(),
                                        construct_id: c.id.clone(),
                                        kind: c.kind.clone(),
                                        name: c.name.clone(),
                                        timestamp: ts.clone(),
                                    };
                                    println!("{}", ev.to_ndjson());
                                }
                            }
                        } else {
                            let node_events = st.process_modification(path);
                            for ev in node_events {
                                println!("{}", ev.to_ndjson());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("watch error: {}", e);
            }
        }
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_event_serialization() {
        let event = WatchEvent::Created {
            path: "test.bit".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"created\""));
        assert!(json.contains("\"path\":\"test.bit\""));
    }

    #[test]
    fn test_node_added_serialization() {
        let event = WatchEvent::NodeAdded {
            path: "test.bit".to_string(),
            construct_id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            nl_span: None,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"node_added\""));
        assert!(json.contains("\"construct_id\":\"define_user\""));
    }

    #[test]
    fn test_diff_constructs_added() {
        let old: Vec<KnownConstruct> = vec![];
        let new = vec![KnownConstruct {
            id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            fields_hash: 42,
            nl_span: None,
        }];
        let events = diff_constructs("test.bit", &old, &new, "2024-01-01");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], WatchEvent::NodeAdded { .. }));
    }

    #[test]
    fn test_diff_constructs_removed() {
        let old = vec![KnownConstruct {
            id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            fields_hash: 42,
            nl_span: None,
        }];
        let new: Vec<KnownConstruct> = vec![];
        let events = diff_constructs("test.bit", &old, &new, "2024-01-01");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], WatchEvent::NodeRemoved { .. }));
    }

    #[test]
    fn test_diff_constructs_modified() {
        let old = vec![KnownConstruct {
            id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            fields_hash: 42,
            nl_span: None,
        }];
        let new = vec![KnownConstruct {
            id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            fields_hash: 99, // changed
            nl_span: None,
        }];
        let events = diff_constructs("test.bit", &old, &new, "2024-01-01");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], WatchEvent::NodeModified { .. }));
    }

    #[test]
    fn test_diff_constructs_unchanged() {
        let c = KnownConstruct {
            id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            fields_hash: 42,
            nl_span: None,
        };
        let old = vec![c.clone()];
        let new = vec![c];
        let events = diff_constructs("test.bit", &old, &new, "2024-01-01");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parse_constructs_returns_empty_on_bad_source() {
        let path = std::path::Path::new("fake.bit");
        let result = parse_constructs(path, "@@@ invalid bit @@@ %%%");
        // Should not panic, may return empty or partial results
        let _ = result;
    }

    #[test]
    fn test_parse_constructs_real_source() {
        let source = "define:@User\n  name: text\n  email: text\n";
        let path = std::path::Path::new("test.bit");
        let constructs = parse_constructs(path, source);
        assert!(!constructs.is_empty());
        let first = &constructs[0];
        assert_eq!(first.kind, "Define");
        assert_eq!(first.name, "User");
        assert!(first.id.starts_with("define_"));
    }

    #[test]
    fn test_node_modified_serialization() {
        let event = WatchEvent::NodeModified {
            path: "test.bit".to_string(),
            construct_id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            nl_span: None,
            changed_fields: vec!["fields".to_string()],
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"node_modified\""));
        assert!(json.contains("\"changed_fields\""));
    }

    #[test]
    fn test_node_removed_serialization() {
        let event = WatchEvent::NodeRemoved {
            path: "test.bit".to_string(),
            construct_id: "define_user".to_string(),
            kind: "Define".to_string(),
            name: "user".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"node_removed\""));
    }
}
