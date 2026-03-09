//! # Webhook Trigger Node
//!
//! Triggers flow execution when an HTTP request is received
//! at the flow's unique webhook URL.
//! Extracts headers, query params, and body from the incoming request.

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::Z8Result;
use serde_json::{json, Value};
use tracing::info;

pub struct WebhookTriggerNode {
    method: String,
    path: String,
    auth_type: String,
    auth_token: String,
    response_mode: String,
    name: String,
}

#[async_trait::async_trait]
impl NodeExecutor for WebhookTriggerNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            method = %self.method,
            path = %self.path,
            "Webhook trigger received"
        );

        // The incoming msg.payload should already contain the HTTP request data
        // from the hook route handler. We enrich it with trigger metadata.
        let payload = msg.payload.clone();

        let trigger_payload = json!({
            "trigger": "webhook",
            "method": self.method,
            "path": self.path,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "headers": payload.get("headers").cloned().unwrap_or(json!({})),
            "query": payload.get("query").cloned().unwrap_or(json!({})),
            "body": payload.get("body").cloned().unwrap_or(json!(null)),
            "params": payload.get("params").cloned().unwrap_or(json!({})),
        });

        let out = msg.derive(msg.source_node, "output", trigger_payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "method" => method: str_upper,
            "path" => path: str,
            "authType" => auth_type: str,
            "authToken" => auth_token: str,
            "responseMode" => response_mode: str,
            "name" => name: str,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "ANY"];
        if !valid_methods.contains(&self.method.as_str()) {
            return Err(crate::Z8Error::Internal(format!(
                "Invalid HTTP method: {}. Expected one of: {:?}",
                self.method, valid_methods
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "webhook-trigger"
    }
}

node_factory!(WebhookTriggerNodeFactory, WebhookTriggerNode, "webhook-trigger", {
    method: "POST".to_string(),
    path: String::new(),
    auth_type: "none".to_string(),
    auth_token: String::new(),
    response_mode: "last_node".to_string(),
    name: "Webhook Trigger".to_string()
});
