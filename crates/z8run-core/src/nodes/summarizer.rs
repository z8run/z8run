//! Summarizer node: summarizes text using LLM with map-reduce for long texts.
//!
//! Supports two strategies:
//!   - "simple": sends entire text to LLM for summarization
//!   - "map-reduce": splits long texts, summarizes each chunk, then summarizes summaries
//!
//! Outputs:
//!   - "summary" port: summarized text with metadata
//!   - "error" port: on processing errors

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::extract::TEXT_FIELDS;
use crate::utils::llm_client::{call_llm, LlmCallParams};
use tracing::{info, warn};

pub struct SummarizerNode {
    name: String,
    provider: String,
    model: String,
    api_key: String,
    base_url: String,
    strategy: String,
    max_length: usize,
    language: String,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for SummarizerNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let text = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);
        if text.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No text found in message",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        let original_length = text.split_whitespace().count();
        info!(
            node = %self.name,
            strategy = %self.strategy,
            text_length = original_length,
            "Summarization request"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let summary_result = match self.strategy.as_str() {
            "map-reduce" => {
                if text.len() > 4000 {
                    self.summarize_map_reduce(&client, &text, timeout).await
                } else {
                    self.summarize_simple(&client, &text, timeout).await
                }
            }
            _ => self.summarize_simple(&client, &text, timeout).await,
        };

        match summary_result {
            Ok(summary) => {
                let summary_length = summary.split_whitespace().count();
                info!(
                    node = %self.name,
                    original_length = original_length,
                    summary_length = summary_length,
                    "Summarization complete"
                );

                let output_payload = serde_json::json!({
                    "summary": summary,
                    "original_length": original_length,
                    "summary_length": summary_length,
                    "strategy": self.strategy,
                });

                Ok(vec![msg.derive(msg.source_node, "summary", output_payload)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "Summarization failed");
                let err_payload = serde_json::json!({
                    "error": e,
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
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
            "strategy" => strategy: str,
            "maxLength" => max_length: usize,
            "language" => language: str,
            "timeout" => timeout_ms: u64,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.provider != "ollama" && self.api_key.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Summarizer requires an API key (except for Ollama)".to_string(),
            ));
        }
        if self.max_length == 0 {
            return Err(crate::error::Z8Error::Internal(
                "Summarizer requires maxLength > 0".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "summarizer"
    }
}

impl SummarizerNode {
    /// Simple summarization: send entire text to LLM.
    async fn summarize_simple(
        &self,
        client: &reqwest::Client,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        let language_instruction = if self.language.is_empty() {
            "".to_string()
        } else {
            format!("Use {} for the summary. ", self.language)
        };

        let system_prompt = format!(
            "You are an expert summarizer. Summarize the given text concisely in approximately {} words. {}Keep the summary informative and clear.",
            self.max_length, language_instruction
        );

        call_llm(
            client,
            &LlmCallParams {
                provider: &self.provider,
                model: &self.model,
                api_key: &self.api_key,
                base_url: &self.base_url,
                system_prompt: &system_prompt,
                user_prompt: text,
                max_tokens: 2048,
                temperature: 0.1,
                timeout,
            },
        )
        .await
    }

    /// Map-reduce summarization: split text, summarize chunks, then summarize summaries.
    async fn summarize_map_reduce(
        &self,
        client: &reqwest::Client,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        info!(node = %self.name, "Using map-reduce strategy for long text");

        // Map phase: split and summarize chunks
        let chunks = split_text_for_summarization(text, 3000);
        let mut chunk_summaries = Vec::new();

        for (idx, chunk) in chunks.iter().enumerate() {
            info!(node = %self.name, chunk = idx + 1, total = chunks.len(), "Summarizing chunk");

            let language_instruction = if self.language.is_empty() {
                "".to_string()
            } else {
                format!("Use {} for the summary. ", self.language)
            };

            let system_prompt = format!(
                "You are an expert summarizer. Summarize this text chunk in approximately {} words. {}Keep it concise.",
                self.max_length / 2, language_instruction
            );

            match call_llm(
                client,
                &LlmCallParams {
                    provider: &self.provider,
                    model: &self.model,
                    api_key: &self.api_key,
                    base_url: &self.base_url,
                    system_prompt: &system_prompt,
                    user_prompt: chunk,
                    max_tokens: 2048,
                    temperature: 0.1,
                    timeout,
                },
            )
            .await
            {
                Ok(summary) => chunk_summaries.push(summary),
                Err(e) => {
                    warn!(node = %self.name, chunk = idx, error = %e, "Failed to summarize chunk");
                    return Err(format!("Chunk summarization failed: {}", e));
                }
            }
        }

        // Reduce phase: summarize the summaries
        let combined_summaries = chunk_summaries.join("\n\n");

        let language_instruction = if self.language.is_empty() {
            "".to_string()
        } else {
            format!("Use {} for the final summary. ", self.language)
        };

        let final_system_prompt = format!(
            "You are an expert summarizer. Create a comprehensive final summary from these chunk summaries in approximately {} words. {}Keep it clear and coherent.",
            self.max_length, language_instruction
        );

        call_llm(
            client,
            &LlmCallParams {
                provider: &self.provider,
                model: &self.model,
                api_key: &self.api_key,
                base_url: &self.base_url,
                system_prompt: &final_system_prompt,
                user_prompt: &combined_summaries,
                max_tokens: 2048,
                temperature: 0.1,
                timeout,
            },
        )
        .await
    }
}

/// Split text into chunks for map phase (sentence-aware).
fn split_text_for_summarization(text: &str, chunk_size: usize) -> Vec<String> {
    let sentences: Vec<&str> = text
        .split(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for sentence in sentences {
        if current_chunk.len() + sentence.len() + 2 > chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }

        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(sentence);
        current_chunk.push('.');
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

node_factory!(SummarizerNodeFactory, SummarizerNode, "summarizer", {
    name: "Summarizer".to_string(),
    provider: "openai".to_string(),
    model: "gpt-4o-mini".to_string(),
    api_key: String::new(),
    base_url: String::new(),
    strategy: "simple".to_string(),
    max_length: 200,
    language: String::new(),
    timeout_ms: 30000
});
