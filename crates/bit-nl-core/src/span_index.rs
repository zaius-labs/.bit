use bit_core::ByteSpan;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Stable identifier for a compiled construct.
/// Hash-based: hash of (kind, name_or_content) so it survives reordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstructId(pub String);

impl ConstructId {
    /// Generate from segment text alone (legacy, used during classification
    /// before kind is known).
    pub fn from_segment(seg: &crate::segment::Segment) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        // Use first 50 chars of normalized text for stability
        let normalized: String = seg.text.chars()
            .take(50)
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase();
        normalized.hash(&mut h);
        Self(format!("nl_{:016x}", h.finish()))
    }

    /// Generate from segment content + kind for stronger stability.
    pub fn from_segment_with_kind(seg: &crate::segment::Segment, kind: crate::segment::SegmentKind) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        format!("{:?}", kind).hash(&mut h);
        // Use first 50 chars of normalized text for stability
        let normalized: String = seg.text.chars()
            .take(50)
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase();
        normalized.hash(&mut h);
        Self(format!("nl_{:016x}", h.finish()))
    }
}

/// Location of an implementation in source code (from agent annotations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplLocation {
    pub file: String,
    pub function: Option<String>,
    pub line: u32,
    pub construct_id: ConstructId,
}

/// Bidirectional index: ConstructId <-> ByteSpan in both NL and .bit sources
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SpanIndex {
    /// NL source span for each construct
    pub nl_spans: HashMap<ConstructId, ByteSpan>,
    /// .bit output span for each construct
    pub bit_spans: HashMap<ConstructId, ByteSpan>,
    /// Implementation locations (filled by agent harness)
    pub impl_locations: HashMap<ConstructId, ImplLocation>,
    /// Ordered list of construct IDs (for stable iteration)
    pub order: Vec<ConstructId>,
}

impl SpanIndex {
    pub fn new() -> Self { Self::default() }

    /// Register a construct with its NL source span
    pub fn register(&mut self, id: ConstructId, nl_span: ByteSpan) {
        self.nl_spans.insert(id.clone(), nl_span);
        if !self.order.contains(&id) {
            self.order.push(id);
        }
    }

    /// Set the .bit output span for a construct
    pub fn set_bit_span(&mut self, id: &ConstructId, span: ByteSpan) {
        self.bit_spans.insert(id.clone(), span);
    }

    /// Set implementation location (from agent annotation sidecar)
    pub fn set_impl(&mut self, id: &ConstructId, loc: ImplLocation) {
        self.impl_locations.insert(id.clone(), loc);
    }

    /// Find which construct a NL byte offset falls within
    pub fn find_nl_construct(&self, offset: u32) -> Option<&ConstructId> {
        self.nl_spans.iter()
            .find(|(_, span)| span.contains(offset))
            .map(|(id, _)| id)
    }

    /// Find which construct a .bit byte offset falls within
    pub fn find_bit_construct(&self, offset: u32) -> Option<&ConstructId> {
        self.bit_spans.iter()
            .find(|(_, span)| span.contains(offset))
            .map(|(id, _)| id)
    }

    /// Get the NL->.bit span pair for a construct
    pub fn get_spans(&self, id: &ConstructId) -> Option<(ByteSpan, Option<ByteSpan>)> {
        self.nl_spans.get(id).map(|nl| (*nl, self.bit_spans.get(id).copied()))
    }

    /// Apply a text edit: shift all spans after the edit point
    pub fn apply_edit(&mut self, edit_start: u32, old_len: u32, new_len: u32) {
        let delta = new_len as i64 - old_len as i64;
        for span in self.nl_spans.values_mut() {
            if span.start >= edit_start + old_len {
                // Span is entirely after the edit -- shift
                span.start = (span.start as i64 + delta).max(0) as u32;
                span.end = (span.end as i64 + delta).max(0) as u32;
            } else if span.end > edit_start {
                // Span overlaps the edit -- extend/shrink end
                span.end = (span.end as i64 + delta).max(span.start as i64) as u32;
            }
        }
    }

    /// Remove a construct from the index
    pub fn remove(&mut self, id: &ConstructId) {
        self.nl_spans.remove(id);
        self.bit_spans.remove(id);
        self.impl_locations.remove(id);
        self.order.retain(|i| i != id);
    }

    /// Serialize to JSON for .span.json persistence
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    /// Stats for debugging
    pub fn stats(&self) -> (usize, usize, usize) {
        (self.nl_spans.len(), self.bit_spans.len(), self.impl_locations.len())
    }

    /// Merge annotation events from a sidecar into this SpanIndex.
    /// Returns the number of impl_locations updated.
    pub fn merge_annotations(&mut self, events: &[crate::AnnotationMergeEntry]) -> usize {
        let mut count = 0;
        for entry in events {
            let id = ConstructId(entry.construct_id.clone());
            if self.nl_spans.contains_key(&id) {
                self.impl_locations.insert(id, ImplLocation {
                    file: entry.file.clone(),
                    function: entry.function.clone(),
                    line: entry.start_line,
                    construct_id: ConstructId(entry.construct_id.clone()),
                });
                count += 1;
            }
        }
        count
    }
}

/// Build a SpanIndex from classified segments and the emitted .bit source
pub fn build(segments: &[crate::classify::ClassifiedSegment], _bit_source: &str) -> SpanIndex {
    let mut index = SpanIndex::new();
    let mut bit_offset = 0u32;

    for (i, seg) in segments.iter().enumerate() {
        // Register NL span
        index.register(seg.construct_id.clone(), seg.segment.span);

        // Estimate .bit span by emitting this segment individually
        let bit_text = crate::emit::emit_one(seg);
        let bit_span = ByteSpan::new(bit_offset, bit_offset + bit_text.len() as u32);
        index.set_bit_span(&seg.construct_id, bit_span);
        // +2 for \n\n separator between segments (except after last)
        bit_offset += bit_text.len() as u32;
        if i + 1 < segments.len() {
            bit_offset += 2;
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::{Segment, SegmentKind};

    fn seg(text: &str) -> Segment {
        Segment {
            span: ByteSpan::new(0, text.len() as u32),
            text: text.to_string(),
            locked: false,
        }
    }

    #[test]
    fn construct_id_stability() {
        let s = seg("Define a User with name");
        let id1 = ConstructId::from_segment(&s);
        let id2 = ConstructId::from_segment(&s);
        assert_eq!(id1, id2);
    }

    #[test]
    fn construct_id_differs_for_different_kinds() {
        let s = seg("Define a User with name");
        let id_define = ConstructId::from_segment_with_kind(&s, SegmentKind::Define);
        let id_task = ConstructId::from_segment_with_kind(&s, SegmentKind::Task);
        assert_ne!(id_define, id_task);
    }

    #[test]
    fn register_and_find_nl_construct() {
        let mut index = SpanIndex::new();
        let id = ConstructId("test_1".to_string());
        let span = ByteSpan::new(10, 30);
        index.register(id.clone(), span);

        assert_eq!(index.find_nl_construct(15), Some(&id));
        assert_eq!(index.find_nl_construct(5), None);
        assert_eq!(index.find_nl_construct(30), None); // half-open
    }

    #[test]
    fn apply_edit_insert() {
        // Insert 5 bytes at position 10 (old_len=0, new_len=5)
        let mut index = SpanIndex::new();
        let id_before = ConstructId("before".to_string());
        let id_after = ConstructId("after".to_string());
        index.register(id_before.clone(), ByteSpan::new(0, 8));
        index.register(id_after.clone(), ByteSpan::new(15, 25));

        index.apply_edit(10, 0, 5);

        // Span before edit point: unchanged
        assert_eq!(index.nl_spans[&id_before], ByteSpan::new(0, 8));
        // Span after edit point: shifted by +5
        assert_eq!(index.nl_spans[&id_after], ByteSpan::new(20, 30));
    }

    #[test]
    fn apply_edit_delete() {
        // Delete 5 bytes at position 10 (old_len=5, new_len=0)
        let mut index = SpanIndex::new();
        let id_after = ConstructId("after".to_string());
        index.register(id_after.clone(), ByteSpan::new(20, 30));

        index.apply_edit(10, 5, 0);

        // Span after edit: shifted by -5
        assert_eq!(index.nl_spans[&id_after], ByteSpan::new(15, 25));
    }

    #[test]
    fn apply_edit_overlapping_span() {
        // Edit overlaps with a span: span 5..20, edit at 10 replacing 3 bytes with 8
        let mut index = SpanIndex::new();
        let id = ConstructId("overlap".to_string());
        index.register(id.clone(), ByteSpan::new(5, 20));

        index.apply_edit(10, 3, 8); // delta = +5

        // Span overlaps the edit, so end gets shifted
        assert_eq!(index.nl_spans[&id].start, 5);
        assert_eq!(index.nl_spans[&id].end, 25);
    }

    #[test]
    fn json_roundtrip() {
        let mut index = SpanIndex::new();
        let id = ConstructId("test_rt".to_string());
        index.register(id.clone(), ByteSpan::new(0, 10));
        index.set_bit_span(&id, ByteSpan::new(0, 20));

        let json = index.to_json();
        let restored = SpanIndex::from_json(&json).expect("should deserialize");
        assert_eq!(restored.nl_spans[&id], ByteSpan::new(0, 10));
        assert_eq!(restored.bit_spans[&id], ByteSpan::new(0, 20));
        assert_eq!(restored.order.len(), 1);
    }

    #[test]
    fn build_from_classified_segments() {
        let result = crate::compile("Define a User with name\n\nUsers can log in", None);
        let idx = &result.span_index;

        assert_eq!(idx.stats().0, 2); // 2 NL spans
        assert_eq!(idx.stats().1, 2); // 2 bit spans
        assert_eq!(idx.order.len(), 2);

        // Every construct should have both NL and bit spans
        for id in &idx.order {
            assert!(idx.nl_spans.contains_key(id));
            assert!(idx.bit_spans.contains_key(id));
        }
    }

    #[test]
    fn remove_construct() {
        let mut index = SpanIndex::new();
        let id = ConstructId("removeme".to_string());
        index.register(id.clone(), ByteSpan::new(0, 10));
        index.set_bit_span(&id, ByteSpan::new(0, 20));

        index.remove(&id);
        assert!(index.nl_spans.is_empty());
        assert!(index.bit_spans.is_empty());
        assert!(index.order.is_empty());
    }

    #[test]
    fn get_spans_pair() {
        let mut index = SpanIndex::new();
        let id = ConstructId("pair".to_string());
        index.register(id.clone(), ByteSpan::new(5, 15));
        index.set_bit_span(&id, ByteSpan::new(0, 30));

        let (nl, bit) = index.get_spans(&id).unwrap();
        assert_eq!(nl, ByteSpan::new(5, 15));
        assert_eq!(bit, Some(ByteSpan::new(0, 30)));
    }

    #[test]
    fn get_spans_nl_only() {
        let mut index = SpanIndex::new();
        let id = ConstructId("nlonly".to_string());
        index.register(id.clone(), ByteSpan::new(0, 10));

        let (nl, bit) = index.get_spans(&id).unwrap();
        assert_eq!(nl, ByteSpan::new(0, 10));
        assert_eq!(bit, None);
    }
}
