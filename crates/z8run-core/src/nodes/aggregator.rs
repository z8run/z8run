//! Aggregator node: perform aggregate operations on arrays of objects.
//!
//! Operations: `count`, `sum`, `avg`, `min`, `max`, `group_by`
//!
//! Config example:
//! ```json
//! { "operation": "sum", "field": "amount" }
//! { "operation": "group_by", "field": "amount", "groupBy": "category" }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

pub struct AggregatorNode {
    name: String,
    operation: String, // "count" | "sum" | "avg" | "min" | "max" | "group_by"
    field: String,     // field to aggregate on
    group_by: String,  // optional grouping field
}

#[async_trait::async_trait]
impl NodeExecutor for AggregatorNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, op = %self.operation, field = %self.field, "Aggregating");

        let items = extract_array(&msg.payload).ok_or_else(|| {
            crate::error::Z8Error::Internal(
                "Aggregator expects an array payload or {rows}/{data}/{results}/{items} field"
                    .to_string(),
            )
        })?;

        let result = if !self.group_by.is_empty() {
            self.aggregate_grouped(&items)?
        } else {
            self.aggregate_flat(&items)?
        };

        debug!(node = %self.name, "Aggregation complete");
        let out = msg.derive(msg.source_node, "output", result);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(op) = config.get("operation").and_then(|v| v.as_str()) {
            self.operation = op.to_string();
        }
        if let Some(f) = config.get("field").and_then(|v| v.as_str()) {
            self.field = f.to_string();
        }
        if let Some(g) = config.get("groupBy").and_then(|v| v.as_str()) {
            self.group_by = g.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        let valid_ops = ["count", "sum", "avg", "min", "max", "group_by"];
        if !valid_ops.contains(&self.operation.as_str()) {
            return Err(crate::error::Z8Error::Internal(format!(
                "Invalid aggregator operation: '{}'. Expected one of: {:?}",
                self.operation, valid_ops
            )));
        }
        if self.operation != "count" && self.field.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Aggregator requires a 'field' for sum/avg/min/max operations".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "aggregator"
    }
}

impl AggregatorNode {
    fn aggregate_flat(&self, items: &[Value]) -> Z8Result<Value> {
        match self.operation.as_str() {
            "count" => Ok(serde_json::json!({
                "count": items.len(),
                "operation": "count"
            })),
            "sum" => {
                let sum = self.sum_field(items);
                Ok(serde_json::json!({
                    "sum": sum,
                    "field": self.field,
                    "count": items.len()
                }))
            }
            "avg" => {
                let sum = self.sum_field(items);
                let count = items.len() as f64;
                let avg = if count > 0.0 { sum / count } else { 0.0 };
                Ok(serde_json::json!({
                    "avg": avg,
                    "sum": sum,
                    "field": self.field,
                    "count": items.len()
                }))
            }
            "min" => {
                let val = self.min_field(items);
                Ok(serde_json::json!({
                    "min": val,
                    "field": self.field,
                    "count": items.len()
                }))
            }
            "max" => {
                let val = self.max_field(items);
                Ok(serde_json::json!({
                    "max": val,
                    "field": self.field,
                    "count": items.len()
                }))
            }
            _ => Err(crate::error::Z8Error::Internal(format!(
                "Unknown operation: {}",
                self.operation
            ))),
        }
    }

    fn aggregate_grouped(&self, items: &[Value]) -> Z8Result<Value> {
        let mut groups: HashMap<String, Vec<&Value>> = HashMap::new();

        for item in items {
            let key = item
                .get(&self.group_by)
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "_null".to_string());
            groups.entry(key).or_default().push(item);
        }

        let mut result_groups: Vec<Value> = Vec::new();
        for (key, group_items) in &groups {
            let mut entry = serde_json::Map::new();
            entry.insert(self.group_by.clone(), Value::String(key.clone()));
            entry.insert("count".into(), Value::Number(group_items.len().into()));

            if !self.field.is_empty() {
                let values: Vec<Value> = group_items.iter().map(|v| (*v).clone()).collect();
                let sum = self.sum_field(&values);
                let count = values.len() as f64;
                let avg = if count > 0.0 { sum / count } else { 0.0 };

                if let Some(n) = serde_json::Number::from_f64(sum) {
                    entry.insert("sum".into(), Value::Number(n));
                }
                if let Some(n) = serde_json::Number::from_f64(avg) {
                    entry.insert("avg".into(), Value::Number(n));
                }
                entry.insert("min".into(), self.min_field(&values));
                entry.insert("max".into(), self.max_field(&values));
            }

            result_groups.push(Value::Object(entry));
        }

        // Sort by group key for deterministic output
        result_groups.sort_by(|a, b| {
            let ka = a.get(&self.group_by).and_then(|v| v.as_str()).unwrap_or("");
            let kb = b.get(&self.group_by).and_then(|v| v.as_str()).unwrap_or("");
            ka.cmp(kb)
        });

        Ok(serde_json::json!({
            "groups": result_groups,
            "groupBy": self.group_by,
            "field": self.field,
            "totalGroups": result_groups.len()
        }))
    }

    fn extract_number(&self, item: &Value) -> Option<f64> {
        item.get(&self.field).and_then(|v| v.as_f64())
    }

    fn sum_field(&self, items: &[Value]) -> f64 {
        items.iter().filter_map(|v| self.extract_number(v)).sum()
    }

    fn min_field(&self, items: &[Value]) -> Value {
        let min = items
            .iter()
            .filter_map(|v| self.extract_number(v))
            .fold(f64::INFINITY, f64::min);
        if min.is_infinite() {
            Value::Null
        } else {
            serde_json::Number::from_f64(min)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
    }

    fn max_field(&self, items: &[Value]) -> Value {
        let max = items
            .iter()
            .filter_map(|v| self.extract_number(v))
            .fold(f64::NEG_INFINITY, f64::max);
        if max.is_infinite() {
            Value::Null
        } else {
            serde_json::Number::from_f64(max)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
    }
}

/// Extract array from payload (direct array or from known fields).
fn extract_array(payload: &Value) -> Option<Vec<Value>> {
    if let Some(arr) = payload.as_array() {
        return Some(arr.clone());
    }
    for key in &["rows", "data", "results", "items"] {
        if let Some(arr) = payload.get(*key).and_then(|v| v.as_array()) {
            return Some(arr.clone());
        }
    }
    None
}

pub struct AggregatorNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for AggregatorNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = AggregatorNode {
            name: "Aggregator".to_string(),
            operation: "count".to_string(),
            field: String::new(),
            group_by: String::new(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "aggregator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sales_data() -> Value {
        serde_json::json!([
            {"product": "laptop", "category": "electronics", "amount": 1200},
            {"product": "phone", "category": "electronics", "amount": 800},
            {"product": "desk", "category": "furniture", "amount": 450},
            {"product": "chair", "category": "furniture", "amount": 300}
        ])
    }

    #[tokio::test]
    async fn test_count() {
        let node = AggregatorNode {
            name: "test".into(),
            operation: "count".into(),
            field: String::new(),
            group_by: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].payload["count"], 4);
    }

    #[tokio::test]
    async fn test_sum() {
        let node = AggregatorNode {
            name: "test".into(),
            operation: "sum".into(),
            field: "amount".into(),
            group_by: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].payload["sum"], 2750.0);
    }

    #[tokio::test]
    async fn test_avg() {
        let node = AggregatorNode {
            name: "test".into(),
            operation: "avg".into(),
            field: "amount".into(),
            group_by: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].payload["avg"], 687.5);
    }

    #[tokio::test]
    async fn test_min_max() {
        let node_min = AggregatorNode {
            name: "test".into(),
            operation: "min".into(),
            field: "amount".into(),
            group_by: String::new(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node_min.process(msg).await.unwrap();
        assert_eq!(results[0].payload["min"], 300.0);

        let node_max = AggregatorNode {
            name: "test".into(),
            operation: "max".into(),
            field: "amount".into(),
            group_by: String::new(),
        };
        let msg2 = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node_max.process(msg2).await.unwrap();
        assert_eq!(results[0].payload["max"], 1200.0);
    }

    #[tokio::test]
    async fn test_group_by() {
        let node = AggregatorNode {
            name: "test".into(),
            operation: "group_by".into(),
            field: "amount".into(),
            group_by: "category".into(),
        };
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            make_sales_data(),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        let groups = results[0].payload["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 2);

        // Groups are sorted: electronics, furniture
        assert_eq!(groups[0]["category"], "electronics");
        assert_eq!(groups[0]["count"], 2);
        assert_eq!(groups[0]["sum"], 2000.0);
        assert_eq!(groups[1]["category"], "furniture");
        assert_eq!(groups[1]["count"], 2);
        assert_eq!(groups[1]["sum"], 750.0);
    }

    #[tokio::test]
    async fn test_nested_data_field() {
        let node = AggregatorNode {
            name: "test".into(),
            operation: "count".into(),
            field: String::new(),
            group_by: String::new(),
        };
        let payload = serde_json::json!({
            "rows": [{"a": 1}, {"a": 2}, {"a": 3}]
        });
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", payload, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].payload["count"], 3);
    }
}
