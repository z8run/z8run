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

    /// Saves a flow with an owner user_id.
    async fn save_flow_with_user(&self, flow: &Flow, user_id: Uuid) -> Result<(), StorageError>;

    /// Lists flows belonging to a specific user.
    async fn list_flows_by_user(&self, user_id: Uuid) -> Result<Vec<Flow>, StorageError>;

    /// Gets a flow only if it belongs to the given user.
    async fn get_flow_for_user(&self, id: Uuid, user_id: Uuid) -> Result<Flow, StorageError>;

    /// Deletes a flow only if it belongs to the given user.
    async fn delete_flow_for_user(&self, id: Uuid, user_id: Uuid) -> Result<(), StorageError>;
}

/// User record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Connection record (decrypted).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub conn_type: String,
    pub data: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
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

/// Repository trait for users.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    /// Creates a new user.
    async fn create_user(&self, user: &UserRecord) -> Result<(), StorageError>;

    /// Gets a user by ID.
    async fn get_user_by_id(&self, id: Uuid) -> Result<UserRecord, StorageError>;

    /// Gets a user by email.
    async fn get_user_by_email(&self, email: &str) -> Result<UserRecord, StorageError>;

    /// Gets a user by username.
    async fn get_user_by_username(&self, username: &str) -> Result<UserRecord, StorageError>;
}
