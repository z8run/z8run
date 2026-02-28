//! Data model for flow nodes.
//!
//! Each node defines its behavior, input/output ports
//! with specific types, and its particular configuration.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Data type transported by a port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortType {
    /// Any type (accepts everything).
    Any,
    /// Plain text.
    String,
    /// Number (f64).
    Number,
    /// Boolean.
    Boolean,
    /// Arbitrary JSON object.
    Object,
    /// Array of values.
    Array,
    /// Binary data (bytes).
    Binary,
}

impl std::fmt::Display for PortType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => write!(f, "any"),
            Self::String => write!(f, "string"),
            Self::Number => write!(f, "number"),
            Self::Boolean => write!(f, "boolean"),
            Self::Object => write!(f, "object"),
            Self::Array => write!(f, "array"),
            Self::Binary => write!(f, "binary"),
        }
    }
}

impl PortType {
    /// Checks if a type is compatible with another.
    /// `Any` is compatible with all types.
    pub fn is_compatible_with(&self, other: &PortType) -> bool {
        self == other || *self == PortType::Any || *other == PortType::Any
    }
}

/// Direction of a port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    Input,
    Output,
}

/// Port of a node (input or output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    /// Port name (e.g., "payload", "error", "headers").
    pub name: String,
    /// Direction: input or output.
    pub direction: PortDirection,
    /// Data type transported.
    pub port_type: PortType,
    /// Human-readable description for the editor.
    #[serde(default)]
    pub description: String,
    /// Whether a connection is required.
    #[serde(default)]
    pub required: bool,
}

/// Node type: identifier of the module that implements the logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeType(pub String);

impl NodeType {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self(type_name.into())
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A node within a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique identifier of the node (UUID v7).
    pub id: Uuid,
    /// Human-readable name assigned by the user.
    pub name: String,
    /// Node type (reference to WASM module or native node).
    pub node_type: NodeType,
    /// Input ports.
    pub inputs: Vec<Port>,
    /// Output ports.
    pub outputs: Vec<Port>,
    /// Node-specific configuration (arbitrary JSON).
    #[serde(default)]
    pub config: serde_json::Value,
    /// Whether the node is enabled (disabled ones are skipped).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Node {
    /// Creates a new node with minimal values.
    pub fn new(name: impl Into<String>, node_type: impl Into<String>) -> Self {
        Self {
            id: Uuid::now_v7(),
            name: name.into(),
            node_type: NodeType::new(node_type),
            inputs: Vec::new(),
            outputs: Vec::new(),
            config: serde_json::Value::Null,
            enabled: true,
        }
    }

    /// Adds an input port.
    pub fn with_input(mut self, name: impl Into<String>, port_type: PortType) -> Self {
        self.inputs.push(Port {
            name: name.into(),
            direction: PortDirection::Input,
            port_type,
            description: String::new(),
            required: false,
        });
        self
    }

    /// Adds an output port.
    pub fn with_output(mut self, name: impl Into<String>, port_type: PortType) -> Self {
        self.outputs.push(Port {
            name: name.into(),
            direction: PortDirection::Output,
            port_type,
            description: String::new(),
            required: false,
        });
        self
    }

    /// Sets the node configuration.
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Finds an input port by name.
    pub fn find_input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// Finds an output port by name.
    pub fn find_output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node_with_ports() {
        let node = Node::new("HTTP Request", "http-request")
            .with_input("url", PortType::String)
            .with_input("headers", PortType::Object)
            .with_output("response", PortType::Object)
            .with_output("error", PortType::Any);

        assert_eq!(node.name, "HTTP Request");
        assert_eq!(node.node_type.0, "http-request");
        assert_eq!(node.inputs.len(), 2);
        assert_eq!(node.outputs.len(), 2);
        assert!(node.enabled);
    }

    #[test]
    fn test_port_type_compatibility() {
        assert!(PortType::Any.is_compatible_with(&PortType::String));
        assert!(PortType::String.is_compatible_with(&PortType::Any));
        assert!(PortType::String.is_compatible_with(&PortType::String));
        assert!(!PortType::String.is_compatible_with(&PortType::Number));
    }
}
