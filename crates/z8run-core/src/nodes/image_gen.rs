//! Image Generation node: generates images using AI image generation APIs.
//!
//! Supports OpenAI DALL-E and Stability AI.
//!
//! Outputs:
//!   - "image" port: Generated image data with URL and metadata
//!   - "error" port: API or validation errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::{info, warn};

pub struct ImageGenNode {
    name: String,
    provider: String, // "openai" or "stability"
    model: String,    // e.g. "dall-e-3", "dall-e-2"
    api_key: String,
    base_url: String,
    size: String,    // e.g. "1024x1024"
    quality: String, // "standard", "hd"
    style: String,   // "natural", "vivid"
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for ImageGenNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Extract prompt from payload
        let prompt = extract_prompt(&msg.payload);

        if prompt.is_empty() {
            let err = serde_json::json!({
                "error": "No prompt found in message. Expected string payload or fields: prompt, text, input"
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            prompt_len = prompt.len(),
            "Image generation request"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let result = match self.provider.as_str() {
            "stability" => self.call_stability(&client, &prompt, timeout).await,
            _ => self.call_openai(&client, &prompt, timeout).await, // default to OpenAI
        };

        match result {
            Ok(response_data) => {
                info!(node = %self.name, provider = %self.provider, "Image generated successfully");
                Ok(vec![msg.derive(msg.source_node, "image", response_data)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Image generation failed");
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
        if let Some(v) = config.get("size").and_then(|v| v.as_str()) {
            self.size = v.to_string();
        }
        if let Some(v) = config.get("quality").and_then(|v| v.as_str()) {
            self.quality = v.to_lowercase();
        }
        if let Some(v) = config.get("style").and_then(|v| v.as_str()) {
            self.style = v.to_lowercase();
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Image generation requires an API key".to_string(),
            ));
        }
        if self.provider != "openai" && self.provider != "stability" {
            return Err(crate::error::Z8Error::Internal(format!(
                "Unknown provider: {}. Use 'openai' or 'stability'",
                self.provider
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "image-gen"
    }
}

impl ImageGenNode {
    async fn call_openai(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let base = if self.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/images/generations", base);

        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "size": self.size,
            "quality": self.quality,
            "style": self.style,
            "n": 1,
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

        // Extract URL and revised prompt
        let url = json["data"][0]["url"].as_str().unwrap_or("").to_string();

        let revised_prompt = json["data"][0]["revised_prompt"]
            .as_str()
            .map(|s| s.to_string());

        if url.is_empty() {
            return Err("No image URL in response".to_string());
        }

        let mut payload = serde_json::json!({
            "url": url,
            "provider": self.provider,
            "model": self.model,
            "size": self.size,
        });

        if let Some(rp) = revised_prompt {
            payload["revised_prompt"] = serde_json::Value::String(rp);
        }

        Ok(payload)
    }

    async fn call_stability(
        &self,
        client: &reqwest::Client,
        prompt: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let base = if self.base_url.is_empty() {
            "https://api.stability.ai/v1"
        } else {
            &self.base_url
        };

        // Parse size into width and height
        let (width, height) = parse_size(&self.size);

        let url = format!("{}/generation/{}/text-to-image", base, self.model);

        let body = serde_json::json!({
            "text_prompts": [
                {
                    "text": prompt,
                    "weight": 1.0
                }
            ],
            "height": height,
            "width": width,
            "samples": 1,
            "steps": 30,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Stability request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("Stability API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

        // Stability returns image data in base64
        let image_base64 = json["artifacts"][0]["base64"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if image_base64.is_empty() {
            return Err("No image data in response".to_string());
        }

        Ok(serde_json::json!({
            "base64": image_base64,
            "provider": self.provider,
            "model": self.model,
            "size": self.size,
            "finish_reason": json["artifacts"][0]["finish_reason"].as_str().unwrap_or("SUCCESS"),
        }))
    }
}

/// Parse size string like "1024x1024" into (width, height)
fn parse_size(size_str: &str) -> (u32, u32) {
    let parts: Vec<&str> = size_str.split('x').collect();
    if parts.len() == 2 {
        let width = parts[0].parse::<u32>().unwrap_or(1024);
        let height = parts[1].parse::<u32>().unwrap_or(1024);
        (width, height)
    } else {
        (1024, 1024)
    }
}

fn extract_prompt(payload: &serde_json::Value) -> String {
    // If payload is a string directly
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    // Try common field names
    for key in &["prompt", "text", "body", "input", "content"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    // Try nested: req.body.prompt, etc.
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["prompt", "text", "input", "content"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
    }
    String::new()
}

pub struct ImageGenNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for ImageGenNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = ImageGenNode {
            name: "ImageGen".to_string(),
            provider: "openai".to_string(),
            model: "dall-e-3".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            size: "1024x1024".to_string(),
            quality: "standard".to_string(),
            style: "natural".to_string(),
            timeout_ms: 60000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "image-gen"
    }
}
