//! # z8run-api
//!
//! z8run HTTP and WebSocket server.
//! Exposes REST API for flow management
//! and WebSockets for real-time communication.

pub mod routes;
pub mod ws;
pub mod state;
pub mod auth;
pub mod error;

use axum::Router;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use state::AppState;

/// Builds the main application router.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", routes::api_routes())
        .nest("/hook", routes::hook_routes())
        .nest("/ws", ws::ws_routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
