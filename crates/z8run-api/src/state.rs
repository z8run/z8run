//! Application shared state.

use std::sync::Arc;
use z8run_core::engine::FlowEngine;
use z8run_storage::repository::FlowRepository;

/// Global application state, shared between handlers.
pub struct AppState {
    /// Flow engine.
    pub engine: FlowEngine,
    /// Storage backend (SQLite or PostgreSQL).
    pub storage: Arc<dyn FlowRepository>,
    /// Secret for signing JWT tokens.
    pub jwt_secret: String,
    /// Server port.
    pub port: u16,
}

impl AppState {
    pub fn new(storage: Arc<dyn FlowRepository>, jwt_secret: String, port: u16) -> Self {
        Self {
            engine: FlowEngine::new(),
            storage,
            jwt_secret,
            port,
        }
    }
}
