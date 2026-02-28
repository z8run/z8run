//! Flow repository: CRUD operations on the database.

use uuid::Uuid;
use z8run_core::flow::Flow;

use crate::StorageError;

/// Repository trait for flows.
/// Implemented for both SQLite and PostgreSQL.
#[async_trait::async_trait]
pub trait FlowRepository: Send + Sync {
    /// Saves a flow (creates or updates).
    async fn save_flow(&self, flow: &Flow) -> Result<(), StorageError>;

    /// Gets a flow by ID.
    async fn get_flow(&self, id: Uuid) -> Result<Flow, StorageError>;

    /// Lists all flows.
    async fn list_flows(&self) -> Result<Vec<Flow>, StorageError>;

    /// Deletes a flow by ID.
    async fn delete_flow(&self, id: Uuid) -> Result<(), StorageError>;

    /// Searches flows by name (partial match).
    async fn search_flows(&self, query: &str) -> Result<Vec<Flow>, StorageError>;
}

/// Record of a flow execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionRecord {
    pub id: Uuid,
    pub flow_id: Uuid,
    pub trace_id: Uuid,
    pub status: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub node_logs: serde_json::Value,
}

/// Repository trait for execution history.
#[async_trait::async_trait]
pub trait ExecutionRepository: Send + Sync {
    /// Records the start of an execution.
    async fn record_start(&self, flow_id: Uuid, trace_id: Uuid) -> Result<Uuid, StorageError>;

    /// Records the completion of an execution.
    async fn record_completion(
        &self,
        execution_id: Uuid,
        status: &str,
        duration_ms: u64,
        error: Option<&str>,
    ) -> Result<(), StorageError>;

    /// Gets the execution history of a flow.
    async fn get_history(
        &self,
        flow_id: Uuid,
        limit: usize,
    ) -> Result<Vec<ExecutionRecord>, StorageError>;
}
