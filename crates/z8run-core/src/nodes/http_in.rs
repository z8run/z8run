//! HTTP In node: trigger node that starts a flow.
//!
//! When triggered from a webhook, the incoming message already contains
//! the real HTTP request data. The node restructures it into a standard
//! `{ req: { method, path, headers, query, body } }` format.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::info;

pub struct HttpInNode {
    name: String,
    method: String,
    path: String,
}

#[async_trait::async_trait]
impl NodeExecutor for HttpInNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            method = %self.method,
            path = %self.path,
            "HTTP In trigger"
        );

        // Check if the incoming message already has real HTTP data (from webhook)
        let payload = if msg.payload.get("method").is_some() {
            // Real webhook trigger — payload has { method, path, headers, query, body }
            serde_json::json!({ "req": msg.payload })
        } else if msg.payload.get("req").is_some() {
            // Already wrapped in "req" — pass through
            msg.payload.clone()
        } else {
            // Default trigger (no real HTTP data) — generate stub
            serde_json::json!({
                "req": {
                    "method": self.method,
                    "path": self.path,
                    "headers": {},
                    "query": {},
                    "body": msg.payload,
                }
            })
        };

        let out = msg.derive(msg.source_node, "output", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(method) = config.get("method").and_then(|v| v.as_str()) {
            self.method = method.to_string();
        }
        if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
            self.path = path.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "http-in"
    }
}

pub struct HttpInNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for HttpInNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = HttpInNode {
            name: "HTTP In".to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "http-in"
    }
}
