//! # Switch Node
//!
//! Multi-branch routing based on a field value.
//! Each case maps a value to a named output port.
//! Includes a "default" port for unmatched values.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::message::FlowMessage;
use crate::node_factory;
use crate::Z8Result;
use crate::utils::node_helpers::require_non_empty;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

pub struct SwitchNode {
    field: String,
    cases: Vec<SwitchCase>,
    name: String,
}

#[derive(Clone, Debug)]
struct SwitchCase {
    value: String,
    port: String,
}

impl SwitchNode {
    fn extract_field<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = data;
        for part in parts {
            match current {
                Value::Object(map) => {
                    current = map.get(part)?;
                }
                Value::Array(arr) => {
                    let idx: usize = part.parse().ok()?;
                    current = arr.get(idx)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    fn value_to_string(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Null => "".to_string(),
            other => other.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for SwitchNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let field_value = Self::extract_field(&msg.payload, &self.field);
        let field_str = field_value
            .map(Self::value_to_string)
            .unwrap_or_default();

        // Find matching case
        let matched_port = self
            .cases
            .iter()
            .find(|c| c.value == field_str)
            .map(|c| c.port.as_str())
            .unwrap_or("default");

        info!(
            node = %self.name,
            field = %self.field,
            value = %field_str,
            port = %matched_port,
            "Switch routed"
        );

        let out = msg.derive(msg.source_node, matched_port, msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("field").and_then(|v| v.as_str()) {
            self.field = v.to_string();
        }
        if let Some(v) = config.get("cases").and_then(|v| v.as_array()) {
            self.cases = v
                .iter()
                .filter_map(|c| {
                    let value = c.get("value")?.as_str()?.to_string();
                    let port = c.get("port")?.as_str()?.to_string();
                    Some(SwitchCase { value, port })
                })
                .collect();
        }
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.field, "Field path is required")?;
        if self.cases.is_empty() {
            return Err(crate::Z8Error::Config(
                "At least one case is required".into(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "switch"
    }
}

pub struct SwitchNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for SwitchNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = SwitchNode {
            field: "payload.action".to_string(),
            cases: vec![
                SwitchCase {
                    value: "create".to_string(),
                    port: "case_0".to_string(),
                },
                SwitchCase {
                    value: "update".to_string(),
                    port: "case_1".to_string(),
                },
                SwitchCase {
                    value: "delete".to_string(),
                    port: "case_2".to_string(),
                },
            ],
            name: "Switch".to_string(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "switch"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_msg(payload: Value) -> FlowMessage {
        FlowMessage {
            id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            source_node: Uuid::new_v4(),
            payload,
            metadata: Default::default(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_switch_matches_case() {
        let node = SwitchNode {
            field: "action".to_string(),
            cases: vec![
                SwitchCase { value: "create".to_string(), port: "case_0".to_string() },
                SwitchCase { value: "delete".to_string(), port: "case_1".to_string() },
            ],
            name: "test".to_string(),
        };

        let msg = make_msg(json!({"action": "create"}));
        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].metadata.get("_port").unwrap(), "case_0");
    }

    #[tokio::test]
    async fn test_switch_default() {
        let node = SwitchNode {
            field: "action".to_string(),
            cases: vec![
                SwitchCase { value: "create".to_string(), port: "case_0".to_string() },
            ],
            name: "test".to_string(),
        };

        let msg = make_msg(json!({"action": "unknown"}));
        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].metadata.get("_port").unwrap(), "default");
    }
}
