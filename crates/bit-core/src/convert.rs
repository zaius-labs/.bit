//! Structural converters between JSON, Markdown, and .bit format.

use crate::types::*;
use serde_json::Value;
use std::fmt;

// ── Error type ─────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConvertError {
    InvalidJson(String),
    InvalidMarkdown(String),
    SerializeError(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::InvalidJson(msg) => write!(f, "JSON conversion error: {}", msg),
            ConvertError::InvalidMarkdown(msg) => write!(f, "Markdown conversion error: {}", msg),
            ConvertError::SerializeError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for ConvertError {}

// ── from_json ──────────────────────────────────────────────────

/// Convert a JSON string into a .bit Document.
///
/// - Object keys become entity names in `define:@Entity` blocks
/// - Arrays of objects produce multiple define blocks
/// - Primitive values become fields with appropriate types
pub fn from_json(json_str: &str) -> Result<Document, ConvertError> {
    let value: Value =
        serde_json::from_str(json_str).map_err(|e| ConvertError::InvalidJson(e.to_string()))?;

    let nodes = json_value_to_nodes(&value)?;
    Ok(Document { nodes, ..Default::default() })
}

fn json_value_to_nodes(value: &Value) -> Result<Vec<Node>, ConvertError> {
    match value {
        Value::Object(map) => {
            let mut nodes = Vec::new();
            for (key, val) in map {
                match val {
                    // Array of objects → multiple defines with the same entity name
                    Value::Array(arr) => {
                        for item in arr {
                            if let Value::Object(_) = item {
                                nodes.push(json_object_to_define(key, item)?);
                            } else {
                                // Array of primitives → single define with a list-style field
                                nodes.push(Node::Prose(Prose {
                                    text: format!("{}: {}", key, item),
                                    inline: vec![],
                                }));
                            }
                        }
                    }
                    // Nested object → single define
                    Value::Object(_) => {
                        nodes.push(json_object_to_define(key, val)?);
                    }
                    // Top-level primitive → prose
                    _ => {
                        nodes.push(Node::Prose(Prose {
                            text: format!("{}: {}", key, json_primitive_to_string(val)),
                            inline: vec![],
                        }));
                    }
                }
            }
            Ok(nodes)
        }
        Value::Array(arr) => {
            let mut nodes = Vec::new();
            for item in arr {
                let mut sub = json_value_to_nodes(item)?;
                nodes.append(&mut sub);
            }
            Ok(nodes)
        }
        _ => Ok(vec![Node::Prose(Prose {
            text: json_primitive_to_string(value),
            inline: vec![],
        })]),
    }
}

fn json_object_to_define(entity: &str, value: &Value) -> Result<Node, ConvertError> {
    let map = value.as_object().ok_or_else(|| {
        ConvertError::InvalidJson(format!("expected object for entity {}", entity))
    })?;

    let mut fields = Vec::new();
    for (k, v) in map {
        let default = json_value_to_field_default(v);
        fields.push(FieldDef {
            name: k.clone(),
            plural: false,
            default,
        });
    }

    Ok(Node::Define(Define {
        entity: entity.to_string(),
        atoms: vec![],
        fields,
        from_scope: None,
        mod_scope: None,
        workspace_scope: None,
    }))
}

fn json_value_to_field_default(value: &Value) -> FieldDefault {
    match value {
        Value::String(s) => FieldDefault::Str(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FieldDefault::Int(i)
            } else if let Some(f) = n.as_f64() {
                FieldDefault::Float(f)
            } else {
                FieldDefault::Str(n.to_string())
            }
        }
        Value::Bool(b) => FieldDefault::Bool(*b),
        Value::Null => FieldDefault::Nil,
        Value::Array(_) => FieldDefault::List,
        Value::Object(_) => FieldDefault::Str(value.to_string()),
    }
}

fn json_primitive_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "nil".to_string(),
        other => other.to_string(),
    }
}

// ── from_markdown ──────────────────────────────────────────────

/// Convert Markdown text into a .bit Document.
///
/// - `# Heading` → Group (depth 1)
/// - `## Subheading` → Group (depth 2)
/// - `- [ ] Todo` → Task (pending)
/// - `- [x] Done` → Task (completed)
/// - `- Item` → Task (open)
/// - `` ```lang ... ``` `` → CodeBlock
/// - Other lines → Prose
pub fn from_markdown(md_str: &str) -> Result<Document, ConvertError> {
    let lines: Vec<&str> = md_str.lines().collect();
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Code block
        if line.trim_start().starts_with("```") {
            let lang_part = line.trim_start().trim_start_matches('`');
            let lang = if lang_part.is_empty() {
                None
            } else {
                Some(lang_part.to_string())
            };
            let mut content = String::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(lines[i]);
                i += 1;
            }
            // skip closing ```
            if i < lines.len() {
                i += 1;
            }
            nodes.push(Node::CodeBlock(CodeBlock { lang, content }));
            continue;
        }

        // Heading
        if line.starts_with('#') {
            let trimmed = line.trim_start_matches('#');
            let depth = line.len() - trimmed.len();
            let name = trimmed.trim().to_string();
            if depth > 0 && depth <= 6 && !name.is_empty() {
                nodes.push(Node::Group(Group {
                    depth: depth as u8,
                    name,
                    atoms: vec![],
                    gates: vec![],
                    children: vec![],
                }));
                i += 1;
                continue;
            }
        }

        // Task: - [ ] / - [x] / - item
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [") {
            if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
                nodes.push(make_task(TaskKind::Required, rest));
                i += 1;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("- [x] ") {
                nodes.push(make_task(TaskKind::Completed, rest));
                i += 1;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("- [X] ") {
                nodes.push(make_task(TaskKind::Completed, rest));
                i += 1;
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            nodes.push(make_task(TaskKind::Required, rest));
            i += 1;
            continue;
        }

        // Blank lines → skip
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Prose (accumulate consecutive non-blank, non-special lines)
        let mut text = String::new();
        while i < lines.len() {
            let l = lines[i].trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with("- ") || l.starts_with("```") {
                break;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(l);
            i += 1;
        }
        if !text.is_empty() {
            nodes.push(Node::Prose(Prose {
                text,
                inline: vec![],
            }));
        }
    }

    Ok(Document { nodes, ..Default::default() })
}

fn make_task(kind: TaskKind, text: &str) -> Node {
    Node::Task(Task {
        marker: TaskMarker {
            kind,
            priority: Priority::None,
            prefix: TaskPrefix::None,
            seq: None,
        },
        label: None,
        text: text.to_string(),
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
    })
}

// ── to_json ────────────────────────────────────────────────────

/// Convert a .bit Document into a JSON string.
///
/// - Define blocks → JSON objects keyed by entity name
/// - Groups → nested objects with children
/// - Tasks → objects with status/text fields
/// - Prose → string values
pub fn to_json(doc: &Document) -> Result<String, ConvertError> {
    let value = nodes_to_json_value(&doc.nodes);
    serde_json::to_string_pretty(&value).map_err(|e| ConvertError::SerializeError(e.to_string()))
}

fn nodes_to_json_value(nodes: &[Node]) -> Value {
    let mut map = serde_json::Map::new();
    let mut define_arrays: std::collections::HashMap<String, Vec<Value>> =
        std::collections::HashMap::new();

    for node in nodes {
        match node {
            Node::Define(d) => {
                let obj = define_to_json(d);
                define_arrays.entry(d.entity.clone()).or_default().push(obj);
            }
            Node::Group(g) => {
                let mut group_obj = serde_json::Map::new();
                if !g.children.is_empty() {
                    let children_val = nodes_to_json_value(&g.children);
                    if let Value::Object(child_map) = children_val {
                        group_obj.extend(child_map);
                    }
                }
                map.insert(g.name.clone(), Value::Object(group_obj));
            }
            Node::Task(t) => {
                let status = match t.marker.kind {
                    TaskKind::Completed => "completed",
                    TaskKind::Required => "pending",
                    TaskKind::Optional => "optional",
                    TaskKind::Open => "open",
                };
                let mut task_obj = serde_json::Map::new();
                task_obj.insert("status".to_string(), Value::String(status.to_string()));
                task_obj.insert("text".to_string(), Value::String(t.text.clone()));
                // Use a tasks array if there isn't one yet
                let tasks = map
                    .entry("tasks".to_string())
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = tasks {
                    arr.push(Value::Object(task_obj));
                }
            }
            Node::Prose(p) => {
                let prose = map
                    .entry("prose".to_string())
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = prose {
                    arr.push(Value::String(p.text.clone()));
                }
            }
            Node::CodeBlock(cb) => {
                let mut block_obj = serde_json::Map::new();
                if let Some(lang) = &cb.lang {
                    block_obj.insert("lang".to_string(), Value::String(lang.clone()));
                }
                block_obj.insert("content".to_string(), Value::String(cb.content.clone()));
                let blocks = map
                    .entry("code_blocks".to_string())
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = blocks {
                    arr.push(Value::Object(block_obj));
                }
            }
            // Other node types are not converted (we focus on common patterns)
            _ => {}
        }
    }

    // Merge define arrays into the map
    for (entity, items) in define_arrays {
        if items.len() == 1 {
            map.insert(entity, items.into_iter().next().unwrap());
        } else {
            map.insert(entity, Value::Array(items));
        }
    }

    Value::Object(map)
}

fn define_to_json(d: &Define) -> Value {
    let mut obj = serde_json::Map::new();
    for field in &d.fields {
        let val = field_default_to_json(&field.default);
        obj.insert(field.name.clone(), val);
    }
    Value::Object(obj)
}

fn field_default_to_json(fd: &FieldDefault) -> Value {
    match fd {
        FieldDefault::Str(s) => Value::String(s.clone()),
        FieldDefault::Int(i) => Value::Number(serde_json::Number::from(*i)),
        FieldDefault::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FieldDefault::Bool(b) => Value::Bool(*b),
        FieldDefault::Nil => Value::Null,
        FieldDefault::Atom(s) => Value::String(s.clone()),
        FieldDefault::Enum(v) => Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        FieldDefault::Ref(s) => Value::String(format!("@{}", s)),
        FieldDefault::List => Value::Array(vec![]),
        FieldDefault::Timestamp(s) => Value::String(s.clone()),
        FieldDefault::Trit(t) => Value::Number(serde_json::Number::from(*t as i64)),
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_object_to_define() {
        let json = r#"{"User": {"name": "alice"}}"#;
        let doc = from_json(json).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Define(d) => {
                assert_eq!(d.entity, "User");
                assert_eq!(d.fields.len(), 1);
                assert_eq!(d.fields[0].name, "name");
                match &d.fields[0].default {
                    FieldDefault::Str(s) => assert_eq!(s, "alice"),
                    other => panic!("expected Str, got {:?}", other),
                }
            }
            other => panic!("expected Define, got {:?}", other),
        }
    }

    #[test]
    fn test_json_array_to_defines() {
        let json = r#"{"Users": [{"name": "alice"}, {"name": "bob"}]}"#;
        let doc = from_json(json).unwrap();
        assert_eq!(doc.nodes.len(), 2);
        for (i, expected_name) in [("alice", 0), ("bob", 1)] {
            match &doc.nodes[expected_name] {
                Node::Define(d) => {
                    assert_eq!(d.entity, "Users");
                    assert_eq!(d.fields[0].name, "name");
                    match &d.fields[0].default {
                        FieldDefault::Str(s) => assert_eq!(s, i),
                        other => panic!("expected Str, got {:?}", other),
                    }
                }
                other => panic!("expected Define, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_json_nested_types() {
        let json = r#"{"Config": {"count": 42, "enabled": true, "ratio": 3.14}}"#;
        let doc = from_json(json).unwrap();
        match &doc.nodes[0] {
            Node::Define(d) => {
                assert_eq!(d.entity, "Config");
                // Check that we get the right field types (order may vary in JSON)
                for field in &d.fields {
                    match field.name.as_str() {
                        "count" => assert!(matches!(field.default, FieldDefault::Int(42))),
                        "enabled" => assert!(matches!(field.default, FieldDefault::Bool(true))),
                        "ratio" =>
                        {
                            #[allow(clippy::approx_constant)]
                            if let FieldDefault::Float(f) = field.default {
                                assert!((f - 3.14_f64).abs() < 0.001);
                            } else {
                                panic!("expected Float");
                            }
                        }
                        _ => panic!("unexpected field {}", field.name),
                    }
                }
            }
            other => panic!("expected Define, got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_headers_to_groups() {
        let md = "# Title\n## Sub";
        let doc = from_markdown(md).unwrap();
        assert_eq!(doc.nodes.len(), 2);
        match &doc.nodes[0] {
            Node::Group(g) => {
                assert_eq!(g.depth, 1);
                assert_eq!(g.name, "Title");
            }
            other => panic!("expected Group, got {:?}", other),
        }
        match &doc.nodes[1] {
            Node::Group(g) => {
                assert_eq!(g.depth, 2);
                assert_eq!(g.name, "Sub");
            }
            other => panic!("expected Group, got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_tasks() {
        let md = "- [ ] Pending\n- [x] Done";
        let doc = from_markdown(md).unwrap();
        assert_eq!(doc.nodes.len(), 2);
        match &doc.nodes[0] {
            Node::Task(t) => {
                assert_eq!(t.marker.kind, TaskKind::Required);
                assert_eq!(t.text, "Pending");
            }
            other => panic!("expected Task, got {:?}", other),
        }
        match &doc.nodes[1] {
            Node::Task(t) => {
                assert_eq!(t.marker.kind, TaskKind::Completed);
                assert_eq!(t.text, "Done");
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let doc = from_markdown(md).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::CodeBlock(cb) => {
                assert_eq!(cb.lang.as_deref(), Some("rust"));
                assert_eq!(cb.content, "fn main() {}");
            }
            other => panic!("expected CodeBlock, got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_prose() {
        let md = "Hello world.\nThis is a paragraph.";
        let doc = from_markdown(md).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Prose(p) => {
                assert_eq!(p.text, "Hello world. This is a paragraph.");
            }
            other => panic!("expected Prose, got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_unadorned_list_items() {
        let md = "- Buy groceries\n- Walk dog";
        let doc = from_markdown(md).unwrap();
        assert_eq!(doc.nodes.len(), 2);
        match &doc.nodes[0] {
            Node::Task(t) => {
                assert_eq!(t.text, "Buy groceries");
                assert_eq!(t.marker.kind, TaskKind::Required);
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn test_to_json_define() {
        let doc = Document {
            nodes: vec![Node::Define(Define {
                entity: "User".to_string(),
                atoms: vec![],
                fields: vec![FieldDef {
                    name: "name".to_string(),
                    plural: false,
                    default: FieldDefault::Str("alice".to_string()),
                }],
                from_scope: None,
                mod_scope: None,
                workspace_scope: None,
            })], ..Default::default()
        };
        let json_str = to_json(&doc).unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["User"]["name"], "alice");
    }

    #[test]
    fn test_to_json_tasks() {
        let doc = Document {
            nodes: vec![
                make_task(TaskKind::Required, "Do thing"),
                make_task(TaskKind::Completed, "Did thing"),
            ], ..Default::default()
        };
        let json_str = to_json(&doc).unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["status"], "pending");
        assert_eq!(tasks[0]["text"], "Do thing");
        assert_eq!(tasks[1]["status"], "completed");
        assert_eq!(tasks[1]["text"], "Did thing");
    }

    #[test]
    fn test_to_json_roundtrip() {
        // JSON → Document → JSON → Document, fields should match
        let original = r#"{"User": {"name": "alice", "age": 30}}"#;
        let doc1 = from_json(original).unwrap();
        let json1 = to_json(&doc1).unwrap();
        let doc2 = from_json(&json1).unwrap();

        // Both docs should have the same define structure
        match (&doc1.nodes[0], &doc2.nodes[0]) {
            (Node::Define(d1), Node::Define(d2)) => {
                assert_eq!(d1.entity, d2.entity);
                assert_eq!(d1.fields.len(), d2.fields.len());
                for f1 in &d1.fields {
                    let f2 = d2.fields.iter().find(|f| f.name == f1.name).unwrap();
                    // Compare serialized defaults since FieldDefault doesn't impl PartialEq
                    let v1 = field_default_to_json(&f1.default);
                    let v2 = field_default_to_json(&f2.default);
                    assert_eq!(v1, v2, "field {} mismatch", f1.name);
                }
            }
            _ => panic!("expected Define nodes in both"),
        }
    }

    #[test]
    fn test_invalid_json() {
        let result = from_json("not json");
        assert!(result.is_err());
        match result.unwrap_err() {
            ConvertError::InvalidJson(_) => {}
            other => panic!("expected InvalidJson, got {:?}", other),
        }
    }
}
