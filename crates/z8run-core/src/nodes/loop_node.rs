//! # Loop/Iterator Node
//!
//! Processes arrays item by item.
//! Takes an array field from the input and emits one message per element.
//! Each output message includes the item, index, and total count.

use crate::engine::NodeExecutor;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::node_helpers::require_non_empty;
use crate::Z8Result;
use serde_json::{json, Value};
use tracing::info;

pub struct LoopNode {
    field: String,
    name: String,
}

impl LoopNode {
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
}

#[async_trait::async_trait]
impl NodeExecutor for LoopNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let field_value = Self::extract_field(&msg.payload, &self.field);

        let items = match field_value {
            Some(Value::Array(arr)) => arr.clone(),
            Some(other) => {
                // If field is not an array, wrap in single-element array
                vec![other.clone()]
            }
            None => {
                info!(
                    node = %self.name,
                    field = %self.field,
                    "Field not found, passing through empty"
                );
                // No items — emit nothing on "item" port, signal done
                let done = msg.derive(
                    msg.source_node,
                    "done",
                    json!({
                        "total": 0,
                        "originalPayload": msg.payload,
                    }),
                );
                return Ok(vec![done]);
            }
        };

        let total = items.len();
        info!(
            node = %self.name,
            field = %self.field,
            items = total,
            "Loop iterating"
        );

        let mut outputs = Vec::with_capacity(total + 1);

        // Emit one message per item on "item" port
        for (index, item) in items.iter().enumerate() {
            let item_payload = json!({
                "item": item,
                "index": index,
                "total": total,
                "isFirst": index == 0,
                "isLast": index == total - 1,
            });
            let out = msg.derive(msg.source_node, "item", item_payload);
            outputs.push(out);
        }

        // Emit completion signal on "done" port
        let done = msg.derive(
            msg.source_node,
            "done",
            json!({
                "total": total,
                "originalPayload": msg.payload,
            }),
        );
        outputs.push(done);

        Ok(outputs)
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("field").and_then(|v| v.as_str()) {
            self.field = v.to_string();
        }
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.field, "Array field path is required")?;
        Ok(())
    }

    fn node_type(&self) -> &str {
        "loop"
    }
}

node_factory!(LoopNodeFactory, LoopNode, "loop", {     name: String::new(),
field: "payload.items".to_string() });

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_msg(payload: Value) -> FlowMessage {
        FlowMessage::new(Uuid::new_v4(), "input", payload, Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_loop_iterates_array() {
        let node = LoopNode {
            field: "items".to_string(),
            name: "test".to_string(),
        };

        let msg = make_msg(json!({"items": ["a", "b", "c"]}));
        let result = node.process(msg).await.unwrap();
        // 3 items + 1 done = 4 messages
        assert_eq!(result.len(), 4);

        // Check first item
        assert_eq!(result[0].payload["item"], json!("a"));
        assert_eq!(result[0].payload["index"], json!(0));
        assert_eq!(result[0].payload["isFirst"], json!(true));

        // Check last item
        assert_eq!(result[2].payload["item"], json!("c"));
        assert_eq!(result[2].payload["isLast"], json!(true));

        // Check done
        assert_eq!(result[3].payload["total"], json!(3));
    }

    #[tokio::test]
    async fn test_loop_empty_field() {
        let node = LoopNode {
            field: "missing".to_string(),
            name: "test".to_string(),
        };

        let msg = make_msg(json!({"other": "data"}));
        let result = node.process(msg).await.unwrap();
        assert_eq!(result.len(), 1); // only "done"
        assert_eq!(result[0].payload["total"], json!(0));
    }
}
