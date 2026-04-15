use crate::classify::ClassifiedSegment;
use crate::segment::SegmentKind;
use regex::Regex;
use std::sync::LazyLock;

/// Emit .bit source for all classified segments.
pub fn emit_all(segments: &[ClassifiedSegment]) -> String {
    segments.iter().map(|s| emit_one(s)).collect::<Vec<_>>().join("\n\n")
}

pub fn emit_one(seg: &ClassifiedSegment) -> String {
    let conf = seg.confidence;

    let body = match seg.kind {
        SegmentKind::Define => emit_define(seg),
        SegmentKind::Task => emit_task(seg),
        SegmentKind::Flow => emit_flow(seg),
        SegmentKind::Gate => emit_gate(seg),
        SegmentKind::Policy => emit_policy(seg),
        SegmentKind::Mutate => emit_mutate(seg),
        SegmentKind::Comment => return format!("// {}", seg.segment.text.trim()),
        SegmentKind::Schema => emit_schema(seg),
        SegmentKind::Unknown => return emit_stub(seg),
    };

    // Wrap medium-confidence output in a confidence annotation
    if conf < 0.5 {
        return emit_stub(seg);
    }
    if conf < 0.85 {
        return format!("// confidence: {:.0}%\n{}", conf * 100.0, body);
    }
    body
}

// ---------------------------------------------------------------------------
// Noun/verb extraction helpers
// ---------------------------------------------------------------------------

static ENTITY_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:define|create|add|model|a|an)\s+(?:a[n]?\s+)?([A-Z]\w+|[a-z]\w+)").unwrap()
});

static FIELD_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:with|has|contains)\s+(.+)$").unwrap()
});

static ARROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:→|->|-->)").unwrap()
});

/// Extract an entity name — first capitalized word after define/create/a/an.
fn extract_entity_name(text: &str) -> String {
    if let Some(caps) = ENTITY_NAME_RE.captures(text) {
        let name = caps.get(1).unwrap().as_str();
        // Capitalize first letter
        let mut c = name.chars();
        match c.next() {
            None => name.to_string(),
            Some(first) => format!("{}{}", first.to_uppercase(), c.collect::<String>()),
        }
    } else {
        // Fallback: find first capitalized word
        text.split_whitespace()
            .find(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
            .unwrap_or("Entity")
            .to_string()
    }
}

/// Extract field names from "with X and Y" or "has X, Y, Z" patterns.
fn extract_fields(text: &str) -> Vec<String> {
    if let Some(caps) = FIELD_LIST_RE.captures(text) {
        let fields_str = caps.get(1).unwrap().as_str();
        // Split on "and", ",", "+"
        fields_str
            .split(|c: char| c == ',' || c == '+')
            .flat_map(|s| s.split(" and "))
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(|s| {
                // Take just the last word if multi-word (e.g., "a name" -> "name")
                s.split_whitespace()
                    .last()
                    .unwrap_or(&s)
                    .to_string()
            })
            .collect()
    } else {
        vec![]
    }
}

/// Extract condition text from gate-like sentences.
fn extract_condition(text: &str) -> String {
    // Remove leading verbs
    let cleaned = Regex::new(r"(?i)^(validate|verify|check|ensure|confirm|requires?)\s+")
        .unwrap()
        .replace(text, "");
    cleaned.trim().to_string()
}

/// Derive a snake_case identifier from text.
fn slugify(text: &str) -> String {
    let words: Vec<String> = text
        .split_whitespace()
        .take(4)
        .map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        "unnamed".to_string()
    } else {
        words.join("_")
    }
}

// ---------------------------------------------------------------------------
// Emitters
// ---------------------------------------------------------------------------

fn emit_define(seg: &ClassifiedSegment) -> String {
    let text = &seg.segment.text;
    let name = extract_entity_name(text);
    let fields = extract_fields(text);

    if fields.is_empty() {
        format!("define:@{}", name)
    } else {
        let field_lines: Vec<String> = fields
            .iter()
            .map(|f| format!("    {}: unknown", f))
            .collect();
        format!("define:@{}\n{}", name, field_lines.join("\n"))
    }
}

fn emit_task(seg: &ClassifiedSegment) -> String {
    let text = seg.segment.text.trim();
    format!("[!] {}", text)
}

fn emit_flow(seg: &ClassifiedSegment) -> String {
    let text = &seg.segment.text;

    // If contains arrow notation, extract states from it
    if ARROW_RE.is_match(text) {
        let states: Vec<&str> = ARROW_RE.split(text)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let transitions = states.join(" --> ");
        return format!("flow:\n    {}", transitions);
    }

    // Otherwise extract meaningful words as states
    // Look for "when X then Y" pattern
    let when_then = Regex::new(r"(?i)\b(?:when|after|once)\b\s+(.+?)\s+\b(?:then|triggers?)\b\s+(.+)")
        .unwrap();
    if let Some(caps) = when_then.captures(text) {
        let from = caps.get(1).unwrap().as_str().trim();
        let to = caps.get(2).unwrap().as_str().trim();
        return format!("flow:\n    {} --> {}", from, to);
    }

    // Fallback: emit text as a simple flow
    format!("flow:\n    {}", text.trim())
}

fn emit_gate(seg: &ClassifiedSegment) -> String {
    let text = &seg.segment.text;
    let condition = extract_condition(text);
    let name = slugify(&condition);
    format!("gate:{}\n    [!] {}", name, condition)
}

fn emit_policy(seg: &ClassifiedSegment) -> String {
    let text = seg.segment.text.trim();
    format!("gate:policy\n    [!] {}", text)
}

fn emit_mutate(seg: &ClassifiedSegment) -> String {
    let text = seg.segment.text.trim();
    format!("mutate:\n    {}", text)
}

fn emit_schema(seg: &ClassifiedSegment) -> String {
    let text = seg.segment.text.trim();
    format!("# schema: {}", text)
}

fn emit_stub(seg: &ClassifiedSegment) -> String {
    format!(
        "# STUB: low confidence ({:.0}%)\n# nl_source: {:?}",
        seg.confidence * 100.0,
        seg.segment.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::Segment;
    use crate::span_index::ConstructId;
    use bit_core::ByteSpan;

    fn make_seg(text: &str, kind: SegmentKind, confidence: f32) -> ClassifiedSegment {
        let segment = Segment {
            span: ByteSpan::new(0, text.len() as u32),
            text: text.to_string(),
            locked: false,
        };
        let construct_id = ConstructId::from_segment(&segment);
        ClassifiedSegment {
            segment,
            kind,
            confidence,
            alternatives: vec![],
            construct_id,
        }
    }

    #[test]
    fn emit_define_with_fields() {
        let seg = make_seg("Define a User with name and email", SegmentKind::Define, 0.90);
        let out = emit_all(&[seg]);
        assert!(out.contains("define:@User"));
        assert!(out.contains("name: unknown"));
        assert!(out.contains("email: unknown"));
    }

    #[test]
    fn emit_define_no_fields() {
        let seg = make_seg("Create a Project", SegmentKind::Define, 0.85);
        let out = emit_all(&[seg]);
        assert!(out.contains("define:@Project"));
    }

    #[test]
    fn emit_task_format() {
        let seg = make_seg("Users can log in with email", SegmentKind::Task, 0.88);
        let out = emit_all(&[seg]);
        assert!(out.contains("[!] Users can log in with email"));
    }

    #[test]
    fn emit_flow_with_arrows() {
        let seg = make_seg("Draft -> Review -> Published", SegmentKind::Flow, 0.90);
        let out = emit_all(&[seg]);
        assert!(out.contains("flow:"));
        assert!(out.contains("Draft --> Review --> Published"));
    }

    #[test]
    fn emit_flow_when_then() {
        let seg = make_seg("When the user submits then validate the form", SegmentKind::Flow, 0.85);
        let out = emit_all(&[seg]);
        assert!(out.contains("flow:"));
        assert!(out.contains("-->"));
    }

    #[test]
    fn emit_gate_format() {
        let seg = make_seg("Validate email before sending", SegmentKind::Gate, 0.80);
        let out = emit_all(&[seg]);
        assert!(out.contains("gate:"));
        assert!(out.contains("[!]"));
    }

    #[test]
    fn emit_policy_format() {
        let seg = make_seg("Never allow duplicate usernames", SegmentKind::Policy, 0.80);
        let out = emit_all(&[seg]);
        assert!(out.contains("gate:policy"));
        assert!(out.contains("[!]"));
    }

    #[test]
    fn emit_comment_format() {
        let seg = make_seg("Note: this is temporary", SegmentKind::Comment, 0.90);
        let out = emit_all(&[seg]);
        assert!(out.starts_with("// "));
        assert!(out.contains("Note: this is temporary"));
    }

    #[test]
    fn emit_unknown_becomes_stub() {
        let seg = make_seg("The quick brown fox", SegmentKind::Unknown, 0.1);
        let out = emit_all(&[seg]);
        assert!(out.contains("# STUB"));
    }

    #[test]
    fn emit_medium_confidence_annotated() {
        let seg = make_seg("Enable caching", SegmentKind::Task, 0.70);
        let out = emit_all(&[seg]);
        assert!(out.contains("// confidence: 70%"));
        assert!(out.contains("[!] Enable caching"));
    }

    #[test]
    fn emit_low_confidence_stubbed() {
        let seg = make_seg("Something weird", SegmentKind::Task, 0.3);
        let out = emit_all(&[seg]);
        assert!(out.contains("# STUB"));
    }
}
