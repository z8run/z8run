//! Function node: processes messages with configurable transforms.
//!
//! Supports two modes:
//! 1. **Template mode** (`config.template`): A JSON object with `{path.to.field}` placeholders
//!    that get resolved against the input payload using dot-notation.
//! 2. **Static output** (`config.outputValue`): Returns a fixed JSON value.
//! 3. **Pass-through**: If neither is set, forwards the input payload as-is.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::{debug, warn};

pub struct FunctionNode {
    name: String,
    /// Template with `{path}` placeholders to resolve against input.
    template: Option<Value>,
    /// Optional static output value (legacy/testing).
    output_value: Option<Value>,
}

/// Resolve `{path.to.field}` placeholders in a JSON template against input data.
fn resolve_template(template: &Value, data: &Value) -> Value {
    match template {
        Value::String(s) => {
            if !s.contains('{') {
                return Value::String(s.clone());
            }

            // Check if the entire string is a single placeholder like "{req.body.name}"
            let trimmed = s.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.matches('{').count() == 1 {
                let path = &trimmed[1..trimmed.len() - 1];
                let looked_up = json_path_lookup(data, path);
                if !looked_up.is_null() {
                    return looked_up;
                }
            }

            // Replace all {path} placeholders with their string representations
            let mut result = s.clone();
            // Simple placeholder replacement using manual scanning
            loop {
                let start = match result.find('{') {
                    Some(i) => i,
                    None => break,
                };
                let end = match result[start..].find('}') {
                    Some(i) => start + i,
                    None => break,
                };
                let path = &result[start + 1..end];
                let value = json_path_lookup(data, path);
                let replacement = match &value {
                    Value::String(s) => s.clone(),
                    Value::Null => "null".to_string(),
                    other => other.to_string(),
                };
                result = format!("{}{}{}", &result[..start], replacement, &result[end + 1..]);
            }

            // Try to parse as a JSON primitive (number, bool), fallback to string
            if let Ok(v) = serde_json::from_str::<Value>(&result) {
                if v.is_number() || v.is_boolean() {
                    return v;
                }
            }
            Value::String(result)
        }
        Value::Object(map) => {
            let resolved: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), resolve_template(v, data)))
                .collect();
            Value::Object(resolved)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_template(v, data)).collect())
        }
        // Pass through numbers, bools, null as-is
        other => other.clone(),
    }
}

/// Look up a value in a JSON object using dot-notation path (e.g. "req.body.name").
fn json_path_lookup(data: &Value, path: &str) -> Value {
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

#[async_trait::async_trait]
impl NodeExecutor for FunctionNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, "Processing function node");

        let payload = if let Some(ref tmpl) = self.template {
            // Template mode: resolve placeholders against input
            debug!(template = %tmpl, input = %msg.payload, "Resolving template");
            let resolved = resolve_template(tmpl, &msg.payload);
            debug!(result = %resolved, "Template resolved");
            resolved
        } else if let Some(ref output) = self.output_value {
            output.clone()
        } else {
            // Pass-through
            msg.payload.clone()
        };

        let out = msg.derive(msg.source_node, "output", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(tmpl) = config.get("template") {
            // Template can be a JSON object or a JSON string that parses to an object
            if tmpl.is_object() || tmpl.is_array() {
                self.template = Some(tmpl.clone());
            } else if let Some(tmpl_str) = tmpl.as_str() {
                match serde_json::from_str::<Value>(tmpl_str) {
                    Ok(parsed) => self.template = Some(parsed),
                    Err(e) => {
                        warn!(error = %e, "Failed to parse template string as JSON, using as-is");
                        self.template = Some(tmpl.clone());
                    }
                }
            }
        }
        if let Some(output) = config.get("outputValue") {
            self.output_value = Some(output.clone());
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "function"
    }
}

pub struct FunctionNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for FunctionNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = FunctionNode {
            name: "Function".to_string(),
            template: None,
            output_value: None,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "function"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_template() {
        let template = serde_json::json!({
            "message": "Hello {req.body.name}",
            "valid": true
        });
        let data = serde_json::json!({
            "req": {"body": {"name": "Pool", "age": 30}}
        });
        let result = resolve_template(&template, &data);
        assert_eq!(result["message"], "Hello Pool");
        assert_eq!(result["valid"], true);
    }

    #[test]
    fn test_direct_value_placeholder() {
        let template = serde_json::json!({
            "age": "{req.body.age}"
        });
        let data = serde_json::json!({
            "req": {"body": {"age": 30}}
        });
        let result = resolve_template(&template, &data);
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_nested_lookup() {
        let data = serde_json::json!({"a": {"b": {"c": "deep"}}});
        assert_eq!(json_path_lookup(&data, "a.b.c"), Value::String("deep".into()));
        assert_eq!(json_path_lookup(&data, "a.b.missing"), Value::Null);
    }
}
