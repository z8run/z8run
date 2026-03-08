//! # If/Else Node
//!
//! Conditional routing node that evaluates a condition against the
//! incoming message and routes to either the "true" or "false" output port.
//!
//! Supports comparison operators: ==, !=, >, <, >=, <=,
//! contains, not_contains, starts_with, ends_with, is_empty, is_not_empty,
//! exists, not_exists, matches (regex).

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::message::FlowMessage;
use crate::Z8Result;
use regex::Regex;
use serde_json::{json, Value};
use tracing::info;

pub struct IfElseNode {
    field: String,
    operator: String,
    value: Value,
    name: String,
}

impl IfElseNode {
    /// Extracts a nested field from a JSON value using dot notation.
    /// e.g., "payload.data.status" traverses into nested objects.
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

    /// Evaluates the condition against the extracted field value.
    fn evaluate(&self, field_value: Option<&Value>) -> bool {
        match self.operator.as_str() {
            // Existence checks (work even if field is missing)
            "exists" => field_value.is_some(),
            "not_exists" => field_value.is_none(),
            "is_empty" => match field_value {
                None => true,
                Some(Value::Null) => true,
                Some(Value::String(s)) => s.is_empty(),
                Some(Value::Array(a)) => a.is_empty(),
                Some(Value::Object(o)) => o.is_empty(),
                _ => false,
            },
            "is_not_empty" => match field_value {
                None => false,
                Some(Value::Null) => false,
                Some(Value::String(s)) => !s.is_empty(),
                Some(Value::Array(a)) => !a.is_empty(),
                Some(Value::Object(o)) => !o.is_empty(),
                _ => true,
            },

            // Value comparisons (require field to exist)
            op => {
                let Some(fv) = field_value else {
                    return false;
                };

                match op {
                    "==" | "eq" => self.compare_equal(fv),
                    "!=" | "neq" => !self.compare_equal(fv),
                    ">" | "gt" => self.compare_numeric(fv, |a, b| a > b),
                    "<" | "lt" => self.compare_numeric(fv, |a, b| a < b),
                    ">=" | "gte" => self.compare_numeric(fv, |a, b| a >= b),
                    "<=" | "lte" => self.compare_numeric(fv, |a, b| a <= b),
                    "contains" => {
                        let fv_str = self.value_to_string(fv);
                        let target = self.value_to_string(&self.value);
                        fv_str.contains(&target)
                    }
                    "not_contains" => {
                        let fv_str = self.value_to_string(fv);
                        let target = self.value_to_string(&self.value);
                        !fv_str.contains(&target)
                    }
                    "starts_with" => {
                        let fv_str = self.value_to_string(fv);
                        let target = self.value_to_string(&self.value);
                        fv_str.starts_with(&target)
                    }
                    "ends_with" => {
                        let fv_str = self.value_to_string(fv);
                        let target = self.value_to_string(&self.value);
                        fv_str.ends_with(&target)
                    }
                    "matches" => {
                        let fv_str = self.value_to_string(fv);
                        let pattern = self.value_to_string(&self.value);
                        Regex::new(&pattern)
                            .map(|re| re.is_match(&fv_str))
                            .unwrap_or(false)
                    }
                    _ => false,
                }
            }
        }
    }

    fn compare_equal(&self, field_value: &Value) -> bool {
        // Try numeric comparison first
        if let (Some(a), Some(b)) = (self.to_f64(field_value), self.to_f64(&self.value)) {
            return (a - b).abs() < f64::EPSILON;
        }
        // Try boolean comparison
        if let (Some(a), Some(b)) = (field_value.as_bool(), self.value.as_bool()) {
            return a == b;
        }
        // Fall back to string comparison
        self.value_to_string(field_value) == self.value_to_string(&self.value)
    }

    fn compare_numeric(&self, field_value: &Value, cmp: fn(f64, f64) -> bool) -> bool {
        match (self.to_f64(field_value), self.to_f64(&self.value)) {
            (Some(a), Some(b)) => cmp(a, b),
            _ => false,
        }
    }

    fn to_f64(&self, v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn value_to_string(&self, v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Null => "".to_string(),
            other => other.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for IfElseNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let field_value = Self::extract_field(&msg.payload, &self.field);
        let result = self.evaluate(field_value);

        info!(
            node = %self.name,
            field = %self.field,
            operator = %self.operator,
            result = result,
            "If/Else evaluated"
        );

        let port = if result { "true" } else { "false" };
        let out = msg.derive(msg.source_node, port, msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("field").and_then(|v| v.as_str()) {
            self.field = v.to_string();
        }
        if let Some(v) = config.get("operator").and_then(|v| v.as_str()) {
            self.operator = v.to_string();
        }
        if let Some(v) = config.get("value") {
            self.value = v.clone();
        }
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.field.is_empty() {
            return Err(crate::Z8Error::Internal("Field path is required".into()));
        }
        let valid_ops = [
            "==", "!=", ">", "<", ">=", "<=", "eq", "neq", "gt", "lt", "gte", "lte",
            "contains", "not_contains", "starts_with", "ends_with",
            "is_empty", "is_not_empty", "exists", "not_exists", "matches",
        ];
        if !valid_ops.contains(&self.operator.as_str()) {
            return Err(crate::Z8Error::Internal(format!(
                "Invalid operator: {}",
                self.operator
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "if-else"
    }
}

pub struct IfElseNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for IfElseNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = IfElseNode {
            field: "payload.status".to_string(),
            operator: "==".to_string(),
            value: json!("success"),
            name: "If/Else".to_string(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "if-else"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_field_nested() {
        let data = json!({
            "payload": {
                "data": {
                    "status": "ok"
                }
            }
        });
        let val = IfElseNode::extract_field(&data, "payload.data.status");
        assert_eq!(val, Some(&json!("ok")));
    }

    #[test]
    fn test_extract_field_array() {
        let data = json!({
            "items": ["a", "b", "c"]
        });
        let val = IfElseNode::extract_field(&data, "items.1");
        assert_eq!(val, Some(&json!("b")));
    }

    #[test]
    fn test_evaluate_equals() {
        let node = IfElseNode {
            field: "status".to_string(),
            operator: "==".to_string(),
            value: json!("ok"),
            name: "test".to_string(),
        };
        assert!(node.evaluate(Some(&json!("ok"))));
        assert!(!node.evaluate(Some(&json!("error"))));
    }

    #[test]
    fn test_evaluate_numeric() {
        let node = IfElseNode {
            field: "count".to_string(),
            operator: ">".to_string(),
            value: json!(10),
            name: "test".to_string(),
        };
        assert!(node.evaluate(Some(&json!(15))));
        assert!(!node.evaluate(Some(&json!(5))));
    }

    #[test]
    fn test_evaluate_contains() {
        let node = IfElseNode {
            field: "message".to_string(),
            operator: "contains".to_string(),
            value: json!("error"),
            name: "test".to_string(),
        };
        assert!(node.evaluate(Some(&json!("An error occurred"))));
        assert!(!node.evaluate(Some(&json!("All good"))));
    }

    #[test]
    fn test_evaluate_exists() {
        let node = IfElseNode {
            field: "key".to_string(),
            operator: "exists".to_string(),
            value: json!(null),
            name: "test".to_string(),
        };
        assert!(node.evaluate(Some(&json!("anything"))));
        assert!(!node.evaluate(None));
    }

    #[test]
    fn test_evaluate_regex() {
        let node = IfElseNode {
            field: "email".to_string(),
            operator: "matches".to_string(),
            value: json!(r"^[a-z]+@[a-z]+\.[a-z]+$"),
            name: "test".to_string(),
        };
        assert!(node.evaluate(Some(&json!("test@example.com"))));
        assert!(!node.evaluate(Some(&json!("invalid"))));
    }
}
