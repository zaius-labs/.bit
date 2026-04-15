use lsp_types::{InlayHint, InlayHintLabel, InlayHintKind, Position, Range};
use bit_nl_core::{ClassifiedSegment, offset_to_position};

/// Build inlay hints for segments that overlap the given visible range.
///
/// Format: `↪ {node_type} [{confidence:.2}{?}]`
/// where `?` appears when confidence < 0.85.
pub fn build_inlay_hints(segments: &[ClassifiedSegment], source: &str, range: &Range) -> Vec<InlayHint> {
    segments
        .iter()
        .filter_map(|seg| {
            // Convert segment span to LSP positions
            let (start_line, start_char) = offset_to_position(source, seg.segment.span.start);
            let (end_line, end_char) = offset_to_position(source, seg.segment.span.end);

            // Check overlap with visible range
            let seg_start = Position { line: start_line, character: start_char };
            let seg_end = Position { line: end_line, character: end_char };
            if !ranges_overlap(
                range,
                &Range { start: seg_start, end: seg_end },
            ) {
                return None;
            }

            // Build hint label
            let uncertain = if seg.confidence < 0.85 { "?" } else { "" };
            let label = format!(
                "↪ {:?} [{:.2}{}]",
                seg.kind,
                seg.confidence,
                uncertain,
            );

            // Place hint at the end of the segment's last line
            Some(InlayHint {
                position: Position { line: end_line, character: end_char },
                label: InlayHintLabel::String(label),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            })
        })
        .collect()
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    // Both ranges are [start, end), overlap if a.start < b.end && b.start < a.end
    pos_lt(&a.start, &b.end) && pos_lt(&b.start, &a.end)
}

fn pos_lt(a: &Position, b: &Position) -> bool {
    a.line < b.line || (a.line == b.line && a.character < b.character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_nl_core::{SegmentKind};
    use bit_nl_core::classify::ClassifiedSegment;
    use bit_nl_core::segment::Segment;
    use bit_nl_core::span_index::ConstructId;
    use bit_nl_core::ByteSpan;

    fn make_seg(text: &str, kind: SegmentKind, confidence: f32, start: u32) -> ClassifiedSegment {
        ClassifiedSegment {
            segment: Segment {
                span: ByteSpan::new(start, start + text.len() as u32),
                text: text.to_string(),
                locked: false,
            },
            kind,
            confidence,
            alternatives: vec![],
            construct_id: ConstructId("inlay_test".to_string()),
        }
    }

    #[test]
    fn high_conf_no_question_mark() {
        let source = "Define a User with name";
        let seg = make_seg(source, SegmentKind::Define, 0.95, 0);
        let visible = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 1, character: 0 },
        };
        let hints = build_inlay_hints(&[seg], source, &visible);
        assert_eq!(hints.len(), 1);
        if let InlayHintLabel::String(label) = &hints[0].label {
            assert!(!label.contains('?'));
            assert!(label.contains("Define"));
        }
    }

    #[test]
    fn low_conf_adds_question_mark() {
        let source = "something unknown here";
        let seg = make_seg(source, SegmentKind::Unknown, 0.40, 0);
        let visible = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 1, character: 0 },
        };
        let hints = build_inlay_hints(&[seg], source, &visible);
        assert_eq!(hints.len(), 1);
        if let InlayHintLabel::String(label) = &hints[0].label {
            assert!(label.contains('?'));
        }
    }
}
