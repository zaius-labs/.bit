//! Strongly-typed .bit domain primitives.
//!
//! Newtype wrappers prevent mixing up entity names, gate refs, flow states, etc.
//! at compile time. The .bit surface syntax is unchanged — these types are
//! internal to the Rust implementation.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Entity ────────────────────────────────────────────────────

/// An entity name: @User, @Order, @Deployment
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityName(pub String);

impl EntityName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}

impl From<String> for EntityName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EntityName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ── Entity ID ─────────────────────────────────────────────────

/// An entity instance ID: alice, ord-123, deploy-456
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ":{}", self.0)
    }
}

impl From<String> for EntityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── Entity Key (Name + ID) ───────────────────────────────────

/// Full entity key: @User:alice, @Order:ord-123
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityKey {
    pub entity: EntityName,
    pub id: Option<EntityId>,
}

impl EntityKey {
    pub fn schema(name: impl Into<EntityName>) -> Self {
        Self {
            entity: name.into(),
            id: None,
        }
    }

    pub fn instance(name: impl Into<EntityName>, id: impl Into<EntityId>) -> Self {
        Self {
            entity: name.into(),
            id: Some(id.into()),
        }
    }
}

impl fmt::Display for EntityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.id {
            Some(id) => write!(f, "@{}:{}", self.entity.0, id.0),
            None => write!(f, "@{}", self.entity.0),
        }
    }
}

// ── Flow State ────────────────────────────────────────────────

/// A state in a flow/state machine: :draft, :confirmed, :shipped
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowState(pub String);

impl FlowState {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FlowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ":{}", self.0)
    }
}

impl From<String> for FlowState {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── Flow Name ─────────────────────────────────────────────────

/// A flow identifier: order-lifecycle, deploy-pipeline
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowName(pub String);

impl FlowName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for FlowName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "flow:{}", self.0)
    }
}

// ── Gate Ref ──────────────────────────────────────────────────

/// A gate reference: code-review, payment-check, approval
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GateRef(pub String);

impl GateRef {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}", self.0)
    }
}

impl From<String> for GateRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── Field Name ────────────────────────────────────────────────

/// A field name within an entity: name, status, budget
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FieldName(pub String);

impl FieldName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FieldName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FieldName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ── Workspace Name ────────────────────────────────────────────

/// A workspace reference: @workspace:sales-crm
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceName(pub String);

impl fmt::Display for WorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@workspace:{}", self.0)
    }
}

// ── Mod Name ──────────────────────────────────────────────────

/// A mod reference: $LinearMod, $GoogleWorkspace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModName(pub String);

impl fmt::Display for ModName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

// ── Bound Name ────────────────────────────────────────────────

/// A bound/constraint name: rate-limit, budget-cap
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoundName(pub String);

impl fmt::Display for BoundName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bound:{}", self.0)
    }
}

// ── Transition ────────────────────────────────────────────────

/// A flow transition: from_state --> to_state with optional gate
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub from: FlowState,
    pub to: FlowState,
    pub label: Option<String>,
    pub gate: Option<GateRef>,
}

impl fmt::Display for Transition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} --> {}", self.from, self.to)?;
        if let Some(label) = &self.label {
            write!(f, " ({})", label)?;
        }
        if let Some(gate) = &self.gate {
            write!(f, " gate:{}", gate.0)?;
        }
        Ok(())
    }
}

// ── Cron Schedule ─────────────────────────────────────────────

/// A cron schedule expression: "0 9 * * 1-5", "*/15 * * * *"
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronSchedule(pub String);

impl fmt::Display for CronSchedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_key_display() {
        let key = EntityKey::instance(EntityName::new("Order"), EntityId::new("ord-123"));
        assert_eq!(key.to_string(), "@Order:ord-123");
    }

    #[test]
    fn entity_schema_display() {
        let key = EntityKey::schema(EntityName::new("User"));
        assert_eq!(key.to_string(), "@User");
    }

    #[test]
    fn flow_state_display() {
        let state = FlowState::new("confirmed");
        assert_eq!(state.to_string(), ":confirmed");
    }

    #[test]
    fn gate_ref_display() {
        let gate = GateRef::new("code-review");
        assert_eq!(gate.to_string(), "{code-review}");
    }

    #[test]
    fn transition_display() {
        let t = Transition {
            from: FlowState::new("draft"),
            to: FlowState::new("confirmed"),
            label: Some("payment".into()),
            gate: Some(GateRef::new("payment-check")),
        };
        assert_eq!(
            t.to_string(),
            ":draft --> :confirmed (payment) gate:payment-check"
        );
    }

    #[test]
    fn newtype_prevents_mixup() {
        // This is a compile-time test — if these types were all String,
        // you could accidentally pass an EntityName where a GateRef is expected.
        let entity = EntityName::new("Order");
        let gate = GateRef::new("approval");
        let state = FlowState::new("draft");

        // These are different types — can't mix them up
        assert_ne!(entity.0, gate.0);
        assert_ne!(gate.0, state.0);
    }
}
