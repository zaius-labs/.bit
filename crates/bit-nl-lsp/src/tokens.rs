use lsp_types::{SemanticTokenModifier, SemanticTokenType};
use bit_nl_core::SegmentKind;

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::new("nlDefine"),   // 0 → entity semantic class
    SemanticTokenType::new("nlTask"),     // 1 → function semantic class
    SemanticTokenType::new("nlFlow"),     // 2 → keyword semantic class
    SemanticTokenType::new("nlGate"),     // 3 → decorator semantic class
    SemanticTokenType::new("nlSchema"),   // 4 → type semantic class
    SemanticTokenType::new("nlLowConf"),  // 5 → comment (gray italic)
    SemanticTokenType::new("nlUnknown"),  // 6 → regexp (red)
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::new("locked"),      // 0
    SemanticTokenModifier::new("implemented"), // 1
];

/// Map a segment kind and confidence score to a semantic token type index.
///
/// - confidence < 0.5  → 6 (nlUnknown)
/// - 0.5 ≤ conf < 0.85 → 5 (nlLowConf)
/// - conf ≥ 0.85       → map kind: Define→0, Task→1, Flow→2, Gate→3, Schema→4, else→6
pub fn segment_kind_to_token_type(kind: SegmentKind, confidence: f32) -> u32 {
    if confidence < 0.5 {
        return 6;
    }
    if confidence < 0.85 {
        return 5;
    }
    match kind {
        SegmentKind::Define => 0,
        SegmentKind::Task => 1,
        SegmentKind::Flow => 2,
        SegmentKind::Gate => 3,
        SegmentKind::Schema => 4,
        _ => 6,
    }
}

/// Encode modifiers into a bitmask.
pub fn segment_to_modifiers(locked: bool, has_impl: bool) -> u32 {
    let mut bits = 0u32;
    if locked {
        bits |= 1 << 0;
    }
    if has_impl {
        bits |= 1 << 1;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_high_confidence() {
        assert_eq!(segment_kind_to_token_type(SegmentKind::Define, 0.97), 0);
    }

    #[test]
    fn unknown_low_confidence() {
        assert_eq!(segment_kind_to_token_type(SegmentKind::Unknown, 0.21), 6);
    }

    #[test]
    fn task_medium_confidence() {
        // 0.71 is in [0.5, 0.85) → nlLowConf (5)
        assert_eq!(segment_kind_to_token_type(SegmentKind::Task, 0.71), 5);
    }

    #[test]
    fn modifiers_locked_only() {
        assert_eq!(segment_to_modifiers(true, false), 1);
    }

    #[test]
    fn modifiers_impl_only() {
        assert_eq!(segment_to_modifiers(false, true), 2);
    }

    #[test]
    fn modifiers_both() {
        assert_eq!(segment_to_modifiers(true, true), 3);
    }
}
