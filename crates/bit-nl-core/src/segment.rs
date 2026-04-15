use bit_core::ByteSpan;

#[derive(Debug, Clone)]
pub struct Segment {
    pub span: ByteSpan,
    pub text: String,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    Define, Task, Flow, Gate, Mutate, Schema, Policy, Comment, Unknown,
}

/// Common abbreviations that should NOT be treated as sentence boundaries.
const ABBREVIATIONS: &[&str] = &[
    "e.g.", "i.e.", "vs.", "etc.", "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.",
    "Sr.", "Jr.", "Inc.", "Ltd.", "Corp.", "approx.", "dept.", "est.",
    "fig.", "min.", "max.", "no.", "vol.", "ref.",
];

/// Check if a period at `dot_pos` in `text` is part of a known abbreviation.
fn is_abbreviation(text: &str, dot_pos: usize) -> bool {
    // Look backwards from dot_pos to find the start of the word
    let before = &text[..=dot_pos];
    for abbr in ABBREVIATIONS {
        if before.ends_with(abbr) {
            return true;
        }
    }
    // Check for single-letter abbreviation pattern like "U.S." or "A."
    if dot_pos >= 1 {
        let prev = text.as_bytes()[dot_pos - 1];
        if prev.is_ascii_alphabetic() && (dot_pos < 2 || text.as_bytes()[dot_pos - 2] == b'.') {
            return true;
        }
    }
    false
}

/// Check if a period at `dot_pos` is part of an ellipsis ("..." or ". . .")
fn is_ellipsis(text: &str, dot_pos: usize) -> bool {
    let bytes = text.as_bytes();
    // "..."
    if dot_pos + 2 < bytes.len() && bytes[dot_pos + 1] == b'.' && bytes[dot_pos + 2] == b'.' {
        return true;
    }
    if dot_pos >= 2 && bytes[dot_pos - 1] == b'.' && bytes[dot_pos - 2] == b'.' {
        return true;
    }
    if dot_pos >= 1 && dot_pos + 1 < bytes.len() && bytes[dot_pos - 1] == b'.' && bytes[dot_pos + 1] == b'.' {
        return true;
    }
    false
}

/// Split a paragraph into sentences at `.?!` followed by whitespace + capital letter,
/// handling abbreviations, ellipsis, and quoted text.
fn split_sentences(paragraph: &str) -> Vec<String> {
    if paragraph.trim().is_empty() {
        return vec![];
    }

    let bytes = paragraph.as_bytes();
    let len = bytes.len();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // Track quoted regions — don't split inside quotes
        if b == b'"' || b == b'\'' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }

        if in_quote {
            i += 1;
            continue;
        }

        if b == b'.' || b == b'?' || b == b'!' {
            // For '.', check abbreviations — skip them entirely.
            // For ellipsis, consume all dots but still allow sentence split after the group.
            if b == b'.' && is_abbreviation(paragraph, i) {
                i += 1;
                continue;
            }
            if b == b'.' && is_ellipsis(paragraph, i) {
                // Advance past all consecutive dots
                while i < len && bytes[i] == b'.' {
                    i += 1;
                }
                // Now i points past the last dot; fall through to boundary check below
                // but we need to back up by 1 so the boundary logic works from the last dot
                i -= 1;
            }

            // Look ahead: skip optional closing quotes/parens, then require whitespace + capital
            let mut j = i + 1;
            // Skip trailing punctuation (e.g., `."` or `!)`)
            while j < len && (bytes[j] == b'"' || bytes[j] == b'\'' || bytes[j] == b')' || bytes[j] == b']') {
                j += 1;
            }

            if j < len && (bytes[j] == b' ' || bytes[j] == b'\n' || bytes[j] == b'\t') {
                // Skip all whitespace
                let ws_start = j;
                while j < len && (bytes[j] == b' ' || bytes[j] == b'\n' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < len && bytes[j].is_ascii_uppercase() {
                    // Found sentence boundary
                    let boundary = ws_start;
                    let sentence = paragraph[start..boundary].trim();
                    if !sentence.is_empty() {
                        sentences.push(sentence.to_string());
                    }
                    start = j;
                    i = j;
                    continue;
                }
            }
        }

        i += 1;
    }

    // Remaining text
    let tail = paragraph[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail.to_string());
    }

    sentences
}

/// Segment NL text into semantic units.
/// Splits on double-newlines (paragraph boundaries), then on sentence boundaries within each paragraph.
pub fn segment(source: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut para_start: usize = 0;

    for paragraph in source.split("\n\n") {
        let para_len = paragraph.len();
        let trimmed = paragraph.trim();
        if !trimmed.is_empty() {
            let locked = trimmed.contains("<!-- bit:lock -->");
            let sentences = split_sentences(trimmed);

            if sentences.len() <= 1 {
                // Single sentence or unsplittable — emit as one segment
                // Find the trimmed text's offset within the paragraph
                let trim_offset = paragraph.find(trimmed).unwrap_or(0);
                let abs_start = para_start + trim_offset;
                segments.push(Segment {
                    span: ByteSpan::new(abs_start as u32, (abs_start + trimmed.len()) as u32),
                    text: trimmed.to_string(),
                    locked,
                });
            } else {
                // Multiple sentences — find each in the original paragraph
                let mut search_from = 0;
                for sentence in &sentences {
                    if let Some(rel_pos) = paragraph[search_from..].find(sentence.as_str()) {
                        let abs_start = para_start + search_from + rel_pos;
                        segments.push(Segment {
                            span: ByteSpan::new(abs_start as u32, (abs_start + sentence.len()) as u32),
                            text: sentence.clone(),
                            locked,
                        });
                        search_from += rel_pos + sentence.len();
                    } else {
                        // Fallback: can't locate precisely, use paragraph offset
                        segments.push(Segment {
                            span: ByteSpan::new(para_start as u32, (para_start + para_len) as u32),
                            text: sentence.clone(),
                            locked,
                        });
                    }
                }
            }
        }
        para_start += para_len + 2; // +2 for the \n\n
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_splitting() {
        let input = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let segs = segment(input);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "First paragraph.");
        assert_eq!(segs[1].text, "Second paragraph.");
        assert_eq!(segs[2].text, "Third paragraph.");
    }

    #[test]
    fn sentence_detection() {
        let input = "Define a User. The user has a name. Users can log in.";
        let segs = segment(input);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "Define a User.");
        assert_eq!(segs[1].text, "The user has a name.");
        assert_eq!(segs[2].text, "Users can log in.");
    }

    #[test]
    fn abbreviation_handling() {
        let input = "Use e.g. a pattern vs. another. The system works.";
        let segs = segment(input);
        // Should NOT split at "e.g." or "vs."
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Use e.g. a pattern vs. another.");
        assert_eq!(segs[1].text, "The system works.");
    }

    #[test]
    fn ellipsis_handling() {
        let input = "Wait for it... The answer is here.";
        let segs = segment(input);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn bit_lock_detection() {
        let input = "<!-- bit:lock --> Do not change this.";
        let segs = segment(input);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].locked);
    }

    #[test]
    fn empty_input() {
        let segs = segment("");
        assert!(segs.is_empty());
    }

    #[test]
    fn byte_span_offsets() {
        let input = "Hello.\n\nWorld.";
        let segs = segment(input);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].span.start, 0);
        assert_eq!(segs[0].span.end, 6); // "Hello."
        assert_eq!(segs[1].span.start, 8); // after \n\n
        assert_eq!(segs[1].span.end, 14); // "World."
    }

    #[test]
    fn question_mark_boundary() {
        let input = "What is a User? A User has a name.";
        let segs = segment(input);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "What is a User?");
        assert_eq!(segs[1].text, "A User has a name.");
    }
}
