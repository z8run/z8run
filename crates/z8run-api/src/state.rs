//! Application shared state.

use std::sync::Arc;
use z8run_core::engine::FlowEngine;
use z8run_storage::sqlite::SqliteStorage;

/// Global application state, shared between handlers.
pub struct AppState {
    /// Flow engine.
    pub engine: FlowEngine,
    /// SQLite storage for persistence.
    pub storage: Arc<SqliteStorage>,
    /// Secret for signing JWT tokens.
    pub jwt_secret: String,
    /// Server port.
    pub port: u16,
}

impl AppState {
    pub fn new(storage: Arc<SqliteStorage>, jwt_secret: String, port: u16) -> Self {
        Self {
            engine: FlowEngine::new(),
            storage,
            jwt_secret,
            port,
        }
    }
}
