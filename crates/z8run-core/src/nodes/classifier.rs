//! Classifier node: classifies text into user-defined categories using an LLM.
//!
//! Uses OpenAI/Anthropic/Ollama as backend. The user defines categories
//! and the LLM picks the best match.
//!
//! Outputs:
//!   - "result" port: classification result with category, confidence, reasoning
//!   - "error" port: API errors

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::extract::TEXT_FIELDS;
use crate::utils::llm_client::{call_llm, LlmCallParams};
use tracing::{info, warn};

pub struct ClassifierNode {
    name: String,
    provider: String,
    model: String,
    api_key: String,
    base_url: String,
    categories: Vec<String>, // e.g. ["positive", "negative", "neutral"]
    context: String,         // optional: describes what we're classifying
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for ClassifierNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let text = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);
        if text.is_empty() {
            let err = serde_json::json!({"error": "No text found in message"});
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        if self.categories.is_empty() {
            let err = serde_json::json!({"error": "No categories defined"});
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        info!(node = %self.name, categories = self.categories.len(), "Classification request");

        // Build classification prompt
        let categories_str = self
            .categories
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. {}", i + 1, c))
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt = format!(
            "You are a text classifier. Classify the given text into exactly one of these categories:\n{}\n\n{}\nRespond with ONLY a JSON object: {{\"category\": \"<chosen category>\", \"confidence\": <0.0-1.0>, \"reasoning\": \"<brief explanation>\"}}",
            categories_str,
            if self.context.is_empty() {
                String::new()
            } else {
                format!("Context: {}", self.context)
            }
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        // Use the same LLM call pattern - reuse OpenAI-compatible API
        let result = call_llm(
            &client,
            &LlmCallParams {
                provider: &self.provider,
                model: &self.model,
                api_key: &self.api_key,
                base_url: &self.base_url,
                system_prompt: &system_prompt,
                user_prompt: &text,
                max_tokens: 256,
                temperature: 0.1,
                timeout,
            },
        )
        .await;

        match result {
            Ok(response_text) => {
                // Try to parse JSON from the response
                let classification = parse_classification(&response_text, &self.categories);
                info!(node = %self.name, category = %classification["category"], "Classified");

                let mut payload = classification;
                payload["text"] = serde_json::Value::String(text);
                payload["model"] = serde_json::Value::String(self.model.clone());

                Ok(vec![msg.derive(msg.source_node, "result", payload)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Classification failed");
                let payload = serde_json::json!({"error": e});
                Ok(vec![msg.derive(msg.source_node, "error", payload)])
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "provider" => provider: str_lower,
            "model" => model: str,
            "apiKey" => api_key: str,
            "baseUrl" => base_url: str,
            "context" => context: str,
            "timeout" => timeout_ms: u64,
        );

        if let Some(cats) = config.get("categories").and_then(|v| v.as_array()) {
            self.categories = cats
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        // Also support comma-separated string
        if let Some(cats_str) = config.get("categories").and_then(|v| v.as_str()) {
            self.categories = cats_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.categories.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Classifier requires at least one category".to_string(),
            ));
        }
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Classifier requires an API key (except for Ollama)".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "classifier"
    }
}

/// Parse the LLM response into a classification JSON.
/// Falls back gracefully if the LLM doesn't return perfect JSON.
fn parse_classification(response: &str, categories: &[String]) -> serde_json::Value {
    // Try parsing as JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(response) {
        if json.get("category").is_some() {
            return json;
        }
    }

    // Try extracting JSON from markdown code blocks
    let trimmed = response.trim();
    let json_str = if trimmed.contains("```json") {
        trimmed
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(trimmed)
    } else if trimmed.contains("```") {
        trimmed
            .split("```")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(trimmed)
    } else {
        trimmed
    };

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str.trim()) {
        if json.get("category").is_some() {
            return json;
        }
    }

    // Fallback: check if any category name appears in the response
    let lower = response.to_lowercase();
    for cat in categories {
        if lower.contains(&cat.to_lowercase()) {
            return serde_json::json!({
                "category": cat,
                "confidence": 0.5,
                "reasoning": "Extracted from raw response",
            });
        }
    }

    serde_json::json!({
        "category": "unknown",
        "confidence": 0.0,
        "reasoning": format!("Could not parse response: {}", response),
        "raw_response": response,
    })
}

node_factory!(ClassifierNodeFactory, ClassifierNode, "classifier", {
    name: "Classifier".to_string(),
    provider: "openai".to_string(),
    model: "gpt-4o-mini".to_string(),
    api_key: String::new(),
    base_url: String::new(),
    categories: vec![
        "positive".to_string(),
        "negative".to_string(),
        "neutral".to_string(),
    ],
    context: String::new(),
    timeout_ms: 15000
});
