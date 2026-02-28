//! Data model of the flow (directed graph of nodes).
//!
//! A flow is represented as a directed acyclic graph (DAG)
//! of nodes connected by edges between their ports.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Z8Error, Z8Result};
use crate::node::Node;

/// Connection between an output port of one node and an input port of another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Unique edge ID.
    pub id: Uuid,
    /// Source node.
    pub from_node: Uuid,
    /// Output port of the source node.
    pub from_port: String,
    /// Target node.
    pub to_node: Uuid,
    /// Input port of the target node.
    pub to_port: String,
}

impl Edge {
    pub fn new(
        from_node: Uuid,
        from_port: impl Into<String>,
        to_node: Uuid,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            from_node,
            from_port: from_port.into(),
            to_node,
            to_port: to_port.into(),
        }
    }
}

/// Execution state of a flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowStatus {
    /// Created but never executed.
    Idle,
    /// Currently running.
    Running,
    /// Paused by user or breakpoint.
    Paused,
    /// Successfully completed.
    Completed,
    /// Stopped with error.
    Error,
    /// Manually stopped by user.
    Stopped,
}

impl std::fmt::Display for FlowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Error => write!(f, "error"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// Visual metadata of the flow (positions, zoom, groupings).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowMeta {
    /// Node positions on canvas { node_id: {x, y} }.
    #[serde(default)]
    pub positions: serde_json::Map<String, serde_json::Value>,
    /// Editor zoom level.
    #[serde(default = "default_zoom")]
    pub zoom: f64,
    /// Editor notes/comments.
    #[serde(default)]
    pub notes: Vec<String>,
}

fn default_zoom() -> f64 {
    1.0
}

/// Global flow configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowConfig {
    /// Environment variables available to nodes.
    #[serde(default)]
    pub env_vars: serde_json::Map<String, serde_json::Value>,
    /// References to credentials stored in the vault.
    #[serde(default)]
    pub credentials: Vec<String>,
    /// Global execution timeout in milliseconds (0 = no limit).
    #[serde(default)]
    pub timeout_ms: u64,
    /// Backpressure buffer size between nodes.
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

fn default_buffer_size() -> usize {
    256
}

/// A flow: directed graph of nodes with their connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Human-readable flow name.
    pub name: String,
    /// Flow description.
    #[serde(default)]
    pub description: String,
    /// Semantic version of the flow.
    pub version: String,
    /// Nodes that compose the flow.
    pub nodes: Vec<Node>,
    /// Connections between ports.
    pub edges: Vec<Edge>,
    /// Visual metadata of the editor.
    #[serde(default)]
    pub metadata: FlowMeta,
    /// Global configuration.
    #[serde(default)]
    pub config: FlowConfig,
    /// Current state.
    #[serde(default = "default_status")]
    pub status: FlowStatus,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modification timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn default_status() -> FlowStatus {
    FlowStatus::Idle
}

impl Flow {
    /// Creates an empty flow.
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::now_v7(),
            name: name.into(),
            description: String::new(),
            version: "0.1.0".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: FlowMeta::default(),
            config: FlowConfig::default(),
            status: FlowStatus::Idle,
            created_at: now,
            updated_at: now,
        }
    }

    /// Adds a node to the flow.
    pub fn add_node(&mut self, node: Node) {
        self.nodes.push(node);
        self.updated_at = chrono::Utc::now();
    }

    /// Connects an output port of one node with an input port of another.
    pub fn connect(
        &mut self,
        from_node: Uuid,
        from_port: impl Into<String>,
        to_node: Uuid,
        to_port: impl Into<String>,
    ) -> Z8Result<&Edge> {
        let from_port = from_port.into();
        let to_port = to_port.into();

        // Validate that nodes exist
        let from = self.find_node(from_node).ok_or(Z8Error::InvalidEdge {
            from: from_node,
            to: to_node,
        })?;
        let to = self.find_node(to_node).ok_or(Z8Error::InvalidEdge {
            from: from_node,
            to: to_node,
        })?;

        // Validate that ports exist
        let out_port = from.find_output(&from_port).ok_or(Z8Error::PortNotFound {
            node_id: from_node,
            port: from_port.clone(),
        })?;
        let in_port = to.find_input(&to_port).ok_or(Z8Error::PortNotFound {
            node_id: to_node,
            port: to_port.clone(),
        })?;

        // Validate type compatibility
        if !out_port.port_type.is_compatible_with(&in_port.port_type) {
            return Err(Z8Error::TypeMismatch {
                from_port: from_port.clone(),
                from_type: out_port.port_type.to_string(),
                to_port: to_port.clone(),
                to_type: in_port.port_type.to_string(),
            });
        }

        let edge = Edge::new(from_node, from_port, to_node, to_port);
        self.edges.push(edge);
        self.updated_at = chrono::Utc::now();
        Ok(self.edges.last().unwrap())
    }

    /// Finds a node by ID.
    pub fn find_node(&self, id: Uuid) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Finds a mutable node by ID.
    pub fn find_node_mut(&mut self, id: Uuid) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Returns nodes that have no incoming edges (root/trigger nodes).
    pub fn root_nodes(&self) -> Vec<&Node> {
        let targets: std::collections::HashSet<Uuid> =
            self.edges.iter().map(|e| e.to_node).collect();
        self.nodes
            .iter()
            .filter(|n| n.enabled && !targets.contains(&n.id))
            .collect()
    }

    /// Returns edges that leave from a specific node.
    pub fn outgoing_edges(&self, node_id: Uuid) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == node_id)
            .collect()
    }

    /// Returns edges that enter a specific node.
    pub fn incoming_edges(&self, node_id: Uuid) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| e.to_node == node_id)
            .collect()
    }

    /// Validates that the flow contains no cycles (is a DAG).
    pub fn validate_acyclic(&self) -> Z8Result<()> {
        let node_ids: std::collections::HashSet<Uuid> =
            self.nodes.iter().map(|n| n.id).collect();

        // Kahn's algorithm for cycle detection
        let mut in_degree: std::collections::HashMap<Uuid, usize> =
            node_ids.iter().map(|&id| (id, 0)).collect();

        for edge in &self.edges {
            *in_degree.entry(edge.to_node).or_insert(0) += 1;
        }

        let mut queue: std::collections::VecDeque<Uuid> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut visited = 0;

        while let Some(node_id) = queue.pop_front() {
            visited += 1;
            for edge in self.outgoing_edges(node_id) {
                if let Some(deg) = in_degree.get_mut(&edge.to_node) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(edge.to_node);
                    }
                }
            }
        }

        if visited != node_ids.len() {
            return Err(Z8Error::CycleDetected(self.id));
        }

        Ok(())
    }

    /// Generates the topological execution order.
    pub fn topological_order(&self) -> Z8Result<Vec<Uuid>> {
        self.validate_acyclic()?;

        let mut in_degree: std::collections::HashMap<Uuid, usize> =
            self.nodes.iter().map(|n| (n.id, 0)).collect();

        for edge in &self.edges {
            *in_degree.entry(edge.to_node).or_insert(0) += 1;
        }

        let mut queue: std::collections::VecDeque<Uuid> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut order = Vec::with_capacity(self.nodes.len());

        while let Some(node_id) = queue.pop_front() {
            order.push(node_id);
            for edge in self.outgoing_edges(node_id) {
                if let Some(deg) = in_degree.get_mut(&edge.to_node) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(edge.to_node);
                    }
                }
            }
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::PortType;

    fn create_test_flow() -> Flow {
        let mut flow = Flow::new("Test Flow");

        let trigger = Node::new("HTTP Trigger", "http-trigger")
            .with_output("response", PortType::Object);
        let process = Node::new("JSON Parser", "json-parse")
            .with_input("input", PortType::Object)
            .with_output("parsed", PortType::Object);
        let debug = Node::new("Debug", "debug")
            .with_input("input", PortType::Any);

        let trigger_id = trigger.id;
        let process_id = process.id;
        let debug_id = debug.id;

        flow.add_node(trigger);
        flow.add_node(process);
        flow.add_node(debug);

        flow.connect(trigger_id, "response", process_id, "input").unwrap();
        flow.connect(process_id, "parsed", debug_id, "input").unwrap();

        flow
    }

    #[test]
    fn test_flow_creation() {
        let flow = Flow::new("My Flow");
        assert_eq!(flow.name, "My Flow");
        assert_eq!(flow.status, FlowStatus::Idle);
        assert!(flow.nodes.is_empty());
    }

    #[test]
    fn test_topological_order() {
        let flow = create_test_flow();
        let order = flow.topological_order().unwrap();
        assert_eq!(order.len(), 3);
        // The trigger must be first
        assert_eq!(order[0], flow.nodes[0].id);
    }

    #[test]
    fn test_root_nodes() {
        let flow = create_test_flow();
        let roots = flow.root_nodes();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].node_type.0, "http-trigger");
    }

    #[test]
    fn test_type_mismatch() {
        let mut flow = Flow::new("Bad Flow");
        let a = Node::new("A", "a").with_output("out", PortType::String);
        let b = Node::new("B", "b").with_input("in", PortType::Number);
        let a_id = a.id;
        let b_id = b.id;
        flow.add_node(a);
        flow.add_node(b);

        let result = flow.connect(a_id, "out", b_id, "in");
        assert!(result.is_err());
    }
}
