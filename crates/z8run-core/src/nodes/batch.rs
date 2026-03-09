//! Batch node: split an array into chunks and emit each as a separate message.
//!
//! Useful for processing large datasets in manageable pieces.
//!
//! Config example:
//! ```json
//! { "size": 100, "field": "data" }
//! ```

use crate::configure_fields;
use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::debug;

pub struct BatchNode {
    name: String,
    size: usize,   // items per batch
    field: String, // payload field containing the array (empty = root payload)
}

#[async_trait::async_trait]
impl NodeExecutor for BatchNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, size = %self.size, "Batching");

        let items = self.extract_items(&msg.payload).ok_or_else(|| {
            crate::error::Z8Error::Internal(
                "Batch expects an array in the payload or in the configured field".to_string(),
            )
        })?;

        let total_items = items.len();
        let chunks: Vec<&[Value]> = items.chunks(self.size).collect();
        let total_batches = chunks.len();

        let mut outputs = Vec::with_capacity(total_batches);
        for (i, chunk) in chunks.into_iter().enumerate() {
            let payload = serde_json::json!({
                "data": chunk,
                "batch_index": i,
                "batch_total": total_batches,
                "batch_size": chunk.len(),
                "total_items": total_items
            });
            outputs.push(msg.derive(msg.source_node, "output", payload));
        }

        debug!(
            node = %self.name,
            batches = total_batches,
            items = total_items,
            "Batch split complete"
        );
        Ok(outputs)
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "size" => size: usize,
            "field" => field: str,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.size == 0 {
            return Err(crate::error::Z8Error::Internal(
                "Batch size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "batch"
    }
}

impl BatchNode {
    fn extract_items(&self, payload: &Value) -> Option<Vec<Value>> {
        // If a field is specified, look there first
        if !self.field.is_empty() {
            if let Some(arr) = payload.get(&self.field).and_then(|v| v.as_array()) {
                return Some(arr.clone());
            }
        }

        // Direct array
        if let Some(arr) = payload.as_array() {
            return Some(arr.clone());
        }

        // Common field names
        for key in &["rows", "data", "results", "items"] {
            if let Some(arr) = payload.get(*key).and_then(|v| v.as_array()) {
                return Some(arr.clone());
            }
        }

        None
    }
}

pub struct BatchNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for BatchNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = BatchNode {
            name: "Batch".to_string(),
            size: 100,
            field: String::new(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "batch"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_splits_array() {
        let node = BatchNode {
            name: "test".into(),
            size: 2,
            field: String::new(),
        };
        let data = serde_json::json!([1, 2, 3, 4, 5]);
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", data, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 3); // [1,2], [3,4], [5]

        assert_eq!(results[0].payload["data"], serde_json::json!([1, 2]));
        assert_eq!(results[0].payload["batch_index"], 0);
        assert_eq!(results[0].payload["batch_total"], 3);
        assert_eq!(results[0].payload["batch_size"], 2);
        assert_eq!(results[0].payload["total_items"], 5);

        assert_eq!(results[1].payload["data"], serde_json::json!([3, 4]));
        assert_eq!(results[1].payload["batch_index"], 1);

        assert_eq!(results[2].payload["data"], serde_json::json!([5]));
        assert_eq!(results[2].payload["batch_index"], 2);
        assert_eq!(results[2].payload["batch_size"], 1);
    }

    #[tokio::test]
    async fn test_batch_from_nested_field() {
        let node = BatchNode {
            name: "test".into(),
            size: 3,
            field: "records".into(),
        };
        let data = serde_json::json!({
            "records": [1, 2, 3, 4, 5, 6, 7]
        });
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", data, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 3); // [1,2,3], [4,5,6], [7]
    }

    #[tokio::test]
    async fn test_batch_exact_size() {
        let node = BatchNode {
            name: "test".into(),
            size: 3,
            field: String::new(),
        };
        let data = serde_json::json!([1, 2, 3]);
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", data, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].payload["batch_total"], 1);
    }

    #[tokio::test]
    async fn test_batch_auto_detect_rows() {
        let node = BatchNode {
            name: "test".into(),
            size: 2,
            field: String::new(),
        };
        let data = serde_json::json!({
            "rows": [{"a": 1}, {"a": 2}, {"a": 3}]
        });
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", data, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 2);
    }
}
