//! REST API v1 routes.

use std::sync::Arc;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use z8run_core::flow::Flow;
use z8run_storage::repository::FlowRepository;

/// Mounts the REST API routes.
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Flows
        .route("/flows", get(list_flows).post(create_flow))
        .route("/flows/{id}", get(get_flow).delete(delete_flow))
        .route("/flows/{id}/start", post(start_flow))
        .route("/flows/{id}/stop", post(stop_flow))
        // Health check
        .route("/health", get(health_check))
        .route("/info", get(server_info))
}

/// GET /api/v1/health
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "z8run",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// GET /api/v1/info
async fn server_info(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let active_flows = state.engine.active_flow_ids().await;
    Json(serde_json::json!({
        "service": "z8run",
        "version": env!("CARGO_PKG_VERSION"),
        "port": state.port,
        "active_flows": active_flows.len(),
    }))
}

/// GET /api/v1/flows
async fn list_flows(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flows = state.storage.list_flows().await.map_err(ApiError::from)?;

    let flow_summaries: Vec<serde_json::Value> = flows
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id.to_string(),
                "name": f.name,
                "description": f.description,
                "status": f.status.to_string(),
                "nodes": f.nodes.len(),
                "edges": f.edges.len(),
                "created_at": f.created_at.to_rfc3339(),
                "updated_at": f.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "flows": flow_summaries,
        "total": flow_summaries.len(),
    })))
}

/// POST /api/v1/flows
async fn create_flow(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = payload["name"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Field 'name' is required"))?;

    let description = payload["description"].as_str().unwrap_or("");

    let mut flow = Flow::new(name);
    flow.description = description.to_string();

    // Persist to database
    state.storage.save_flow(&flow).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "status": "idle",
        "created_at": flow.created_at.to_rfc3339(),
    })))
}

/// GET /api/v1/flows/:id
async fn get_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flow = state.storage.get_flow(id).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "version": flow.version,
        "status": flow.status.to_string(),
        "nodes": flow.nodes,
        "edges": flow.edges,
        "config": flow.config,
        "metadata": flow.metadata,
        "created_at": flow.created_at.to_rfc3339(),
        "updated_at": flow.updated_at.to_rfc3339(),
    })))
}

/// DELETE /api/v1/flows/:id
async fn delete_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.storage.delete_flow(id).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "deleted": id.to_string(),
    })))
}

/// POST /api/v1/flows/:id/start
async fn start_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Verify flow exists in storage
    let _flow = state.storage.get_flow(id).await.map_err(ApiError::from)?;

    // TODO: load flow and execute via engine
    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "status": "starting",
    })))
}

/// POST /api/v1/flows/:id/stop
async fn stop_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.engine.stop(id).await.map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "status": "stopped",
    })))
}
