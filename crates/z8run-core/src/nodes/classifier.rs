//! Classifier node: classifies text into user-defined categories using an LLM.
//!
//! Uses OpenAI/Anthropic/Ollama as backend. The user defines categories
//! and the LLM picks the best match.
//!
//! Outputs:
//!   - "result" port: classification result with category, confidence, reasoning
//!   - "error" port: API errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
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
        let text = extract_text(&msg.payload);
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

        // Use the same LLM call pattern — reuse OpenAI-compatible API
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
        if let Some(v) = config.get("context").and_then(|v| v.as_str()) {
            self.context = v.to_string();
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
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

/// Shared LLM call function (OpenAI-compatible, Anthropic, or Ollama).
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
                "max_tokens": 256,
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
                "options": {"temperature": 0.1, "num_predict": 256},
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
                "temperature": 0.1,
                "max_tokens": 256,
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

fn extract_text(payload: &serde_json::Value) -> String {
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    for key in &["text", "input", "content", "prompt", "body", "message"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["text", "input", "content", "prompt"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
    }
    String::new()
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

pub struct ClassifierNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for ClassifierNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = ClassifierNode {
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
            timeout_ms: 15000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "classifier"
    }
}
