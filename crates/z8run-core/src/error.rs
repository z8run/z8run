//! Global error types for z8run-core.

use thiserror::Error;
use uuid::Uuid;

/// Standard Result type for z8run.
pub type Z8Result<T> = Result<T, Z8Error>;

/// Errors from the z8run flow engine.
#[derive(Debug, Error)]
pub enum Z8Error {
    // ── Graph validation errors ──
    #[error("Flow '{0}' contains a detected cycle between nodes")]
    CycleDetected(Uuid),

    #[error("Node '{node_id}' not found in flow '{flow_id}'")]
    NodeNotFound { flow_id: Uuid, node_id: Uuid },

    #[error("Port '{port}' does not exist in node '{node_id}'")]
    PortNotFound { node_id: Uuid, port: String },

    #[error("Type mismatch: port '{from_port}' produces '{from_type}' but '{to_port}' expects '{to_type}'")]
    TypeMismatch {
        from_port: String,
        from_type: String,
        to_port: String,
        to_type: String,
    },

    #[error("Invalid edge: source node '{from}' or target node '{to}' does not exist")]
    InvalidEdge { from: Uuid, to: Uuid },

    // ── Execution errors ──
    #[error("Node '{node_id}' exceeded timeout of {timeout_ms}ms")]
    NodeTimeout { node_id: Uuid, timeout_ms: u64 },

    #[error("Error in node '{node_id}': {message}")]
    NodeExecution { node_id: Uuid, message: String },

    #[error("Flow '{flow_id}' is not in executable state (current state: {status})")]
    FlowNotRunnable { flow_id: Uuid, status: String },

    #[error("Communication channel closed for node '{node_id}'")]
    ChannelClosed { node_id: Uuid },

    // ── Configuration errors ──
    #[error("Invalid configuration for node '{node_id}': {reason}")]
    InvalidConfig { node_id: Uuid, reason: String },

    // ── Generic errors ──
    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
