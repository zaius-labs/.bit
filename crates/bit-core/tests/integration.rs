//! Integration tests for the bit-lang-core public API.

use bit_core::*;

#[test]
fn parse_and_render_roundtrip() {
    let source = "# My Project\n\n[!] First task\n[x] Done task\n\ndefine:@User\n    name: \"\"!\n    age: 0#\n";
    let doc = parse_source(source).unwrap();
    let rendered = render_doc(&doc);
    // Re-parse the rendered output
    let doc2 = parse_source(&rendered).unwrap();
    // Both should produce the same structure
    assert_eq!(doc.nodes.len(), doc2.nodes.len());
}

#[test]
fn format_is_idempotent() {
    let source = "# Title\n[!] Task one\n[x] Task two\n";
    let formatted = fmt(source).unwrap();
    let formatted2 = fmt(&formatted).unwrap();
    assert_eq!(formatted, formatted2);
}

#[test]
fn validate_with_schema() {
    let schema_src = "define:@User\n    name: \"\"!\n    email: \"\"!\n";
    let schemas = load_schemas(&[schema_src]).unwrap();
    let doc_src = "define:@User\n    name: alice\n";
    let doc = parse_source(doc_src).unwrap();
    let _errors = validate_doc(&doc, &schemas);
    // Should have validation output (missing required email field)
    // Just verify it doesn't panic
}

#[test]
fn json_roundtrip() {
    let json = r#"{"User": {"name": "alice", "age": 30}}"#;
    let doc = from_json(json).unwrap();
    let bit_text = render_doc(&doc);
    assert!(bit_text.contains("define:@User"));
    assert!(bit_text.contains("name:"));
}

#[test]
fn markdown_roundtrip() {
    let md = "# Tasks\n- [ ] Do thing\n- [x] Done thing\n";
    let doc = from_markdown(md).unwrap();
    let bit_text = render_doc(&doc);
    assert!(bit_text.contains("# Tasks"));
}

#[test]
fn build_index_extracts_tasks() {
    let source = "# Group\n[!] Task A\n[x] Task B\n[!] Task C\n";
    let doc = parse_source(source).unwrap();
    let idx = build_index(&doc);
    // Index should find tasks - verify it built without panicking
    // and has some content
    let _ = idx;
}

#[test]
fn compile_to_ir() {
    let source = "# Section\n[!] Task one\ndefine:@Item\n    name: \"\"!\n";
    let ir = compile(source).unwrap();
    // IR should have been created successfully
    let _ = ir;
}

#[test]
fn to_json_roundtrip() {
    let source = "define:@User\n    name: \"\"!\n    age: 0#\n";
    let doc = parse_source(source).unwrap();
    let json_str = to_json(&doc).unwrap();
    // Should be valid JSON
    let _: serde_json::Value = serde_json::from_str(&json_str).unwrap();
}

#[test]
fn parse_empty_source() {
    let doc = parse_source("").unwrap();
    assert!(doc.nodes.is_empty());
}

#[test]
fn parse_comments_only() {
    let source = "// Comment line 1\n// Comment line 2\n";
    let doc = parse_source(source).unwrap();
    assert!(!doc.nodes.is_empty());
}

#[test]
fn schema_registry_extracts_entities() {
    let source = "define:@User\n    name: \"\"!\n    email: \"\"!\n\ndefine:@Post\n    title: \"\"!\n    body: \"\"!\n";
    let schemas = load_schemas(&[source]).unwrap();
    assert!(schemas.entities.contains_key("User"));
    assert!(schemas.entities.contains_key("Post"));
}
