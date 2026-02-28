//! Messages flowing between nodes.
//!
//! Each message contains a data payload along with metadata
//! for traceability, debugging, and real-time monitoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message flowing between nodes in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowMessage {
    /// Unique message ID for traceability.
    pub id: Uuid,
    /// Data carried by the message.
    pub payload: serde_json::Value,
    /// Node that originated the message.
    pub source_node: Uuid,
    /// Source output port.
    pub source_port: String,
    /// Exact time of emission.
    pub timestamp: DateTime<Utc>,
    /// Full execution ID (to correlate messages).
    pub trace_id: Uuid,
    /// Additional metadata (headers, tags, etc.).
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl FlowMessage {
    /// Creates a new message from a specific node and port.
    pub fn new(
        source_node: Uuid,
        source_port: impl Into<String>,
        payload: serde_json::Value,
        trace_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            payload,
            source_node,
            source_port: source_port.into(),
            timestamp: Utc::now(),
            trace_id,
            metadata: serde_json::Map::new(),
        }
    }

    /// Adds metadata to the message.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Returns the payload as a concrete type (deserializes).
    pub fn payload_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.payload.clone())
    }

    /// Creates a derived message (same trace_id, new source).
    pub fn derive(
        &self,
        new_source_node: Uuid,
        new_source_port: impl Into<String>,
        new_payload: serde_json::Value,
    ) -> Self {
        Self::new(new_source_node, new_source_port, new_payload, self.trace_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_message() {
        let node_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();
        let msg = FlowMessage::new(
            node_id,
            "output",
            serde_json::json!({"status": 200}),
            trace_id,
        );

        assert_eq!(msg.source_node, node_id);
        assert_eq!(msg.source_port, "output");
        assert_eq!(msg.trace_id, trace_id);
    }

    #[test]
    fn test_derive_message() {
        let node_a = Uuid::now_v7();
        let node_b = Uuid::now_v7();
        let trace_id = Uuid::now_v7();

        let original = FlowMessage::new(node_a, "out", serde_json::json!("hello"), trace_id);
        let derived = original.derive(node_b, "processed", serde_json::json!("HELLO"));

        assert_eq!(derived.trace_id, trace_id); // same trace
        assert_eq!(derived.source_node, node_b); // different node
    }
}
