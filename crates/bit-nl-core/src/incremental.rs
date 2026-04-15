pub struct IncrementalState {
    pub last_source: String,
    pub last_segments: Vec<crate::classify::ClassifiedSegment>,
}
