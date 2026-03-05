//! Delay node: pauses message for a configured duration.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::debug;

pub struct DelayNode {
    name: String,
    delay_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for DelayNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, delay_ms = self.delay_ms, "Delaying message");
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;

        let out = msg.derive(msg.source_node, "output", msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(ms) = config.get("delayMs").and_then(|v| v.as_u64()) {
            self.delay_ms = ms;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "delay"
    }
}

pub struct DelayNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for DelayNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = DelayNode {
            name: "Delay".to_string(),
            delay_ms: 1000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "delay"
    }
}
