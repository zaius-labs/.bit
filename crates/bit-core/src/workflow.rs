use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowGraph {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdgeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: FlowNodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowNodeKind {
    Task,
    Gate,
    Decision,
    Terminal,
    Escalation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdgeEntry {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub parallel: bool,
    pub gate: Option<String>,
    pub wait: Option<String>,
    pub timeout: Option<String>,
}

pub fn extract_flows(doc: &Document) -> Vec<FlowGraph> {
    let mut graphs = Vec::new();
    extract_flows_from_nodes(&doc.nodes, &mut graphs);
    graphs
}

fn extract_flows_from_nodes(nodes: &[Node], graphs: &mut Vec<FlowGraph>) {
    for node in nodes {
        match node {
            Node::Flow(f) => graphs.push(build_graph(&f.edges)),
            Node::Group(g) => extract_flows_from_nodes(&g.children, graphs),
            Node::Validate(v) => extract_flows_from_nodes(&v.children, graphs),
            _ => {}
        }
    }
}

fn build_graph(edges: &[FlowEdge]) -> FlowGraph {
    let mut node_ids: HashMap<String, FlowNode> = HashMap::new();
    let mut graph_edges = Vec::new();

    for edge in edges {
        for id in edge.from.iter().chain(edge.to.iter()) {
            if !id.is_empty() {
                node_ids.entry(id.clone()).or_insert_with(|| FlowNode {
                    id: id.clone(),
                    kind: classify_node(id, edge),
                });
            }
        }

        for from in &edge.from {
            for to in &edge.to {
                if !from.is_empty() && !to.is_empty() {
                    graph_edges.push(FlowEdgeEntry {
                        from: from.clone(),
                        to: to.clone(),
                        label: edge.label.clone(),
                        parallel: edge.parallel,
                        gate: edge.gate.clone(),
                        wait: edge.wait.clone(),
                        timeout: edge.timeout.clone(),
                    });
                }
            }
        }
    }

    FlowGraph {
        nodes: node_ids.into_values().collect(),
        edges: graph_edges,
    }
}

fn classify_node(id: &str, edge: &FlowEdge) -> FlowNodeKind {
    if id == "PASS" || id == "FAIL" {
        FlowNodeKind::Terminal
    } else if id.starts_with("escalate") {
        FlowNodeKind::Escalation
    } else if id.starts_with('{') {
        FlowNodeKind::Gate
    } else if edge.label.is_some() {
        FlowNodeKind::Decision
    } else {
        FlowNodeKind::Task
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn extract_flows_from_simple_doc() {
        let src = "flow:\n    A --> B --> C";
        let doc = parse::parse(src).expect("parse failed");
        let flows = extract_flows(&doc);
        assert_eq!(flows.len(), 1);
        assert!(!flows[0].edges.is_empty());
    }

    #[test]
    fn extract_flows_from_doc_without_flows() {
        let src = "# Project\n\n    [!] Do something";
        let doc = parse::parse(src).expect("parse failed");
        let flows = extract_flows(&doc);
        assert!(flows.is_empty());
    }

    #[test]
    fn extract_flows_from_nested_validate() {
        let src = "validate pipeline:\n    flow:\n        A --> B";
        let doc = parse::parse(src).expect("parse failed");
        let flows = extract_flows(&doc);
        assert_eq!(flows.len(), 1);
    }

    #[test]
    fn flow_graph_contains_nodes() {
        let edges = vec![FlowEdge {
            from: vec!["start".to_string()],
            to: vec!["end".to_string()],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        }];
        let graph = build_graph(&edges);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn flow_graph_multi_edge() {
        let edges = vec![
            FlowEdge {
                from: vec!["A".to_string()],
                to: vec!["B".to_string()],
                label: None,
                parallel: false,
                gate: None,
                wait: None,
                timeout: None,
            },
            FlowEdge {
                from: vec!["B".to_string()],
                to: vec!["C".to_string()],
                label: None,
                parallel: false,
                gate: None,
                wait: None,
                timeout: None,
            },
        ];
        let graph = build_graph(&edges);
        // A, B, C = 3 nodes
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn flow_graph_parallel_fan_out() {
        let edges = vec![FlowEdge {
            from: vec!["start".to_string()],
            to: vec!["a".to_string(), "b".to_string()],
            label: None,
            parallel: true,
            gate: None,
            wait: None,
            timeout: None,
        }];
        let graph = build_graph(&edges);
        // start, a, b = 3 nodes; start->a, start->b = 2 edges
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn flow_graph_empty_ids_skipped() {
        let edges = vec![FlowEdge {
            from: vec!["".to_string()],
            to: vec!["B".to_string()],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        }];
        let graph = build_graph(&edges);
        // Empty "from" should not produce a node or edge
        assert_eq!(graph.nodes.len(), 1); // only B
        assert_eq!(graph.edges.len(), 0);
    }

    // ── classify_node ──

    #[test]
    fn classify_pass_terminal() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(
            classify_node("PASS", &edge),
            FlowNodeKind::Terminal
        ));
    }

    #[test]
    fn classify_fail_terminal() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(
            classify_node("FAIL", &edge),
            FlowNodeKind::Terminal
        ));
    }

    #[test]
    fn classify_escalation() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(
            classify_node("escalate-to-human", &edge),
            FlowNodeKind::Escalation
        ));
    }

    #[test]
    fn classify_gate() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(
            classify_node("{review}", &edge),
            FlowNodeKind::Gate
        ));
    }

    #[test]
    fn classify_decision() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: Some("yes".to_string()),
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(
            classify_node("check", &edge),
            FlowNodeKind::Decision
        ));
    }

    #[test]
    fn classify_task_default() {
        let edge = FlowEdge {
            from: vec![],
            to: vec![],
            label: None,
            parallel: false,
            gate: None,
            wait: None,
            timeout: None,
        };
        assert!(matches!(classify_node("build", &edge), FlowNodeKind::Task));
    }
}
