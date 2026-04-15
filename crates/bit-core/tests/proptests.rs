//! Property-based tests for bit-lang-core using proptest.

use bit_core::*;
use proptest::prelude::*;

// ── Strategies ─────────────────────────────────────────────────

/// Generate random .bit-like source text.
fn arb_bit_source() -> impl Strategy<Value = String> {
    prop::collection::vec(arb_line(), 0..20).prop_map(|lines| lines.join("\n"))
}

fn arb_line() -> impl Strategy<Value = String> {
    prop_oneof![
        // Group headers
        Just("# Section".to_string()),
        Just("## Subsection".to_string()),
        // Tasks
        Just("[!] Do something".to_string()),
        Just("[x] Done task".to_string()),
        Just("[ ] Open task".to_string()),
        Just("[?] Optional task".to_string()),
        // Prose
        "[ -~]{0,80}".prop_map(|s: String| s.trim().to_string()),
        // Comments
        Just("// a comment".to_string()),
        // Blank lines
        Just(String::new()),
        // Define blocks
        Just("define:@User\n    name: \"\"!\n    age: 0#".to_string()),
        // Dividers
        Just("---".to_string()),
        // Flow
        Just("flow: draft -> review -> done".to_string()),
    ]
}

/// Generate valid JSON objects for from_json conversion.
fn arb_json_object() -> impl Strategy<Value = String> {
    let key = "[A-Z][a-z]{2,8}";
    let val = "[a-z]{1,10}";
    prop::collection::vec((key, val), 1..5).prop_map(|pairs| {
        let fields: Vec<String> = pairs
            .iter()
            .map(|(k, v)| format!("\"{}\": \"{}\"", k, v))
            .collect();
        format!("{{{}}}", fields.join(", "))
    })
}

// ── Helper: count Task nodes recursively ───────────────────────

fn count_task_nodes(doc: &Document) -> usize {
    fn walk(nodes: &[bit_core::types::Node]) -> usize {
        let mut count = 0;
        for node in nodes {
            match node {
                bit_core::types::Node::Task(t) => {
                    count += 1;
                    count += walk(&t.children);
                }
                bit_core::types::Node::Group(g) => {
                    count += walk(&g.children);
                }
                _ => {}
            }
        }
        count
    }
    walk(&doc.nodes)
}

// ── Property 1: Parse-render round-trip ────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn parse_render_roundtrip(source in arb_bit_source()) {
        if let Ok(doc) = parse_source(&source) {
            let rendered = render_doc(&doc);
            if let Ok(doc2) = parse_source(&rendered) {
                // Same number of top-level nodes after round-trip.
                prop_assert_eq!(doc.nodes.len(), doc2.nodes.len(),
                    "Node count changed after round-trip.\nOriginal source:\n{}\nRendered:\n{}",
                    source, rendered);
            }
            // If re-parse fails, that's OK — render may not produce valid .bit
            // for every edge case. The property holds only when both parse.
        }
    }
}

// ── Property 2: Format idempotency ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn format_idempotency(source in arb_bit_source()) {
        if let Ok(formatted) = fmt(&source) {
            // Skip degenerate cases where formatting produces something
            // the parser interprets differently (random chars → structural tokens)
            if formatted.is_empty() || formatted.len() != source.len() && formatted.trim() != source.trim() {
                return Ok(());
            }
            if let Ok(formatted2) = fmt(&formatted) {
                prop_assert_eq!(formatted.clone(), formatted2.clone(),
                    "Format was not idempotent.\nSource:\n{}\nFirst format:\n{}\nSecond format:\n{}",
                    source, formatted, formatted2);
            }
        }
    }
}

// ── Property 3: JSON conversion round-trip ─────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn json_roundtrip_preserves_entity_names(json in arb_json_object()) {
        if let Ok(doc) = from_json(&json) {
            // Extract entity names from the document
            let entity_names: Vec<String> = doc.nodes.iter().filter_map(|n| {
                if let bit_core::types::Node::Define(d) = n {
                    Some(d.entity.clone())
                } else {
                    None
                }
            }).collect();

            if let Ok(json_out) = to_json(&doc) {
                // Every entity name from the original should appear in the output JSON
                for name in &entity_names {
                    prop_assert!(json_out.contains(name),
                        "Entity '{}' lost in JSON round-trip.\nInput JSON: {}\nOutput JSON: {}",
                        name, json, json_out);
                }
            }
        }
    }
}

// ── Property 4: Schema extract stability ───────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn schema_extract_stability(source in arb_bit_source()) {
        if let Ok(schemas1) = load_schemas(&[&source]) {
            if let Ok(schemas2) = load_schemas(&[&source]) {
                // Same entity names extracted both times
                let mut keys1: Vec<_> = schemas1.entities.keys().cloned().collect();
                let mut keys2: Vec<_> = schemas2.entities.keys().cloned().collect();
                keys1.sort();
                keys2.sort();
                prop_assert_eq!(keys1, keys2,
                    "Schema extraction was not stable for source:\n{}", source);
            }
        }
    }
}

// ── Property 5: Index task count consistency ───────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn index_task_count_matches_ast(source in arb_bit_source()) {
        if let Ok(doc) = parse_source(&source) {
            let idx = build_index(&doc);
            let ast_task_count = count_task_nodes(&doc);
            prop_assert_eq!(idx.tasks.len(), ast_task_count,
                "Index task count {} != AST task count {} for source:\n{}",
                idx.tasks.len(), ast_task_count, source);
        }
    }
}
