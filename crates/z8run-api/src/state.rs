//! Application shared state.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use z8run_core::engine::FlowEngine;
use z8run_core::nodes::http_out::{self, WebhookResponders};
use z8run_storage::repository::{FlowRepository, UserRepository};
use z8run_storage::credential_vault::CredentialVault;

/// Global application state, shared between handlers.
pub struct AppState {
    /// Flow engine.
    pub engine: FlowEngine,
    /// Storage backend (SQLite or PostgreSQL).
    pub storage: Arc<dyn FlowRepository>,
    /// User storage backend for authentication.
    pub user_storage: Arc<dyn UserRepository>,
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
        vault: Arc<dyn CredentialVault>,
        jwt_secret: String,
        port: u16,
    ) -> Self {
        let responders: WebhookResponders = Arc::new(RwLock::new(HashMap::new()));
        // Initialize the global responder map so http-out nodes can access it
        http_out::init_webhook_responders(Arc::clone(&responders));

        Self {
            engine: FlowEngine::new(),
            storage,
            user_storage,
            vault,
            jwt_secret,
            port,
            webhook_responders: responders,
        }
    }
}
