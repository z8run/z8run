//! Conversation Memory node: stores and retrieves conversation history for multi-session agents.
//!
//! Supports multiple conversation storage actions:
//! - **save**: Append a message to a conversation history
//! - **load**: Retrieve full conversation history
//! - **clear**: Remove a conversation from storage
//! - **list**: Get all conversations with metadata
//!
//! Outputs:
//!   - "saved" port: Confirmation of message appended (save action)
//!   - "history" port: Retrieved conversation history (load action)
//!   - "listed" port: All conversations metadata (list action)
//!   - "cleared" port: Confirmation of deletion (clear action)
//!   - "error" port: Errors during operation

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Stores conversation history and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationData {
    /// Messages in the conversation.
    pub messages: Vec<Value>,
    /// Timestamp when conversation was created (seconds since epoch).
    pub created_at: u64,
    /// Timestamp when conversation was last updated (seconds since epoch).
    pub updated_at: u64,
    /// Additional metadata about the conversation.
    #[serde(default)]
    pub metadata: Value,
}

impl ConversationData {
    /// Creates a new conversation.
    fn new() -> Self {
        let now = current_timestamp();
        Self {
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: Value::Object(serde_json::Map::new()),
        }
    }

    /// Appends a message and updates the timestamp.
    fn append_message(&mut self, message: Value) {
        self.messages.push(message);
        self.updated_at = current_timestamp();
    }

    /// Checks if conversation has expired based on TTL.
    fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = current_timestamp();
        self.updated_at + ttl_seconds < now
    }
}

/// Conversation Memory node for storing/retrieving conversation history.
pub struct ConversationMemoryNode {
    name: String,
    /// Action to perform: "save", "load", "clear", "list".
    action: String,
    /// Maximum number of messages to store per conversation.
    max_messages: usize,
    /// Time-to-live in seconds. Conversations older than this are removed on load.
    ttl_seconds: u64,
    /// Thread-safe storage of conversations.
    conversations: Arc<RwLock<HashMap<String, ConversationData>>>,
}

/// Extract conversation ID from payload using multiple possible field names.
fn extract_conversation_id(payload: &Value) -> Option<String> {
    // Try multiple field name variations
    for field in &["conversationId", "conversation_id", "sessionId", "session_id", "chatId"] {
        if let Some(id) = payload.get(field).and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    None
}

/// Extract message from payload.
/// If payload has a "role" field, treat it as a chat message {role, content}.
/// Otherwise, use the "message" field or entire payload if it has message-like structure.
fn extract_message(payload: &Value) -> Value {
    // If it has a "role" field, it's likely chat format {role, content}
    if payload.get("role").is_some() {
        return payload.clone();
    }

    // Try to extract "message" field
    if let Some(msg) = payload.get("message") {
        return msg.clone();
    }

    // Fall back to entire payload
    payload.clone()
}

/// Extract metadata from payload if present.
fn extract_metadata(payload: &Value) -> Value {
    payload
        .get("metadata")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

/// Get current timestamp as seconds since Unix epoch.
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[async_trait::async_trait]
impl NodeExecutor for ConversationMemoryNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        match self.action.as_str() {
            "save" => self.handle_save(msg).await,
            "load" => self.handle_load(msg).await,
            "clear" => self.handle_clear(msg).await,
            "list" => self.handle_list(msg).await,
            _ => {
                let err_payload = serde_json::json!({
                    "error": format!("Unknown conversation memory action: {}. Expected 'save', 'load', 'clear', or 'list'", self.action),
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("action").and_then(|v| v.as_str()) {
            self.action = v.to_string();
        }
        if let Some(v) = config.get("maxMessages").and_then(|v| v.as_u64()) {
            self.max_messages = v as usize;
        }
        if let Some(v) = config.get("ttlSeconds").and_then(|v| v.as_u64()) {
            self.ttl_seconds = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "conversation-memory"
    }
}

impl ConversationMemoryNode {
    /// Saves a message to a conversation.
    async fn handle_save(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let conversation_id = match extract_conversation_id(&msg.payload) {
            Some(id) => id,
            None => {
                let err_payload = serde_json::json!({
                    "error": "Missing conversation ID in payload. Provide one of: conversationId, conversation_id, sessionId, session_id, chatId"
                });
                return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
            }
        };

        let message = extract_message(&msg.payload);
        let metadata = extract_metadata(&msg.payload);

        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .entry(conversation_id.clone())
            .or_insert_with(ConversationData::new);

        conversation.append_message(message);

        // Update metadata if provided
        if !matches!(metadata, Value::Null) && metadata.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
            conversation.metadata = metadata;
        }

        // Trim to max_messages
        if conversation.messages.len() > self.max_messages {
            let excess = conversation.messages.len() - self.max_messages;
            conversation.messages.drain(0..excess);
        }

        let output_payload = serde_json::json!({
            "conversationId": conversation_id,
            "messageCount": conversation.messages.len(),
            "createdAt": conversation.created_at,
            "updatedAt": conversation.updated_at,
        });

        info!(
            conversation_id = %conversation_id,
            message_count = conversation.messages.len(),
            "Message saved to conversation"
        );

        Ok(vec![msg.derive(msg.source_node, "saved", output_payload)])
    }

    /// Loads conversation history.
    async fn handle_load(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let conversation_id = match extract_conversation_id(&msg.payload) {
            Some(id) => id,
            None => {
                let err_payload = serde_json::json!({
                    "error": "Missing conversation ID in payload. Provide one of: conversationId, conversation_id, sessionId, session_id, chatId"
                });
                return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
            }
        };

        let mut conversations = self.conversations.write().await;

        // Check if conversation exists and is not expired
        if let Some(conversation) = conversations.get(&conversation_id) {
            if conversation.is_expired(self.ttl_seconds) {
                info!(
                    conversation_id = %conversation_id,
                    ttl_seconds = self.ttl_seconds,
                    "Conversation expired, removing from storage"
                );
                conversations.remove(&conversation_id);

                let output_payload = serde_json::json!({
                    "conversationId": conversation_id,
                    "messages": [],
                    "messageCount": 0,
                    "createdAt": null,
                    "updatedAt": null,
                });

                return Ok(vec![msg.derive(msg.source_node, "history", output_payload)]);
            }
        }

        // Get conversation or return empty
        let (messages, created_at, updated_at) = if let Some(conversation) = conversations.get(&conversation_id) {
            (
                conversation.messages.clone(),
                conversation.created_at,
                conversation.updated_at,
            )
        } else {
            (Vec::new(), 0, 0)
        };

        let output_payload = serde_json::json!({
            "conversationId": conversation_id,
            "messages": messages,
            "messageCount": messages.len(),
            "createdAt": if created_at > 0 { created_at } else { Value::Null },
            "updatedAt": if updated_at > 0 { updated_at } else { Value::Null },
        });

        info!(
            conversation_id = %conversation_id,
            message_count = messages.len(),
            "Conversation history loaded"
        );

        Ok(vec![msg.derive(msg.source_node, "history", output_payload)])
    }

    /// Clears a conversation from storage.
    async fn handle_clear(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let conversation_id = match extract_conversation_id(&msg.payload) {
            Some(id) => id,
            None => {
                let err_payload = serde_json::json!({
                    "error": "Missing conversation ID in payload. Provide one of: conversationId, conversation_id, sessionId, session_id, chatId"
                });
                return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
            }
        };

        let mut conversations = self.conversations.write().await;
        let existed = conversations.remove(&conversation_id).is_some();

        let output_payload = serde_json::json!({
            "conversationId": conversation_id,
            "cleared": existed,
        });

        info!(
            conversation_id = %conversation_id,
            existed = existed,
            "Conversation cleared from storage"
        );

        Ok(vec![msg.derive(msg.source_node, "cleared", output_payload)])
    }

    /// Lists all conversations with metadata.
    async fn handle_list(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let conversations = self.conversations.read().await;

        let mut conversations_list = Vec::new();
        for (conversation_id, data) in conversations.iter() {
            conversations_list.push(serde_json::json!({
                "conversationId": conversation_id,
                "messageCount": data.messages.len(),
                "createdAt": data.created_at,
                "updatedAt": data.updated_at,
            }));
        }

        let output_payload = serde_json::json!({
            "conversations": conversations_list,
            "total": conversations.len(),
        });

        info!(
            total_conversations = conversations.len(),
            "Listed all conversations"
        );

        Ok(vec![msg.derive(msg.source_node, "listed", output_payload)])
    }
}

/// Factory for creating ConversationMemoryNode instances.
pub struct ConversationMemoryNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for ConversationMemoryNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = ConversationMemoryNode {
            name: "ConversationMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "conversation-memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_extract_conversation_id_variants() {
        let payload_1 = serde_json::json!({"conversationId": "conv123"});
        assert_eq!(extract_conversation_id(&payload_1), Some("conv123".to_string()));

        let payload_2 = serde_json::json!({"conversation_id": "conv456"});
        assert_eq!(extract_conversation_id(&payload_2), Some("conv456".to_string()));

        let payload_3 = serde_json::json!({"sessionId": "sess789"});
        assert_eq!(extract_conversation_id(&payload_3), Some("sess789".to_string()));

        let payload_4 = serde_json::json!({"session_id": "sess101"});
        assert_eq!(extract_conversation_id(&payload_4), Some("sess101".to_string()));

        let payload_5 = serde_json::json!({"chatId": "chat202"});
        assert_eq!(extract_conversation_id(&payload_5), Some("chat202".to_string()));

        let payload_6 = serde_json::json!({"other": "value"});
        assert_eq!(extract_conversation_id(&payload_6), None);
    }

    #[test]
    fn test_extract_message_chat_format() {
        let payload = serde_json::json!({
            "conversationId": "conv123",
            "role": "user",
            "content": "Hello world"
        });

        let msg = extract_message(&payload);
        assert_eq!(msg.get("role").and_then(|v| v.as_str()), Some("user"));
        assert_eq!(msg.get("content").and_then(|v| v.as_str()), Some("Hello world"));
    }

    #[test]
    fn test_extract_message_explicit_field() {
        let payload = serde_json::json!({
            "conversationId": "conv123",
            "message": "Hello world"
        });

        let msg = extract_message(&payload);
        assert_eq!(msg.as_str(), Some("Hello world"));
    }

    #[test]
    fn test_extract_message_fallback() {
        let payload = serde_json::json!({
            "conversationId": "conv123",
            "text": "Hello world"
        });

        let msg = extract_message(&payload);
        assert_eq!(msg.get("text").and_then(|v| v.as_str()), Some("Hello world"));
    }

    #[test]
    fn test_conversation_data_creation() {
        let conv = ConversationData::new();
        assert!(conv.messages.is_empty());
        assert!(conv.created_at > 0);
        assert_eq!(conv.created_at, conv.updated_at);
    }

    #[test]
    fn test_conversation_data_append() {
        let mut conv = ConversationData::new();
        let initial_time = conv.created_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        conv.append_message(serde_json::json!({"role": "user", "content": "hi"}));
        conv.append_message(serde_json::json!({"role": "assistant", "content": "hello"}));

        assert_eq!(conv.messages.len(), 2);
        assert!(conv.updated_at >= initial_time);
    }

    #[test]
    fn test_conversation_data_expiry() {
        let mut conv = ConversationData::new();
        // Set a very old timestamp
        conv.updated_at = 0;

        assert!(conv.is_expired(10)); // Expired if TTL is 10 seconds
        assert!(!conv.is_expired(u64::MAX)); // Not expired if TTL is very large
    }

    #[tokio::test]
    async fn test_save_action() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();
        let payload = serde_json::json!({
            "conversationId": "conv123",
            "message": "Hello world"
        });

        let msg = FlowMessage::new(source_id, "input", payload, trace_id);
        let results = node.process(msg).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "saved");

        let output = &results[0].payload;
        assert_eq!(
            output.get("conversationId").and_then(|v| v.as_str()),
            Some("conv123")
        );
        assert_eq!(output.get("messageCount").and_then(|v| v.as_u64()), Some(1));

        // Verify message was stored
        let conversations = node.conversations.read().await;
        assert!(conversations.contains_key("conv123"));
        assert_eq!(conversations.get("conv123").unwrap().messages.len(), 1);
    }

    #[tokio::test]
    async fn test_load_action_empty() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "load".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();
        let payload = serde_json::json!({
            "conversationId": "conv_nonexistent"
        });

        let msg = FlowMessage::new(source_id, "input", payload, trace_id);
        let results = node.process(msg).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "history");

        let output = &results[0].payload;
        assert_eq!(
            output.get("conversationId").and_then(|v| v.as_str()),
            Some("conv_nonexistent")
        );
        let messages = output.get("messages").and_then(|v| v.as_array()).unwrap();
        assert!(messages.is_empty());
        assert_eq!(output.get("messageCount").and_then(|v| v.as_u64()), Some(0));
    }

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();

        // Save first message
        let save_payload_1 = serde_json::json!({
            "conversationId": "conv123",
            "message": {"role": "user", "content": "Hello"}
        });
        let save_msg_1 = FlowMessage::new(source_id, "input", save_payload_1, trace_id);
        node.process(save_msg_1).await.unwrap();

        // Save second message
        let save_payload_2 = serde_json::json!({
            "conversationId": "conv123",
            "message": {"role": "assistant", "content": "Hi there"}
        });
        let save_msg_2 = FlowMessage::new(source_id, "input", save_payload_2, trace_id);
        node.process(save_msg_2).await.unwrap();

        // Load the conversation
        let load_node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "load".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: node.conversations.clone(),
        };

        let load_payload = serde_json::json!({
            "conversationId": "conv123"
        });
        let load_msg = FlowMessage::new(source_id, "input", load_payload, trace_id);
        let results = load_node.process(load_msg).await.unwrap();

        assert_eq!(results.len(), 1);
        let output = &results[0].payload;
        assert_eq!(output.get("messageCount").and_then(|v| v.as_u64()), Some(2));

        let messages = output.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].get("content").and_then(|v| v.as_str()),
            Some("Hello")
        );
        assert_eq!(
            messages[1].get("content").and_then(|v| v.as_str()),
            Some("Hi there")
        );
    }

    #[tokio::test]
    async fn test_clear_action() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();

        // Save a message first
        let save_payload = serde_json::json!({
            "conversationId": "conv123",
            "message": "Hello"
        });
        let save_msg = FlowMessage::new(source_id, "input", save_payload, trace_id);
        node.process(save_msg).await.unwrap();

        // Now clear the conversation
        let clear_node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "clear".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: node.conversations.clone(),
        };

        let clear_payload = serde_json::json!({
            "conversationId": "conv123"
        });
        let clear_msg = FlowMessage::new(source_id, "input", clear_payload, trace_id);
        let results = clear_node.process(clear_msg).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "cleared");

        let output = &results[0].payload;
        assert_eq!(output.get("cleared").and_then(|v| v.as_bool()), Some(true));

        // Verify conversation was removed
        let conversations = clear_node.conversations.read().await;
        assert!(!conversations.contains_key("conv123"));
    }

    #[tokio::test]
    async fn test_list_action() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();

        // Save messages to two conversations
        for conv_id in &["conv1", "conv2"] {
            for i in 0..3 {
                let payload = serde_json::json!({
                    "conversationId": conv_id,
                    "message": format!("Message {}", i)
                });
                let msg = FlowMessage::new(source_id, "input", payload, trace_id);
                node.process(msg).await.unwrap();
            }
        }

        // List all conversations
        let list_node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "list".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: node.conversations.clone(),
        };

        let list_payload = serde_json::json!({});
        let list_msg = FlowMessage::new(source_id, "input", list_payload, trace_id);
        let results = list_node.process(list_msg).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "listed");

        let output = &results[0].payload;
        assert_eq!(output.get("total").and_then(|v| v.as_u64()), Some(2));

        let conversations = output.get("conversations").and_then(|v| v.as_array()).unwrap();
        assert_eq!(conversations.len(), 2);

        // Both should have 3 messages
        for conv in conversations {
            assert_eq!(conv.get("messageCount").and_then(|v| v.as_u64()), Some(3));
        }
    }

    #[tokio::test]
    async fn test_max_messages_trimming() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 3, // Only keep 3 messages
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();

        // Save 5 messages (should trim to 3)
        for i in 0..5 {
            let payload = serde_json::json!({
                "conversationId": "conv123",
                "message": format!("Message {}", i)
            });
            let msg = FlowMessage::new(source_id, "input", payload, trace_id);
            node.process(msg).await.unwrap();
        }

        // Verify only 3 messages are kept (and they're the last 3)
        let conversations = node.conversations.read().await;
        let conv = conversations.get("conv123").unwrap();
        assert_eq!(conv.messages.len(), 3);

        // Check that we have messages 2, 3, 4 (the last 3)
        assert_eq!(
            conv.messages[0].as_str().map(|s| s.contains("2")),
            Some(true)
        );
        assert_eq!(
            conv.messages[1].as_str().map(|s| s.contains("3")),
            Some(true)
        );
        assert_eq!(
            conv.messages[2].as_str().map(|s| s.contains("4")),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_missing_conversation_id_error() {
        let node = ConversationMemoryNode {
            name: "TestMemory".to_string(),
            action: "save".to_string(),
            max_messages: 50,
            ttl_seconds: 3600,
            conversations: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_id = Uuid::now_v7();
        let trace_id = Uuid::now_v7();
        let payload = serde_json::json!({
            "message": "Hello world"
            // Missing conversation ID
        });

        let msg = FlowMessage::new(source_id, "input", payload, trace_id);
        let results = node.process(msg).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        let output = &results[0].payload;
        assert!(output
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("Missing conversation ID"));
    }

    #[tokio::test]
    async fn test_factory_creation() {
        let factory = ConversationMemoryNodeFactory;
        let config = serde_json::json!({
            "name": "MyMemory",
            "action": "save",
            "maxMessages": 100,
            "ttlSeconds": 7200
        });

        let executor = factory.create(config).await.unwrap();
        assert_eq!(executor.node_type(), "conversation-memory");
    }

    #[test]
    fn test_factory_node_type() {
        let factory = ConversationMemoryNodeFactory;
        assert_eq!(factory.node_type(), "conversation-memory");
    }
}
