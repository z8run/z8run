//! LLM node: sends prompts to AI language models.
//!
//! Supports OpenAI, Anthropic, and Ollama (local) providers.
//! Supports vision mode for image analysis (GPT-4o, Claude, LLaVA).
//!
//! Outputs:
//!   - "response" port: AI model response
//!   - "error" port: API errors

use crate::engine::{EngineEvent, NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

#[allow(dead_code)]
pub struct LlmNode {
    name: String,
    provider: String, // "openai", "anthropic", "ollama"
    model: String,    // e.g. "gpt-4o", "claude-sonnet-4-20250514", "llama3"
    api_key: String,
    base_url: String, // custom endpoint (for Ollama: http://localhost:11434)
    system_prompt: String,
    temperature: f64,
    max_tokens: u64,
    timeout_ms: u64,
    vision: bool, // enable vision mode for image analysis
    event_tx: Option<broadcast::Sender<EngineEvent>>,
    flow_id: Option<Uuid>,
    node_id: Option<Uuid>,
}

#[async_trait::async_trait]
impl NodeExecutor for LlmNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Extract prompt from the incoming message
        let prompt = extract_prompt(&msg.payload);

        if prompt.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No prompt found in message. Expected string payload or fields: prompt, body, text",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        // Extract image for vision mode (base64 or URL)
        let image = if self.vision {
            extract_image(&msg.payload)
        } else {
            None
        };

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            vision = self.vision && image.is_some(),
            "LLM request"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let flow_id = msg.trace_id;
        let node_id = msg.source_node;

        let result = if self.event_tx.is_some() {
            match self.provider.as_str() {
                "anthropic" => {
                    self.stream_anthropic(
                        &client,
                        &prompt,
                        image.as_deref(),
                        timeout,
                        flow_id,
                        node_id,
                    )
                    .await
                }
                "ollama" => {
                    self.stream_ollama(
                        &client,
                        &prompt,
                        image.as_deref(),
                        timeout,
                        flow_id,
                        node_id,
                    )
                    .await
                }
                _ => {
                    self.stream_openai(
                        &client,
                        &prompt,
                        image.as_deref(),
                        timeout,
                        flow_id,
                        node_id,
                    )
                    .await
                }
            }
        } else {
            match self.provider.as_str() {
                "anthropic" => {
                    self.call_anthropic(&client, &prompt, image.as_deref(), timeout)
                        .await
                }
                "ollama" => {
                    self.call_ollama(&client, &prompt, image.as_deref(), timeout)
                        .await
                }
                _ => {
                    self.call_openai(&client, &prompt, image.as_deref(), timeout)
                        .await
                }
            }
        };

        match result {
            Ok(response_text) => {
                info!(node = %self.name, chars = response_text.len(), "LLM response received");
                let payload = serde_json::json!({
                    "text": response_text,
                    "model": self.model,
                    "provider": self.provider,
                    "prompt": prompt,
                });
                Ok(vec![msg.derive(msg.source_node, "response", payload)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "LLM request failed");
                let payload = serde_json::json!({
                    "error": e,
                    "provider": self.provider,
                    "model": self.model,
                });
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
        if let Some(v) = config.get("systemPrompt").and_then(|v| v.as_str()) {
            self.system_prompt = v.to_string();
        }
        if let Some(v) = config.get("temperature").and_then(|v| v.as_f64()) {
            self.temperature = v;
        }
        if let Some(v) = config.get("maxTokens").and_then(|v| v.as_u64()) {
            self.max_tokens = v;
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        if let Some(v) = config.get("vision").and_then(|v| v.as_bool()) {
            self.vision = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "LLM node requires an API key (except for Ollama)".to_string(),
            ));
        }
        Ok(())
    }

    fn set_event_emitter(&mut self, tx: broadcast::Sender<EngineEvent>) {
        self.event_tx = Some(tx);
    }

    fn node_type(&self) -> &str {
        "llm"
    }
}

impl LlmNode {
    async fn call_openai(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/chat/completions", base);

        let mut messages = Vec::new();
        if !self.system_prompt.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": self.system_prompt}));
        }
        messages.push(build_user_message_openai(prompt, image));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("OpenAI API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(content)
    }

    async fn call_anthropic(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "https://api.anthropic.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/messages", base);

        let user_content = build_user_content_anthropic(prompt, image);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": [{"role": "user", "content": user_content}],
        });

        if !self.system_prompt.is_empty() {
            body["system"] = serde_json::Value::String(self.system_prompt.clone());
        }

        let resp = client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("Anthropic API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
        // Anthropic returns content as array of blocks
        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(content)
    }

    async fn call_ollama(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "http://localhost:11434"
        } else {
            &self.base_url
        };
        let url = format!("{}/api/chat", base);

        let mut messages = Vec::new();
        if !self.system_prompt.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": self.system_prompt}));
        }
        messages.push(build_user_message_ollama(prompt, image));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": self.temperature,
                "num_predict": self.max_tokens,
            }
        });

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("Ollama API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;
        let content = json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(content)
    }

    async fn stream_openai(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
        flow_id: Uuid,
        node_id: Uuid,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/chat/completions", base);

        let mut messages = Vec::new();
        if !self.system_prompt.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": self.system_prompt}));
        }
        messages.push(build_user_message_openai(prompt, image));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": true,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("OpenAI API error ({}): {}", status, text));
        }

        let mut stream = resp.bytes_stream();
        let mut full_text = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream error: {}", e))?;
            let text = String::from_utf8_lossy(&bytes);

            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                            full_text.push_str(content);
                            if let Some(tx) = &self.event_tx {
                                let _ = tx.send(EngineEvent::StreamChunk {
                                    flow_id,
                                    node_id,
                                    chunk: content.to_string(),
                                    done: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Send final done event
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(EngineEvent::StreamChunk {
                flow_id,
                node_id,
                chunk: String::new(),
                done: true,
            });
        }

        Ok(full_text)
    }

    async fn stream_anthropic(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
        flow_id: Uuid,
        node_id: Uuid,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "https://api.anthropic.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/messages", base);

        let user_content = build_user_content_anthropic(prompt, image);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": [{"role": "user", "content": user_content}],
            "stream": true,
        });

        if !self.system_prompt.is_empty() {
            body["system"] = serde_json::Value::String(self.system_prompt.clone());
        }

        let resp = client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Anthropic API error ({}): {}", status, text));
        }

        let mut stream = resp.bytes_stream();
        let mut full_text = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream error: {}", e))?;
            let text = String::from_utf8_lossy(&bytes);

            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(event_type) = json.get("type").and_then(|v| v.as_str()) {
                            if event_type == "content_block_delta" {
                                if let Some(delta_text) = json
                                    .get("delta")
                                    .and_then(|d| d.get("text"))
                                    .and_then(|t| t.as_str())
                                {
                                    full_text.push_str(delta_text);
                                    if let Some(tx) = &self.event_tx {
                                        let _ = tx.send(EngineEvent::StreamChunk {
                                            flow_id,
                                            node_id,
                                            chunk: delta_text.to_string(),
                                            done: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Send final done event
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(EngineEvent::StreamChunk {
                flow_id,
                node_id,
                chunk: String::new(),
                done: true,
            });
        }

        Ok(full_text)
    }

    async fn stream_ollama(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        image: Option<&str>,
        timeout: std::time::Duration,
        flow_id: Uuid,
        node_id: Uuid,
    ) -> Result<String, String> {
        let base = if self.base_url.is_empty() {
            "http://localhost:11434"
        } else {
            &self.base_url
        };
        let url = format!("{}/api/chat", base);

        let mut messages = Vec::new();
        if !self.system_prompt.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": self.system_prompt}));
        }
        messages.push(build_user_message_ollama(prompt, image));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "options": {
                "temperature": self.temperature,
                "num_predict": self.max_tokens,
            }
        });

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}", e))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Ollama API error ({}): {}", status, text));
        }

        let mut stream = resp.bytes_stream();
        let mut full_text = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream error: {}", e))?;
            let text = String::from_utf8_lossy(&bytes);

            for line in text.lines() {
                if !line.is_empty() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(content) = json
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            full_text.push_str(content);
                            if let Some(tx) = &self.event_tx {
                                let _ = tx.send(EngineEvent::StreamChunk {
                                    flow_id,
                                    node_id,
                                    chunk: content.to_string(),
                                    done: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Send final done event
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(EngineEvent::StreamChunk {
                flow_id,
                node_id,
                chunk: String::new(),
                done: true,
            });
        }

        Ok(full_text)
    }
}

/// Extract image data from payload (base64 string or URL).
fn extract_image(payload: &serde_json::Value) -> Option<String> {
    for key in &["image", "image_url", "imageUrl", "img", "photo"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    // Nested: req.body.image
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["image", "image_url", "imageUrl"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Build OpenAI user message with optional vision content.
fn build_user_message_openai(prompt: &str, image: Option<&str>) -> serde_json::Value {
    match image {
        Some(img) => {
            let image_url = if img.starts_with("http://") || img.starts_with("https://") {
                serde_json::json!({"url": img})
            } else {
                // Assume base64 — detect media type or default to jpeg
                let media_type = if img.starts_with("/9j/") {
                    "image/jpeg"
                } else if img.starts_with("iVBOR") {
                    "image/png"
                } else {
                    "image/jpeg"
                };
                serde_json::json!({"url": format!("data:{};base64,{}", media_type, img)})
            };
            serde_json::json!({
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": image_url}
                ]
            })
        }
        None => serde_json::json!({"role": "user", "content": prompt}),
    }
}

/// Build Anthropic user content with optional vision.
fn build_user_content_anthropic(prompt: &str, image: Option<&str>) -> serde_json::Value {
    match image {
        Some(img) => {
            if img.starts_with("http://") || img.starts_with("https://") {
                // Anthropic supports URL source type
                serde_json::json!([
                    {"type": "image", "source": {"type": "url", "url": img}},
                    {"type": "text", "text": prompt}
                ])
            } else {
                let media_type = if img.starts_with("/9j/") {
                    "image/jpeg"
                } else if img.starts_with("iVBOR") {
                    "image/png"
                } else {
                    "image/jpeg"
                };
                serde_json::json!([
                    {"type": "image", "source": {"type": "base64", "media_type": media_type, "data": img}},
                    {"type": "text", "text": prompt}
                ])
            }
        }
        None => serde_json::Value::String(prompt.to_string()),
    }
}

/// Build Ollama user message with optional vision (for LLaVA, etc.).
fn build_user_message_ollama(prompt: &str, image: Option<&str>) -> serde_json::Value {
    match image {
        Some(img) => {
            // Ollama expects images as base64 array in the message
            let img_data = if img.starts_with("http://") || img.starts_with("https://") {
                // Ollama doesn't support URLs directly — pass as-is, model may handle it
                img.to_string()
            } else {
                img.to_string()
            };
            serde_json::json!({"role": "user", "content": prompt, "images": [img_data]})
        }
        None => serde_json::json!({"role": "user", "content": prompt}),
    }
}

fn extract_prompt(payload: &serde_json::Value) -> String {
    // If payload is a string directly
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    // Try common field names
    for key in &["prompt", "text", "body", "message", "content", "input"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    // Try nested: req.body.prompt, req.body.text, etc.
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["prompt", "text", "message", "content", "input"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
        // If body is a string
        if let Some(s) = body.as_str() {
            return s.to_string();
        }
    }
    String::new()
}

pub struct LlmNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for LlmNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = LlmNode {
            name: "LLM".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            system_prompt: String::new(),
            temperature: 0.7,
            max_tokens: 1024,
            timeout_ms: 30000,
            vision: false,
            event_tx: None,
            flow_id: None,
            node_id: None,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "llm"
    }
}
