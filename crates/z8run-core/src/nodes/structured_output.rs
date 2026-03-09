//! Structured Output node: forces LLM response into a JSON schema.
//!
//! Sends text to an LLM with instructions to respond only in the given JSON schema format.
//! Retries up to the configured limit if JSON parsing fails.
//!
//! Outputs:
//!   - "output" port: parsed JSON object matching the schema
//!   - "error" port: if all retries fail

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::extract::TEXT_FIELDS;
use crate::utils::llm_client::{call_llm, LlmCallParams};
use tracing::{info, warn};

pub struct StructuredOutputNode {
    name: String,
    provider: String,
    model: String,
    api_key: String,
    base_url: String,
    schema: serde_json::Value,
    retries: u32,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for StructuredOutputNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let text = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);
        if text.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No text found in message",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        info!(node = %self.name, provider = %self.provider, model = %self.model, "Structured output request");

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        // Build system prompt with schema
        let schema_str = self.schema.to_string();
        let system_prompt = format!(
            "You are a JSON extraction assistant. You MUST respond ONLY with a valid JSON object that matches this schema:\n\n{}\n\nRespond with ONLY the JSON object, nothing else.",
            schema_str
        );

        let mut last_error = String::new();
        for attempt in 0..=self.retries {
            let result = call_llm(
                &client,
                &LlmCallParams {
                    provider: &self.provider,
                    model: &self.model,
                    api_key: &self.api_key,
                    base_url: &self.base_url,
                    system_prompt: &system_prompt,
                    user_prompt: &text,
                    max_tokens: 4096,
                    temperature: 0.0,
                    timeout,
                },
            )
            .await;

            match result {
                Ok(response_text) => {
                    // Try to parse JSON
                    match parse_json_from_response(&response_text) {
                        Ok(json) => {
                            info!(node = %self.name, attempt = attempt, "Structured output parsed successfully");
                            return Ok(vec![msg.derive(msg.source_node, "output", json)]);
                        }
                        Err(parse_err) => {
                            last_error = format!("JSON parse error: {}", parse_err);
                            if attempt < self.retries {
                                warn!(node = %self.name, attempt = attempt, error = %last_error, "Retrying after parse failure");
                                // Retry with error feedback
                                let retry_text = format!(
                                    "Previous attempt failed: {}. Please try again and respond with ONLY valid JSON.\n\nOriginal text: {}",
                                    last_error, text
                                );
                                let retry_result = call_llm(
                                    &client,
                                    &LlmCallParams {
                                        provider: &self.provider,
                                        model: &self.model,
                                        api_key: &self.api_key,
                                        base_url: &self.base_url,
                                        system_prompt: &system_prompt,
                                        user_prompt: &retry_text,
                                        max_tokens: 4096,
                                        temperature: 0.0,
                                        timeout,
                                    },
                                )
                                .await;

                                if let Ok(retry_response) = retry_result {
                                    if let Ok(json) = parse_json_from_response(&retry_response) {
                                        info!(node = %self.name, attempt = attempt, "Structured output parsed on retry");
                                        return Ok(vec![msg.derive(
                                            msg.source_node,
                                            "output",
                                            json,
                                        )]);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    last_error = e.clone();
                    if attempt < self.retries {
                        warn!(node = %self.name, attempt = attempt, error = %e, "LLM request failed, retrying");
                    }
                }
            }
        }

        warn!(node = %self.name, retries = self.retries, error = %last_error, "All retries exhausted");
        let err_payload = serde_json::json!({
            "error": last_error,
            "retries": self.retries,
        });
        Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "provider" => provider: str_lower,
            "model" => model: str,
            "apiKey" => api_key: str,
            "baseUrl" => base_url: str,
            "schema" => schema: value,
            "timeout" => timeout_ms: u64,
        );

        if let Some(v) = config.get("retries").and_then(|v| v.as_u64()) {
            self.retries = v as u32;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Structured output node requires an API key (except for Ollama)".to_string(),
            ));
        }
        if self.schema.is_null() {
            return Err(crate::error::Z8Error::Internal(
                "Structured output node requires a schema".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "structured-output"
    }
}

/// Parse JSON from response, handling markdown code blocks.
fn parse_json_from_response(response: &str) -> Result<serde_json::Value, String> {
    let trimmed = response.trim();

    // Try direct JSON parsing
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(json);
    }

    // Try extracting from markdown code blocks
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

    serde_json::from_str::<serde_json::Value>(json_str.trim())
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

node_factory!(StructuredOutputNodeFactory, StructuredOutputNode, "structured-output", {
    name: "StructuredOutput".to_string(),
    provider: "openai".to_string(),
    model: "gpt-4o-mini".to_string(),
    api_key: String::new(),
    base_url: String::new(),
    schema: serde_json::json!({}),
    retries: 2,
    timeout_ms: 30000
});
