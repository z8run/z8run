//! Vector Store node: in-memory vector storage with cosine similarity search.
//!
//! Provides vector storage across collections with semantic search capabilities.
//!
//! Actions:
//!   - "store": Add vectors and metadata to a collection
//!   - "search": Find similar vectors using cosine similarity
//!   - "delete": Remove a specific vector entry
//!   - "clear": Clear an entire collection
//!
//! Outputs:
//!   - "stored" port: Confirmation of stored entry
//!   - "results" port: Search results with scores
//!   - "deleted" port: Confirmation of deletion
//!   - "cleared" port: Confirmation of clear operation
//!   - "error" port: Operation errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::info;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f64>,
    pub text: String,
    pub metadata: serde_json::Value,
}

// Global in-memory vector store
static GLOBAL_STORE: OnceLock<std::sync::Arc<tokio::sync::RwLock<HashMap<String, Vec<VectorEntry>>>>> =
    OnceLock::new();

fn get_store() -> &'static std::sync::Arc<tokio::sync::RwLock<HashMap<String, Vec<VectorEntry>>>> {
    GLOBAL_STORE.get_or_init(|| {
        std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new()))
    })
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

pub struct VectorStoreNode {
    name: String,
    action: String,           // "store", "search", "delete", "clear"
    collection: String,       // collection name
    top_k: usize,            // for search: return top K results
    min_score: f64,          // for search: minimum similarity threshold
}

#[async_trait::async_trait]
impl NodeExecutor for VectorStoreNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(node = %self.name, action = %self.action, collection = %self.collection, "Vector store operation");

        match self.action.as_str() {
            "store" => self.store(&msg).await,
            "search" => self.search(&msg).await,
            "delete" => self.delete(&msg).await,
            "clear" => self.clear(&msg).await,
            _ => {
                let err = serde_json::json!({
                    "error": format!("Unknown action: {}", self.action)
                });
                Ok(vec![msg.derive(msg.source_node, "error", err)])
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("action").and_then(|v| v.as_str()) {
            self.action = v.to_lowercase();
        }
        if let Some(v) = config.get("collection").and_then(|v| v.as_str()) {
            self.collection = v.to_string();
        }
        if let Some(v) = config.get("topK").and_then(|v| v.as_u64()) {
            self.top_k = v as usize;
        }
        if let Some(v) = config.get("minScore").and_then(|v| v.as_f64()) {
            self.min_score = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.action.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Vector store requires an action".to_string(),
            ));
        }
        match self.action.as_str() {
            "store" | "search" | "delete" | "clear" => Ok(()),
            _ => Err(crate::error::Z8Error::Internal(format!(
                "Invalid action: {}",
                self.action
            ))),
        }
    }

    fn node_type(&self) -> &str {
        "vector-store"
    }
}

impl VectorStoreNode {
    async fn store(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let store = get_store();

        // Extract embedding from payload
        let embedding = msg
            .payload
            .get("embedding")
            .and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_f64())
                        .collect::<Vec<f64>>()
                })
            })
            .unwrap_or_default();

        if embedding.is_empty() {
            let err = serde_json::json!({
                "error": "No embedding array provided in payload"
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        let text = msg
            .payload
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let id = msg
            .payload
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let metadata = msg
            .payload
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let entry = VectorEntry {
            id: id.clone(),
            embedding,
            text,
            metadata,
        };

        // Write to store
        let mut collections = store.write().await;
        collections
            .entry(self.collection.clone())
            .or_insert_with(Vec::new)
            .push(entry);

        let total_entries = collections
            .get(&self.collection)
            .map(|c| c.len())
            .unwrap_or(0);

        info!(
            node = %self.name,
            id = %id,
            collection = %self.collection,
            total_entries = total_entries,
            "Vector stored"
        );

        let payload = serde_json::json!({
            "id": id,
            "collection": self.collection,
            "total_entries": total_entries,
        });

        Ok(vec![msg.derive(msg.source_node, "stored", payload)])
    }

    async fn search(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let store = get_store();

        // Extract query embedding
        let query_embedding = msg
            .payload
            .get("embedding")
            .and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_f64())
                        .collect::<Vec<f64>>()
                })
            })
            .unwrap_or_default();

        if query_embedding.is_empty() {
            let err = serde_json::json!({
                "error": "No embedding array provided for search"
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        let collections = store.read().await;
        let entries = collections
            .get(&self.collection)
            .cloned()
            .unwrap_or_default();

        if entries.is_empty() {
            let payload = serde_json::json!({
                "results": [],
                "query_dimensions": query_embedding.len(),
                "collection": self.collection,
            });
            return Ok(vec![msg.derive(msg.source_node, "results", payload)]);
        }

        // Compute similarities
        let mut scored_results: Vec<(String, String, f64, serde_json::Value)> = entries
            .iter()
            .map(|entry| {
                let score = cosine_similarity(&query_embedding, &entry.embedding);
                (
                    entry.id.clone(),
                    entry.text.clone(),
                    score,
                    entry.metadata.clone(),
                )
            })
            .filter(|(_, _, score, _)| *score >= self.min_score)
            .collect();

        // Sort by score descending
        scored_results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Take top K
        let top_results: Vec<serde_json::Value> = scored_results
            .iter()
            .take(self.top_k)
            .map(|(id, text, score, metadata)| {
                serde_json::json!({
                    "id": id,
                    "text": text,
                    "score": score,
                    "metadata": metadata,
                })
            })
            .collect();

        info!(
            node = %self.name,
            collection = %self.collection,
            results = top_results.len(),
            "Search completed"
        );

        let payload = serde_json::json!({
            "results": top_results,
            "query_dimensions": query_embedding.len(),
            "collection": self.collection,
        });

        Ok(vec![msg.derive(msg.source_node, "results", payload)])
    }

    async fn delete(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let store = get_store();

        let id = msg
            .payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if id.is_empty() {
            let err = serde_json::json!({
                "error": "No id provided for deletion"
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err)]);
        }

        let mut collections = store.write().await;
        let mut deleted = false;

        if let Some(entries) = collections.get_mut(&self.collection) {
            let initial_len = entries.len();
            entries.retain(|e| e.id != id);
            deleted = entries.len() < initial_len;
        }

        info!(
            node = %self.name,
            id = %id,
            collection = %self.collection,
            deleted = deleted,
            "Vector deleted"
        );

        let payload = serde_json::json!({
            "id": id,
            "collection": self.collection,
            "deleted": deleted,
        });

        Ok(vec![msg.derive(msg.source_node, "deleted", payload)])
    }

    async fn clear(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let store = get_store();
        let mut collections = store.write().await;

        let count = collections
            .remove(&self.collection)
            .map(|c| c.len())
            .unwrap_or(0);

        info!(
            node = %self.name,
            collection = %self.collection,
            entries_cleared = count,
            "Collection cleared"
        );

        let payload = serde_json::json!({
            "collection": self.collection,
            "entries_cleared": count,
        });

        Ok(vec![msg.derive(msg.source_node, "cleared", payload)])
    }
}

pub struct VectorStoreNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for VectorStoreNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = VectorStoreNode {
            name: "VectorStore".to_string(),
            action: "search".to_string(),
            collection: "default".to_string(),
            top_k: 5,
            min_score: 0.0,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "vector-store"
    }
}
