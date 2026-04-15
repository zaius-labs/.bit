use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Mul;

/// Epistemic state — implicit qualifier on any parsed value.
/// Tracked by the runtime; never surfaced to .bit generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EpistemicState {
    Known,   // value present and resolved
    Unknown, // syntactically present but unresolved
    Invalid, // parse error or schema violation
}

impl EpistemicState {
    /// Dominance-order (severity-ranked) propagation over a slice of states.
    ///
    /// Ordering: Invalid > Unknown > Known — the most severe state wins.
    /// This is NOT a fold over kleene_and; it is a dominance-order max.
    /// kleene_and(Known, Unknown) == Unknown, but so does this — they agree
    /// on the AND truth table, but the semantics are distinct: this function
    /// answers "what is the worst epistemic state present?" not "what is the
    /// logical conjunction?".
    ///
    /// Empty slice returns Known (vacuous truth).
    pub fn propagate(states: &[EpistemicState]) -> EpistemicState {
        let mut result = EpistemicState::Known;
        for &s in states {
            result = match (result, s) {
                (_, EpistemicState::Invalid) | (EpistemicState::Invalid, _) => {
                    EpistemicState::Invalid
                }
                (_, EpistemicState::Unknown) | (EpistemicState::Unknown, _) => {
                    EpistemicState::Unknown
                }
                _ => EpistemicState::Known,
            };
            if result == EpistemicState::Invalid {
                break;
            }
        }
        result
    }

    pub fn kleene_and(a: EpistemicState, b: EpistemicState) -> EpistemicState {
        use EpistemicState::*;
        match (a, b) {
            (Invalid, _) | (_, Invalid) => Invalid,
            (Unknown, _) | (_, Unknown) => Unknown,
            (Known, Known) => Known,
        }
    }

    pub fn kleene_or(a: EpistemicState, b: EpistemicState) -> EpistemicState {
        use EpistemicState::*;
        match (a, b) {
            (Known, _) | (_, Known) => Known,
            (Unknown, _) | (_, Unknown) => Unknown,
            (Invalid, Invalid) => Invalid,
        }
    }

    pub fn kleene_not(a: EpistemicState) -> EpistemicState {
        use EpistemicState::*;
        match a {
            Known => Invalid,
            Unknown => Unknown,
            Invalid => Known,
        }
    }
}

/// Domain trit — for genuinely triadic domain fields.
/// NOT for epistemic uncertainty. Use EpistemicState for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trit {
    Pos,     // +1: passed, added, advance
    Neutral, // 0:  pending, unchanged, hold
    Neg,     // -1: blocked, removed, retreat
}

impl Trit {
    pub fn from_i8(v: i8) -> Option<Trit> {
        match v {
            1 => Some(Trit::Pos),
            0 => Some(Trit::Neutral),
            -1 => Some(Trit::Neg),
            _ => None,
        }
    }
    pub fn to_i8(self) -> i8 {
        match self {
            Trit::Pos => 1,
            Trit::Neutral => 0,
            Trit::Neg => -1,
        }
    }
}

impl Mul for Trit {
    type Output = Trit;
    fn mul(self, rhs: Trit) -> Trit {
        match (self, rhs) {
            (Trit::Neutral, _) | (_, Trit::Neutral) => Trit::Neutral,
            (Trit::Pos, Trit::Pos) | (Trit::Neg, Trit::Neg) => Trit::Pos,
            _ => Trit::Neg,
        }
    }
}

/// Compute ternary diff between two JSON field maps.
///
/// Returns `field_name → i8` with three possible values:
/// - `+1` — field was **added** (present only in `after`) or **changed** (present in both but value
///   differs). Both cases map to `+1` because both signal forward movement in the training pressure
///   signal. A caller cannot reconstruct the exact mutation (add vs. change) from this output, and
///   that is intentional — the distinction doesn't matter for scheduling pressure.
/// - ` 0` — field is **unchanged** (same key and value in both `before` and `after`).
/// - `-1` — field was **removed** (present only in `before`).
///
/// **Non-object inputs:** if `before` or `after` is not a JSON object (e.g. an array, string, or
/// null), it is treated as an empty object. Fields in `after` that have no counterpart in an
/// empty-object `before` become `+1`; fields in `before` that have no counterpart in an
/// empty-object `after` become `-1`.
pub fn bit_diff(before: &Value, after: &Value) -> HashMap<String, i8> {
    let before_obj = before.as_object().cloned().unwrap_or_default();
    let after_obj = after.as_object().cloned().unwrap_or_default();
    let mut result = HashMap::new();

    for (key, bv) in &before_obj {
        match after_obj.get(key) {
            None => {
                result.insert(key.clone(), -1i8);
            } // removed
            Some(av) if av == bv => {
                result.insert(key.clone(), 0i8);
            } // unchanged
            Some(_) => {
                result.insert(key.clone(), 1i8);
            } // changed
        }
    }
    for key in after_obj.keys() {
        if !before_obj.contains_key(key) {
            result.insert(key.clone(), 1i8); // new field
        }
    }
    result
}

/// Sequential diff composition: last nonzero wins, Neutral is identity.
pub fn compose_sequential(a: &[Trit], b: &[Trit]) -> Vec<Trit> {
    assert_eq!(a.len(), b.len(), "diff vectors must have same length");
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| match bi {
            Trit::Neutral => ai,
            _ => bi,
        })
        .collect()
}

/// Concurrent diff composition: agree = keep, conflict = Neutral (unknown).
pub fn compose_concurrent(a: &[Trit], b: &[Trit]) -> Vec<Trit> {
    assert_eq!(a.len(), b.len(), "diff vectors must have same length");
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| {
            if ai == bi {
                ai
            } else if ai == Trit::Neutral {
                bi
            } else if bi == Trit::Neutral {
                ai
            } else {
                Trit::Neutral
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epistemic_propagate_known_known() {
        let states = vec![EpistemicState::Known, EpistemicState::Known];
        assert_eq!(EpistemicState::propagate(&states), EpistemicState::Known);
    }
    #[test]
    fn test_epistemic_propagate_known_unknown() {
        let states = vec![EpistemicState::Known, EpistemicState::Unknown];
        assert_eq!(EpistemicState::propagate(&states), EpistemicState::Unknown);
    }
    #[test]
    fn test_epistemic_propagate_any_invalid() {
        let states = vec![EpistemicState::Known, EpistemicState::Invalid];
        assert_eq!(EpistemicState::propagate(&states), EpistemicState::Invalid);
    }
    #[test]
    fn test_epistemic_invalid_beats_unknown() {
        let states = vec![EpistemicState::Unknown, EpistemicState::Invalid];
        assert_eq!(EpistemicState::propagate(&states), EpistemicState::Invalid);
    }
    #[test]
    fn test_epistemic_propagate_empty_slice() {
        // Empty slice returns Known (vacuous truth).
        assert_eq!(EpistemicState::propagate(&[]), EpistemicState::Known);
    }
    #[test]
    fn test_kleene_and_truth_table() {
        use EpistemicState::{Invalid as I, Known as K, Unknown as U};
        assert_eq!(EpistemicState::kleene_and(K, K), K);
        assert_eq!(EpistemicState::kleene_and(K, U), U);
        assert_eq!(EpistemicState::kleene_and(K, I), I);
        assert_eq!(EpistemicState::kleene_and(U, U), U);
        assert_eq!(EpistemicState::kleene_and(U, I), I);
        assert_eq!(EpistemicState::kleene_and(I, I), I);
    }
    #[test]
    fn test_kleene_or_truth_table() {
        use EpistemicState::{Invalid as I, Known as K, Unknown as U};
        assert_eq!(EpistemicState::kleene_or(K, K), K);
        assert_eq!(EpistemicState::kleene_or(K, U), K);
        assert_eq!(EpistemicState::kleene_or(K, I), K);
        assert_eq!(EpistemicState::kleene_or(U, U), U);
        assert_eq!(EpistemicState::kleene_or(U, I), U);
        assert_eq!(EpistemicState::kleene_or(I, I), I);
    }
    #[test]
    fn test_kleene_not() {
        assert_eq!(
            EpistemicState::kleene_not(EpistemicState::Known),
            EpistemicState::Invalid
        );
        assert_eq!(
            EpistemicState::kleene_not(EpistemicState::Unknown),
            EpistemicState::Unknown
        );
        assert_eq!(
            EpistemicState::kleene_not(EpistemicState::Invalid),
            EpistemicState::Known
        );
    }
    #[test]
    fn test_trit_from_i8() {
        assert_eq!(Trit::from_i8(1), Some(Trit::Pos));
        assert_eq!(Trit::from_i8(0), Some(Trit::Neutral));
        assert_eq!(Trit::from_i8(-1), Some(Trit::Neg));
        assert_eq!(Trit::from_i8(2), None);
    }
    #[test]
    fn test_trit_multiply() {
        assert_eq!(Trit::Pos * Trit::Pos, Trit::Pos);
        assert_eq!(Trit::Pos * Trit::Neg, Trit::Neg);
        assert_eq!(Trit::Neg * Trit::Neg, Trit::Pos);
        assert_eq!(Trit::Neutral * Trit::Pos, Trit::Neutral);
    }
    #[test]
    fn test_trit_multiply_neg_pos() {
        assert_eq!(Trit::Neg * Trit::Pos, Trit::Neg);
    }
    #[test]
    fn test_diff_sequential_compose_identity() {
        let a = vec![Trit::Neutral, Trit::Pos, Trit::Neg];
        let b = vec![Trit::Pos, Trit::Neutral, Trit::Neutral];
        let result = compose_sequential(&a, &b);
        assert_eq!(result, vec![Trit::Pos, Trit::Pos, Trit::Neg]);
    }
    #[test]
    fn test_diff_sequential_both_neutral() {
        // Both Neutral: Neutral is identity, so result is Neutral.
        let a = vec![Trit::Neutral, Trit::Neutral];
        let b = vec![Trit::Neutral, Trit::Neutral];
        let result = compose_sequential(&a, &b);
        assert_eq!(result, vec![Trit::Neutral, Trit::Neutral]);
    }
    #[test]
    fn test_diff_concurrent_conflict() {
        let a = vec![Trit::Pos];
        let b = vec![Trit::Neg];
        let result = compose_concurrent(&a, &b);
        assert_eq!(result, vec![Trit::Neutral]);
    }
    #[test]
    fn test_diff_concurrent_agree() {
        let a = vec![Trit::Pos, Trit::Neg];
        let b = vec![Trit::Pos, Trit::Neg];
        let result = compose_concurrent(&a, &b);
        assert_eq!(result, vec![Trit::Pos, Trit::Neg]);
    }
    #[test]
    fn test_diff_concurrent_one_sided_neutral() {
        // [Pos] vs [Neutral]: Neutral defers to the other side, result is Pos.
        let a = vec![Trit::Pos];
        let b = vec![Trit::Neutral];
        let result = compose_concurrent(&a, &b);
        assert_eq!(result, vec![Trit::Pos]);
    }

    // --- bit_diff tests ---

    #[test]
    fn test_bit_diff_added_field() {
        // Field absent in before, present in after → +1
        let before = serde_json::json!({}); // "status" not present at all
        let after = serde_json::json!({ "status": "active" });
        let diff = bit_diff(&before, &after);
        assert_eq!(diff.get("status").copied().unwrap(), 1i8);
    }

    #[test]
    fn test_bit_diff_removed_field() {
        let before = serde_json::json!({ "status": "active" });
        let after = serde_json::json!({});
        let diff = bit_diff(&before, &after);
        assert_eq!(diff.get("status").copied().unwrap(), -1i8);
    }

    #[test]
    fn test_bit_diff_unchanged_field() {
        let before = serde_json::json!({ "status": "active" });
        let after = serde_json::json!({ "status": "active" });
        let diff = bit_diff(&before, &after);
        assert_eq!(diff.get("status").copied().unwrap(), 0i8);
    }

    #[test]
    fn test_bit_diff_changed_field() {
        let before = serde_json::json!({ "status": "active" });
        let after = serde_json::json!({ "status": "done" });
        let diff = bit_diff(&before, &after);
        assert_eq!(diff.get("status").copied().unwrap(), 1i8);
    }

    #[test]
    fn test_bit_diff_multi_field_mixed() {
        // Simultaneously: unchanged (a), changed (b), removed (c), added (d)
        let before = serde_json::json!({ "a": 1, "b": 2, "c": 3 });
        let after = serde_json::json!({ "a": 1, "b": 99, "d": 4 });
        let diff = bit_diff(&before, &after);
        assert_eq!(
            diff.get("a").copied().unwrap(),
            0i8,
            "unchanged field should be 0"
        );
        assert_eq!(
            diff.get("b").copied().unwrap(),
            1i8,
            "changed field should be +1"
        );
        assert_eq!(
            diff.get("c").copied().unwrap(),
            -1i8,
            "removed field should be -1"
        );
        assert_eq!(
            diff.get("d").copied().unwrap(),
            1i8,
            "added field should be +1"
        );
        assert_eq!(diff.len(), 4, "should have exactly 4 entries");
    }

    #[test]
    fn test_bit_diff_non_object_before_treated_as_empty() {
        // Non-object before (array, string, null) is treated as empty object — all after fields are +1
        let before = serde_json::json!("not-an-object");
        let after = serde_json::json!({ "status": "active" });
        let diff = bit_diff(&before, &after);
        assert_eq!(diff.get("status").copied().unwrap(), 1i8);
        // and nothing from before to remove
        assert_eq!(diff.len(), 1);
    }
}
