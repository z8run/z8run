//! JSON Transform node: parse, stringify, or extract fields from JSON payloads.
//!
//! Actions:
//! - `parse`: Parse a JSON string into an object.
//! - `stringify`: Convert an object into a JSON string.
//! - `extract`: Extract a nested field using dot-notation path.
//!
//! Config example:
//! ```json
//! { "action": "parse" }
//! { "action": "extract", "path": "data.users" }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::debug;

use super::switch::json_path_lookup;

pub struct JsonTransformNode {
    name: String,
    action: String, // "parse" | "stringify" | "extract"
    path: String,   // dot-notation path for extract
}

#[async_trait::async_trait]
impl NodeExecutor for JsonTransformNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, action = %self.action, "JSON transform");

        let result = match self.action.as_str() {
            "parse" => {
                // Expect payload to be a string containing JSON
                match msg.payload.as_str() {
                    Some(s) => match serde_json::from_str::<Value>(s) {
                        Ok(parsed) => parsed,
                        Err(e) => {
                            let err = serde_json::json!({
                                "error": format!("Failed to parse JSON: {}", e),
                                "input": msg.payload
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            return Ok(vec![out]);
                        }
                    },
                    None => {
                        // If it's already an object/array, pass through
                        if msg.payload.is_object() || msg.payload.is_array() {
                            msg.payload.clone()
                        } else {
                            let err = serde_json::json!({
                                "error": "Expected a JSON string to parse",
                                "input": msg.payload
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            return Ok(vec![out]);
                        }
                    }
                }
            }
            "stringify" => {
                if msg.payload.is_string() {
                    // Already a string, pass through
                    msg.payload.clone()
                } else {
                    Value::String(serde_json::to_string(&msg.payload).unwrap_or_default())
                }
            }
            "extract" => {
                if self.path.is_empty() {
                    msg.payload.clone()
                } else {
                    json_path_lookup(&msg.payload, &self.path)
                }
            }
            other => {
                let err = serde_json::json!({
                    "error": format!("Unknown action: {}", other),
                    "supported": ["parse", "stringify", "extract"]
                });
                let out = msg.derive(msg.source_node, "error", err);
                return Ok(vec![out]);
            }
        };

        debug!(node = %self.name, "Transform complete");
        let out = msg.derive(msg.source_node, "output", result);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(action) = config.get("action").and_then(|v| v.as_str()) {
            self.action = action.to_string();
        }
        if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
            self.path = path.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if !["parse", "stringify", "extract"].contains(&self.action.as_str()) {
            return Err(crate::error::Z8Error::Internal(format!(
                "Invalid JSON transform action: '{}'. Expected: parse, stringify, extract",
                self.action
            )));
        }
        if self.action == "extract" && self.path.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Extract action requires a 'path' field".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "json"
    }
}

pub struct JsonTransformNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for JsonTransformNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = JsonTransformNode {
            name: "JSON Transform".to_string(),
            action: "parse".to_string(),
            path: String::new(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "json"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_string_to_object() {
        let node = JsonTransformNode {
            name: "test".into(),
            action: "parse".into(),
            path: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            Value::String(r#"{"name":"z8run","version":1}"#.into()),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].payload["name"], "z8run");
        assert_eq!(results[0].payload["version"], 1);
        assert_eq!(results[0].source_port, "output");
    }

    #[tokio::test]
    async fn test_stringify_object() {
        let node = JsonTransformNode {
            name: "test".into(),
            action: "stringify".into(),
            path: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({"key": "value"}),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].payload.is_string());
    }

    #[tokio::test]
    async fn test_extract_nested_field() {
        let node = JsonTransformNode {
            name: "test".into(),
            action: "extract".into(),
            path: "data.users".into(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({"data": {"users": [1, 2, 3]}}),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].payload, serde_json::json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn test_parse_invalid_json_goes_to_error() {
        let node = JsonTransformNode {
            name: "test".into(),
            action: "parse".into(),
            path: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            Value::String("not valid json{".into()),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].source_port, "error");
    }
}
