//! Debug node: logs message payload for inspection.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::info;

pub struct DebugNode {
    name: String,
    log_payload: bool,
}

#[async_trait::async_trait]
impl NodeExecutor for DebugNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        if self.log_payload {
            info!(
                node = %self.name,
                source = %msg.source_node,
                payload = %msg.payload,
                "[DEBUG] {}",
                self.name
            );
        } else {
            info!(
                node = %self.name,
                source = %msg.source_node,
                "[DEBUG] {} — message received",
                self.name
            );
        }

        // Debug is a sink by default, but still forwards the message
        let out = msg.derive(msg.source_node, "output", msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(log) = config.get("logPayload").and_then(|v| v.as_bool()) {
            self.log_payload = log;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "debug"
    }
}

pub struct DebugNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for DebugNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = DebugNode {
            name: "Debug".to_string(),
            log_payload: true,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "debug"
    }
}
