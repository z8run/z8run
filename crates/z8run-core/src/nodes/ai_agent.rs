//! AI Agent node: LLM with multi-turn tool-use capability.
//!
//! Supports iterative agent loops with function calling.
//! Supports OpenAI, Anthropic, and Ollama providers.
//!
//! The agent maintains conversation history across tool calls:
//! 1. First call: sends user message → LLM may return text or tool_call
//! 2. If tool_call: emits on "tool_call" port with conversation_history
//! 3. When receiving a tool_result (with conversation_history), continues the loop
//! 4. Repeats until text response or maxIterations reached
//!
//! Outputs:
//!   - "response" port: Final text response from agent
//!   - "tool_call" port: When agent wants to call a tool (includes conversation_history)
//!   - "error" port: API or configuration errors

use crate::configure_fields;
use crate::engine::{EngineEvent, NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::utils::extract::TEXT_FIELDS;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[allow(dead_code)]
pub struct AiAgentNode {
    name: String,
    provider: String, // "openai", "anthropic", "ollama"
    model: String,
    api_key: String,
    base_url: String,
    system_prompt: String,
    tools: Vec<ToolDefinition>,
    max_iterations: u32,
    temperature: f64,
    timeout_ms: u64,
    event_tx: Option<broadcast::Sender<EngineEvent>>,
    flow_id: Option<Uuid>,
    node_id: Option<Uuid>,
}

/// Represents the result of a single LLM call.
enum AgentStep {
    /// Agent returned a text response (no tool call).
    TextResponse(String),
    /// Agent wants to call a tool.
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
        tool_call_id: String,
    },
}

#[async_trait::async_trait]
impl NodeExecutor for AiAgentNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        // Check if this is a continuation (tool_result coming back)
        let (mut history, iteration) =
            if let Some(history_val) = msg.payload.get("conversation_history") {
                let history: Vec<serde_json::Value> =
                    history_val.as_array().cloned().unwrap_or_default();
                let iter = msg
                    .payload
                    .get("iteration")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;

                // Append tool result to history
                let tool_result = msg
                    .payload
                    .get("tool_result")
                    .cloned()
                    .unwrap_or(serde_json::json!(""));
                let tool_call_id = msg
                    .payload
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call_0")
                    .to_string();

                let mut h = history;
                match self.provider.as_str() {
                    "anthropic" => {
                        h.push(serde_json::json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": tool_result.as_str().unwrap_or(&tool_result.to_string())
                            }]
                        }));
                    }
                    _ => {
                        // OpenAI format
                        h.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": tool_result.as_str().unwrap_or(&tool_result.to_string())
                        }));
                    }
                }

                (h, iter + 1)
            } else {
                // Fresh conversation
                let user_message = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);
                if user_message.is_empty() {
                    let err = serde_json::json!({"error": "No message text found in payload"});
                    return Ok(vec![msg.derive(msg.source_node, "error", err)]);
                }

                let mut h = Vec::new();
                if !self.system_prompt.is_empty() {
                    h.push(serde_json::json!({"role": "system", "content": self.system_prompt}));
                }
                h.push(serde_json::json!({"role": "user", "content": user_message}));
                (h, 1u32)
            };

        // Check iteration limit
        if iteration > self.max_iterations {
            warn!(
                node = %self.name,
                iteration,
                max = self.max_iterations,
                "Agent reached max iterations"
            );
            let payload = serde_json::json!({
                "error": format!("Agent reached max iterations ({})", self.max_iterations),
                "conversation_history": history,
                "iteration": iteration,
            });
            return Ok(vec![msg.derive(msg.source_node, "error", payload)]);
        }

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            iteration,
            "AI agent iteration"
        );

        // Emit streaming event for iteration start
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(EngineEvent::StreamChunk {
                flow_id: msg.trace_id,
                node_id: msg.source_node,
                chunk: format!("[Agent iteration {}]", iteration),
                done: false,
            });
        }

        // Call LLM
        let result = match self.provider.as_str() {
            "anthropic" => self.call_anthropic_agent(&client, &history, timeout).await,
            "ollama" => self.call_ollama_agent(&client, &history, timeout).await,
            _ => self.call_openai_agent(&client, &history, timeout).await,
        };

        match result {
            Ok((step, assistant_msg)) => {
                // Append assistant message to history
                history.push(assistant_msg);

                match step {
                    AgentStep::ToolCall {
                        tool_name,
                        arguments,
                        tool_call_id,
                    } => {
                        info!(
                            node = %self.name,
                            tool = %tool_name,
                            iteration,
                            "Agent requested tool call"
                        );
                        let payload = serde_json::json!({
                            "tool_name": tool_name,
                            "arguments": arguments,
                            "tool_call_id": tool_call_id,
                            "iteration": iteration,
                            "conversation_history": history,
                        });
                        Ok(vec![msg.derive(msg.source_node, "tool_call", payload)])
                    }
                    AgentStep::TextResponse(text) => {
                        info!(
                            node = %self.name,
                            chars = text.len(),
                            iterations = iteration,
                            "Agent completed"
                        );
                        let payload = serde_json::json!({
                            "text": text,
                            "model": self.model,
                            "provider": self.provider,
                            "iterations": iteration,
                        });
                        Ok(vec![msg.derive(msg.source_node, "response", payload)])
                    }
                }
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Agent request failed");
                let payload = serde_json::json!({
                    "error": e,
                    "provider": self.provider,
                    "iteration": iteration,
                });
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
            "systemPrompt" => system_prompt: str,
            "temperature" => temperature: f64,
            "timeout" => timeout_ms: u64,
        );

        if let Some(v) = config.get("maxIterations").and_then(|v| v.as_u64()) {
            self.max_iterations = v as u32;
        }
        // Tools can come as array or JSON string
        if let Some(tools_arr) = config.get("tools").and_then(|v| v.as_array()) {
            self.tools = parse_tools(tools_arr);
        } else if let Some(tools_str) = config.get("tools").and_then(|v| v.as_str()) {
            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(tools_str) {
                self.tools = parse_tools(&parsed);
            }
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

    fn set_event_emitter(&mut self, tx: broadcast::Sender<EngineEvent>) {
        self.event_tx = Some(tx);
    }

    fn node_type(&self) -> &str {
        "ai-agent"
    }
}

fn parse_tools(arr: &[serde_json::Value]) -> Vec<ToolDefinition> {
    arr.iter()
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
        .collect()
}

impl AiAgentNode {
    /// Call OpenAI API and return the agent step + raw assistant message for history.
    async fn call_openai_agent(
        &self,
        client: &reqwest::Client,
        messages: &[serde_json::Value],
        timeout: std::time::Duration,
    ) -> Result<(AgentStep, serde_json::Value), String> {
        let base = if self.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/chat/completions", base);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": 2048,
        });

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
            .bearer_auth(&self.api_key)
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

        let assistant_msg = json["choices"][0]["message"].clone();

        // Check for tool calls
        if let Some(tool_calls) = assistant_msg["tool_calls"].as_array() {
            if let Some(tc) = tool_calls.first() {
                let tool_name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let arguments_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let arguments =
                    serde_json::from_str(arguments_str).unwrap_or(serde_json::json!({}));
                let tool_call_id = tc["id"].as_str().unwrap_or("call_0").to_string();

                return Ok((
                    AgentStep::ToolCall {
                        tool_name,
                        arguments,
                        tool_call_id,
                    },
                    assistant_msg,
                ));
            }
        }

        let content = assistant_msg["content"].as_str().unwrap_or("").to_string();
        Ok((AgentStep::TextResponse(content), assistant_msg))
    }

    /// Call Anthropic API.
    async fn call_anthropic_agent(
        &self,
        client: &reqwest::Client,
        messages: &[serde_json::Value],
        timeout: std::time::Duration,
    ) -> Result<(AgentStep, serde_json::Value), String> {
        let base = if self.base_url.is_empty() {
            "https://api.anthropic.com/v1"
        } else {
            &self.base_url
        };
        let url = format!("{}/messages", base);

        // Separate system message from conversation messages
        let (system_msg, conv_messages): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 2048,
            "messages": conv_messages,
        });

        if let Some(sys) = system_msg.first() {
            if let Some(content) = sys.get("content").and_then(|c| c.as_str()) {
                body["system"] = serde_json::Value::String(content.to_string());
            }
        }

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

        let assistant_msg = serde_json::json!({
            "role": "assistant",
            "content": json["content"]
        });

        // Check for tool use in content blocks
        if let Some(content) = json["content"].as_array() {
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let tool_name = block["name"].as_str().unwrap_or("unknown").to_string();
                    let arguments = block["input"].clone();
                    let tool_call_id = block["id"].as_str().unwrap_or("call_0").to_string();

                    return Ok((
                        AgentStep::ToolCall {
                            tool_name,
                            arguments,
                            tool_call_id,
                        },
                        assistant_msg,
                    ));
                }
            }
        }

        // Text response
        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok((AgentStep::TextResponse(content), assistant_msg))
    }

    /// Call Ollama API (no tool support yet, but returns same format).
    async fn call_ollama_agent(
        &self,
        client: &reqwest::Client,
        messages: &[serde_json::Value],
        timeout: std::time::Duration,
    ) -> Result<(AgentStep, serde_json::Value), String> {
        let base = if self.base_url.is_empty() {
            "http://localhost:11434"
        } else {
            &self.base_url
        };
        let url = format!("{}/api/chat", base);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": self.temperature,
                "num_predict": 2048,
            }
        });

        // Ollama supports tools for models like llama3.1+
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

        let assistant_msg = json["message"].clone();

        // Check for tool calls (Ollama format)
        if let Some(tool_calls) = json["message"]["tool_calls"].as_array() {
            if let Some(tc) = tool_calls.first() {
                let tool_name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let arguments = tc["function"]["arguments"].clone();

                return Ok((
                    AgentStep::ToolCall {
                        tool_name,
                        arguments,
                        tool_call_id: "call_0".to_string(),
                    },
                    assistant_msg,
                ));
            }
        }

        let content = json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok((AgentStep::TextResponse(content), assistant_msg))
    }
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
            event_tx: None,
            flow_id: None,
            node_id: None,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "ai-agent"
    }
}
