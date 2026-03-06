//! Text Splitter node: splits text into chunks for embedding/RAG pipelines.
//!
//! Supports multiple splitting strategies:
//!   - "fixed": split by character count with overlap
//!   - "sentences": split on sentence boundaries
//!   - "paragraphs": split on double newlines
//!   - "tokens": approximate token count based on character count
//!
//! Outputs:
//!   - "chunks" port: array of text chunks with metadata
//!   - "error" port: on processing errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use tracing::info;

pub struct TextSplitterNode {
    name: String,
    strategy: String,
    chunk_size: usize,
    overlap: usize,
    separator: Option<String>,
}

#[async_trait::async_trait]
impl NodeExecutor for TextSplitterNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let text = extract_text(&msg.payload);
        if text.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No text found in message",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        info!(
            node = %self.name,
            strategy = %self.strategy,
            text_len = text.len(),
            "Splitting text"
        );

        let chunks = match self.strategy.as_str() {
            "sentences" => split_by_sentences(&text, self.chunk_size, self.overlap),
            "paragraphs" => split_by_paragraphs(&text, self.chunk_size, self.overlap),
            "tokens" => split_by_tokens(&text, self.chunk_size, self.overlap),
            _ => split_by_fixed(&text, self.chunk_size, self.overlap), // default to fixed
        };

        let total_chunks = chunks.len();
        let chunks_json: Vec<serde_json::Value> = chunks
            .into_iter()
            .enumerate()
            .map(|(idx, (text, start, end))| {
                serde_json::json!({
                    "text": text,
                    "index": idx,
                    "start": start,
                    "end": end,
                })
            })
            .collect();

        let output_payload = serde_json::json!({
            "chunks": chunks_json,
            "total_chunks": total_chunks,
            "strategy": self.strategy,
        });

        info!(node = %self.name, total_chunks = total_chunks, "Text split complete");
        Ok(vec![msg.derive(msg.source_node, "chunks", output_payload)])
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("strategy").and_then(|v| v.as_str()) {
            self.strategy = v.to_string();
        }
        if let Some(v) = config.get("chunkSize").and_then(|v| v.as_u64()) {
            self.chunk_size = v as usize;
        }
        if let Some(v) = config.get("overlap").and_then(|v| v.as_u64()) {
            self.overlap = v as usize;
        }
        if let Some(v) = config.get("separator").and_then(|v| v.as_str()) {
            self.separator = Some(v.to_string());
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.chunk_size == 0 {
            return Err(crate::error::Z8Error::Internal(
                "Text splitter requires chunkSize > 0".to_string(),
            ));
        }
        if self.overlap >= self.chunk_size {
            return Err(crate::error::Z8Error::Internal(
                "Text splitter overlap must be less than chunkSize".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "text-splitter"
    }
}

/// Split text by fixed character count with overlap.
/// Returns (text, start_pos, end_pos) tuples.
fn split_by_fixed(text: &str, chunk_size: usize, overlap: usize) -> Vec<(String, usize, usize)> {
    let mut chunks = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        let end = std::cmp::min(start + chunk_size, bytes.len());
        if let Ok(chunk_text) = std::str::from_utf8(&bytes[start..end]) {
            chunks.push((chunk_text.to_string(), start, end));
        }

        if end >= bytes.len() {
            break;
        }
        start = end.saturating_sub(overlap);
    }

    chunks
}

/// Split text by sentence boundaries.
fn split_by_sentences(
    text: &str,
    chunk_size: usize,
    overlap: usize,
) -> Vec<(String, usize, usize)> {
    let sentences: Vec<&str> = text
        .split(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut start_pos = 0;
    let mut last_end: usize = 0;

    for sentence in sentences {
        if current_chunk.len() + sentence.len() + 1 > chunk_size && !current_chunk.is_empty() {
            chunks.push((current_chunk.clone(), start_pos, last_end));
            // Apply overlap
            let overlap_chars = current_chunk.chars().take(overlap).collect::<String>();
            current_chunk = overlap_chars;
            start_pos = last_end.saturating_sub(overlap);
        }

        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(sentence);
        last_end = text.find(sentence).unwrap_or(last_end) + sentence.len();
    }

    if !current_chunk.is_empty() {
        chunks.push((current_chunk, start_pos, last_end));
    }

    chunks
}

/// Split text by paragraph boundaries (double newlines).
fn split_by_paragraphs(
    text: &str,
    chunk_size: usize,
    overlap: usize,
) -> Vec<(String, usize, usize)> {
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut start_pos = 0;
    let mut last_end: usize = 0;

    for paragraph in paragraphs {
        if current_chunk.len() + paragraph.len() + 1 > chunk_size && !current_chunk.is_empty() {
            chunks.push((current_chunk.clone(), start_pos, last_end));
            let overlap_chars = current_chunk.chars().take(overlap).collect::<String>();
            current_chunk = overlap_chars;
            start_pos = last_end.saturating_sub(overlap);
        }

        if !current_chunk.is_empty() {
            current_chunk.push('\n');
        }
        current_chunk.push_str(paragraph);
        last_end = text.find(paragraph).unwrap_or(last_end) + paragraph.len();
    }

    if !current_chunk.is_empty() {
        chunks.push((current_chunk, start_pos, last_end));
    }

    chunks
}

/// Split by approximate token count (rough estimate: chars/4 per token).
fn split_by_tokens(text: &str, token_limit: usize, overlap: usize) -> Vec<(String, usize, usize)> {
    let char_limit = token_limit * 4; // Rough estimate: 4 chars per token
    let mut chunks = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        let end = std::cmp::min(start + char_limit, bytes.len());
        if let Ok(chunk_text) = std::str::from_utf8(&bytes[start..end]) {
            chunks.push((chunk_text.to_string(), start, end));
        }

        if end >= bytes.len() {
            break;
        }
        start = end.saturating_sub(overlap * 4);
    }

    chunks
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

pub struct TextSplitterNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for TextSplitterNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = TextSplitterNode {
            name: "TextSplitter".to_string(),
            strategy: "fixed".to_string(),
            chunk_size: 512,
            overlap: 50,
            separator: None,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "text-splitter"
    }
}
