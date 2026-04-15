use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use bit_nl_core::{CompileResult, NlDiagnostic, offset_to_position};
use bit_nl_core::DiagnosticSeverity as NlSeverity;

fn nl_span_to_range(text: &str, diag: &NlDiagnostic) -> Range {
    let (start_line, start_char) = offset_to_position(text, diag.span.start);
    let (end_line, end_char) = offset_to_position(text, diag.span.end);
    Range {
        start: Position { line: start_line, character: start_char },
        end: Position { line: end_line, character: end_char },
    }
}

fn nl_severity_to_lsp(sev: NlSeverity) -> DiagnosticSeverity {
    match sev {
        NlSeverity::Error => DiagnosticSeverity::ERROR,
        NlSeverity::Warning => DiagnosticSeverity::WARNING,
        NlSeverity::Information => DiagnosticSeverity::INFORMATION,
        NlSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

/// Convert a CompileResult into LSP diagnostics.
///
/// Severity mapping:
/// - segment confidence < 0.5  → Error
/// - segment confidence 0.5–0.85 → Warning
/// - segment confidence > 0.85  → no diagnostic
/// - NlDiagnostic with Error severity → Error
/// - NlDiagnostic with Warning → Warning
/// - NlDiagnostic with Information → Information
pub fn compile_result_to_diagnostics(result: &CompileResult, source: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Emit per-segment diagnostics based on confidence threshold
    for seg in &result.segments {
        let severity = if seg.confidence < 0.5 {
            DiagnosticSeverity::ERROR
        } else if seg.confidence < 0.85 {
            DiagnosticSeverity::WARNING
        } else {
            continue; // high confidence — no diagnostic
        };

        let (start_line, start_char) = offset_to_position(source, seg.segment.span.start);
        let (end_line, end_char) = offset_to_position(source, seg.segment.span.end);
        diags.push(Diagnostic {
            range: Range {
                start: Position { line: start_line, character: start_char },
                end: Position { line: end_line, character: end_char },
            },
            severity: Some(severity),
            message: format!(
                "Low confidence ({:.0}%): classified as {:?}",
                seg.confidence * 100.0,
                seg.kind
            ),
            source: Some("bit-nl".to_string()),
            ..Default::default()
        });
    }

    // Also include diagnostics from the compile result itself
    for nl_diag in &result.diagnostics {
        // Avoid duplicating if we already emitted for this span
        let already = diags.iter().any(|d| {
            let (sl, sc) = offset_to_position(source, nl_diag.span.start);
            d.range.start.line == sl && d.range.start.character == sc
        });
        if already {
            continue;
        }
        diags.push(Diagnostic {
            range: nl_span_to_range(source, nl_diag),
            severity: Some(nl_severity_to_lsp(nl_diag.severity)),
            message: nl_diag.message.clone(),
            source: Some("bit-nl".to_string()),
            ..Default::default()
        });
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result_with_confidence(confidence: f32) -> (CompileResult, String) {
        use bit_nl_core::{SegmentKind, SpanIndex};
        use bit_nl_core::classify::ClassifiedSegment;
        use bit_nl_core::segment::Segment;
        use bit_nl_core::span_index::ConstructId;
        use bit_nl_core::ByteSpan;

        let source = "Some text here".to_string();
        let seg = Segment {
            span: ByteSpan::new(0, source.len() as u32),
            text: source.clone(),
            locked: false,
        };
        let classified = ClassifiedSegment {
            segment: seg,
            kind: SegmentKind::Unknown,
            confidence,
            alternatives: vec![],
            construct_id: ConstructId("test".to_string()),
        };
        let result = CompileResult {
            bit_source: String::new(),
            segments: vec![classified],
            span_index: SpanIndex::new(),
            diagnostics: vec![],
        };
        (result, source)
    }

    #[test]
    fn low_confidence_yields_error() {
        let (result, source) = make_result_with_confidence(0.3);
        let diags = compile_result_to_diagnostics(&result, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn medium_confidence_yields_warning() {
        let (result, source) = make_result_with_confidence(0.7);
        let diags = compile_result_to_diagnostics(&result, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn high_confidence_no_diagnostic() {
        let (result, source) = make_result_with_confidence(0.95);
        let diags = compile_result_to_diagnostics(&result, &source);
        assert!(diags.is_empty());
    }

    #[test]
    fn ambiguous_segment_information() {
        use bit_nl_core::{NlDiagnostic, DiagnosticSeverity as NlSev, SpanIndex, SegmentKind};
        use bit_nl_core::classify::ClassifiedSegment;
        use bit_nl_core::segment::Segment;
        use bit_nl_core::span_index::ConstructId;
        use bit_nl_core::ByteSpan;

        let source = "Ambiguous text here".to_string();
        let seg = Segment {
            span: ByteSpan::new(0, source.len() as u32),
            text: source.clone(),
            locked: false,
        };
        // high confidence classified segment — no seg-level diagnostic
        let classified = ClassifiedSegment {
            segment: seg.clone(),
            kind: SegmentKind::Task,
            confidence: 0.90,
            alternatives: vec![],
            construct_id: ConstructId("test2".to_string()),
        };
        let nl_diag = NlDiagnostic {
            span: ByteSpan::new(0, source.len() as u32),
            severity: NlSev::Information,
            message: "AmbiguousSegment: multiple interpretations".to_string(),
        };
        let result = CompileResult {
            bit_source: String::new(),
            segments: vec![classified],
            span_index: SpanIndex::new(),
            diagnostics: vec![nl_diag],
        };
        let diags = compile_result_to_diagnostics(&result, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::INFORMATION));
    }
}
