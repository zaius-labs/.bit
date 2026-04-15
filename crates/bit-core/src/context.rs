use crate::index::DocIndex;
use serde::{Deserialize, Serialize};

/// Extracts a focused context window for a specific agent, task, or entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub target: String,
    pub tasks: Vec<TaskContext>,
    pub related_refs: Vec<String>,
    pub active_gates: Vec<String>,
    pub variables: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub text: String,
    pub kind: String,
    pub label: Option<String>,
    pub gates: Vec<String>,
    pub group_path: Vec<String>,
}

pub fn extract_for_agent(index: &DocIndex, agent_ref: &str) -> ContextWindow {
    let tasks: Vec<TaskContext> = index
        .tasks
        .iter()
        .filter(|t| {
            t.assignee.as_ref().is_some_and(|a| a.contains(agent_ref)) || t.text.contains(agent_ref)
        })
        .map(|t| TaskContext {
            text: t.text.clone(),
            kind: t.kind.clone(),
            label: t.label.clone(),
            gates: t.gates.clone(),
            group_path: t.group_path.clone(),
        })
        .collect();

    let related_refs: Vec<String> = index
        .refs
        .iter()
        .filter(|r| r.context.contains(agent_ref))
        .map(|r| format!("@{}", r.path.join(":")))
        .collect();

    let active_gates: Vec<String> = tasks.iter().flat_map(|t| t.gates.clone()).collect();

    let variables: Vec<(String, String)> = index
        .variables
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    ContextWindow {
        target: agent_ref.to_string(),
        tasks,
        related_refs,
        active_gates,
        variables,
    }
}

pub fn extract_for_group(index: &DocIndex, group_name: &str) -> ContextWindow {
    let tasks: Vec<TaskContext> = index
        .tasks
        .iter()
        .filter(|t| t.group_path.iter().any(|p| p == group_name))
        .map(|t| TaskContext {
            text: t.text.clone(),
            kind: t.kind.clone(),
            label: t.label.clone(),
            gates: t.gates.clone(),
            group_path: t.group_path.clone(),
        })
        .collect();

    ContextWindow {
        target: group_name.to_string(),
        tasks,
        related_refs: Vec::new(),
        active_gates: Vec::new(),
        variables: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{RefEntry, TaskEntry};

    fn make_index() -> DocIndex {
        let mut idx = DocIndex::default();
        idx.tasks.push(TaskEntry {
            text: "Build API for @agent".to_string(),
            label: Some("A".to_string()),
            kind: "Required".to_string(),
            group_path: vec!["Backend".to_string()],
            assignee: Some("@agent".to_string()),
            gates: vec!["review".to_string()],
        });
        idx.tasks.push(TaskEntry {
            text: "Write frontend".to_string(),
            label: None,
            kind: "Open".to_string(),
            group_path: vec!["Frontend".to_string()],
            assignee: Some("@human".to_string()),
            gates: vec![],
        });
        idx.refs.push(RefEntry {
            path: vec!["Task".to_string(), "t1".to_string()],
            context: "mentions @agent in context".to_string(),
        });
        idx.variables
            .insert("budget".to_string(), "50000".to_string());
        idx
    }

    #[test]
    fn extract_for_agent_finds_assigned_tasks() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "agent");
        assert_eq!(ctx.tasks.len(), 1);
        assert_eq!(ctx.tasks[0].text, "Build API for @agent");
    }

    #[test]
    fn extract_for_agent_finds_refs() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "agent");
        assert_eq!(ctx.related_refs.len(), 1);
        assert!(ctx.related_refs[0].contains("Task"));
    }

    #[test]
    fn extract_for_agent_collects_gates() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "agent");
        assert_eq!(ctx.active_gates, vec!["review"]);
    }

    #[test]
    fn extract_for_agent_collects_variables() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "agent");
        assert!(!ctx.variables.is_empty());
    }

    #[test]
    fn extract_for_agent_no_match() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "nobody");
        assert!(ctx.tasks.is_empty());
        assert!(ctx.related_refs.is_empty());
    }

    #[test]
    fn extract_for_group_finds_tasks() {
        let idx = make_index();
        let ctx = extract_for_group(&idx, "Backend");
        assert_eq!(ctx.tasks.len(), 1);
        assert_eq!(ctx.tasks[0].text, "Build API for @agent");
    }

    #[test]
    fn extract_for_group_no_match() {
        let idx = make_index();
        let ctx = extract_for_group(&idx, "Nonexistent");
        assert!(ctx.tasks.is_empty());
    }

    #[test]
    fn context_window_target_is_set() {
        let idx = make_index();
        let ctx = extract_for_agent(&idx, "agent");
        assert_eq!(ctx.target, "agent");
    }
}
