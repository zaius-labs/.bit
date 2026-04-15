//! Typed error hierarchy for the .bit language stack.
//!
//! Replaces string-based errors with structured enums that carry
//! exact context about what went wrong and where.

use crate::bit_types::*;
use std::fmt;

use crate::ir::ConstructKind;

/// Top-level error that can occur anywhere in the .bit pipeline.
#[derive(Debug, Clone)]
pub enum BitError {
    Parse(ParseError),
    IR(IRValidationError),
    Exec(ExecError),
}

impl fmt::Display for BitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BitError::Parse(e) => write!(f, "parse: {}", e),
            BitError::IR(e) => write!(f, "ir: {}", e),
            BitError::Exec(e) => write!(f, "exec: {}", e),
        }
    }
}

// ── Parse Errors ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub line: usize,
    pub col: usize,
    pub context: String,
}

#[derive(Debug, Clone)]
pub enum ParseErrorKind {
    UnexpectedToken { expected: String, found: String },
    InvalidIndentation { expected: usize, found: usize },
    UnclosedCodeBlock,
    InvalidFieldSyntax,
    UnknownConstruct { keyword: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {:?}", self.line, self.kind)
    }
}

// ── IR Validation Errors ──────────────────────────────────────

#[derive(Debug, Clone)]
pub enum IRValidationError {
    /// @Entity reference to undefined schema.
    UnresolvedEntity {
        name: EntityName,
        referenced_in: ConstructKind,
        line: usize,
    },

    /// Gate referenced in flow doesn't exist.
    UnresolvedGate {
        gate: GateRef,
        referenced_in: Option<FlowName>,
        line: usize,
    },

    /// Duplicate define:@Entity.
    DuplicateSchema {
        name: EntityName,
        first_line: usize,
        second_line: usize,
    },

    /// Mutation sets a state that isn't reachable from current state.
    InvalidTransition {
        entity: EntityName,
        from: FlowState,
        to: FlowState,
        flow: Option<FlowName>,
        line: usize,
    },

    /// Parallel spawn contains mutations (data race).
    ParallelMutation {
        spawn_line: usize,
        mutate_target: EntityName,
        mutate_line: usize,
    },

    /// Use/import with empty source.
    InvalidImport { line: usize, detail: String },

    /// Field type mismatch against schema.
    TypeMismatch {
        entity: EntityName,
        field: FieldName,
        expected: String,
        found: String,
        line: usize,
    },

    /// Required field missing in mutation.
    MissingRequiredField {
        entity: EntityName,
        field: FieldName,
        line: usize,
    },
}

impl fmt::Display for IRValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedEntity { name, referenced_in: _, line } =>
                write!(f, "line {}: references undefined entity {}", line, name),
            Self::UnresolvedGate { gate, referenced_in: _, line } =>
                write!(f, "line {}: references undefined gate {}", line, gate),
            Self::DuplicateSchema { name, first_line, second_line } =>
                write!(f, "line {}: duplicate define:{} (first at line {})", second_line, name, first_line),
            Self::InvalidTransition { entity, from, to, flow: _, line } =>
                write!(f, "line {}: invalid transition {} → {} for {}", line, from, to, entity),
            Self::ParallelMutation { spawn_line, mutate_target, mutate_line } =>
                write!(f, "line {}: parallel spawn (+) contains mutation of {} at line {} — use sequential (++) for writes", spawn_line, mutate_target, mutate_line),
            Self::InvalidImport { line, detail } =>
                write!(f, "line {}: invalid import: {}", line, detail),
            Self::TypeMismatch { entity, field, expected, found, line } =>
                write!(f, "line {}: {}.{}: expected {}, found {}", line, entity, field, expected, found),
            Self::MissingRequiredField { entity, field, line } =>
                write!(f, "line {}: {}.{} is required but missing", line, entity, field),
        }
    }
}

// ── Execution Errors ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ExecError {
    /// Flow has no valid transition from current state.
    NoTransition {
        flow: Option<FlowName>,
        current_state: FlowState,
    },

    /// Gate evaluation failed.
    GateFailed {
        gate: GateRef,
        missing_conditions: Vec<String>,
    },

    /// Bound constraint violated.
    BoundViolation {
        bound: BoundName,
        field: String,
        limit: String,
        actual: String,
    },

    /// Webhook dispatch failed.
    WebhookFailed { url: String, error: String },

    /// Conditional evaluation error.
    ConditionError { condition: String, error: String },

    /// Entity not found for mutation/deletion.
    EntityNotFound { key: EntityKey },
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTransition {
                flow,
                current_state,
            } => write!(f, "no transition from {} in {:?}", current_state, flow),
            Self::GateFailed {
                gate,
                missing_conditions,
            } => write!(
                f,
                "gate {} failed: missing {}",
                gate,
                missing_conditions.join(", ")
            ),
            Self::BoundViolation {
                bound,
                field,
                limit,
                actual,
            } => write!(
                f,
                "bound {} violated: {}.{} (limit {}, actual {})",
                bound, field, field, limit, actual
            ),
            Self::WebhookFailed { url, error } => write!(f, "webhook {} failed: {}", url, error),
            Self::ConditionError { condition, error } => {
                write!(f, "condition '{}' error: {}", condition, error)
            }
            Self::EntityNotFound { key } => write!(f, "entity {} not found", key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = IRValidationError::UnresolvedEntity {
            name: EntityName::new("Order"),
            referenced_in: ConstructKind::Mutate,
            line: 15,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("@Order"));
        assert!(msg.contains("15"));
    }

    #[test]
    fn parallel_mutation_error() {
        let err = IRValidationError::ParallelMutation {
            spawn_line: 10,
            mutate_target: EntityName::new("Invoice"),
            mutate_line: 12,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("parallel"));
        assert!(msg.contains("@Invoice"));
        assert!(msg.contains("sequential"));
    }

    #[test]
    fn invalid_transition_error() {
        let err = IRValidationError::InvalidTransition {
            entity: EntityName::new("Ticket"),
            from: FlowState::new("draft"),
            to: FlowState::new("shipped"),
            flow: Some(FlowName::new("ticket-lifecycle")),
            line: 20,
        };
        let msg = format!("{}", err);
        assert!(msg.contains(":draft"));
        assert!(msg.contains(":shipped"));
        assert!(msg.contains("@Ticket"));
    }
}
