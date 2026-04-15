use lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, CodeActionResponse,
    TextEdit, WorkspaceEdit, Range, Position, Url,
};
use bit_nl_core::{ClassifiedSegment, SegmentKind, offset_to_position};
use std::collections::HashMap;

/// Build code actions for the given segment at the cursor position.
pub fn code_actions_for_segment(
    uri: &Url,
    source: &str,
    segment: &ClassifiedSegment,
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // Action 1: bit:lock this segment (always available)
    actions.push(lock_action(uri, source, segment, segment.kind, segment.confidence));

    // Action 2: resolve ambiguity — only if confidence < 0.85
    if segment.confidence < 0.85 {
        for (alt_kind, alt_conf) in &segment.alternatives {
            actions.push(resolve_ambiguity_action(uri, source, segment, *alt_kind, *alt_conf));
        }
    }

    actions
}

/// Build a "Resolve as {kind}" code action that inserts a bit:lock annotation.
fn resolve_ambiguity_action(
    uri: &Url,
    source: &str,
    segment: &ClassifiedSegment,
    chosen_kind: SegmentKind,
    confidence: f32,
) -> CodeAction {
    let title = format!(
        "Resolve as {:?} ({:.0}%)",
        chosen_kind,
        confidence * 100.0,
    );

    let edit = build_lock_edit(uri, source, segment, chosen_kind);

    CodeAction {
        title,
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }
}

/// Build a "Lock this classification" code action.
fn lock_action(
    uri: &Url,
    source: &str,
    segment: &ClassifiedSegment,
    kind: SegmentKind,
    confidence: f32,
) -> CodeAction {
    let title = format!(
        "Lock as {:?} ({:.0}%) — bypass future reclassification",
        kind,
        confidence * 100.0,
    );

    let edit = build_lock_edit(uri, source, segment, kind);

    CodeAction {
        title,
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred: Some(confidence >= 0.85),
        disabled: None,
        data: None,
    }
}

/// Build a WorkspaceEdit that wraps the segment in <!-- bit:lock --> annotations.
pub fn build_lock_edit(
    uri: &Url,
    source: &str,
    segment: &ClassifiedSegment,
    _chosen_kind: SegmentKind,
) -> WorkspaceEdit {
    let span = segment.segment.span;
    let id = &segment.construct_id.0;

    // Build the annotation text
    // Before: <!-- bit:lock id="construct_id" -->
    // After:  <!-- /bit:lock -->
    let before_text = format!("<!-- bit:lock id=\"{}\" -->\n", id);
    let after_text = "\n<!-- /bit:lock -->".to_string();

    // Convert byte offsets to LSP positions
    let (before_line, before_char) = offset_to_position(source, span.start);
    let before_pos = Position { line: before_line, character: before_char };

    let (after_line, after_char) = offset_to_position(source, span.end);
    let after_pos = Position { line: after_line, character: after_char };

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![
        // Insert before the segment (at span.start)
        TextEdit {
            range: Range { start: before_pos, end: before_pos },
            new_text: before_text,
        },
        // Insert after the segment (at span.end)
        TextEdit {
            range: Range { start: after_pos, end: after_pos },
            new_text: after_text,
        },
    ]);

    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Entry point called from the LSP server's code_action handler.
/// The real per-document logic lives in main.rs where DocumentState is accessible.
pub fn code_actions(_params: &CodeActionParams) -> Option<CodeActionResponse> {
    Some(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_nl_core::SegmentKind;
    use bit_nl_core::ByteSpan;
    use bit_nl_core::classify::ClassifiedSegment;
    use bit_nl_core::segment::Segment;
    use bit_nl_core::span_index::ConstructId;

    fn make_url() -> Url {
        Url::parse("file:///test.nl").unwrap()
    }

    fn make_segment(kind: SegmentKind, confidence: f32, text: &str, start: u32) -> ClassifiedSegment {
        let alternatives = if confidence < 0.85 {
            vec![(SegmentKind::Task, 0.15), (SegmentKind::Policy, 0.07)]
        } else {
            vec![]
        };
        ClassifiedSegment {
            segment: Segment {
                span: ByteSpan::new(start, start + text.len() as u32),
                text: text.to_string(),
                locked: false,
            },
            kind,
            confidence,
            alternatives,
            construct_id: ConstructId(format!("{:?}_{}", kind, start).to_lowercase()),
        }
    }

    #[test]
    fn test_high_confidence_has_lock_action_only() {
        let source = "Define a User with name and email";
        let seg = make_segment(SegmentKind::Define, 0.97, source, 0);
        let actions = code_actions_for_segment(&make_url(), source, &seg);
        // Only the lock action (no resolve ambiguity for high confidence)
        assert_eq!(actions.len(), 1);
        assert!(actions[0].title.contains("Lock as"));
    }

    #[test]
    fn test_low_confidence_has_resolve_and_lock_actions() {
        let source = "users should be able to log in";
        let seg = make_segment(SegmentKind::Task, 0.71, source, 0);
        let actions = code_actions_for_segment(&make_url(), source, &seg);
        // Lock action + 2 resolve alternatives
        assert!(actions.len() >= 2);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("Lock as")));
        assert!(titles.iter().any(|t| t.contains("Resolve as")));
    }

    #[test]
    fn test_build_lock_edit_inserts_before_and_after() {
        let source = "Define a User with name";
        let seg = make_segment(SegmentKind::Define, 0.97, source, 0);
        let edit = build_lock_edit(&make_url(), source, &seg, SegmentKind::Define);
        let changes = edit.changes.unwrap();
        let edits = changes.values().next().unwrap();
        assert_eq!(edits.len(), 2);
        // First edit inserts before
        assert!(edits[0].new_text.contains("bit:lock"));
        // Second edit inserts after
        assert!(edits[1].new_text.contains("/bit:lock"));
    }

    #[test]
    fn test_lock_action_preferred_for_high_confidence() {
        let source = "Define a User";
        let seg = make_segment(SegmentKind::Define, 0.95, source, 0);
        let actions = code_actions_for_segment(&make_url(), source, &seg);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].is_preferred, Some(true));
    }

    #[test]
    fn test_lock_action_not_preferred_for_low_confidence() {
        let source = "make auth work";
        let seg = make_segment(SegmentKind::Unknown, 0.40, source, 0);
        let actions = code_actions_for_segment(&make_url(), source, &seg);
        // Lock action is not preferred when confidence < 0.85
        let lock_action = actions.iter().find(|a| a.title.contains("Lock as"));
        assert!(lock_action.is_some());
        assert_eq!(lock_action.unwrap().is_preferred, Some(false));
    }

    #[test]
    fn test_resolve_action_title_includes_kind_and_percentage() {
        let source = "users should be able to log in";
        let seg = make_segment(SegmentKind::Task, 0.71, source, 0);
        let actions = code_actions_for_segment(&make_url(), source, &seg);
        let resolve = actions.iter().find(|a| a.title.contains("Resolve as Task")).unwrap();
        assert!(resolve.title.contains("15%"), "Expected 15% but got: {}", resolve.title);
    }
}
