//! Embeddings node: generates vector embeddings from text.
//!
//! Supports OpenAI and Ollama providers.
//!
//! Outputs:
//!   - "embedding" port: vector array + metadata
//!   - "error" port: API errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::{info, warn};

pub struct EmbeddingsNode {
    name: String,
    provider: String, // "openai", "ollama"
    model: String,    // e.g. "text-embedding-3-small", "nomic-embed-text"
    api_key: String,
    base_url: String,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for EmbeddingsNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let text = extract_text(&msg.payload);
        if text.is_empty() {
            let err = serde_json::json!({"error": "No text found in message"});
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        info!(node = %self.name, provider = %self.provider, chars = text.len(), "Embedding request");

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let result = match self.provider.as_str() {
            "ollama" => self.call_ollama(&client, &text, timeout).await,
            _ => self.call_openai(&client, &text, timeout).await,
        };

        match result {
            Ok(embedding) => {
                info!(node = %self.name, dimensions = embedding.len(), "Embedding generated");
                let payload = serde_json::json!({
                    "embedding": embedding,
                    "dimensions": embedding.len(),
                    "model": self.model,
                    "text": text,
                });
                Ok(vec![msg.derive(msg.source_node, "embedding", payload)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Embedding request failed");
                let payload = serde_json::json!({"error": e, "provider": self.provider});
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
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Embeddings node requires an API key (except for Ollama)".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "embeddings"
    }
}

impl EmbeddingsNode {
    async fn call_openai(
        &self,
        client: &reqwest::Client,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<Vec<f64>, String> {
        let base = if self.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/embeddings", base);

        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or("No embedding in response")?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect();
        Ok(embedding)
    }

    async fn call_ollama(
        &self,
        client: &reqwest::Client,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<Vec<f64>, String> {
        let base = if self.base_url.is_empty() {
            "http://localhost:11434"
        } else {
            &self.base_url
        };
        let url = format!("{}/api/embeddings", base);

        let body = serde_json::json!({
            "model": self.model,
            "prompt": text,
        });

        let resp = client
            .post(&url)
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
        let embedding = json["embedding"]
            .as_array()
            .ok_or("No embedding in response")?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect();
        Ok(embedding)
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
        if let Some(s) = body.as_str() {
            return s.to_string();
        }
    }
    String::new()
}

pub struct EmbeddingsNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for EmbeddingsNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = EmbeddingsNode {
            name: "Embeddings".to_string(),
            provider: "openai".to_string(),
            model: "text-embedding-3-small".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            timeout_ms: 15000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "embeddings"
    }
}
