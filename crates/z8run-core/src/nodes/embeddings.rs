//! Embeddings node: generates vector embeddings from text.
//!
//! Supports OpenAI and Ollama providers.
//!
//! Outputs:
//!   - "embedding" port: vector array + metadata
//!   - "error" port: API errors

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::extract::TEXT_FIELDS;
use crate::utils::node_helpers::{error_output, error_output_with_context};
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
        let text = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);
        if text.is_empty() {
            return Ok(error_output(&msg, "No text found in message"));
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
                return Ok(error_output_with_context(
                    &msg,
                    &e,
                    serde_json::json!({"provider": self.provider}),
                ));
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
            "timeout" => timeout_ms: u64,
        );
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
            .bearer_auth(&self.api_key)
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

node_factory!(EmbeddingsNodeFactory, EmbeddingsNode, "embeddings", {
    name: "Embeddings".to_string(),
    provider: "openai".to_string(),
    model: "text-embedding-3-small".to_string(),
    api_key: String::new(),
    base_url: String::new(),
    timeout_ms: 15000
});
