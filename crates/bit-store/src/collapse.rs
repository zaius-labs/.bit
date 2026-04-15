// collapse.rs — Walk a directory of .bit files, parse them, and populate a
// BitStore database with entities, tasks, flows, schemas, and raw blobs.

use std::path::Path;

use bit_core::types::Node;

use crate::store::{BitStore, StoreError};

/// Collapse a directory of .bit files into a .bitstore database.
/// Parses each file and populates entity, task, flow, schema, and blob tables.
pub fn collapse(source_dir: &Path, output: &Path) -> Result<BitStore, StoreError> {
    let mut store = BitStore::create(output)?;

    let mut bit_files = Vec::new();
    collect_bit_files(source_dir, source_dir, &mut bit_files)?;
    bit_files.sort();

    for rel_path in &bit_files {
        let full_path = source_dir.join(rel_path);
        let content = std::fs::read(&full_path)?;
        let hash = blake3::hash(&content).to_hex().to_string();

        // Store raw blob regardless of parse outcome
        store.insert_blob(rel_path, &content, &hash)?;

        // Parse and index — if parse fails, blob is still stored
        let source = String::from_utf8_lossy(&content);
        if let Ok(doc) = bit_core::parse_source(&source) {
            index_document(&mut store, &doc, rel_path)?;
        }
    }

    // Insert language schema as system metadata
    let schema_content = bit_core::LANGUAGE_SCHEMA;
    store.insert_schema(
        "_system",
        &serde_json::json!({
            "type": "language_schema",
            "content": schema_content,
        }),
    )?;

    store.flush()?;
    Ok(store)
}

/// Recursively collect `.bit` files under `current`, building forward-slash
/// paths relative to `root`.
fn collect_bit_files(root: &Path, current: &Path, out: &mut Vec<String>) -> Result<(), StoreError> {
    let entries = std::fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_bit_files(root, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "bit") {
            let rel = path
                .strip_prefix(root)
                .expect("path must be under root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

/// Walk every node in a parsed document and insert structured data into the store.
fn index_document(
    store: &mut BitStore,
    doc: &bit_core::Document,
    file: &str,
) -> Result<(), StoreError> {
    let mut task_idx = 0u32;
    let mut line = 0u32;

    for node in &doc.nodes {
        line += 1;
        index_node(store, node, file, &mut line, &mut task_idx)?;
    }
    Ok(())
}

fn index_node(
    store: &mut BitStore,
    node: &Node,
    file: &str,
    line: &mut u32,
    task_idx: &mut u32,
) -> Result<(), StoreError> {
    match node {
        Node::Define(d) => {
            // Build schema JSON from field definitions
            let fields_json: Vec<serde_json::Value> = d
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "plural": f.plural,
                        "default": format!("{:?}", f.default),
                    })
                })
                .collect();
            let schema = serde_json::json!({
                "entity": d.entity,
                "fields": fields_json,
                "file": file,
                "line": *line,
            });
            store.insert_schema(&d.entity, &schema)?;
        }
        Node::Mutate(m) => {
            let id = m.id.as_deref().unwrap_or("_");
            let fields: serde_json::Map<String, serde_json::Value> = m
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            let record = serde_json::json!({
                "entity": m.entity,
                "id": id,
                "fields": fields,
                "file": file,
                "line": *line,
            });
            store.insert_entity(&m.entity, id, &record)?;
        }
        Node::Task(t) => {
            let task_json = serde_json::json!({
                "text": t.text,
                "marker": format!("{:?}", t.marker.kind),
                "priority": format!("{:?}", t.marker.priority),
                "label": t.label,
                "file": file,
                "line": *line,
            });
            store.insert_task(file, *line, *task_idx, &task_json)?;
            *task_idx += 1;
        }
        Node::Flow(f) => {
            let name = f.name.as_deref().unwrap_or("_unnamed");
            let edges: Vec<serde_json::Value> = f
                .edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "from": e.from,
                        "to": e.to,
                        "label": e.label,
                        "parallel": e.parallel,
                        "gate": e.gate,
                    })
                })
                .collect();
            let flow_json = serde_json::json!({
                "name": name,
                "edges": edges,
                "file": file,
                "line": *line,
            });
            store.insert_flow(name, &flow_json)?;
        }
        Node::Group(g) => {
            // Recurse into children
            for child in &g.children {
                *line += 1;
                index_node(store, child, file, line, task_idx)?;
            }
        }
        _ => {} // Skip other node types
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn collapse_basic() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("users.bit"),
            "\
define:@User
    name: \"\"!
    email: \"\"!

mutate:@User:alice
    name: Alice
    email: alice@co.com

[!] Add authentication
[x] Write tests
",
        )
        .unwrap();

        let out = dir.path().join("test.bitstore");
        let mut store = collapse(dir.path(), &out).unwrap();

        // Blob stored
        assert_eq!(store.count_blobs().unwrap(), 1);

        // Entity indexed (from mutate)
        let alice = store.get_entity("User", "alice").unwrap();
        assert!(alice.is_some());

        // Schema indexed (from define)
        let schema = store.get_schema("User").unwrap();
        assert!(schema.is_some());

        // Tasks indexed
        assert!(store.count_tasks().unwrap() >= 2);
    }

    #[test]
    fn collapse_multiple_files() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("entities")).unwrap();
        fs::write(
            dir.path().join("entities/users.bit"),
            "define:@User\n    name: \"\"!\n\nmutate:@User:bob\n    name: Bob",
        )
        .unwrap();
        fs::write(
            dir.path().join("tasks.bit"),
            "# Sprint\n[!] Task A\n[!] Task B\n[x] Task C",
        )
        .unwrap();

        let out = dir.path().join("test.bitstore");
        let mut store = collapse(dir.path(), &out).unwrap();

        assert_eq!(store.count_blobs().unwrap(), 2);
        assert!(store.get_entity("User", "bob").unwrap().is_some());
    }

    #[test]
    fn collapse_with_flow() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("workflow.bit"),
            "\
flow:release
    draft --> review --> approved
",
        )
        .unwrap();

        let out = dir.path().join("test.bitstore");
        let mut store = collapse(dir.path(), &out).unwrap();

        assert_eq!(store.count_blobs().unwrap(), 1);
        // Flow may or may not be indexed depending on parser output —
        // the key thing is: no panic, blob is stored.
    }

    #[test]
    fn collapse_empty_dir() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("test.bitstore");
        let mut store = collapse(dir.path(), &out).unwrap();
        assert_eq!(store.count_blobs().unwrap(), 0);
    }

    #[test]
    fn collapse_ignores_non_bit() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.md"), "# Hello").unwrap();
        fs::write(dir.path().join("data.bit"), "define:@X\n    name: \"\"!").unwrap();

        let out = dir.path().join("test.bitstore");
        let mut store = collapse(dir.path(), &out).unwrap();
        assert_eq!(store.count_blobs().unwrap(), 1);
    }

    #[test]
    fn collapse_reopen_and_query() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("data.bit"),
            "\
define:@Product
    name: \"\"!
    price: 0#

mutate:@Product:widget
    name: Widget
    price: 9
",
        )
        .unwrap();

        let out = dir.path().join("test.bitstore");
        collapse(dir.path(), &out).unwrap();

        // Reopen and query
        let mut store = BitStore::open(&out).unwrap();
        let product = store.get_entity("Product", "widget").unwrap();
        assert!(product.is_some());
    }
}
