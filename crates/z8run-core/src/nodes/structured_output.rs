//! Structured Output node: forces LLM response into a JSON schema.
//!
//! Sends text to an LLM with instructions to respond only in the given JSON schema format.
//! Retries up to the configured limit if JSON parsing fails.
//!
//! Outputs:
//!   - "output" port: parsed JSON object matching the schema
//!   - "error" port: if all retries fail

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
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
        let text = extract_text(&msg.payload);
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
                &self.provider,
                &self.model,
                &self.api_key,
                &self.base_url,
                &system_prompt,
                &text,
                timeout,
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
                                    &self.provider,
                                    &self.model,
                                    &self.api_key,
                                    &self.base_url,
                                    &system_prompt,
                                    &retry_text,
                                    timeout,
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
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("provider").and_then(|v| v.as_str()) {
            self.provider = v.to_lowercase();
        }
        if let Some(v) = config.get("model").and_then(|v| v.as_str()) {
            self.model = v.to_string();
        }
        if let Some(v) = config.get("apiKey").and_then(|v| v.as_str()) {
            self.api_key = v.to_string();
        }
        if let Some(v) = config.get("baseUrl").and_then(|v| v.as_str()) {
            self.base_url = v.to_string();
        }
        if let Some(v) = config.get("schema") {
            self.schema = v.clone();
        }
        if let Some(v) = config.get("retries").and_then(|v| v.as_u64()) {
            self.retries = v as u32;
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
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

/// Call LLM with system and user prompts (same as classifier.rs pattern).
async fn call_llm(
    client: &reqwest::Client,
    provider: &str,
    model: &str,
    api_key: &str,
    base_url: &str,
    system_prompt: &str,
    user_prompt: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    match provider {
        "anthropic" => {
            let base = if base_url.is_empty() {
                "https://api.anthropic.com/v1"
            } else {
                base_url
            };
            let url = format!("{}/messages", base);
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "system": system_prompt,
                "messages": [{"role": "user", "content": user_prompt}],
            });
            let resp = client
                .post(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .timeout(timeout)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let status = resp.status().as_u16();
            let text = resp
                .text()
                .await
                .map_err(|e| format!("Read error: {}", e))?;
            if status != 200 {
                return Err(format!("API error ({}): {}", status, text));
            }
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
            Ok(json["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string())
        }
        "ollama" => {
            let base = if base_url.is_empty() {
                "http://localhost:11434"
            } else {
                base_url
            };
            let url = format!("{}/api/chat", base);
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_prompt},
                ],
                "stream": false,
                "options": {"temperature": 0.0, "num_predict": 4096},
            });
            let resp = client
                .post(&url)
                .timeout(timeout)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let status = resp.status().as_u16();
            let text = resp
                .text()
                .await
                .map_err(|e| format!("Read error: {}", e))?;
            if status != 200 {
                return Err(format!("API error ({}): {}", status, text));
            }
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
            Ok(json["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string())
        }
        _ => {
            // OpenAI-compatible
            let base = if base_url.is_empty() {
                "https://api.openai.com/v1"
            } else {
                base_url
            };
            let url = format!("{}/chat/completions", base);
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_prompt},
                ],
                "temperature": 0.0,
                "max_tokens": 4096,
            });
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .timeout(timeout)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let status = resp.status().as_u16();
            let text = resp
                .text()
                .await
                .map_err(|e| format!("Read error: {}", e))?;
            if status != 200 {
                return Err(format!("API error ({}): {}", status, text));
            }
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
            Ok(json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string())
        }
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

fn extract_text(payload: &serde_json::Value) -> String {
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    for key in &["text", "content", "body", "prompt", "input", "message"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

pub struct StructuredOutputNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for StructuredOutputNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = StructuredOutputNode {
            name: "StructuredOutput".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            schema: serde_json::json!({}),
            retries: 2,
            timeout_ms: 30000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "structured-output"
    }
}
