//! Timer node: triggers flow execution on a configurable interval.
//!
//! This is an input node — it generates messages rather than processing them.
//! When the flow engine processes it, the timer emits a tick message with
//! the current timestamp and tick count.
//!
//! Config example:
//! ```json
//! { "interval": 5000, "unit": "ms" }
//! ```
//!
//! Note: The actual scheduling (setInterval-like behavior) is handled by the
//! flow engine or a supervisor. This node simply emits a tick message each
//! time it is invoked.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;

pub struct TimerNode {
    name: String,
    interval_ms: u64,
    tick_count: AtomicU64,
}

#[async_trait::async_trait]
impl NodeExecutor for TimerNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let tick = self.tick_count.fetch_add(1, Ordering::SeqCst) + 1;
        debug!(node = %self.name, tick = tick, interval_ms = self.interval_ms, "Timer tick");

        let payload = serde_json::json!({
            "tick": tick,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "interval_ms": self.interval_ms,
        });

        let out = msg.derive(msg.source_node, "tick", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }

        let raw_interval = config
            .get("interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000);

        let unit = config.get("unit").and_then(|v| v.as_str()).unwrap_or("ms");

        self.interval_ms = match unit {
            "s" => raw_interval * 1000,
            "m" => raw_interval * 60_000,
            _ => raw_interval, // ms
        };

        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.interval_ms < 100 {
            return Err(crate::error::Z8Error::Internal(
                "Timer interval must be at least 100ms".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "timer"
    }
}

pub struct TimerNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for TimerNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = TimerNode {
            name: "Timer".to_string(),
            interval_ms: 5000,
            tick_count: AtomicU64::new(0),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "timer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timer_emits_tick() {
        let node = TimerNode {
            name: "test".into(),
            interval_ms: 1000,
            tick_count: AtomicU64::new(0),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "trigger",
            serde_json::json!({}),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "tick");
        assert_eq!(results[0].payload["tick"], 1);
    }

    #[tokio::test]
    async fn test_timer_increments_tick() {
        let node = TimerNode {
            name: "test".into(),
            interval_ms: 1000,
            tick_count: AtomicU64::new(0),
        };
        let trace = uuid::Uuid::now_v7();
        let src = uuid::Uuid::now_v7();

        let msg1 = FlowMessage::new(src, "trigger", serde_json::json!({}), trace);
        let r1 = node.process(msg1).await.unwrap();
        assert_eq!(r1[0].payload["tick"], 1);

        let msg2 = FlowMessage::new(src, "trigger", serde_json::json!({}), trace);
        let r2 = node.process(msg2).await.unwrap();
        assert_eq!(r2[0].payload["tick"], 2);
    }

    #[tokio::test]
    async fn test_timer_unit_conversion() {
        let mut node = TimerNode {
            name: "test".into(),
            interval_ms: 5000,
            tick_count: AtomicU64::new(0),
        };
        node.configure(serde_json::json!({"interval": 5, "unit": "s"}))
            .await
            .unwrap();
        assert_eq!(node.interval_ms, 5000);

        node.configure(serde_json::json!({"interval": 2, "unit": "m"}))
            .await
            .unwrap();
        assert_eq!(node.interval_ms, 120_000);
    }
}
