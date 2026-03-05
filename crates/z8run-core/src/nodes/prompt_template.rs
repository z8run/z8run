//! Prompt Template node: renders templates with variable injection.
//!
//! Supports Mustache-like {{variable}} syntax with dot notation for nested access.
//! Variables are extracted from the incoming message payload.
//!
//! Outputs:
//!   - "output" port: rendered template with metadata
//!   - "error" port: if template has unresolved variables in strict mode

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::{info, warn};
use std::collections::HashMap;

pub struct PromptTemplateNode {
    name: String,
    template: String,
    strict_mode: bool,
}

#[async_trait::async_trait]
impl NodeExecutor for PromptTemplateNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(node = %self.name, template_len = self.template.len(), "Rendering template");

        // Extract variables from payload
        let variables = extract_variables(&msg.payload);

        // Render the template
        let rendered = render_template(&self.template, &variables);

        // Check for unresolved variables if in strict mode
        let has_unresolved = rendered.contains("{{");
        if has_unresolved && self.strict_mode {
            warn!(node = %self.name, "Template has unresolved variables in strict mode");
            let err_payload = serde_json::json!({
                "error": "Template has unresolved variables",
                "template": self.template,
                "resolved": rendered,
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        if has_unresolved && !self.strict_mode {
            info!(node = %self.name, "Template has unresolved variables but continuing in non-strict mode");
        }

        let output_payload = serde_json::json!({
            "text": rendered,
            "template": self.template,
            "variables": serde_json::Value::Object(
                variables.into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect()
            ),
        });

        Ok(vec![msg.derive(msg.source_node, "output", output_payload)])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("template").and_then(|v| v.as_str()) {
            self.template = v.to_string();
        }
        if let Some(v) = config.get("strictMode").and_then(|v| v.as_bool()) {
            self.strict_mode = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.template.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Prompt template node requires a template".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "prompt-template"
    }
}

/// Extracts all variables from a JSON value using dot notation for nested access.
fn extract_variables(value: &serde_json::Value) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    extract_variables_recursive(value, "", &mut vars);
    vars
}

/// Recursively extract variables with dot-notation keys.
fn extract_variables_recursive(
    value: &serde_json::Value,
    prefix: &str,
    vars: &mut HashMap<String, String>,
) {
    match value {
        serde_json::Value::String(s) => {
            vars.insert(prefix.to_string(), s.clone());
        }
        serde_json::Value::Number(n) => {
            vars.insert(prefix.to_string(), n.to_string());
        }
        serde_json::Value::Bool(b) => {
            vars.insert(prefix.to_string(), b.to_string());
        }
        serde_json::Value::Null => {
            vars.insert(prefix.to_string(), "null".to_string());
        }
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                extract_variables_recursive(val, &new_prefix, vars);
            }
        }
        serde_json::Value::Array(arr) => {
            for (idx, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, idx);
                extract_variables_recursive(val, &new_prefix, vars);
            }
        }
    }
}

/// Renders a template by replacing {{variable}} placeholders with values.
fn render_template(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }

    result
}

pub struct PromptTemplateNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for PromptTemplateNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = PromptTemplateNode {
            name: "PromptTemplate".to_string(),
            template: String::new(),
            strict_mode: false,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "prompt-template"
    }
}
