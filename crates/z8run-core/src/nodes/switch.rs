//! Switch node: routes messages to different outputs based on conditions.
//!
//! Evaluates the configured `property` path against a list of rules.
//! Each rule has a `type` (eq, neq, gt, lt, gte, lte, regex, contains, empty, notempty)
//! and a `value` to compare against. The first matching rule wins, and the
//! message is sent out on the corresponding port (out1, out2, ...).
//! If no rule matches, the message goes to the `default` port.
//!
//! Config example:
//! ```json
//! {
//!   "property": "req.body.action",
//!   "rules": [
//!     { "type": "eq", "value": "create", "port": "out1" },
//!     { "type": "eq", "value": "delete", "port": "out2" }
//!   ]
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

/// A single routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchRule {
    /// Comparison type.
    #[serde(rename = "type")]
    pub rule_type: String,
    /// Value to compare against (not used for empty/notempty).
    #[serde(default)]
    pub value: Value,
    /// Output port name (e.g. "out1", "out2").
    #[serde(default = "default_port")]
    pub port: String,
}

fn default_port() -> String {
    "default".to_string()
}

pub struct SwitchNode {
    name: String,
    /// Dot-notation path to evaluate (e.g. "req.body.action").
    property: String,
    /// Ordered list of rules to check.
    rules: Vec<SwitchRule>,
    /// If true, check all rules (fan-out). If false, stop at first match.
    check_all: bool,
}

/// Look up a value in a JSON object using dot-notation path.
pub fn json_path_lookup(data: &Value, path: &str) -> Value {
    let mut current = data;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = match map.get(segment) {
                    Some(v) => v,
                    None => return Value::Null,
                };
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    current = match arr.get(idx) {
                        Some(v) => v,
                        None => return Value::Null,
                    };
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

/// Evaluate a single rule against a value.
pub fn evaluate_rule(actual: &Value, rule: &SwitchRule) -> bool {
    match rule.rule_type.as_str() {
        "eq" => values_equal(actual, &rule.value),
        "neq" => !values_equal(actual, &rule.value),
        "gt" => compare_numbers(actual, &rule.value, |a, b| a > b),
        "lt" => compare_numbers(actual, &rule.value, |a, b| a < b),
        "gte" => compare_numbers(actual, &rule.value, |a, b| a >= b),
        "lte" => compare_numbers(actual, &rule.value, |a, b| a <= b),
        "contains" => {
            if let (Some(haystack), Some(needle)) = (actual.as_str(), rule.value.as_str()) {
                haystack.contains(needle)
            } else {
                // Check if array contains value
                if let Value::Array(arr) = actual {
                    arr.iter().any(|item| values_equal(item, &rule.value))
                } else {
                    false
                }
            }
        }
        "regex" => {
            if let (Some(text), Some(pattern)) = (actual.as_str(), rule.value.as_str()) {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(text))
                    .unwrap_or(false)
            } else {
                false
            }
        }
        "empty" => is_empty(actual),
        "notempty" => !is_empty(actual),
        "true" | "truthy" => is_truthy(actual),
        "false" | "falsy" => !is_truthy(actual),
        other => {
            warn!(rule_type = other, "Unknown switch rule type");
            false
        }
    }
}

/// Compare two JSON values for equality (type-coerced).
fn values_equal(a: &Value, b: &Value) -> bool {
    // Direct comparison first
    if a == b {
        return true;
    }
    // Try numeric comparison (e.g. 30 == 30.0, or "30" == 30)
    if let (Some(na), Some(nb)) = (as_f64(a), as_f64(b)) {
        return (na - nb).abs() < f64::EPSILON;
    }
    // String comparison as fallback
    value_to_string(a) == value_to_string(b)
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn compare_numbers(a: &Value, b: &Value, cmp: fn(f64, f64) -> bool) -> bool {
    match (as_f64(a), as_f64(b)) {
        (Some(na), Some(nb)) => cmp(na, nb),
        _ => false,
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(false, |f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[async_trait::async_trait]
impl NodeExecutor for SwitchNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, property = %self.property, "Evaluating switch rules");

        let actual = json_path_lookup(&msg.payload, &self.property);
        debug!(property = %self.property, value = %actual, "Property resolved");

        let mut outputs = Vec::new();
        let mut matched = false;

        for (i, rule) in self.rules.iter().enumerate() {
            let result = evaluate_rule(&actual, rule);
            debug!(rule_index = i, rule_type = %rule.rule_type, matched = result, "Rule evaluated");

            if result {
                matched = true;
                let out = msg.derive(msg.source_node, &rule.port, msg.payload.clone());
                outputs.push(out);

                if !self.check_all {
                    break; // First match wins
                }
            }
        }

        // If no rule matched, send to default port
        if !matched {
            debug!(node = %self.name, "No rules matched, routing to default");
            let out = msg.derive(msg.source_node, "default", msg.payload.clone());
            outputs.push(out);
        }

        Ok(outputs)
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(prop) = config.get("property").and_then(|v| v.as_str()) {
            self.property = prop.to_string();
        }
        if let Some(rules) = config.get("rules") {
            if let Ok(parsed) = serde_json::from_value::<Vec<SwitchRule>>(rules.clone()) {
                self.rules = parsed;
            } else {
                warn!("Failed to parse switch rules, keeping defaults");
            }
        }
        if let Some(all) = config.get("checkAll").and_then(|v| v.as_bool()) {
            self.check_all = all;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.property.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Switch node requires a 'property' to evaluate".to_string(),
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
            name: "Switch".to_string(),
            property: "payload".to_string(),
            rules: Vec::new(),
            check_all: false,
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

    #[test]
    fn test_json_path_lookup() {
        let data = serde_json::json!({"req": {"body": {"action": "create", "count": 5}}});
        assert_eq!(
            json_path_lookup(&data, "req.body.action"),
            Value::String("create".into())
        );
        assert_eq!(
            json_path_lookup(&data, "req.body.count"),
            serde_json::json!(5)
        );
        assert_eq!(json_path_lookup(&data, "req.body.missing"), Value::Null);
    }

    #[test]
    fn test_eq_rule() {
        let rule = SwitchRule {
            rule_type: "eq".into(),
            value: serde_json::json!("create"),
            port: "out1".into(),
        };
        assert!(evaluate_rule(&serde_json::json!("create"), &rule));
        assert!(!evaluate_rule(&serde_json::json!("delete"), &rule));
    }

    #[test]
    fn test_numeric_comparison() {
        let gt = SwitchRule {
            rule_type: "gt".into(),
            value: serde_json::json!(10),
            port: "out1".into(),
        };
        assert!(evaluate_rule(&serde_json::json!(15), &gt));
        assert!(!evaluate_rule(&serde_json::json!(5), &gt));
    }

    #[test]
    fn test_contains_string() {
        let rule = SwitchRule {
            rule_type: "contains".into(),
            value: serde_json::json!("error"),
            port: "out1".into(),
        };
        assert!(evaluate_rule(
            &serde_json::json!("an error occurred"),
            &rule
        ));
        assert!(!evaluate_rule(&serde_json::json!("all good"), &rule));
    }

    #[test]
    fn test_empty_notempty() {
        let empty_rule = SwitchRule {
            rule_type: "empty".into(),
            value: Value::Null,
            port: "out1".into(),
        };
        assert!(evaluate_rule(&Value::Null, &empty_rule));
        assert!(evaluate_rule(&serde_json::json!(""), &empty_rule));
        assert!(!evaluate_rule(&serde_json::json!("hi"), &empty_rule));

        let notempty_rule = SwitchRule {
            rule_type: "notempty".into(),
            value: Value::Null,
            port: "out1".into(),
        };
        assert!(!evaluate_rule(&Value::Null, &notempty_rule));
        assert!(evaluate_rule(&serde_json::json!("hi"), &notempty_rule));
    }

    #[test]
    fn test_truthy_falsy() {
        let truthy = SwitchRule {
            rule_type: "true".into(),
            value: Value::Null,
            port: "out1".into(),
        };
        assert!(evaluate_rule(&serde_json::json!(true), &truthy));
        assert!(evaluate_rule(&serde_json::json!(1), &truthy));
        assert!(evaluate_rule(&serde_json::json!("yes"), &truthy));
        assert!(!evaluate_rule(&serde_json::json!(false), &truthy));
        assert!(!evaluate_rule(&serde_json::json!(0), &truthy));
        assert!(!evaluate_rule(&Value::Null, &truthy));
    }

    #[test]
    fn test_type_coerced_equality() {
        // Number string vs number
        assert!(values_equal(
            &serde_json::json!("30"),
            &serde_json::json!(30)
        ));
        assert!(values_equal(
            &serde_json::json!(30),
            &serde_json::json!(30.0)
        ));
    }
}
