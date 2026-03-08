//! Mapper node: pick, rename, and reshape fields from the input payload.
//!
//! Useful for trimming large payloads (e.g. webhook data) down to just the
//! fields you need, and optionally renaming them for downstream nodes.
//!
//! Config example:
//! ```json
//! {
//!   "mappings": [
//!     { "from": "body.name",              "to": "name" },
//!     { "from": "headers.authorization",  "to": "auth" },
//!     { "from": "method",                 "to": "method" }
//!   ],
//!   "passThrough": false
//! }
//! ```
//!
//! - `from`: dot-notation path in the input
//! - `to`:   key name in the output (supports dot-notation for nested output)
//! - `passThrough`: if true, include ALL input fields plus the mapped aliases.
//!                   if false (default), output ONLY the mapped fields.

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::debug;

#[derive(Debug, Clone)]
struct FieldMapping {
    from: String,
    to: String,
    default: Option<Value>,
}

pub struct MapperNode {
    name: String,
    mappings: Vec<FieldMapping>,
    pass_through: bool,
}

/// Look up a value in a JSON value using dot-notation path.
fn json_path_lookup(data: &Value, path: &str) -> Option<Value> {
    let mut current = data;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = match map.get(segment) {
                    Some(v) => v,
                    None => return None,
                };
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    current = match arr.get(idx) {
                        Some(v) => v,
                        None => return None,
                    };
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Set a value in a JSON object using dot-notation path, creating
/// intermediate objects as needed.
fn json_path_set(data: &mut Value, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    let mut current = data;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Value::Object(map) = current {
                map.insert(part.to_string(), value);
            }
            return;
        }

        // Ensure intermediate object exists
        if let Value::Object(map) = current {
            if !map.contains_key(*part) || !map[*part].is_object() {
                map.insert(part.to_string(), Value::Object(serde_json::Map::new()));
            }
            current = map.get_mut(*part).unwrap();
        } else {
            return;
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for MapperNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let mut output = if self.pass_through {
            msg.payload.clone()
        } else {
            Value::Object(serde_json::Map::new())
        };

        let mut mapped_count = 0usize;
        let mut missing_fields: Vec<String> = Vec::new();

        for mapping in &self.mappings {
            match json_path_lookup(&msg.payload, &mapping.from) {
                Some(value) => {
                    json_path_set(&mut output, &mapping.to, value);
                    mapped_count += 1;
                }
                None => {
                    // Use default value if provided, otherwise skip
                    if let Some(default_val) = &mapping.default {
                        json_path_set(&mut output, &mapping.to, default_val.clone());
                        mapped_count += 1;
                    } else {
                        missing_fields.push(mapping.from.clone());
                    }
                }
            }
        }

        debug!(
            node = %self.name,
            mapped = mapped_count,
            missing = missing_fields.len(),
            pass_through = self.pass_through,
            "Mapper complete"
        );

        let out = msg.derive(msg.source_node, "output", output);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(pt) = config.get("passThrough").and_then(|v| v.as_bool()) {
            self.pass_through = pt;
        }

        // Parse mappings array
        if let Some(mappings) = config.get("mappings").and_then(|v| v.as_array()) {
            self.mappings = mappings
                .iter()
                .filter_map(|m| {
                    let from = m.get("from")?.as_str()?.to_string();
                    let to = m
                        .get("to")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&from)
                        .to_string();
                    let default = m.get("default").cloned();
                    Some(FieldMapping { from, to, default })
                })
                .collect();
        }

        // Also support a simple comma-separated string format: "from1:to1, from2:to2"
        // or just "field1, field2" (keeps same name)
        if let Some(mappings_str) = config.get("mappings").and_then(|v| v.as_str()) {
            self.mappings = mappings_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|entry| {
                    if let Some((from, to)) = entry.split_once(':') {
                        FieldMapping {
                            from: from.trim().to_string(),
                            to: to.trim().to_string(),
                            default: None,
                        }
                    } else {
                        // Same name for from and to
                        let field = entry.trim().to_string();
                        // Use last segment as the output key
                        let to = field
                            .rsplit('.')
                            .next()
                            .unwrap_or(&field)
                            .to_string();
                        FieldMapping {
                            from: field,
                            to,
                            default: None,
                        }
                    }
                })
                .collect();
        }

        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.mappings.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Mapper node requires at least one field mapping".to_string(),
            ));
        }
        for m in &self.mappings {
            if m.from.is_empty() {
                return Err(crate::error::Z8Error::Internal(
                    "Mapper field 'from' cannot be empty".to_string(),
                ));
            }
            if m.to.is_empty() {
                return Err(crate::error::Z8Error::Internal(
                    "Mapper field 'to' cannot be empty".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "mapper"
    }
}

pub struct MapperNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for MapperNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = MapperNode {
            name: "Mapper".to_string(),
            mappings: vec![],
            pass_through: false,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "mapper"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_msg(payload: Value) -> FlowMessage {
        FlowMessage::new(Uuid::nil(), "test", payload, Uuid::nil())
    }

    #[tokio::test]
    async fn test_basic_mapping() {
        let mut node = MapperNode {
            name: "test".to_string(),
            mappings: vec![],
            pass_through: false,
        };
        node.configure(serde_json::json!({
            "mappings": [
                { "from": "body.name", "to": "name" },
                { "from": "method", "to": "http_method" }
            ]
        }))
        .await
        .unwrap();

        let msg = make_msg(serde_json::json!({
            "method": "POST",
            "headers": { "authorization": "Bearer token123" },
            "body": { "name": "Pool", "age": 30 }
        }));

        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].payload["name"], "Pool");
        assert_eq!(result[0].payload["http_method"], "POST");
        // headers should NOT be in output (passThrough = false)
        assert!(result[0].payload.get("headers").is_none());
    }

    #[tokio::test]
    async fn test_pass_through_mode() {
        let mut node = MapperNode {
            name: "test".to_string(),
            mappings: vec![],
            pass_through: true,
        };
        node.configure(serde_json::json!({
            "passThrough": true,
            "mappings": [
                { "from": "body.name", "to": "user_name" }
            ]
        }))
        .await
        .unwrap();

        let msg = make_msg(serde_json::json!({
            "method": "POST",
            "body": { "name": "Pool" }
        }));

        let result = node.process(msg).await.unwrap();
        // Both original fields AND mapped alias exist
        assert_eq!(result[0].payload["user_name"], "Pool");
        assert_eq!(result[0].payload["method"], "POST");
    }

    #[tokio::test]
    async fn test_nested_output() {
        let mut node = MapperNode {
            name: "test".to_string(),
            mappings: vec![],
            pass_through: false,
        };
        node.configure(serde_json::json!({
            "mappings": [
                { "from": "body.name", "to": "user.name" },
                { "from": "body.age", "to": "user.age" }
            ]
        }))
        .await
        .unwrap();

        let msg = make_msg(serde_json::json!({
            "body": { "name": "Pool", "age": 30 }
        }));

        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].payload["user"]["name"], "Pool");
        assert_eq!(result[0].payload["user"]["age"], 30);
    }

    #[tokio::test]
    async fn test_string_format_mappings() {
        let mut node = MapperNode {
            name: "test".to_string(),
            mappings: vec![],
            pass_through: false,
        };
        node.configure(serde_json::json!({
            "mappings": "body.name:nombre, body.age:edad, method"
        }))
        .await
        .unwrap();

        let msg = make_msg(serde_json::json!({
            "method": "POST",
            "body": { "name": "Pool", "age": 30 }
        }));

        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].payload["nombre"], "Pool");
        assert_eq!(result[0].payload["edad"], 30);
        assert_eq!(result[0].payload["method"], "POST");
    }

    #[tokio::test]
    async fn test_default_value() {
        let mut node = MapperNode {
            name: "test".to_string(),
            mappings: vec![],
            pass_through: false,
        };
        node.configure(serde_json::json!({
            "mappings": [
                { "from": "body.name", "to": "name" },
                { "from": "body.role", "to": "role", "default": "user" }
            ]
        }))
        .await
        .unwrap();

        let msg = make_msg(serde_json::json!({
            "body": { "name": "Pool" }
        }));

        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].payload["name"], "Pool");
        assert_eq!(result[0].payload["role"], "user");
    }
}
