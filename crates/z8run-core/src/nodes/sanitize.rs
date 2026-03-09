//! Sanitize node: removes or masks sensitive data from messages.
//!
//! Supports multiple strategies for handling sensitive fields:
//! - **remove**: Delete the field entirely
//! - **mask**: Replace with asterisks (e.g. "sk-abc123" → "sk-***123")
//! - **hash**: Replace with a SHA-256 hash
//! - **redact**: Replace with "[REDACTED]"
//!
//! Also supports built-in pattern detection for common sensitive data:
//! - Credit card numbers
//! - Email addresses
//! - Bearer tokens / Authorization headers
//! - Phone numbers
//! - IP addresses
//!
//! Config example:
//! ```json
//! {
//!   "fields": ["headers.authorization", "body.password", "body.ssn"],
//!   "strategy": "mask",
//!   "detectPatterns": true,
//!   "patterns": ["credit_card", "email", "bearer_token", "phone", "ip_address"]
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::utils::json_path::{json_path_get, json_path_remove, json_path_set};
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::debug;

pub struct SanitizeNode {
    name: String,
    /// Dot-notation paths to fields that should be sanitized.
    fields: Vec<String>,
    /// Strategy: "remove", "mask", "hash", "redact"
    strategy: String,
    /// Whether to auto-detect sensitive patterns in string values.
    detect_patterns: bool,
    /// Which built-in patterns to detect.
    patterns: Vec<String>,
}

/// Built-in regex patterns for sensitive data detection.
struct SensitivePatterns {
    credit_card: Regex,
    email: Regex,
    bearer_token: Regex,
    phone: Regex,
    ip_address: Regex,
}

impl SensitivePatterns {
    fn new() -> Self {
        Self {
            credit_card: Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap(),
            email: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
            bearer_token: Regex::new(r"(?i)(Bearer\s+)\S+").unwrap(),
            phone: Regex::new(r"\b\+?\d{1,3}[\s-]?\(?\d{2,4}\)?[\s-]?\d{3,4}[\s-]?\d{3,4}\b")
                .unwrap(),
            ip_address: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        }
    }

    fn apply(&self, value: &str, enabled_patterns: &[String]) -> String {
        let mut result = value.to_string();

        for pattern_name in enabled_patterns {
            match pattern_name.as_str() {
                "credit_card" => {
                    result = self
                        .credit_card
                        .replace_all(&result, "****-****-****-$0")
                        .to_string();
                    // Better: keep last 4 digits
                    result = self
                        .credit_card
                        .replace_all(&result, |caps: &regex::Captures| {
                            let full = caps[0].replace([' ', '-'], "");
                            if full.len() >= 4 {
                                format!("****-****-****-{}", &full[full.len() - 4..])
                            } else {
                                "****-****-****-****".to_string()
                            }
                        })
                        .to_string();
                }
                "email" => {
                    result = self
                        .email
                        .replace_all(&result, |caps: &regex::Captures| {
                            let email = &caps[0];
                            if let Some(at_idx) = email.find('@') {
                                let local = &email[..at_idx];
                                let domain = &email[at_idx..];
                                if local.len() > 2 {
                                    format!("{}***{}", &local[..1], domain)
                                } else {
                                    format!("***{}", domain)
                                }
                            } else {
                                "***@***.***".to_string()
                            }
                        })
                        .to_string();
                }
                "bearer_token" => {
                    result = self
                        .bearer_token
                        .replace_all(&result, "${1}[REDACTED]")
                        .to_string();
                }
                "phone" => {
                    result = self
                        .phone
                        .replace_all(&result, |caps: &regex::Captures| {
                            let num = caps[0].replace([' ', '-', '(', ')'], "");
                            if num.len() >= 4 {
                                format!("***{}", &num[num.len() - 4..])
                            } else {
                                "***".to_string()
                            }
                        })
                        .to_string();
                }
                "ip_address" => {
                    result = self
                        .ip_address
                        .replace_all(&result, |caps: &regex::Captures| {
                            let ip = &caps[0];
                            if let Some(last_dot) = ip.rfind('.') {
                                format!("*.*.*{}", &ip[last_dot..])
                            } else {
                                "*.*.*.*".to_string()
                            }
                        })
                        .to_string();
                }
                _ => {}
            }
        }

        result
    }
}

/// Apply the sanitization strategy to a single value.
fn apply_strategy(value: &Value, strategy: &str) -> Value {
    match strategy {
        "remove" => Value::Null,
        "redact" => Value::String("[REDACTED]".to_string()),
        "hash" => {
            let input_str = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let mut hasher = Sha256::new();
            hasher.update(input_str.as_bytes());
            let hash = format!("sha256:{:x}", hasher.finalize());
            Value::String(hash)
        }
        _ => {
            // Mask: show first and last few chars
            match value {
                Value::String(s) => {
                    let len = s.len();
                    if len <= 4 {
                        Value::String("****".to_string())
                    } else if len <= 8 {
                        Value::String(format!("{}***", &s[..1]))
                    } else {
                        let visible_start = std::cmp::min(3, len / 4);
                        let visible_end = std::cmp::min(3, len / 4);
                        Value::String(format!(
                            "{}***{}",
                            &s[..visible_start],
                            &s[len - visible_end..]
                        ))
                    }
                }
                Value::Number(_) => Value::String("***".to_string()),
                _ => Value::String("[MASKED]".to_string()),
            }
        }
    }
}

/// Recursively scan all string values and apply pattern detection.
fn scan_and_sanitize_patterns(value: &mut Value, patterns: &SensitivePatterns, enabled: &[String]) {
    match value {
        Value::String(s) => {
            let sanitized = patterns.apply(s, enabled);
            if sanitized != *s {
                *s = sanitized;
            }
        }
        Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                scan_and_sanitize_patterns(v, patterns, enabled);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                scan_and_sanitize_patterns(v, patterns, enabled);
            }
        }
        _ => {}
    }
}

#[async_trait::async_trait]
impl NodeExecutor for SanitizeNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let mut payload = msg.payload.clone();
        // Step 1: Sanitize explicitly listed fields
        let mut count = 0usize;
        for field_path in &self.fields {
            if let Some(current_value) = json_path_get(&payload, field_path) {
                if self.strategy == "remove" {
                    // For remove: delete the key entirely
                    json_path_remove(&mut payload, field_path);
                } else {
                    let sanitized = apply_strategy(&current_value, &self.strategy);
                    json_path_set(&mut payload, field_path, sanitized);
                }
                count += 1;
            }
        }

        // Step 2: Auto-detect sensitive patterns in all string values
        if self.detect_patterns && !self.patterns.is_empty() {
            let sensitive_patterns = SensitivePatterns::new();
            scan_and_sanitize_patterns(&mut payload, &sensitive_patterns, &self.patterns);
        }

        let sanitized_count = count;
        debug!(
            node = %self.name,
            fields_sanitized = sanitized_count,
            detect_patterns = self.detect_patterns,
            strategy = %self.strategy,
            "Sanitize complete"
        );

        let mut out = msg.derive(msg.source_node, "output", payload);

        // Add metadata about what was sanitized
        if let Value::Object(ref mut map) = out.payload {
            map.insert(
                "_sanitized".to_string(),
                serde_json::json!({
                    "fields": self.fields,
                    "strategy": self.strategy,
                    "count": sanitized_count,
                    "patterns_enabled": self.detect_patterns,
                }),
            );
        }

        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(fields) = config.get("fields").and_then(|v| v.as_array()) {
            self.fields = fields
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        // Also support comma-separated string
        if let Some(fields_str) = config.get("fields").and_then(|v| v.as_str()) {
            self.fields = fields_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Some(strategy) = config.get("strategy").and_then(|v| v.as_str()) {
            self.strategy = strategy.to_string();
        }
        if let Some(detect) = config.get("detectPatterns").and_then(|v| v.as_bool()) {
            self.detect_patterns = detect;
        }
        if let Some(patterns) = config.get("patterns").and_then(|v| v.as_array()) {
            self.patterns = patterns
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        // Also support comma-separated string for patterns
        if let Some(patterns_str) = config.get("patterns").and_then(|v| v.as_str()) {
            self.patterns = patterns_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.fields.is_empty() && !self.detect_patterns {
            return Err(crate::error::Z8Error::Internal(
                "Sanitize node requires at least one field path or pattern detection enabled"
                    .to_string(),
            ));
        }
        let valid_strategies = ["remove", "mask", "hash", "redact"];
        if !valid_strategies.contains(&self.strategy.as_str()) {
            return Err(crate::error::Z8Error::Internal(format!(
                "Invalid sanitize strategy '{}'. Valid: {:?}",
                self.strategy, valid_strategies
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "sanitize"
    }
}

pub struct SanitizeNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for SanitizeNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = SanitizeNode {
            name: "Sanitize".to_string(),
            fields: vec![],
            strategy: "mask".to_string(),
            detect_patterns: true,
            patterns: vec![
                "credit_card".to_string(),
                "email".to_string(),
                "bearer_token".to_string(),
                "phone".to_string(),
                "ip_address".to_string(),
            ],
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "sanitize"
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
    async fn test_mask_string_field() {
        let mut node = SanitizeNode {
            name: "test".to_string(),
            fields: vec!["headers.authorization".to_string()],
            strategy: "mask".to_string(),
            detect_patterns: false,
            patterns: vec![],
        };
        node.configure(serde_json::json!({})).await.unwrap();

        let msg = make_msg(serde_json::json!({
            "headers": {
                "authorization": "Bearer GfEfqLalRgmJrlB4uF8HCnd4K26eoqA2lbjkXCd4x8Y"
            }
        }));
        let result = node.process(msg).await.unwrap();
        let auth = result[0].payload["headers"]["authorization"]
            .as_str()
            .unwrap();
        assert!(auth.contains("***"));
        assert!(!auth.contains("GfEfqLalRgmJrlB4uF8HCnd4K26eoqA2lbjkXCd4x8Y"));
    }

    #[tokio::test]
    async fn test_redact_field() {
        let node = SanitizeNode {
            name: "test".to_string(),
            fields: vec!["body.password".to_string()],
            strategy: "redact".to_string(),
            detect_patterns: false,
            patterns: vec![],
        };
        let msg = make_msg(serde_json::json!({
            "body": { "password": "supersecret123" }
        }));
        let result = node.process(msg).await.unwrap();
        assert_eq!(result[0].payload["body"]["password"], "[REDACTED]");
    }

    #[tokio::test]
    async fn test_hash_field() {
        let node = SanitizeNode {
            name: "test".to_string(),
            fields: vec!["token".to_string()],
            strategy: "hash".to_string(),
            detect_patterns: false,
            patterns: vec![],
        };
        let msg = make_msg(serde_json::json!({ "token": "my-secret-token" }));
        let result = node.process(msg).await.unwrap();
        let hashed = result[0].payload["token"].as_str().unwrap();
        assert!(hashed.starts_with("sha256:"));
    }

    #[tokio::test]
    async fn test_pattern_detection_email() {
        let node = SanitizeNode {
            name: "test".to_string(),
            fields: vec![],
            strategy: "mask".to_string(),
            detect_patterns: true,
            patterns: vec!["email".to_string()],
        };
        let msg = make_msg(serde_json::json!({
            "data": { "contact": "Send to john.doe@example.com please" }
        }));
        let result = node.process(msg).await.unwrap();
        let contact = result[0].payload["data"]["contact"].as_str().unwrap();
        assert!(!contact.contains("john.doe@example.com"));
        assert!(contact.contains("***"));
    }

    #[tokio::test]
    async fn test_pattern_detection_bearer() {
        let node = SanitizeNode {
            name: "test".to_string(),
            fields: vec![],
            strategy: "mask".to_string(),
            detect_patterns: true,
            patterns: vec!["bearer_token".to_string()],
        };
        let msg = make_msg(serde_json::json!({
            "headers": {
                "authorization": "Bearer GfEfqLalRgmJrlB4uF8HCnd4K26eoqA2lbjkXCd4x8Y"
            }
        }));
        let result = node.process(msg).await.unwrap();
        let auth = result[0].payload["headers"]["authorization"]
            .as_str()
            .unwrap();
        assert!(auth.contains("[REDACTED]"));
        assert!(!auth.contains("GfEfqLalRgmJrlB4uF8HCnd4K26eoqA2lbjkXCd4x8Y"));
    }

    #[tokio::test]
    async fn test_remove_strategy() {
        let node = SanitizeNode {
            name: "test".to_string(),
            fields: vec!["secret".to_string()],
            strategy: "remove".to_string(),
            detect_patterns: false,
            patterns: vec![],
        };
        let msg = make_msg(serde_json::json!({
            "secret": "top-secret",
            "public": "visible"
        }));
        let result = node.process(msg).await.unwrap();
        assert!(result[0].payload["secret"].is_null());
        assert_eq!(result[0].payload["public"], "visible");
    }
}
