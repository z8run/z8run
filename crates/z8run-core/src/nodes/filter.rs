//! Filter node: passes or rejects messages based on a condition.
//!
//! Evaluates `property` against a single condition.
//! If the condition is met, the message is sent to the `pass` port.
//! Otherwise, it goes to the `reject` port.
//!
//! Config example:
//! ```json
//! {
//!   "property": "req.body.age",
//!   "condition": "gte",
//!   "value": 18
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::utils::json_path::json_path_lookup;
use crate::utils::node_helpers::require_non_empty;
use serde_json::Value;
use tracing::debug;

// Re-use evaluation helpers from the switch module.
use super::switch::{evaluate_rule, SwitchRule};
use crate::configure_fields;

pub struct FilterNode {
    name: String,
    /// Dot-notation path to evaluate.
    property: String,
    /// Condition type (eq, neq, gt, lt, gte, lte, contains, regex, empty, notempty, true, false).
    condition: String,
    /// Value to compare against.
    value: Value,
}

#[async_trait::async_trait]
impl NodeExecutor for FilterNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, property = %self.property, condition = %self.condition, "Evaluating filter");

        let actual = json_path_lookup(&msg.payload, &self.property);

        let rule = SwitchRule {
            rule_type: self.condition.clone(),
            value: self.value.clone(),
            port: String::new(), // Not used here
        };

        let passed = evaluate_rule(&actual, &rule);
        debug!(node = %self.name, passed = passed, actual = %actual, "Filter result");

        let port = if passed { "pass" } else { "reject" };
        let out = msg.derive(msg.source_node, port, msg.payload.clone());
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "property" => property: str,
            "condition" => condition: str,
            "value" => value: value,
        );

        // Support "expression" as alias for simpler config (e.g. "msg.payload != null")
        if let Some(expr) = config.get("expression").and_then(|v| v.as_str()) {
            // Parse simple expressions like "property op value"
            if let Some((prop, cond, val)) = parse_expression(expr) {
                self.property = prop;
                self.condition = cond;
                self.value = val;
            }
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(
            &self.property,
            "Filter node requires a 'property' to evaluate",
        )?;
        Ok(())
    }

    fn node_type(&self) -> &str {
        "filter"
    }
}

/// Parse a simple expression string like "req.body.age >= 18" into (property, condition, value).
fn parse_expression(expr: &str) -> Option<(String, String, Value)> {
    let operators = [
        ("!=", "neq"),
        (">=", "gte"),
        ("<=", "lte"),
        ("==", "eq"),
        (">", "gt"),
        ("<", "lt"),
    ];

    for (op_str, op_name) in &operators {
        if let Some(idx) = expr.find(op_str) {
            let prop = expr[..idx].trim().to_string();
            let val_str = expr[idx + op_str.len()..].trim();

            let value = if val_str == "null" {
                Value::Null
            } else if val_str == "true" {
                Value::Bool(true)
            } else if val_str == "false" {
                Value::Bool(false)
            } else if let Ok(n) = val_str.parse::<f64>() {
                serde_json::json!(n)
            } else {
                // Strip quotes if present
                let cleaned = val_str.trim_matches('"').trim_matches('\'');
                Value::String(cleaned.to_string())
            };

            return Some((prop, op_name.to_string(), value));
        }
    }

    // Check for "!= null" style (notempty)
    if expr.ends_with("!= null") {
        let prop = expr.trim_end_matches("!= null").trim().to_string();
        return Some((prop, "notempty".to_string(), Value::Null));
    }

    None
}

pub struct FilterNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for FilterNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = FilterNode {
            name: "Filter".to_string(),
            property: "payload".to_string(),
            condition: "notempty".to_string(),
            value: Value::Null,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "filter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expression_gte() {
        let (prop, cond, val) = parse_expression("req.body.age >= 18").unwrap();
        assert_eq!(prop, "req.body.age");
        assert_eq!(cond, "gte");
        assert_eq!(val, serde_json::json!(18.0));
    }

    #[test]
    fn test_parse_expression_eq_string() {
        let (prop, cond, val) = parse_expression("req.body.role == \"admin\"").unwrap();
        assert_eq!(prop, "req.body.role");
        assert_eq!(cond, "eq");
        assert_eq!(val, Value::String("admin".into()));
    }

    #[test]
    fn test_parse_expression_neq_null() {
        let (prop, cond, val) = parse_expression("payload != null").unwrap();
        assert_eq!(prop, "payload");
        assert_eq!(cond, "neq");
        assert_eq!(val, Value::Null);
    }
}
