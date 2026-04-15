use serde::{Serialize, Deserialize};

/// Events written to the annotation sidecar (NDJSON, one per line).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AnnotationEvent {
    ImplComplete {
        construct_id: String,
        file: String,
        function: Option<String>,
        start_line: u32,
        end_line: u32,
        language: String,
        timestamp: String,
        validation: ValidationOutcome,
    },
    ImplFailed {
        construct_id: String,
        error: String,
        timestamp: String,
    },
    ImplSkipped {
        construct_id: String,
        reason: String,
        timestamp: String,
    },
    GateFailed {
        construct_id: String,
        gate: String,
        reason: String,
        timestamp: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcome {
    Passed,
    Failed,
    NotRun,
}

impl AnnotationEvent {
    pub fn to_ndjson(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn construct_id(&self) -> &str {
        match self {
            Self::ImplComplete { construct_id, .. } => construct_id,
            Self::ImplFailed { construct_id, .. } => construct_id,
            Self::ImplSkipped { construct_id, .. } => construct_id,
            Self::GateFailed { construct_id, .. } => construct_id,
        }
    }
}

/// Read all annotation events from an NDJSON file.
pub fn read_annotation_file(path: &std::path::Path) -> Vec<AnnotationEvent> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Append one annotation event to an NDJSON file.
pub fn append_annotation(path: &std::path::Path, event: &AnnotationEvent) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", event.to_ndjson())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impl_complete_serialization() {
        let event = AnnotationEvent::ImplComplete {
            construct_id: "define_user_abc123".to_string(),
            file: "src/models/user.rs".to_string(),
            function: Some("create_user".to_string()),
            start_line: 12,
            end_line: 18,
            language: "rust".to_string(),
            timestamp: "2024-01-15T10:35:00Z".to_string(),
            validation: ValidationOutcome::Passed,
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"impl_complete\""));
        assert!(json.contains("\"construct_id\":\"define_user_abc123\""));
        assert!(json.contains("\"validation\":\"passed\""));
    }

    #[test]
    fn test_impl_failed_serialization() {
        let event = AnnotationEvent::ImplFailed {
            construct_id: "task_login".to_string(),
            error: "EmailService not defined".to_string(),
            timestamp: "2024-01-15T10:35:01Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"impl_failed\""));
        assert!(json.contains("EmailService"));
    }

    #[test]
    fn test_impl_skipped_serialization() {
        let event = AnnotationEvent::ImplSkipped {
            construct_id: "gate_stub".to_string(),
            reason: "confidence below threshold".to_string(),
            timestamp: "2024-01-15T10:35:02Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"impl_skipped\""));
    }

    #[test]
    fn test_gate_failed_serialization() {
        let event = AnnotationEvent::GateFailed {
            construct_id: "flow_reg".to_string(),
            gate: "require(@User.email)".to_string(),
            reason: "@User has no email field".to_string(),
            timestamp: "2024-01-15T10:35:03Z".to_string(),
        };
        let json = event.to_ndjson();
        assert!(json.contains("\"event\":\"gate_failed\""));
    }

    #[test]
    fn test_construct_id_accessor() {
        let event = AnnotationEvent::ImplFailed {
            construct_id: "test_construct".to_string(),
            error: "err".to_string(),
            timestamp: "now".to_string(),
        };
        assert_eq!(event.construct_id(), "test_construct");
    }

    #[test]
    fn test_read_annotation_file_missing() {
        let path = std::path::Path::new("/nonexistent/file.ndjson");
        let events = read_annotation_file(path);
        assert!(events.is_empty());
    }

    #[test]
    fn test_read_annotation_file_parses_valid_lines() {
        // Write a temp file
        let dir = std::env::temp_dir();
        let path = dir.join("test_annotations.ndjson");
        let event = AnnotationEvent::ImplFailed {
            construct_id: "test".to_string(),
            error: "err".to_string(),
            timestamp: "2024-01-01".to_string(),
        };
        std::fs::write(&path, format!("{}\n", event.to_ndjson())).unwrap();
        let events = read_annotation_file(&path);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].construct_id(), "test");
        std::fs::remove_file(&path).ok();
    }
}
