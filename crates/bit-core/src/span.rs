//! Byte-offset spans for source mapping.
//!
//! `ByteSpan` tracks the half-open byte range `[start, end)` into the original
//! source text. Used by the NL->bit compiler and (future) LSP to map generated
//! constructs back to their origin.

use serde::{Deserialize, Serialize};

/// A half-open byte range `[start, end)` into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: u32,
    pub end: u32,
}

impl ByteSpan {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    /// A zero-width span at the given offset.
    pub fn empty(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Length in bytes.
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Whether this span is zero-width.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Whether the given byte offset falls within this span.
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Whether two spans overlap (share at least one byte).
    pub fn overlaps(&self, other: &ByteSpan) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Merge two spans into the smallest span covering both.
    pub fn merge(&self, other: &ByteSpan) -> ByteSpan {
        ByteSpan {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl Default for ByteSpan {
    fn default() -> Self {
        Self { start: 0, end: 0 }
    }
}

impl std::fmt::Display for ByteSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_accessors() {
        let span = ByteSpan::new(10, 20);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn empty_span() {
        let span = ByteSpan::empty(5);
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert_eq!(span.start, 5);
        assert_eq!(span.end, 5);
    }

    #[test]
    fn contains() {
        let span = ByteSpan::new(10, 20);
        assert!(!span.contains(9));
        assert!(span.contains(10));
        assert!(span.contains(15));
        assert!(span.contains(19));
        assert!(!span.contains(20)); // half-open: end is exclusive
    }

    #[test]
    fn contains_empty_span() {
        let span = ByteSpan::empty(10);
        assert!(!span.contains(10)); // empty span contains nothing
    }

    #[test]
    fn overlaps() {
        let a = ByteSpan::new(10, 20);
        let b = ByteSpan::new(15, 25);
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));

        let c = ByteSpan::new(20, 30);
        assert!(!a.overlaps(&c)); // adjacent, not overlapping
        assert!(!c.overlaps(&a));

        let d = ByteSpan::new(5, 11);
        assert!(a.overlaps(&d));
    }

    #[test]
    fn overlaps_empty() {
        // A zero-width span at a point inside another span still satisfies
        // the half-open interval overlap check (start < other.end && other.start < end).
        let a = ByteSpan::new(10, 20);
        let inside = ByteSpan::empty(15);
        assert!(a.overlaps(&inside));
        assert!(inside.overlaps(&a));

        // But an empty span at the boundary does NOT overlap (half-open).
        let at_end = ByteSpan::empty(20);
        assert!(!a.overlaps(&at_end));
        assert!(!at_end.overlaps(&a));

        // Two empty spans at the same point don't overlap.
        let e1 = ByteSpan::empty(10);
        let e2 = ByteSpan::empty(10);
        assert!(!e1.overlaps(&e2));
    }

    #[test]
    fn merge() {
        let a = ByteSpan::new(10, 20);
        let b = ByteSpan::new(15, 30);
        let merged = a.merge(&b);
        assert_eq!(merged, ByteSpan::new(10, 30));
    }

    #[test]
    fn default_is_empty_at_zero() {
        let span = ByteSpan::default();
        assert_eq!(span, ByteSpan::empty(0));
        assert!(span.is_empty());
    }

    #[test]
    fn display() {
        let span = ByteSpan::new(42, 100);
        assert_eq!(format!("{}", span), "42..100");
    }

    #[test]
    fn serde_roundtrip() {
        let span = ByteSpan::new(10, 20);
        let json = serde_json::to_string(&span).unwrap();
        let parsed: ByteSpan = serde_json::from_str(&json).unwrap();
        assert_eq!(span, parsed);
    }

    #[test]
    fn serde_option_none_roundtrip() {
        let opt: Option<ByteSpan> = None;
        let json = serde_json::to_string(&opt).unwrap();
        assert_eq!(json, "null");
        let parsed: Option<ByteSpan> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, None);
    }

    #[test]
    fn serde_option_some_roundtrip() {
        let opt: Option<ByteSpan> = Some(ByteSpan::new(5, 15));
        let json = serde_json::to_string(&opt).unwrap();
        let parsed: Option<ByteSpan> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, opt);
    }

    #[test]
    fn serde_default_deserialization() {
        // Simulates old JSON that doesn't have a span field.
        // With #[serde(default)] on the struct field, missing keys deserialize as None.
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default)]
            span: Option<ByteSpan>,
        }
        let parsed: Wrapper = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.span, None);
    }
}
