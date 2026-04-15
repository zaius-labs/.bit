use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use bit_nl_core::{ClassifiedSegment, confidence_tier, ConfidenceTier};
use bit_nl_core::span_index::ImplLocation;
use bit_nl_core::emit;

/// Build the three-panel markdown content for a hover response.
///
/// - Panel 1: compiled .bit IR fragment
/// - Panel 2: confidence score + tier + component breakdown + alternatives
/// - Panel 3: impl location if present, else "Not yet implemented — run `bit apply`"
pub fn build_hover_content(
    segment: &ClassifiedSegment,
    impl_loc: Option<&ImplLocation>,
) -> String {
    // Panel 1: .bit IR fragment
    let bit_ir = emit::emit_one(segment);
    let panel1 = format!("### .bit IR\n\n```bit\n{}\n```", bit_ir.trim());

    // Panel 2: confidence details
    let tier = confidence_tier(segment.confidence);
    let tier_label = match tier {
        ConfidenceTier::High => "High ✓",
        ConfidenceTier::Medium => "Medium ⚠",
        ConfidenceTier::Low => "Low ✗",
    };
    let mut panel2 = format!(
        "### Confidence\n\n**{:.0}%** — {} ({:?})",
        segment.confidence * 100.0,
        tier_label,
        segment.kind,
    );
    if !segment.alternatives.is_empty() {
        panel2.push_str("\n\n**Alternatives:**\n");
        for (alt_kind, alt_conf) in &segment.alternatives {
            panel2.push_str(&format!("- `{:?}` ({:.0}%)\n", alt_kind, alt_conf * 100.0));
        }
    }

    // Panel 3: implementation location
    let panel3 = match impl_loc {
        Some(loc) => {
            let func = loc
                .function
                .as_deref()
                .map(|f| format!("  - Function: `{}`\n", f))
                .unwrap_or_default();
            format!(
                "### Implementation\n\n- File: `{}`\n- Line: {}\n{}",
                loc.file, loc.line, func
            )
        }
        None => {
            "### Implementation\n\nNot yet implemented — run `bit apply`".to_string()
        }
    };

    format!("{}\n\n---\n\n{}\n\n---\n\n{}", panel1, panel2, panel3)
}

/// Build a tower-lsp `Hover` value for a segment.
pub fn build_hover(
    segment: &ClassifiedSegment,
    impl_loc: Option<&ImplLocation>,
) -> Hover {
    let content = build_hover_content(segment, impl_loc);
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_nl_core::SegmentKind;
    use bit_nl_core::classify::ClassifiedSegment;
    use bit_nl_core::segment::Segment;
    use bit_nl_core::span_index::{ConstructId, ImplLocation};
    use bit_nl_core::ByteSpan;

    fn make_segment(kind: SegmentKind, confidence: f32, text: &str) -> ClassifiedSegment {
        ClassifiedSegment {
            segment: Segment {
                span: ByteSpan::new(0, text.len() as u32),
                text: text.to_string(),
                locked: false,
            },
            kind,
            confidence,
            alternatives: vec![],
            construct_id: ConstructId("hover_test".to_string()),
        }
    }

    #[test]
    fn no_impl_location_shows_not_implemented() {
        let seg = make_segment(SegmentKind::Define, 0.95, "Define a User with name");
        let content = build_hover_content(&seg, None);
        assert!(content.contains("Not yet implemented"));
        assert!(content.contains("bit apply"));
    }

    #[test]
    fn high_confidence_shows_score() {
        let seg = make_segment(SegmentKind::Task, 0.92, "Users can log in");
        let content = build_hover_content(&seg, None);
        assert!(content.contains("92%"));
        assert!(content.contains("High"));
    }

    #[test]
    fn with_alternatives_lists_them() {
        let mut seg = make_segment(SegmentKind::Flow, 0.88, "When done then notify");
        seg.alternatives = vec![(SegmentKind::Task, 0.65), (SegmentKind::Gate, 0.40)];
        let content = build_hover_content(&seg, None);
        assert!(content.contains("Alternatives"));
        assert!(content.contains("Task"));
        assert!(content.contains("Gate"));
    }

    #[test]
    fn with_impl_location_shows_file_and_line() {
        let seg = make_segment(SegmentKind::Define, 0.95, "Define a User with name");
        let loc = ImplLocation {
            file: "src/models/user.rs".to_string(),
            function: Some("create_user".to_string()),
            line: 42,
            construct_id: ConstructId("hover_test".to_string()),
        };
        let content = build_hover_content(&seg, Some(&loc));
        assert!(!content.contains("Not yet implemented"));
        assert!(content.contains("src/models/user.rs"));
        assert!(content.contains("42"));
        assert!(content.contains("create_user"));
    }
}
