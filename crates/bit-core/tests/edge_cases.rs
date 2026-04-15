//! Edge case tests for bit-lang-core — boundary conditions, malformed input,
//! and stress tests that should never panic or cause UB.

use bit_core::*;

// ── 1. Empty string ────────────────────────────────────────────

#[test]
fn empty_string_parses_without_panic() {
    let doc = parse_source("").unwrap();
    assert!(doc.nodes.is_empty());
}

// ── 2. Whitespace only ─────────────────────────────────────────

#[test]
fn whitespace_only_parses_without_panic() {
    let _ = parse_source("   ");
    let _ = parse_source("\n\n\n");
    let _ = parse_source("  \n  \n  ");
    let _ = parse_source("\t\t\t");
}

// ── 3. Comments only ───────────────────────────────────────────

#[test]
fn comments_only_parses_cleanly() {
    let doc = parse_source("// comment 1\n// comment 2\n// comment 3\n").unwrap();
    assert!(!doc.nodes.is_empty());
}

// ── 4. Very long entity name ───────────────────────────────────

#[test]
fn long_entity_name_does_not_panic() {
    let long_name = "A".repeat(1000);
    let source = format!("define:@{}\n    name: \"\"!\n", long_name);
    let _ = parse_source(&source);
    // Just verifying no panic.
}

// ── 5. Unicode in field values ─────────────────────────────────

#[test]
fn unicode_field_values_work() {
    let source = "define:@User\n    name: \"日本語テスト\"!\n    bio: \"émoji 🎉 café\"!\n";
    let result = parse_source(source);
    assert!(
        result.is_ok(),
        "Unicode field values should parse: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    let rendered = render_doc(&doc);
    assert!(
        rendered.contains("日本語テスト") || rendered.contains("emoji"),
        "Unicode content should survive render"
    );
}

// ── 6. Deeply nested groups ────────────────────────────────────

#[test]
fn deeply_nested_groups_work() {
    let mut lines = Vec::new();
    for i in 1..=10 {
        let hashes = "#".repeat(i);
        lines.push(format!("{} Level {}", hashes, i));
    }
    lines.push("[!] Deep task".to_string());
    let source = lines.join("\n");
    let result = parse_source(&source);
    // Should not panic. Parser may or may not accept all 10 levels.
    let _ = result;
}

// ── 7. Entity with 100 fields ──────────────────────────────────

#[test]
fn entity_with_many_fields_works() {
    let mut lines = vec!["define:@BigEntity".to_string()];
    for i in 0..100 {
        lines.push(format!("    field_{}: \"\"!", i));
    }
    let source = lines.join("\n");
    let result = parse_source(&source);
    assert!(
        result.is_ok(),
        "100-field entity should parse: {:?}",
        result.err()
    );
}

// ── 8. Duplicate entity definitions ────────────────────────────

#[test]
fn duplicate_entity_definitions_dont_crash() {
    let source = "\
define:@User
    name: \"\"!

define:@User
    email: \"\"!
";
    let result = parse_source(source);
    assert!(
        result.is_ok(),
        "Duplicate entities should parse: {:?}",
        result.err()
    );

    // Schema extraction should also not crash
    let schemas = load_schemas(&[source]).unwrap();
    assert!(schemas.entities.contains_key("User"));
}

// ── 9. Circular flow references ────────────────────────────────

#[test]
fn circular_flow_references_dont_infinite_loop() {
    let source = "\
flow:lifecycle
    draft --> review --> done --> draft
";
    let result = parse_source(source);
    assert!(
        result.is_ok(),
        "Circular flow should parse: {:?}",
        result.err()
    );

    let doc = result.unwrap();
    let idx = build_index(&doc);
    // The key property: no infinite loop. Flow may or may not be indexed
    // depending on parser interpretation, but we must terminate.
    let _ = idx;
}

// ── 10. Extremely long lines ───────────────────────────────────

#[test]
fn extremely_long_lines_dont_panic() {
    let long_text = "x".repeat(10_000);
    let source = format!("[!] {}", long_text);
    let _ = parse_source(&source);
    // No panic is the test.
}

// ── 11. Mixed line endings ─────────────────────────────────────

#[test]
fn mixed_line_endings_parse_cleanly() {
    // LF
    let _ = parse_source("# Title\n[!] Task\n");
    // CRLF
    let _ = parse_source("# Title\r\n[!] Task\r\n");
    // CR only
    let _ = parse_source("# Title\r[!] Task\r");
    // Mixed
    let _ = parse_source("# Title\r\n[!] Task\n[x] Done\r");
}

// ── 12. Null bytes in input ────────────────────────────────────

#[test]
fn null_bytes_dont_cause_ub() {
    let source = "# Title\0\n[!] Task\0with null\n";
    let _ = parse_source(source);
    // No panic or UB.
}

#[test]
fn null_bytes_embedded_in_field() {
    let source = "define:@User\n    name: \"hello\0world\"!\n";
    let _ = parse_source(source);
}

// ── 13. All field sigil types ──────────────────────────────────

#[test]
fn all_sigil_types_parse_correctly() {
    let source = "\
define:@AllSigils
    required_str: \"\"!
    integer: 0#
    float: 0.0##
    optional: \"\"?
    timestamp: \"2024-01-01\"@
    computed: \"\"^
    list: []
    map: {}
    ref: ->@Other
";
    let result = parse_source(source);
    assert!(
        result.is_ok(),
        "All sigil types should parse: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    let rendered = render_doc(&doc);
    // Verify the rendered output round-trips
    let result2 = parse_source(&rendered);
    assert!(
        result2.is_ok(),
        "Rendered sigil types should re-parse: {:?}",
        result2.err()
    );
}

// ── Additional edge cases ──────────────────────────────────────

#[test]
fn format_empty_string() {
    let result = fmt("");
    assert!(result.is_ok());
}

#[test]
fn build_index_on_empty_doc() {
    let doc = parse_source("").unwrap();
    let idx = build_index(&doc);
    assert!(idx.tasks.is_empty());
    assert!(idx.groups.is_empty());
}

#[test]
fn compile_empty_source() {
    let ir = compile("");
    assert!(ir.is_ok());
}

#[test]
fn to_json_empty_doc() {
    let doc = parse_source("").unwrap();
    let result = to_json(&doc);
    assert!(result.is_ok());
}

#[test]
fn from_json_empty_object() {
    let result = from_json("{}");
    assert!(result.is_ok());
    let doc = result.unwrap();
    assert!(doc.nodes.is_empty());
}

#[test]
fn from_json_invalid_json() {
    let result = from_json("not json at all");
    assert!(result.is_err());
}

#[test]
fn validate_empty_doc_with_empty_schemas() {
    let doc = parse_source("").unwrap();
    let schemas = SchemaRegistry::default();
    let _ = validate_doc(&doc, &schemas);
}

#[test]
fn multiple_dividers_in_sequence() {
    let source = "---\n---\n---\n";
    let result = parse_source(source);
    assert!(result.is_ok());
}

#[test]
fn task_with_all_marker_types() {
    let source = "\
[!] Required task
[ ] Open task
[x] Completed task
[?] Optional task
";
    let doc = parse_source(source).unwrap();
    let idx = build_index(&doc);
    assert_eq!(idx.tasks.len(), 4);
}

#[test]
fn nested_tasks_under_group() {
    let source = "\
# Parent
## Child
### Grandchild
[!] Deep task
";
    let doc = parse_source(source).unwrap();
    let idx = build_index(&doc);
    assert!(!idx.tasks.is_empty());
    assert!(!idx.groups.is_empty());
}

#[test]
fn only_newlines_many() {
    let source = "\n".repeat(1000);
    let _ = parse_source(&source);
}

#[test]
fn binary_garbage_input() {
    let source: String = (0..=255u8).map(|b| b as char).collect();
    let _ = parse_source(&source);
    // No panic is the test.
}
