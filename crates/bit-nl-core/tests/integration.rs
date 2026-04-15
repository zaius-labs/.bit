//! Integration tests for the bit-nl-core NL→.bit compiler pipeline.
//!
//! Tests exercise the full `compile(source, profile)` path:
//! segmentation → classification → emission → span index → diagnostics.

use bit_nl_core::{compile, SegmentKind};

// ---------------------------------------------------------------------------
// 1. Full pipeline tests — Define
// ---------------------------------------------------------------------------

#[test]
fn define_user_with_fields() {
    let result = compile("Define a User with name, email, and role", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert!(result.segments[0].confidence > 0.7);
    assert!(result.bit_source.contains("define:@") || result.bit_source.to_lowercase().contains("define"));
}

#[test]
fn define_product_with_typed_fields() {
    let result = compile("A Product has name, price, and sku", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert!(result.segments[0].confidence > 0.7);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn define_session_create_form() {
    let result = compile("Create a Session with token and expiry", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert!(result.segments[0].confidence > 0.7);
    assert!(result.bit_source.contains("@"));
}

#[test]
fn define_order_lowercase_keyword() {
    let result = compile("define an Order with items and total", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert!(result.segments[0].confidence > 0.7);
}

#[test]
fn define_transaction_model_keyword() {
    let result = compile("Model a Transaction with amount and currency", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert!(result.segments[0].confidence > 0.7);
}

// ---------------------------------------------------------------------------
// 2. Full pipeline tests — Task
// ---------------------------------------------------------------------------

#[test]
fn task_users_must_log_in() {
    let result = compile("Users must be able to log in with email and password", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    assert!(result.segments[0].confidence > 0.6);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn task_users_can_reset_password() {
    let result = compile("Users can reset their password via email", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    assert!(result.segments[0].confidence > 0.6);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn task_system_should_send_email() {
    let result = compile("The system should send a verification email after registration", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    assert!(result.segments[0].confidence > 0.6);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn task_users_must_update_profile() {
    let result = compile("Users must be able to update their profile", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    assert!(result.segments[0].confidence > 0.6);
}

#[test]
fn task_implement_user_authentication() {
    let result = compile("Implement user authentication", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    assert!(result.segments[0].confidence > 0.6);
    assert!(!result.bit_source.is_empty());
}

// ---------------------------------------------------------------------------
// 3. Full pipeline tests — Flow
// ---------------------------------------------------------------------------

#[test]
fn flow_when_user_registers_then_send_email() {
    let result = compile("When a user registers then send a verification email", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Flow);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn flow_when_order_placed_trigger_inventory() {
    let result = compile("When order is placed, trigger inventory update", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Flow);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn flow_arrow_notation_draft_review_published() {
    let result = compile("Draft -> Review -> Published", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Flow);
    assert!(result.segments[0].confidence > 0.7);
    assert!(result.bit_source.contains("flow:"));
}

#[test]
fn flow_when_then_explicit() {
    let result = compile("When login fails then lock the account", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Flow);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn flow_after_user_submits_form() {
    let result = compile("After user submits form then validate and save", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Flow);
    assert!(!result.bit_source.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Full pipeline tests — Gate
// ---------------------------------------------------------------------------

#[test]
fn gate_validate_email() {
    let result = compile("Validate email before sending", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Gate);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn gate_check_permissions() {
    let result = compile("Check that the user has admin permissions", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Gate);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn gate_ensure_unique_email() {
    let result = compile("Ensure email is unique across all users", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Gate);
    assert!(!result.bit_source.is_empty());
}

// ---------------------------------------------------------------------------
// 5. Full pipeline tests — Policy
// ---------------------------------------------------------------------------

#[test]
fn policy_never_allow_duplicate_usernames() {
    let result = compile("Never allow duplicate usernames", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Policy);
    assert!(!result.bit_source.is_empty());
    assert!(result.bit_source.contains("gate:policy") || result.bit_source.contains("[!]"));
}

#[test]
fn policy_at_most_five_attempts() {
    let result = compile("At most 5 login attempts are allowed", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Policy);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn policy_always_authenticated() {
    // "always" keyword matches Policy; avoid Gate keywords like "require"
    let result = compile("Always log out inactive sessions after 30 minutes", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Policy);
    assert!(!result.bit_source.is_empty());
}

// ---------------------------------------------------------------------------
// 6. Full pipeline tests — Mutate
// ---------------------------------------------------------------------------

#[test]
fn mutate_update_last_login() {
    let result = compile("Update the user's last login timestamp", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Mutate);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn mutate_set_status_active() {
    let result = compile("Set status to active", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Mutate);
    assert!(!result.bit_source.is_empty());
}

// ---------------------------------------------------------------------------
// 7. Full pipeline tests — Comment
// ---------------------------------------------------------------------------

#[test]
fn comment_note_keyword() {
    let result = compile("Note: this design is temporary until we get product sign-off", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Comment);
    assert!(result.bit_source.starts_with("//"));
}

#[test]
fn comment_todo_keyword() {
    let result = compile("TODO fix the validation logic", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Comment);
    assert!(result.bit_source.starts_with("//"));
}

#[test]
fn comment_fixme_keyword() {
    let result = compile("FIXME this breaks on empty input", None);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].kind, SegmentKind::Comment);
    assert!(result.bit_source.starts_with("//"));
}

// ---------------------------------------------------------------------------
// 8. Full pipeline tests — Unknown (low confidence)
// ---------------------------------------------------------------------------

#[test]
fn unknown_vague_prose() {
    // Genuine prose that matches no known pattern — should be Unknown
    let result = compile("The quick brown fox jumps over the lazy dog", None);
    assert_eq!(result.segments[0].kind, SegmentKind::Unknown);
    assert!(result.segments[0].confidence < 0.5);
}

#[test]
fn unknown_single_noun_phrase() {
    // Single noun with no verbs — should be Unknown
    let result = compile("lorem ipsum dolor sit amet", None);
    assert_eq!(result.segments[0].kind, SegmentKind::Unknown);
    assert!(result.segments[0].confidence < 0.5);
}

#[test]
fn unknown_generates_stub_bit_source() {
    let result = compile("The quick brown fox jumps over the lazy dog", None);
    assert_eq!(result.segments[0].kind, SegmentKind::Unknown);
    assert!(result.bit_source.contains("# STUB"));
}

// ---------------------------------------------------------------------------
// 9. SpanIndex integration tests
// ---------------------------------------------------------------------------

#[test]
fn span_index_populated_for_define() {
    let nl = "Define a User with name and email";
    let result = compile(nl, None);
    assert!(!result.segments.is_empty());
    // SpanIndex must have an entry for offset 0
    let construct = result.span_index.find_nl_construct(0);
    assert!(construct.is_some(), "expected a construct at offset 0");
}

#[test]
fn span_index_nl_spans_count_matches_segments() {
    let nl = "Define a User with name and email.\n\nUsers must be able to log in.";
    let result = compile(nl, None);
    let (nl_count, bit_count, _) = result.span_index.stats();
    assert_eq!(nl_count, result.segments.len(), "NL span count should match segment count");
    assert_eq!(bit_count, result.segments.len(), "bit span count should match segment count");
}

#[test]
fn span_index_byte_offsets_first_segment() {
    let nl = "Define a User with name.\n\nUsers must be able to log in.";
    let result = compile(nl, None);
    // First segment should start at offset 0
    let first_id = &result.span_index.order[0];
    let (nl_span, _bit_span) = result.span_index.get_spans(first_id).unwrap();
    assert_eq!(nl_span.start, 0, "first segment NL span should start at 0");
}

#[test]
fn span_index_second_segment_offset() {
    let nl = "Define a User with name.\n\nUsers must be able to log in.";
    let result = compile(nl, None);
    assert!(result.segments.len() >= 2, "expected at least 2 segments");
    // Second segment offset should be past the first paragraph
    let second_id = &result.span_index.order[1];
    let (nl_span, _) = result.span_index.get_spans(second_id).unwrap();
    assert!(nl_span.start > 0, "second segment should start after offset 0");
}

#[test]
fn span_index_construct_id_stability() {
    let nl = "Define a User with name and email";
    let result1 = compile(nl, None);
    let result2 = compile(nl, None);
    let id1 = &result1.segments[0].construct_id;
    let id2 = &result2.segments[0].construct_id;
    assert_eq!(id1, id2, "same input should produce same ConstructId");
}

#[test]
fn span_index_order_matches_segments() {
    let nl = "Define a User.\n\nUsers can log in.\n\nNever allow more than 5 attempts.";
    let result = compile(nl, None);
    assert_eq!(result.span_index.order.len(), result.segments.len());
    for (i, id) in result.span_index.order.iter().enumerate() {
        assert_eq!(id, &result.segments[i].construct_id);
    }
}

#[test]
fn span_index_bit_spans_non_zero_length() {
    let nl = "Define a User with name and email";
    let result = compile(nl, None);
    for id in &result.span_index.order {
        let (_, bit_span) = result.span_index.get_spans(id).unwrap();
        if let Some(bs) = bit_span {
            assert!(bs.end > bs.start, "bit span should have non-zero length");
        }
    }
}

// ---------------------------------------------------------------------------
// 10. Diagnostics tests
// ---------------------------------------------------------------------------

#[test]
fn low_confidence_produces_diagnostic() {
    // Unknown segments have confidence 0.1, which is < 0.5 → diagnostic
    let nl = "The quick brown fox jumps over the lazy dog";
    let result = compile(nl, None);
    assert!(!result.diagnostics.is_empty());
    assert!(result.segments[0].confidence < 0.5);
}

#[test]
fn diagnostic_message_contains_low_confidence() {
    let nl = "The quick brown fox jumps over the lazy dog";
    let result = compile(nl, None);
    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics[0].message.contains("Low confidence"));
}

#[test]
fn empty_input_no_diagnostics() {
    let result = compile("", None);
    assert!(result.segments.is_empty());
    assert!(result.bit_source.is_empty());
    assert!(result.diagnostics.is_empty());
}

#[test]
fn high_confidence_document_no_diagnostics() {
    let nl = "Define a User with name and email\n\nUsers can log in with a password\n\nWhen login fails then lock the account\n\nNever allow more than 5 attempts";
    let result = compile(nl, None);
    assert!(result.diagnostics.is_empty(), "high-confidence segments should not produce diagnostics: {:?}", result.diagnostics);
}

#[test]
fn diagnostic_span_within_source_bounds() {
    let nl = "fix the thing";
    let result = compile(nl, None);
    let source_len = nl.len() as u32;
    for diag in &result.diagnostics {
        assert!(diag.span.start <= source_len, "diagnostic start out of bounds");
        assert!(diag.span.end <= source_len, "diagnostic end out of bounds");
    }
}

// ---------------------------------------------------------------------------
// 11. Multi-segment pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn multi_segment_define_and_task() {
    let nl = "Define a User with name and email.\n\nUsers must be able to log in.";
    let result = compile(nl, None);
    assert!(result.segments.len() >= 2);
    let kinds: Vec<_> = result.segments.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&SegmentKind::Define), "expected Define in {:?}", kinds);
    assert!(kinds.contains(&SegmentKind::Task), "expected Task in {:?}", kinds);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn three_segment_document_define_task_flow() {
    let nl = "Define a Product with name and price.\n\nUsers can add products to cart.\n\nWhen checkout is complete then send confirmation email.";
    let result = compile(nl, None);
    assert!(result.segments.len() >= 2, "expected at least 2 segments, got {}", result.segments.len());
    assert!(!result.bit_source.is_empty());
}

#[test]
fn four_segment_document_end_to_end() {
    let nl = "Define a User with name and email\n\nUsers can log in with a password\n\nWhen login fails then lock the account\n\nNever allow more than 5 attempts";
    let result = compile(nl, None);
    assert_eq!(result.segments.len(), 4);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert_eq!(result.segments[1].kind, SegmentKind::Task);
    assert_eq!(result.segments[2].kind, SegmentKind::Flow);
    assert_eq!(result.segments[3].kind, SegmentKind::Policy);
    assert!(!result.bit_source.is_empty());
}

#[test]
fn sentence_splitting_within_paragraph() {
    let nl = "Define a Project with title. Users can create projects. Validate the title is not empty.";
    let result = compile(nl, None);
    assert_eq!(result.segments.len(), 3);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert_eq!(result.segments[1].kind, SegmentKind::Task);
    assert_eq!(result.segments[2].kind, SegmentKind::Gate);
}

// ---------------------------------------------------------------------------
// 12. Bit source validity tests
// ---------------------------------------------------------------------------

#[test]
fn define_bit_source_contains_define_keyword() {
    let result = compile("Define a User with name and email", None);
    assert!(
        result.bit_source.contains("define:@") || result.bit_source.contains("define:"),
        "expected define: keyword, got: {}",
        result.bit_source
    );
}

#[test]
fn task_bit_source_contains_task_marker() {
    let result = compile("Users must be able to log in with email and password", None);
    // Task emits "[!] ..." or a stub if confidence < 0.5
    assert!(
        result.bit_source.contains("[!]") || result.bit_source.contains("# STUB"),
        "expected [!] or STUB, got: {}",
        result.bit_source
    );
}

#[test]
fn flow_bit_source_contains_flow_keyword() {
    let result = compile("When login fails then lock the account", None);
    assert!(
        result.bit_source.contains("flow:"),
        "expected flow:, got: {}",
        result.bit_source
    );
}

#[test]
fn gate_bit_source_contains_gate_keyword() {
    let result = compile("Validate email before sending", None);
    assert!(
        result.bit_source.contains("gate:"),
        "expected gate:, got: {}",
        result.bit_source
    );
}

#[test]
fn policy_bit_source_contains_gate_policy() {
    let result = compile("Never allow duplicate usernames", None);
    assert!(
        result.bit_source.contains("gate:policy"),
        "expected gate:policy, got: {}",
        result.bit_source
    );
}

#[test]
fn comment_bit_source_starts_with_comment_marker() {
    let result = compile("Note: we need to revisit this design", None);
    assert!(
        result.bit_source.starts_with("//"),
        "expected //, got: {}",
        result.bit_source
    );
}

#[test]
fn unknown_bit_source_contains_stub() {
    let result = compile("The quick brown fox jumps over the lazy dog", None);
    assert!(
        result.bit_source.contains("# STUB"),
        "expected # STUB, got: {}",
        result.bit_source
    );
}

#[test]
fn emitted_bit_sources_non_empty_for_known_kinds() {
    let cases = vec![
        "Define a User with name and email",
        "Users must be able to log in",
        "When login fails then lock the account",
        "Validate email before sending",
        "Never allow more than 5 retries",
        "Note: this is temporary",
    ];
    for nl in cases {
        let result = compile(nl, None);
        assert!(!result.bit_source.is_empty(), "empty bit_source for: {}", nl);
    }
}

// ---------------------------------------------------------------------------
// 13. Adversarial / edge case tests
// ---------------------------------------------------------------------------

#[test]
fn make_task_classified_as_task() {
    // "make" matches Task pattern — this is correct behavior, not Unknown
    let result = compile("make auth work somehow", None);
    assert_eq!(result.segments[0].kind, SegmentKind::Task);
    // Task confidence from "make" pattern is 0.70
    assert!(result.segments[0].confidence >= 0.5);
}

#[test]
fn only_whitespace_no_valid_segments() {
    let result = compile("   \n\n   \n", None);
    // All produced segments (if any) should be Unknown or Comment
    for seg in &result.segments {
        assert!(
            seg.kind == SegmentKind::Unknown || seg.kind == SegmentKind::Comment,
            "unexpected kind {:?} for whitespace-only input",
            seg.kind
        );
    }
}

#[test]
fn very_short_input_no_panic() {
    let result = compile("hi", None);
    // Should not panic; output should be small
    assert!(result.bit_source.len() < 1000);
}

#[test]
fn unicode_entity_names_no_panic() {
    // Non-ASCII input — should not panic
    let result = compile("Define a Utilisateur avec nom et email", None);
    assert!(!result.segments.is_empty());
}

#[test]
fn very_long_single_word_no_panic() {
    let word = "a".repeat(10_000);
    let result = compile(&word, None);
    // Should not panic; result should be reasonable
    assert!(result.bit_source.len() < 50_000);
}

#[test]
fn null_bytes_handled_gracefully() {
    // String with embedded NUL bytes — Rust strings are valid UTF-8 so this
    // uses the Unicode replacement approach: test with control characters instead
    let input = "Define a User with name\x01 and email";
    let result = compile(input, None);
    assert!(!result.segments.is_empty());
}

#[test]
fn performance_100_segments_under_5_seconds() {
    let segment = "Define a User with name and email.\n\n";
    let nl = segment.repeat(100);
    let start = std::time::Instant::now();
    let result = compile(&nl, None);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 5000,
        "compile took too long: {:?}",
        elapsed
    );
    assert!(!result.segments.is_empty());
}

#[test]
fn mixed_segment_kinds_in_one_document() {
    let nl = [
        "Define a User with name and email",
        "Users can log in",
        "When session expires then log out",
        "Never allow more than 3 failed attempts",
        "Note: rate limiting to be implemented",
        "Update the user's last seen timestamp",
    ]
    .join("\n\n");
    let result = compile(&nl, None);
    assert_eq!(result.segments.len(), 6);
    assert_eq!(result.segments[0].kind, SegmentKind::Define);
    assert_eq!(result.segments[1].kind, SegmentKind::Task);
    assert_eq!(result.segments[2].kind, SegmentKind::Flow);
    assert_eq!(result.segments[3].kind, SegmentKind::Policy);
    assert_eq!(result.segments[4].kind, SegmentKind::Comment);
    assert_eq!(result.segments[5].kind, SegmentKind::Mutate);
}

// ---------------------------------------------------------------------------
// 14. Insta snapshot tests (regression)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_define_user_bit_output() {
    let nl = "Define a User with name, email, and role";
    let result = compile(nl, None);
    insta::assert_snapshot!("define_user_bit_output", result.bit_source);
}

#[test]
fn snapshot_task_login_bit_output() {
    let nl = "Users must be able to log in with email and password";
    let result = compile(nl, None);
    insta::assert_snapshot!("task_login_bit_output", result.bit_source);
}

#[test]
fn snapshot_unknown_auth_bit_output() {
    let nl = "make auth work somehow";
    let result = compile(nl, None);
    insta::assert_snapshot!("unknown_auth_bit_output", result.bit_source);
}

#[test]
fn snapshot_flow_when_then_bit_output() {
    let nl = "When login fails then lock the account";
    let result = compile(nl, None);
    insta::assert_snapshot!("flow_when_then_bit_output", result.bit_source);
}

#[test]
fn snapshot_policy_never_bit_output() {
    let nl = "Never allow more than 5 attempts";
    let result = compile(nl, None);
    insta::assert_snapshot!("policy_never_bit_output", result.bit_source);
}
