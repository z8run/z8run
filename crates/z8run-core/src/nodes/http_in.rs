//! HTTP In node: trigger node that starts a flow.
//!
//! When triggered from a webhook, the incoming message already contains
//! the real HTTP request data. The node restructures it into a standard
//! `{ req: { method, path, headers, query, body } }` format.

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
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
            // Real webhook trigger - payload has { method, path, headers, query, body }
            serde_json::json!({ "req": msg.payload })
        } else if msg.payload.get("req").is_some() {
            // Already wrapped in "req" - pass through
            msg.payload.clone()
        } else {
            // Default trigger (no real HTTP data) - generate stub
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
        configure_fields!(config, self,
            "name" => name: str,
            "method" => method: str,
            "path" => path: str,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "http-in"
    }
}

node_factory!(HttpInNodeFactory, HttpInNode, "http-in", {     name: String::new(),
method: "GET".to_string(), path: "/".to_string() });
