//! Application shared state.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use z8run_core::engine::FlowEngine;
use z8run_core::nodes::http_out::{self, WebhookResponders};
use z8run_storage::credential_vault::CredentialVault;
use z8run_storage::repository::{ExecutionRepository, FlowRepository, UserRepository};

/// Global application state, shared between handlers.
pub struct AppState {
    /// Flow engine.
    pub engine: FlowEngine,
    /// Storage backend (SQLite or PostgreSQL).
    pub storage: Arc<dyn FlowRepository>,
    /// User storage backend for authentication.
    pub user_storage: Arc<dyn UserRepository>,
    /// Execution-history storage (records flow runs).
    pub executions: Arc<dyn ExecutionRepository>,
    /// Credential vault for storing encrypted secrets.
    pub vault: Arc<dyn CredentialVault>,
    /// Secret for signing JWT tokens.
    pub jwt_secret: String,
    /// Server port.
    pub port: u16,
    /// Hook response channels keyed by trace_id.
    pub webhook_responders: WebhookResponders,
}

impl AppState {
    pub fn new(
        storage: Arc<dyn FlowRepository>,
        user_storage: Arc<dyn UserRepository>,
        executions: Arc<dyn ExecutionRepository>,
        vault: Arc<dyn CredentialVault>,
        jwt_secret: String,
        port: u16,
    ) -> Self {
        let responders: WebhookResponders = Arc::new(RwLock::new(HashMap::new()));
        // Initialize the global responder map so http-out nodes can access it
        http_out::init_webhook_responders(Arc::clone(&responders));

        let engine = FlowEngine::new();

        // Persist execution history: a background task subscribes to engine
        // events and records each run's start/completion (FUNC-008). It covers
        // every trigger source (API and hooks) since all execution flows
        // through the same event stream.
        crate::execution_recorder::spawn(engine.subscribe_events(), Arc::clone(&executions));

        Self {
            engine,
            storage,
            user_storage,
            executions,
            vault,
            jwt_secret,
            port,
            webhook_responders: responders,
        }
    }
}
