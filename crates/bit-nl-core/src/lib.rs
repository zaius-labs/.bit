//! Probabilistic NL → .bit compiler core.
//!
//! Segments natural language, classifies to .bit node types,
//! scores confidence, emits .bit source, maintains SpanIndex.

pub mod segment;
pub mod classify;
pub mod emit;
pub mod confidence;
pub mod span_index;
pub mod profile;
pub mod incremental;

pub use segment::{Segment, SegmentKind};
pub use classify::ClassifiedSegment;
pub use span_index::{SpanIndex, ConstructId, ImplLocation};
pub use profile::UserProfile;
pub use confidence::{ConfidenceComponents, ConfidenceTier, confidence_tier};
pub use bit_core::ByteSpan;

/// Entry used to merge annotation sidecar data into a SpanIndex.
#[derive(Debug, Clone)]
pub struct AnnotationMergeEntry {
    pub construct_id: String,
    pub file: String,
    pub function: Option<String>,
    pub start_line: u32,
}

/// Convert a byte offset into a (line, character) LSP position (0-indexed).
pub fn offset_to_position(text: &str, offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(text.len());
    let before = &text[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() as u32;
    let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = before[last_newline..].chars().count() as u32;
    (line, character)
}

/// Convert a (line, character) LSP position to a byte offset (0-indexed).
pub fn position_to_offset(text: &str, line: u32, character: u32) -> usize {
    let mut current_line = 0u32;
    let mut offset = 0usize;
    for (i, ch) in text.char_indices() {
        if current_line == line {
            let col = text[offset..i].chars().count() as u32;
            if col == character {
                return i;
            }
        }
        if ch == '\n' {
            current_line += 1;
            if current_line > line {
                return i;
            }
            if current_line == line {
                offset = i + 1;
            }
        }
    }
    // Handle position at end of text
    text.len()
}

/// The top-level compilation result.
#[derive(Debug)]
pub struct CompileResult {
    pub bit_source: String,
    pub segments: Vec<ClassifiedSegment>,
    pub span_index: SpanIndex,
    pub diagnostics: Vec<NlDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct NlDiagnostic {
    pub span: ByteSpan,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticSeverity { Error, Warning, Information, Hint }

/// Compile a full .nl document.
pub fn compile(source: &str, profile: Option<&UserProfile>) -> CompileResult {
    let segments = segment::segment(source);
    let classified: Vec<ClassifiedSegment> = segments.into_iter()
        .map(|seg| classify::classify(seg, profile))
        .collect();
    let bit_source = emit::emit_all(&classified);
    let span_index = span_index::build(&classified, &bit_source);
    let diagnostics = classified.iter()
        .filter(|c| c.confidence < 0.5)
        .map(|c| NlDiagnostic {
            span: c.segment.span,
            severity: if c.confidence < 0.3 { DiagnosticSeverity::Warning } else { DiagnosticSeverity::Information },
            message: format!("Low confidence ({:.0}%): classified as {:?}", c.confidence * 100.0, c.kind),
        })
        .collect();
    CompileResult { bit_source, segments: classified, span_index, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_empty() {
        let result = compile("", None);
        assert!(result.segments.is_empty());
        assert!(result.bit_source.is_empty());
    }

    #[test]
    fn compile_single_paragraph() {
        let result = compile("Define a User with name and email", None);
        assert_eq!(result.segments.len(), 1);
        assert!(!result.bit_source.is_empty());
    }

    #[test]
    fn compile_multiple_paragraphs() {
        let result = compile("Define a User with name\n\nUsers can log in\n\nSend verification email", None);
        assert_eq!(result.segments.len(), 3);
    }

    #[test]
    fn end_to_end_multi_paragraph() {
        let input = "Define a User with name and email\n\nUsers can log in with a password.\n\nWhen login fails then lock the account.\n\nNever allow more than 5 attempts.";
        let result = compile(input, None);

        // Should segment into 4 pieces
        assert_eq!(result.segments.len(), 4);

        // Check classifications
        assert_eq!(result.segments[0].kind, SegmentKind::Define);
        assert_eq!(result.segments[1].kind, SegmentKind::Task);
        assert_eq!(result.segments[2].kind, SegmentKind::Flow);
        assert_eq!(result.segments[3].kind, SegmentKind::Policy);

        // All should have reasonable confidence
        for seg in &result.segments {
            assert!(seg.confidence >= 0.5, "segment {:?} has low confidence {}", seg.kind, seg.confidence);
        }

        // Check emitted .bit source contains expected constructs
        assert!(result.bit_source.contains("define:@User"));
        assert!(result.bit_source.contains("[!]"));
        assert!(result.bit_source.contains("flow:"));
        assert!(result.bit_source.contains("gate:policy"));

        // No diagnostics for high-confidence segments
        // (policy at 0.80 is above 0.5 threshold)
        assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn end_to_end_sentence_splitting() {
        let input = "Define a Project with title. Users can create projects. Validate the title is not empty.";
        let result = compile(input, None);

        // Should split into 3 sentences
        assert_eq!(result.segments.len(), 3);
        assert_eq!(result.segments[0].kind, SegmentKind::Define);
        assert_eq!(result.segments[1].kind, SegmentKind::Task);
        assert_eq!(result.segments[2].kind, SegmentKind::Gate);
    }

    #[test]
    fn end_to_end_unknown_generates_diagnostic() {
        let input = "The quick brown fox jumps over the lazy dog.";
        let result = compile(input, None);
        assert_eq!(result.segments[0].kind, SegmentKind::Unknown);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("Low confidence"));
    }
}
