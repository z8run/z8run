//! # Webhook Trigger Node
//!
//! Triggers flow execution when an HTTP request is received
//! at the flow's unique webhook URL.
//! Extracts headers, query params, and body from the incoming request.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::message::FlowMessage;
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
        if let Some(v) = config.get("method").and_then(|v| v.as_str()) {
            self.method = v.to_uppercase();
        }
        if let Some(v) = config.get("path").and_then(|v| v.as_str()) {
            self.path = v.to_string();
        }
        if let Some(v) = config.get("authType").and_then(|v| v.as_str()) {
            self.auth_type = v.to_string();
        }
        if let Some(v) = config.get("authToken").and_then(|v| v.as_str()) {
            self.auth_token = v.to_string();
        }
        if let Some(v) = config.get("responseMode").and_then(|v| v.as_str()) {
            self.response_mode = v.to_string();
        }
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
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

pub struct WebhookTriggerNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for WebhookTriggerNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = WebhookTriggerNode {
            method: "POST".to_string(),
            path: "".to_string(),
            auth_type: "none".to_string(),
            auth_token: "".to_string(),
            response_mode: "last_node".to_string(),
            name: "Webhook Trigger".to_string(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "webhook-trigger"
    }
}
