//! Function node: processes messages with configurable transforms.
//!
//! For MVP, this is a pass-through that optionally adds data to the payload.
//! Future: integrate a JavaScript/WASM runtime for custom code.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::debug;

pub struct FunctionNode {
    name: String,
    /// Optional static output value (for MVP testing).
    output_value: Option<serde_json::Value>,
}

#[async_trait::async_trait]
impl NodeExecutor for FunctionNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, "Processing function node");

        let payload = if let Some(ref output) = self.output_value {
            output.clone()
        } else {
            // Pass-through: forward the input payload
            msg.payload.clone()
        };

        let out = msg.derive(msg.source_node, "output", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(output) = config.get("outputValue") {
            self.output_value = Some(output.clone());
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "function"
    }
}

pub struct FunctionNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for FunctionNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = FunctionNode {
            name: "Function".to_string(),
            output_value: None,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "function"
    }
}
