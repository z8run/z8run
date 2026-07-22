//! Debug node: logs message payload for inspection.

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use tracing::info;

pub struct DebugNode {
    name: String,
    log_payload: bool,
}

#[async_trait::async_trait]
impl NodeExecutor for DebugNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        if self.log_payload {
            // Avoid writing the full payload into server logs; a size marker is
            // enough for debugging without leaking PII/secrets. The full payload
            // is still forwarded to the debug panel via the functional output below.
            info!(
                node = %self.name,
                source = %msg.source_node,
                payload_bytes = msg.payload.to_string().len(),
                "[DEBUG] {}",
                self.name
            );
        } else {
            info!(
                node = %self.name,
                source = %msg.source_node,
                "[DEBUG] {} - message received",
                self.name
            );
        }

        // Debug is a sink by default, but still forwards the message
        let out = msg.derive(msg.source_node, "output", msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "logPayload" => log_payload: bool,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "debug"
    }
}

node_factory!(DebugNodeFactory, DebugNode, "debug", {     name: String::new(),
log_payload: true });
