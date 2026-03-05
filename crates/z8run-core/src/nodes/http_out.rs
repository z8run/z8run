//! HTTP Out node: terminal node that sends an HTTP response.
//!
//! When used with a webhook, it looks up the oneshot sender by trace_id
//! and sends the response back to the waiting HTTP handler.
//! When no webhook is waiting, it logs the response.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, RwLock};
use uuid::Uuid;

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::{info, warn};

/// Webhook response data sent through the oneshot channel.
#[derive(Debug, Clone)]
pub struct WebhookResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
}

/// Thread-safe map of trace_id → oneshot sender.
pub type WebhookResponders = Arc<RwLock<HashMap<Uuid, oneshot::Sender<WebhookResponse>>>>;

/// Global responder map, set once during app initialization.
static WEBHOOK_RESPONDERS: OnceLock<WebhookResponders> = OnceLock::new();

/// Initialize the global webhook responders map.
/// Call this once during server startup.
pub fn init_webhook_responders(responders: WebhookResponders) {
    let _ = WEBHOOK_RESPONDERS.set(responders);
}

/// Get the global webhook responders map.
pub fn get_webhook_responders() -> Option<&'static WebhookResponders> {
    WEBHOOK_RESPONDERS.get()
}

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
            trace_id = %msg.trace_id,
            payload = %msg.payload,
            "HTTP Out response"
        );

        // Try to send response through webhook oneshot channel
        if let Some(responders) = WEBHOOK_RESPONDERS.get() {
            let sender = responders.write().await.remove(&msg.trace_id);
            if let Some(tx) = sender {
                let response = WebhookResponse {
                    status: self.status_code,
                    headers: HashMap::new(),
                    body: msg.payload.clone(),
                };
                if tx.send(response).is_err() {
                    warn!(trace_id = %msg.trace_id, "Webhook receiver already dropped");
                } else {
                    info!(trace_id = %msg.trace_id, "Sent webhook response");
                }
            }
        }

        // Also emit the response as an output message (for logging/chaining)
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
