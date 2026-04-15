use crate::confidence::{
    self, ConfidenceComponents,
};
use crate::segment::{Segment, SegmentKind};
use crate::profile::UserProfile;
use crate::span_index::ConstructId;
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub struct ClassifiedSegment {
    pub segment: Segment,
    pub kind: SegmentKind,
    pub confidence: f32,
    pub alternatives: Vec<(SegmentKind, f32)>,
    pub construct_id: ConstructId,
}

struct Pattern {
    regex: Regex,
    kind: SegmentKind,
    confidence: f32,
}

impl Pattern {
    fn new(pat: &str, kind: SegmentKind, confidence: f32) -> Self {
        Self {
            regex: Regex::new(pat).expect("invalid pattern regex"),
            kind,
            confidence,
        }
    }
}

static PATTERNS: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // --- Define patterns ---
    Pattern::new(
        r"(?i)^(a|an)\s+\w+\s+(is|has|with)\b",
        SegmentKind::Define, 0.90,
    ),
    Pattern::new(
        r"(?i)^(define|create|add|model)\s+(a[n]?\s+)?\w+\s+(with|has|is)\b",
        SegmentKind::Define, 0.90,
    ),
    Pattern::new(
        r"(?i)^(create|add|model)\s+a[n]?\s+\w+",
        SegmentKind::Define, 0.85,
    ),
    Pattern::new(
        r"(?i)\b(entity|schema|model|table)\s+\w+\s+(has|with|contains)\b",
        SegmentKind::Define, 0.80,
    ),

    // --- Task patterns ---
    Pattern::new(
        r"(?i)^(users?\s+(?:can|should|must|need\s+to))",
        SegmentKind::Task, 0.88,
    ),
    Pattern::new(
        r"(?i)^(the\s+system\s+(?:should|must|will|needs?\s+to))",
        SegmentKind::Task, 0.88,
    ),
    Pattern::new(
        r"(?i)^(implement|build|add|create)\s+",
        SegmentKind::Task, 0.75,
    ),
    Pattern::new(
        r"(?i)^(make|enable|allow|support)\s+",
        SegmentKind::Task, 0.70,
    ),

    // --- Flow patterns ---
    Pattern::new(
        r"(?i)\b(when|after|before|once)\b.*\b(then|triggers?|causes?|leads?\s+to)\b",
        SegmentKind::Flow, 0.85,
    ),
    Pattern::new(
        r"(?i)\b(first|next|then|finally|afterwards)\b",
        SegmentKind::Flow, 0.60,
    ),
    Pattern::new(
        r"(?i)(→|->|-->)\s*\w+",
        SegmentKind::Flow, 0.90,
    ),

    // --- Gate patterns ---
    Pattern::new(
        r"(?i)\b(must\s+pass|requires?|prerequisite|before\s+\w+\s+can)\b",
        SegmentKind::Gate, 0.80,
    ),
    Pattern::new(
        r"(?i)\b(validate|verify|check|ensure|confirm)\s+",
        SegmentKind::Gate, 0.75,
    ),

    // --- Policy/constraint patterns ---
    Pattern::new(
        r"(?i)\b(never|always|at\s+most|at\s+least|no\s+more\s+than)\b",
        SegmentKind::Policy, 0.80,
    ),
    Pattern::new(
        r"(?i)\b(constraint|invariant|rule|policy)\b",
        SegmentKind::Policy, 0.75,
    ),

    // --- Mutate patterns ---
    Pattern::new(
        r"(?i)^(update|modify|change|set)\s+",
        SegmentKind::Mutate, 0.75,
    ),

    // --- Schema patterns ---
    Pattern::new(
        r"(?i)\b(fields?|columns?|attributes?|properties)\b.*:",
        SegmentKind::Schema, 0.70,
    ),

    // --- Comment patterns ---
    Pattern::new(
        r"(?i)^(note|todo|fixme|hack|xxx)\b",
        SegmentKind::Comment, 0.90,
    ),
    Pattern::new(
        r"(?i)^(this\s+is|here\s+we|let's|we\s+should\s+think)\b",
        SegmentKind::Comment, 0.60,
    ),
]);

/// Classify a segment to a .bit node type using rule-based pattern matching
/// with multi-factor confidence scoring.
///
/// 1. Run pattern matching — pick highest-confidence match
/// 2. Build ConfidenceComponents from classifier probability
/// 3. Apply extraction_score and entity_resolution_score
/// 4. Apply expertise_boost from profile
/// 5. Compute final_confidence
pub fn classify(segment: Segment, profile: Option<&UserProfile>) -> ClassifiedSegment {
    let id = ConstructId::from_segment(&segment);
    let text = segment.text.as_str();

    let mut matches: Vec<(SegmentKind, f32)> = Vec::new();

    for pat in PATTERNS.iter() {
        if pat.regex.is_match(text) {
            matches.push((pat.kind, pat.confidence));
        }
    }

    if matches.is_empty() {
        return ClassifiedSegment {
            segment,
            kind: SegmentKind::Unknown,
            confidence: 0.1,
            alternatives: vec![],
            construct_id: id,
        };
    }

    // Sort by confidence descending
    matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_kind, best_conf) = matches[0];
    let alternatives = matches[1..].to_vec();

    // Multi-factor confidence scoring
    let mut components = ConfidenceComponents::new(best_conf);
    components.extraction = confidence::extraction_score(best_kind, text);

    // Only penalize entity resolution when a profile with known entities exists.
    // Without a registry, we can't meaningfully score resolution.
    if let Some(p) = profile {
        if !p.known_entities.is_empty() {
            components.entity_resolution =
                confidence::entity_resolution_score(text, &p.known_entities);
        }
    }

    let base = confidence::base_confidence(&components);

    let boost = match profile {
        Some(p) => confidence::expertise_boost(text, p, best_kind),
        None => 0.0,
    };

    let final_conf = confidence::final_confidence(base, boost);

    ClassifiedSegment {
        segment,
        kind: best_kind,
        confidence: final_conf,
        alternatives,
        construct_id: id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_core::ByteSpan;

    fn seg(text: &str) -> Segment {
        Segment {
            span: ByteSpan::new(0, text.len() as u32),
            text: text.to_string(),
            locked: false,
        }
    }

    #[test]
    fn classify_define_patterns() {
        let c = classify(seg("Define a User with name and email"), None);
        assert_eq!(c.kind, SegmentKind::Define);
        assert!(c.confidence >= 0.85);

        let c = classify(seg("Create a Project"), None);
        assert_eq!(c.kind, SegmentKind::Define);

        let c = classify(seg("A Task is a unit of work with title"), None);
        assert_eq!(c.kind, SegmentKind::Define);
        assert!(c.confidence >= 0.85);

        let c = classify(seg("Entity Order has items and total"), None);
        assert_eq!(c.kind, SegmentKind::Define);
    }

    #[test]
    fn classify_task_patterns() {
        let c = classify(seg("Users can log in with email and password"), None);
        assert_eq!(c.kind, SegmentKind::Task);
        assert!(c.confidence >= 0.85);

        let c = classify(seg("The system should send a confirmation email"), None);
        assert_eq!(c.kind, SegmentKind::Task);

        let c = classify(seg("Implement a search feature"), None);
        assert_eq!(c.kind, SegmentKind::Task);

        let c = classify(seg("Enable two-factor authentication"), None);
        assert_eq!(c.kind, SegmentKind::Task);
    }

    #[test]
    fn classify_flow_patterns() {
        let c = classify(seg("When the user submits then validate the form"), None);
        assert_eq!(c.kind, SegmentKind::Flow);
        assert!(c.confidence >= 0.85);

        let c = classify(seg("Draft -> Review -> Published"), None);
        assert_eq!(c.kind, SegmentKind::Flow);
        assert!(c.confidence >= 0.80);
    }

    #[test]
    fn classify_gate_patterns() {
        let c = classify(seg("Validate email before sending"), None);
        assert_eq!(c.kind, SegmentKind::Gate);

        let c = classify(seg("Requires admin permission"), None);
        assert_eq!(c.kind, SegmentKind::Gate);
    }

    #[test]
    fn classify_policy_patterns() {
        let c = classify(seg("Never allow duplicate usernames"), None);
        assert_eq!(c.kind, SegmentKind::Policy);

        let c = classify(seg("At most 5 retries per request"), None);
        assert_eq!(c.kind, SegmentKind::Policy);
    }

    #[test]
    fn classify_mutate_patterns() {
        let c = classify(seg("Update the user's last login timestamp"), None);
        assert_eq!(c.kind, SegmentKind::Mutate);

        let c = classify(seg("Set status to active"), None);
        assert_eq!(c.kind, SegmentKind::Mutate);
    }

    #[test]
    fn classify_comment_patterns() {
        let c = classify(seg("Note: this is temporary"), None);
        assert_eq!(c.kind, SegmentKind::Comment);
        assert!(c.confidence >= 0.70);

        let c = classify(seg("TODO fix the validation logic"), None);
        assert_eq!(c.kind, SegmentKind::Comment);
    }

    #[test]
    fn classify_unknown_fallback() {
        let c = classify(seg("The quick brown fox"), None);
        assert_eq!(c.kind, SegmentKind::Unknown);
        assert!((c.confidence - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn classify_records_alternatives() {
        // "Implement" matches Task, but if text also contains flow keywords it should record both
        let c = classify(seg("When payment is received then create an invoice"), None);
        assert_eq!(c.kind, SegmentKind::Flow);
        // "create" might also match Task as an alternative
        assert!(!c.alternatives.is_empty() || c.confidence >= 0.85);
    }
}
