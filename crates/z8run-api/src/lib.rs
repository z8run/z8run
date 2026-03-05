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
    // Protected API routes with JWT middleware
    let protected_api = routes::api_routes()
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::jwt_middleware));

    // Auth routes: public (/register, /login) + protected (/me)
    let auth_router = auth::auth_routes().merge(
        auth::auth_protected_routes()
            .layer(axum::middleware::from_fn_with_state(state.clone(), auth::jwt_middleware)),
    );

    // Public API routes (health, info) — no auth required
    let public_api = routes::public_routes();

    Router::new()
        .nest("/api/v1", protected_api.merge(public_api))
        .nest("/auth", auth_router)
        .nest("/hook", routes::hook_routes())
        .nest("/ws", ws::ws_routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
