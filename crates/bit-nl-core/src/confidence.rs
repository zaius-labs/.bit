use crate::segment::SegmentKind;

#[derive(Debug, Clone)]
pub struct ConfidenceComponents {
    pub classifier_prob: f32,
    pub extraction: f32,
    pub entity_resolution: f32,
    pub contradiction: f32,
}

impl ConfidenceComponents {
    pub fn new(classifier_prob: f32) -> Self {
        Self {
            classifier_prob,
            extraction: 1.0,
            entity_resolution: 1.0,
            contradiction: 1.0,
        }
    }
}

/// Weighted geometric mean — zero in any component pulls overall to zero.
pub fn base_confidence(c: &ConfidenceComponents) -> f32 {
    (c.classifier_prob.powf(0.5)
        * c.extraction.powf(0.25)
        * c.entity_resolution.powf(0.15)
        * c.contradiction.powf(0.10))
    .clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceTier {
    High,
    Medium,
    Low,
}

pub fn confidence_tier(conf: f32) -> ConfidenceTier {
    if conf >= 0.85 {
        ConfidenceTier::High
    } else if conf >= 0.50 {
        ConfidenceTier::Medium
    } else {
        ConfidenceTier::Low
    }
}

/// Score extraction completeness for a given segment kind.
/// How many of the required fields/components were found?
pub fn extraction_score(kind: SegmentKind, text: &str) -> f32 {
    match kind {
        SegmentKind::Define => {
            let has_entity_name = text
                .split_whitespace()
                .any(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
            let has_fields =
                text.contains("with") || text.contains("has") || text.contains(":");
            match (has_entity_name, has_fields) {
                (true, true) => 1.0,
                (true, false) => 0.6,
                (false, true) => 0.4,
                (false, false) => 0.2,
            }
        }
        SegmentKind::Task => {
            let has_verb = regex::Regex::new(
                r"(?i)\b(can|should|must|need|implement|build|create|add)\b",
            )
            .unwrap()
            .is_match(text);
            let has_object = text.split_whitespace().count() >= 3;
            match (has_verb, has_object) {
                (true, true) => 1.0,
                (true, false) => 0.7,
                (false, true) => 0.5,
                (false, false) => 0.3,
            }
        }
        SegmentKind::Flow => {
            let has_states =
                text.contains("->") || text.contains("→") || text.contains("then");
            let has_trigger = regex::Regex::new(r"(?i)\b(when|after|before|if)\b")
                .unwrap()
                .is_match(text);
            match (has_states, has_trigger) {
                (true, true) => 1.0,
                (true, false) => 0.7,
                (false, true) => 0.6,
                (false, false) => 0.3,
            }
        }
        _ => 0.5, // Default for other kinds
    }
}

/// Common words that start with uppercase but are NOT entity references.
const NON_ENTITY_WORDS: &[&str] = &[
    "A", "An", "The", "This", "That", "These", "Those", "It", "Its",
    "I", "We", "They", "He", "She", "My", "Our", "Your",
    "Define", "Create", "Add", "Model", "Build", "Implement", "Make",
    "Enable", "Allow", "Support", "Update", "Modify", "Change", "Set",
    "Validate", "Verify", "Check", "Ensure", "Confirm",
    "When", "After", "Before", "Once", "Then", "If", "First", "Next", "Finally",
    "Never", "Always", "Note", "TODO", "Entity", "Users", "System",
    "Requires", "Must", "Should", "Can", "Will", "Need",
    "Send", "Draft", "Review", "Published", "At",
];

/// Check for entity reference resolution.
/// Returns 1.0 if all @References in the text resolve to known entities.
/// Only counts explicit @Refs and PascalCase words that aren't common keywords.
pub fn entity_resolution_score(text: &str, known_entities: &[String]) -> f32 {
    let words: Vec<&str> = text.split_whitespace().collect();
    let refs: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| {
            if w.starts_with('@') {
                return true;
            }
            // Only count capitalized words that aren't common keywords
            if w.len() > 1 && w.chars().next().unwrap().is_uppercase() {
                let clean = w.trim_end_matches(|c: char| !c.is_alphanumeric());
                return !NON_ENTITY_WORDS.iter().any(|nw| nw.eq_ignore_ascii_case(clean));
            }
            false
        })
        .collect();
    if refs.is_empty() {
        return 1.0;
    }
    let resolved = refs
        .iter()
        .filter(|r| {
            let name = r.trim_start_matches('@').trim_end_matches(|c: char| !c.is_alphanumeric());
            known_entities
                .iter()
                .any(|e| e.eq_ignore_ascii_case(name))
        })
        .count();
    resolved as f32 / refs.len() as f32
}

/// Expertise boost from user profile.
pub fn expertise_boost(
    text: &str,
    profile: &crate::profile::UserProfile,
    _kind: SegmentKind,
) -> f32 {
    if profile.expertise_tags.is_empty() {
        return 0.0;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let matching = words
        .iter()
        .filter(|w| {
            profile
                .expertise_tags
                .iter()
                .any(|tag| w.to_lowercase().contains(&tag.to_lowercase()))
        })
        .count();

    let ratio = matching as f32 / words.len().max(1) as f32;
    (ratio - 0.1).clamp(-0.15, 0.15)
}

pub fn final_confidence(base: f32, boost: f32) -> f32 {
    (base + boost).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::UserProfile;

    #[test]
    fn base_confidence_zero_component_yields_zero() {
        let c = ConfidenceComponents {
            classifier_prob: 0.9,
            extraction: 0.0,
            entity_resolution: 0.8,
            contradiction: 1.0,
        };
        assert_eq!(base_confidence(&c), 0.0);

        let c2 = ConfidenceComponents {
            classifier_prob: 0.0,
            extraction: 1.0,
            entity_resolution: 1.0,
            contradiction: 1.0,
        };
        assert_eq!(base_confidence(&c2), 0.0);
    }

    #[test]
    fn base_confidence_all_ones() {
        let c = ConfidenceComponents {
            classifier_prob: 1.0,
            extraction: 1.0,
            entity_resolution: 1.0,
            contradiction: 1.0,
        };
        assert!((base_confidence(&c) - 1.0).abs() < 0.001);
    }

    #[test]
    fn extraction_score_define_with_fields() {
        assert_eq!(extraction_score(SegmentKind::Define, "Define a User with name"), 1.0);
    }

    #[test]
    fn extraction_score_define_without_fields() {
        assert_eq!(extraction_score(SegmentKind::Define, "Create a User"), 0.6);
    }

    #[test]
    fn extraction_score_define_no_entity() {
        assert!((extraction_score(SegmentKind::Define, "something with fields") - 0.4).abs() < 0.001);
    }

    #[test]
    fn entity_resolution_all_resolved() {
        let known = vec!["User".to_string(), "Order".to_string()];
        let score = entity_resolution_score("@User places an @Order", &known);
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn entity_resolution_none_resolved() {
        let known: Vec<String> = vec![];
        let score = entity_resolution_score("@User places an @Order", &known);
        // has refs but none resolve
        assert!(score < 0.5);
    }

    #[test]
    fn entity_resolution_no_refs() {
        let known: Vec<String> = vec![];
        let score = entity_resolution_score("just some plain text here", &known);
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn expertise_boost_matching_tags() {
        let profile = UserProfile::with_expertise(vec!["auth".to_string(), "security".to_string()]);
        let boost = expertise_boost("implement auth security tokens", &profile, SegmentKind::Task);
        assert!(boost > 0.0);
    }

    #[test]
    fn expertise_boost_no_tags() {
        let profile = UserProfile::new();
        let boost = expertise_boost("implement auth", &profile, SegmentKind::Task);
        assert_eq!(boost, 0.0);
    }

    #[test]
    fn expertise_boost_non_matching() {
        let profile = UserProfile::with_expertise(vec!["database".to_string()]);
        let boost = expertise_boost("implement auth security tokens", &profile, SegmentKind::Task);
        assert!(boost < 0.0);
    }

    #[test]
    fn confidence_tier_boundaries() {
        assert_eq!(confidence_tier(0.85), ConfidenceTier::High);
        assert_eq!(confidence_tier(0.90), ConfidenceTier::High);
        assert_eq!(confidence_tier(0.84), ConfidenceTier::Medium);
        assert_eq!(confidence_tier(0.50), ConfidenceTier::Medium);
        assert_eq!(confidence_tier(0.49), ConfidenceTier::Low);
        assert_eq!(confidence_tier(0.0), ConfidenceTier::Low);
    }

    #[test]
    fn final_confidence_clamped() {
        assert_eq!(final_confidence(0.95, 0.15), 1.0);
        assert_eq!(final_confidence(0.1, -0.15), 0.0);
    }
}
