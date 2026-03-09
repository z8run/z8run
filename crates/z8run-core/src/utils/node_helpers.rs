//! Shared helper functions for node implementations.
//!
//! Reduces repeated error-handling boilerplate across 15+ nodes.

use crate::error::{Z8Error, Z8Result};
use crate::message::FlowMessage;
use serde_json::Value;

/// Validate that a string field is not empty.
///
/// Replaces the repeated pattern:
/// ```ignore
/// if self.url.is_empty() {
///     return Err(Z8Error::Internal("HTTP Request node requires a URL".into()));
/// }
/// ```
///
/// Usage:
/// ```ignore
/// require_non_empty(&self.url, "HTTP Request node requires a URL")?;
/// require_non_empty(&self.api_key, "TTS node requires an API key")?;
/// ```
pub fn require_non_empty(field: &str, message: &str) -> Z8Result<()> {
    if field.is_empty() {
        return Err(Z8Error::Internal(message.to_string()));
    }
    Ok(())
}

/// Validate that a value is within a set of allowed options.
///
/// Usage:
/// ```ignore
/// require_one_of(&self.action, &["parse", "stringify"], "Invalid CSV action")?;
/// ```
pub fn require_one_of(value: &str, allowed: &[&str], context: &str) -> Z8Result<()> {
    if !allowed.contains(&value) {
        return Err(Z8Error::Internal(format!(
            "{}: '{}'. Expected one of: {:?}",
            context, value, allowed
        )));
    }
    Ok(())
}

/// Macro to reduce configure() boilerplate.
///
/// Extracts fields from a `serde_json::Value` config and assigns them to `self`.
///
/// Supported field types:
/// - `str`       → `.get("key").and_then(|v| v.as_str()).map(|v| v.to_string())`
/// - `str_lower` → same as `str` but calls `.to_lowercase()`
/// - `str_upper` → same as `str` but calls `.to_uppercase()`
/// - `bool`      → `.get("key").and_then(|v| v.as_bool())`
/// - `u64`       → `.get("key").and_then(|v| v.as_u64())`
/// - `f64`       → `.get("key").and_then(|v| v.as_f64())`
/// - `value`     → `.get("key").cloned()` (for serde_json::Value fields)
///
/// # Examples
/// ```ignore
/// configure_fields!(config, self,
///     "name" => name: str,
///     "provider" => provider: str_lower,
///     "method" => method: str_upper,
///     "apiKey" => api_key: str,
///     "timeout" => timeout_ms: u64,
///     "temperature" => temperature: f64,
///     "vision" => vision: bool,
///     "headers" => headers: value,
/// );
/// ```
#[macro_export]
macro_rules! configure_fields {
    ($config:expr, $self:expr, $( $json_key:literal => $field:ident : $kind:ident ),* $(,)?) => {
        $(
            configure_fields!(@assign $config, $self, $json_key, $field, $kind);
        )*
    };

    // String field
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, str) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_str()) {
            $self.$field = v.to_string();
        }
    };

    // String lowercase
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, str_lower) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_str()) {
            $self.$field = v.to_lowercase();
        }
    };

    // String uppercase
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, str_upper) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_str()) {
            $self.$field = v.to_uppercase();
        }
    };

    // Boolean field
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, bool) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_bool()) {
            $self.$field = v;
        }
    };

    // u64 field
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, u64) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_u64()) {
            $self.$field = v;
        }
    };

    // f64 field
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, f64) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_f64()) {
            $self.$field = v;
        }
    };

    // serde_json::Value clone
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, value) => {
        if let Some(v) = $config.get($key) {
            $self.$field = v.clone();
        }
    };

    // usize field
    (@assign $config:expr, $self:expr, $key:literal, $field:ident, usize) => {
        if let Some(v) = $config.get($key).and_then(|v| v.as_u64()) {
            $self.$field = v as usize;
        }
    };
}

/// Macro to generate a `NodeExecutorFactory` implementation.
///
/// Eliminates ~12-15 lines of boilerplate per node (40 nodes = ~500 lines).
///
/// # Examples
/// ```ignore
/// node_factory!(DebugNodeFactory, DebugNode, "debug", {
///     name: "Debug".to_string(),
///     log_payload: true,
/// });
///
/// // With serde_json defaults:
/// node_factory!(LlmNodeFactory, LlmNode, "llm", {
///     name: "LLM".to_string(),
///     provider: "openai".to_string(),
///     model: "gpt-4o-mini".to_string(),
///     api_key: String::new(),
///     base_url: String::new(),
///     system_prompt: String::new(),
///     temperature: 0.7,
///     max_tokens: 1024_u64,
///     timeout_ms: 30000_u64,
///     vision: false,
///     event_tx: None,
///     flow_id: None,
///     node_id: None,
/// });
/// ```
#[macro_export]
macro_rules! node_factory {
    ($factory:ident, $node:ident, $type_name:literal, { $($field:ident : $default:expr),* $(,)? }) => {
        pub struct $factory;

        #[async_trait::async_trait]
        impl $crate::engine::NodeExecutorFactory for $factory {
            async fn create(
                &self,
                config: serde_json::Value,
            ) -> $crate::error::Z8Result<Box<dyn $crate::engine::NodeExecutor>> {
                let mut node = $node {
                    $($field: $default),*
                };
                node.configure(config).await?;
                Ok(Box::new(node))
            }

            fn node_type(&self) -> &str {
                $type_name
            }
        }
    };
}

/// Create an error output message for a node's "error" port.
///
/// Replaces the repeated pattern:
/// ```ignore
/// let err = serde_json::json!({"error": "..."});
/// Ok(vec![msg.derive(msg.source_node, "error", err)])
/// ```
pub fn error_output(msg: &FlowMessage, error: &str) -> Vec<FlowMessage> {
    let payload = serde_json::json!({ "error": error });
    vec![msg.derive(msg.source_node, "error", payload)]
}

/// Create an error output message with additional context fields.
///
/// # Examples
/// ```ignore
/// return Ok(error_output_with_context(&msg, "API call failed", json!({
///     "provider": "openai",
///     "model": "gpt-4o",
///     "status": 429,
/// })));
/// ```
pub fn error_output_with_context(
    msg: &FlowMessage,
    error: &str,
    context: Value,
) -> Vec<FlowMessage> {
    let mut payload = match context {
        Value::Object(mut map) => {
            map.insert("error".to_string(), Value::String(error.to_string()));
            Value::Object(map)
        }
        _ => serde_json::json!({ "error": error, "context": context }),
    };

    // Ensure "error" key is always present
    if let Value::Object(ref mut map) = payload {
        map.entry("error".to_string())
            .or_insert_with(|| Value::String(error.to_string()));
    }

    vec![msg.derive(msg.source_node, "error", payload)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_msg() -> FlowMessage {
        FlowMessage::new(
            Uuid::nil(),
            "test",
            serde_json::json!({"data": "test"}),
            Uuid::nil(),
        )
    }

    #[test]
    fn test_error_output() {
        let msg = make_msg();
        let result = error_output(&msg, "Something went wrong");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].payload["error"], "Something went wrong");
        assert_eq!(result[0].source_port, "error");
    }

    #[test]
    fn test_require_non_empty_ok() {
        assert!(require_non_empty("hello", "Field required").is_ok());
    }

    #[test]
    fn test_require_non_empty_err() {
        let result = require_non_empty("", "API key is required");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("API key is required"));
    }

    #[test]
    fn test_require_one_of_ok() {
        assert!(require_one_of("parse", &["parse", "stringify"], "Invalid action").is_ok());
    }

    #[test]
    fn test_require_one_of_err() {
        let result = require_one_of("invalid", &["parse", "stringify"], "Invalid action");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid"));
    }

    #[test]
    fn test_error_output_with_context() {
        let msg = make_msg();
        let result = error_output_with_context(
            &msg,
            "API failed",
            serde_json::json!({"provider": "openai", "status": 429}),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].payload["error"], "API failed");
        assert_eq!(result[0].payload["provider"], "openai");
        assert_eq!(result[0].payload["status"], 429);
    }
}
