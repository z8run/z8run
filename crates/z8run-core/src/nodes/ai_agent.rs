//! AI Agent node: LLM with tool-use capability.
//!
//! Supports multi-turn agent loops with function calling.
//! Supports OpenAI, Anthropic, and Ollama providers.
//!
//! Outputs:
//!   - "response" port: Final text response from agent
//!   - "tool_call" port: When agent wants to call a tool (tool_name, arguments, iteration)
//!   - "error" port: API or configuration errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub struct AiAgentNode {
    name: String,
    provider: String,         // "openai", "anthropic", "ollama"
    model: String,
    api_key: String,
    base_url: String,
    system_prompt: String,
    tools: Vec<ToolDefinition>,
    max_iterations: u32,
    temperature: f64,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for AiAgentNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Extract user message
        let user_message = extract_text(&msg.payload);

        if user_message.is_empty() {
            let err = serde_json::json!({
                "error": "No message text found in payload"
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            iteration = 1,
            "AI agent processing message"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        // For v1, we do a single LLM call
        // In a real agent loop, you'd iterate on tool calls and feed results back
        let result = match self.provider.as_str() {
            "anthropic" => self.call_anthropic_agent(&client, &user_message, timeout).await,
            "ollama" => self.call_ollama_agent(&client, &user_message, timeout).await,
            _ => self.call_openai_agent(&client, &user_message, timeout).await,
        };

        match result {
            Ok(agent_response) => {
                // Check if response contains a tool call
                if let Some(tool_call) = extract_tool_call(&agent_response) {
                    info!(
                        node = %self.name,
                        tool_name = %tool_call["tool_name"],
                        "Agent requested tool call"
                    );
                    let mut payload = tool_call;
                    payload["iteration"] = serde_json::Value::Number(1.into());
                    Ok(vec![msg.derive(msg.source_node, "tool_call", payload)])
                } else {
                    // Extract text response
                    let response_text = agent_response
                        .get("text")
                        .or_else(|| agent_response.get("content"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| agent_response.to_string());

                    info!(
                        node = %self.name,
                        chars = response_text.len(),
                        "Agent response generated"
                    );

                    let payload = serde_json::json!({
                        "text": response_text,
                        "model": self.model,
                        "provider": self.provider,
                    });
                    Ok(vec![msg.derive(msg.source_node, "response", payload)])
                }
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Agent request failed");
                let payload = serde_json::json!({
                    "error": e,
                    "provider": self.provider,
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
        if let Some(v) = config.get("maxIterations").and_then(|v| v.as_u64()) {
            self.max_iterations = v as u32;
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        if let Some(tools_arr) = config.get("tools").and_then(|v| v.as_array()) {
            self.tools = tools_arr
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(|n| n.as_str())?;
                    let description = t.get("description").and_then(|d| d.as_str())?;
                    let parameters = t.get("parameters").cloned()?;
                    Some(ToolDefinition {
                        name: name.to_string(),
                        description: description.to_string(),
                        parameters,
                    })
                })
                .collect();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "AI Agent requires an API key (except for Ollama)".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "ai-agent"
    }
}

impl AiAgentNode {
    async fn call_openai_agent(
        &self,
        client: &reqwest::Client,
        user_message: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
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
        messages.push(serde_json::json!({"role": "user", "content": user_message}));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": 2048,
        });

        // Add tools if available
        if !self.tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = self
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools_json);
        }

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

        // Check for tool use
        if let Some(tool_calls) = json["choices"][0]["message"]["tool_calls"].as_array() {
            if !tool_calls.is_empty() {
                let tool_call = &tool_calls[0];
                return Ok(serde_json::json!({
                    "tool_name": tool_call.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("unknown"),
                    "arguments": tool_call.get("function").and_then(|f| f.get("arguments")).cloned().unwrap_or(serde_json::json!({})),
                }));
            }
        }

        // Otherwise return text content
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(serde_json::json!({
            "text": content,
        }))
    }

    async fn call_anthropic_agent(
        &self,
        client: &reqwest::Client,
        user_message: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let base = if self.base_url.is_empty() {
            "https://api.anthropic.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/messages", base);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 2048,
            "messages": [{"role": "user", "content": user_message}],
        });

        if !self.system_prompt.is_empty() {
            body["system"] = serde_json::Value::String(self.system_prompt.clone());
        }

        // Add tools if available
        if !self.tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = self
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": {
                            "type": "object",
                            "properties": t.parameters.get("properties").cloned().unwrap_or(serde_json::json!({})),
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools_json);
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

        // Check for tool use in content blocks
        if let Some(content) = json["content"].as_array() {
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    return Ok(serde_json::json!({
                        "tool_name": block.get("name").and_then(|n| n.as_str()).unwrap_or("unknown"),
                        "arguments": block.get("input").cloned().unwrap_or(serde_json::json!({})),
                    }));
                }
            }
        }

        // Otherwise return text content
        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(serde_json::json!({
            "text": content,
        }))
    }

    async fn call_ollama_agent(
        &self,
        client: &reqwest::Client,
        user_message: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
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
        messages.push(serde_json::json!({"role": "user", "content": user_message}));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": self.temperature,
                "num_predict": 2048,
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

        Ok(serde_json::json!({
            "text": content,
        }))
    }
}

/// Try to extract tool call information from agent response
fn extract_tool_call(response: &serde_json::Value) -> Option<serde_json::Value> {
    // If response has tool_name and arguments fields, it's a tool call
    if response.get("tool_name").is_some() && response.get("arguments").is_some() {
        return Some(response.clone());
    }
    None
}

fn extract_text(payload: &serde_json::Value) -> String {
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    for key in &["text", "input", "content", "message", "prompt", "body"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

pub struct AiAgentNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for AiAgentNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = AiAgentNode {
            name: "AIAgent".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            system_prompt: String::new(),
            tools: Vec::new(),
            max_iterations: 5,
            temperature: 0.7,
            timeout_ms: 30000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "ai-agent"
    }
}
