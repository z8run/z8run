//! HTTP Out node: terminal node that "responds" to an HTTP request.
//!
//! For MVP, this just logs the response. In production,
//! it would send an actual HTTP response via a held connection.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::info;

pub struct HttpOutNode {
    name: String,
    status_code: u16,
}

#[async_trait::async_trait]
impl NodeExecutor for HttpOutNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            status = self.status_code,
            payload = %msg.payload,
            "HTTP Out response"
        );

        let payload = serde_json::json!({
            "res": {
                "status": self.status_code,
                "body": msg.payload,
            }
        });

        let out = msg.derive(msg.source_node, "output", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(code) = config.get("statusCode").and_then(|v| v.as_u64()) {
            self.status_code = code as u16;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "http-out"
    }
}

pub struct HttpOutNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for HttpOutNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = HttpOutNode {
            name: "HTTP Out".to_string(),
            status_code: 200,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "http-out"
    }
}
