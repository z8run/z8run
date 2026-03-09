//! # Cron Trigger Node
//!
//! Triggers flow execution on a schedule using cron expressions.
//! Generates an initial message at each scheduled time.

use crate::configure_fields;
use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::message::FlowMessage;
use crate::utils::node_helpers::require_non_empty;
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
        configure_fields!(config, self,
            "cron" => cron_expression: str,
            "timezone" => timezone: str,
            "payload" => payload: value,
            "name" => name: str,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.cron_expression, "Cron expression is required")?;
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
