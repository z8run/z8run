//! Delay node: pauses message for a configured duration.

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
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
        configure_fields!(config, self,
            "name" => name: str,
            "delayMs" => delay_ms: u64,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "delay"
    }
}

node_factory!(DelayNodeFactory, DelayNode, "delay", {     name: String::new(),
delay_ms: 1000 });
