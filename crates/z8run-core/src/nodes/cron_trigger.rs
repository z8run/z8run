//! # Cron Trigger Node
//!
//! Triggers flow execution on a schedule using cron expressions.
//! Generates an initial message at each scheduled time.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::message::FlowMessage;
use crate::Z8Result;
use serde_json::{json, Value};
use tracing::info;

pub struct CronTriggerNode {
    cron_expression: String,
    timezone: String,
    payload: Value,
    name: String,
}

#[async_trait::async_trait]
impl NodeExecutor for CronTriggerNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            cron = %self.cron_expression,
            tz = %self.timezone,
            "Cron trigger fired"
        );

        let trigger_payload = json!({
            "trigger": "cron",
            "cron": self.cron_expression,
            "timezone": self.timezone,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "payload": self.payload,
        });

        let out = msg.derive(msg.source_node, "output", trigger_payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("cron").and_then(|v| v.as_str()) {
            self.cron_expression = v.to_string();
        }
        if let Some(v) = config.get("timezone").and_then(|v| v.as_str()) {
            self.timezone = v.to_string();
        }
        if let Some(v) = config.get("payload") {
            self.payload = v.clone();
        }
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.cron_expression.is_empty() {
            return Err(crate::Z8Error::Internal(
                "Cron expression is required".into(),
            ));
        }
        // Basic validation: cron should have 5 or 6 fields
        let fields: Vec<&str> = self.cron_expression.split_whitespace().collect();
        if fields.len() < 5 || fields.len() > 6 {
            return Err(crate::Z8Error::Internal(
                "Invalid cron expression: expected 5-6 fields (min hour dom month dow [year])"
                    .into(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "cron-trigger"
    }
}

pub struct CronTriggerNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for CronTriggerNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = CronTriggerNode {
            cron_expression: "0 * * * *".to_string(),
            timezone: "UTC".to_string(),
            payload: json!({}),
            name: "Cron Trigger".to_string(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "cron-trigger"
    }
}
