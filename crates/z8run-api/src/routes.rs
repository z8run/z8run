//! REST API v1 routes.

use std::collections::HashMap;
use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::auth::Claims;
use crate::error::ApiError;
use crate::state::AppState;
use z8run_core::flow::{Flow, Edge};
use z8run_core::message::FlowMessage;
use z8run_core::node::{Node, PortType};
use tracing::{info, warn, error};

/// Mounts the REST API routes (protected by JWT).
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Flows
        .route("/flows", get(list_flows).post(create_flow))
        .route("/flows/{id}", get(get_flow).put(update_flow).delete(delete_flow))
        .route("/flows/{id}/start", post(start_flow))
        .route("/flows/{id}/stop", post(stop_flow))
        .route("/flows/{id}/export", get(export_flow))
        .route("/flows/import", post(import_flow))
        // Vault
        .route("/vault", get(list_credentials).post(store_credential))
        .route("/vault/{key}", get(get_credential).delete(delete_credential))
}

/// Mounts public API routes (no authentication required).
pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health_check))
        .route("/info", get(server_info))
}

/// Mounts hook routes: /hook/{flow_id} and /hook/{flow_id}/{*path}
///
/// Every flow gets a unique namespace under /hook/{flow_id}.
/// The http-in node's path becomes a sub-route within that namespace.
/// Examples:
///   POST /hook/{flow_id}           → triggers the flow directly
///   POST /hook/{flow_id}/branch    → matches http-in with path="/branch"
///   GET  /hook/{flow_id}/users/123 → matches http-in with path="/users/123"
pub fn hook_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{flow_id}", axum::routing::any(hook_handler))
        .route("/{flow_id}/{*path}", axum::routing::any(hook_handler))
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
    axum::Extension(claims): axum::Extension<Claims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flows = state.storage.list_flows_by_user(claims.sub).await.map_err(ApiError::from)?;

    let flow_summaries: Vec<serde_json::Value> = flows
        .iter()
        .map(|f| {
            // Count canvas nodes/edges from metadata (where the frontend stores them)
            let canvas_node_count = f.metadata.positions
                .get("canvas_nodes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(f.nodes.len());
            let canvas_edge_count = f.metadata.positions
                .get("canvas_edges")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(f.edges.len());

            serde_json::json!({
                "id": f.id.to_string(),
                "name": f.name,
                "description": f.description,
                "status": f.status.to_string(),
                "nodes": canvas_node_count,
                "edges": canvas_edge_count,
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
    axum::Extension(claims): axum::Extension<Claims>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = payload["name"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Field 'name' is required"))?;

    let description = payload["description"].as_str().unwrap_or("");

    let mut flow = Flow::new(name);
    flow.description = description.to_string();

    // Persist to database with user ownership
    state.storage.save_flow_with_user(&flow, claims.sub).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "status": "idle",
        "created_at": flow.created_at.to_rfc3339(),
    })))
}

/// PUT /api/v1/flows/:id — Update flow with canvas state (nodes, edges, metadata)
async fn update_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Load existing flow (only if owned by user)
    let mut flow = state.storage.get_flow_for_user(id, claims.sub).await.map_err(ApiError::from)?;

    // Update name if provided
    if let Some(name) = payload["name"].as_str() {
        flow.name = name.to_string();
    }

    // Update description if provided
    if let Some(desc) = payload["description"].as_str() {
        flow.description = desc.to_string();
    }

    // Store the React Flow canvas state in metadata
    // This preserves the full frontend state (positions, data, selections)
    if let Some(canvas_nodes) = payload.get("canvas_nodes") {
        flow.metadata.positions.insert(
            "canvas_nodes".to_string(),
            canvas_nodes.clone(),
        );
    }

    if let Some(canvas_edges) = payload.get("canvas_edges") {
        flow.metadata.positions.insert(
            "canvas_edges".to_string(),
            canvas_edges.clone(),
        );
    }

    if let Some(viewport) = payload.get("viewport") {
        flow.metadata.positions.insert(
            "viewport".to_string(),
            viewport.clone(),
        );
    }

    // Update timestamp
    flow.updated_at = chrono::Utc::now();

    // Persist
    state.storage.save_flow(&flow).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "status": flow.status.to_string(),
        "updated_at": flow.updated_at.to_rfc3339(),
    })))
}

/// GET /api/v1/flows/:id
async fn get_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flow = state.storage.get_flow_for_user(id, claims.sub).await.map_err(ApiError::from)?;

    // Extract canvas state from metadata for the frontend
    let canvas_nodes = flow.metadata.positions.get("canvas_nodes")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let canvas_edges = flow.metadata.positions.get("canvas_edges")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let viewport = flow.metadata.positions.get("viewport")
        .cloned()
        .unwrap_or(serde_json::json!({"x": 0, "y": 0, "zoom": 1}));

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "version": flow.version,
        "status": flow.status.to_string(),
        "nodes": flow.nodes,
        "edges": flow.edges,
        "canvas_nodes": canvas_nodes,
        "canvas_edges": canvas_edges,
        "viewport": viewport,
        "config": flow.config,
        "created_at": flow.created_at.to_rfc3339(),
        "updated_at": flow.updated_at.to_rfc3339(),
    })))
}

/// DELETE /api/v1/flows/:id
async fn delete_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.storage.delete_flow_for_user(id, claims.sub).await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "deleted": id.to_string(),
    })))
}

/// POST /api/v1/flows/:id/start
async fn start_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stored_flow = state.storage.get_flow_for_user(id, claims.sub).await.map_err(ApiError::from)?;

    // Build an executable Flow from canvas state (returns id_map for frontend feedback)
    let (exec_flow, id_map) = canvas_to_flow(&stored_flow)?;

    info!(
        flow_id = %id,
        nodes = exec_flow.nodes.len(),
        edges = exec_flow.edges.len(),
        "Starting flow execution"
    );

    // Resolve hook URLs for http-in nodes
    let registered_routes = register_hook_routes(&stored_flow, id);
    let has_input_nodes = !registered_routes.is_empty();

    // Return canvas_id → core UUID mapping so the frontend can
    // correlate engine events back to canvas nodes for visual feedback.
    let node_map: serde_json::Map<String, serde_json::Value> = id_map
        .into_iter()
        .map(|(canvas_id, uuid)| (canvas_id, serde_json::Value::String(uuid.to_string())))
        .collect();

    if has_input_nodes {
        // Flow has input nodes (http-in, webhook, etc.) — don't execute now.
        // Just register the hook routes and wait for incoming requests.
        info!(flow_id = %id, "Flow deployed — waiting for hook triggers");
        Ok(Json(serde_json::json!({
            "flow_id": id.to_string(),
            "status": "deployed",
            "node_map": node_map,
            "routes": registered_routes,
        })))
    } else {
        // No input nodes — execute immediately (manual/cron flow).
        let trace_id = state.engine.execute(exec_flow).await.map_err(ApiError::from)?;
        Ok(Json(serde_json::json!({
            "flow_id": id.to_string(),
            "trace_id": trace_id.to_string(),
            "status": "running",
            "node_map": node_map,
        })))
    }
}

/// Scans canvas_nodes for http-in nodes and returns their hook URLs.
/// Each flow gets its own namespace: /hook/{flow_id}/{path}
/// No conflict detection needed — namespaces prevent collisions.
fn register_hook_routes(
    stored: &Flow,
    flow_id: Uuid,
) -> Vec<serde_json::Value> {
    let mut registered = Vec::new();

    let canvas_nodes = match stored.metadata.positions.get("canvas_nodes")
        .and_then(|v| v.as_array())
    {
        Some(nodes) => nodes,
        None => return registered,
    };

    for node in canvas_nodes {
        let data = &node["data"];
        let node_type = data["type"].as_str().unwrap_or("");

        if node_type == "http-in" {
            let config = &data["config"];
            let method = config["method"].as_str().unwrap_or("POST").to_uppercase();
            let path = config["path"].as_str().unwrap_or("/").to_string();

            // Build the full hook URL path: /hook/{flow_id}/{sub_path}
            let hook_path = if path == "/" || path.is_empty() {
                format!("/hook/{}", flow_id)
            } else {
                let clean = path.trim_start_matches('/');
                format!("/hook/{}/{}", flow_id, clean)
            };

            info!(
                flow_id = %flow_id,
                method = %method,
                hook_path = %hook_path,
                "Hook route registered"
            );

            registered.push(serde_json::json!({
                "method": method,
                "path": hook_path,
            }));
        }
    }

    registered
}

/// Converts the frontend canvas state (stored in metadata) into
/// an executable core Flow with proper Nodes and Edges.
fn canvas_to_flow(stored: &Flow) -> Result<(Flow, std::collections::HashMap<String, Uuid>), ApiError> {
    let canvas_nodes = stored.metadata.positions
        .get("canvas_nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No canvas nodes found. Save the flow first."))?;

    let canvas_edges = stored.metadata.positions
        .get("canvas_edges")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .clone();

    let mut flow = Flow::new(&stored.name);
    flow.id = stored.id;
    flow.description = stored.description.clone();

    // Namespace UUID for deterministic ID generation.
    // Same flow_id + canvas_id always produces the same core UUID,
    // so Deploy and hook trigger share the same node mapping.
    let namespace = stored.id;

    // Map canvas node IDs (strings like "node_123") to core UUIDs
    let mut id_map: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();

    for canvas_node in canvas_nodes {
        let canvas_id = canvas_node["id"].as_str().unwrap_or("unknown").to_string();
        let data = &canvas_node["data"];

        // Extract the node type (try "type" first, then "nodeType" for curl-created nodes)
        let node_type_str = data["type"].as_str()
            .or_else(|| data["nodeType"].as_str())
            .unwrap_or("function");
        let label = data["label"].as_str().unwrap_or("Node");

        // Build core Node with appropriate ports based on type
        let mut core_node = Node::new(label, node_type_str);

        // Override with deterministic UUID: same canvas_id always → same core UUID
        core_node.id = Uuid::new_v5(&namespace, canvas_id.as_bytes());

        // Add input ports based on canvas data
        if let Some(inputs) = data["inputs"].as_array() {
            for input in inputs {
                let port_name = input["id"].as_str().unwrap_or("input");
                let port_type = parse_port_type(input["type"].as_str().unwrap_or("any"));
                core_node = core_node.with_input(port_name, port_type);
            }
        } else {
            // Default: single input
            core_node = core_node.with_input("input", PortType::Any);
        }

        // Add output ports based on canvas data
        if let Some(outputs) = data["outputs"].as_array() {
            for output in outputs {
                let port_name = output["id"].as_str().unwrap_or("output");
                let port_type = parse_port_type(output["type"].as_str().unwrap_or("any"));
                core_node = core_node.with_output(port_name, port_type);
            }
        } else {
            // Default: single output
            core_node = core_node.with_output("output", PortType::Any);
        }

        // Pass the node config
        if let Some(config) = data.get("config") {
            core_node = core_node.with_config(config.clone());
        }

        id_map.insert(canvas_id, core_node.id);
        flow.nodes.push(core_node);
    }

    // Convert canvas edges to core Edges
    for canvas_edge in &canvas_edges {
        let source = canvas_edge["source"].as_str().unwrap_or("");
        let target = canvas_edge["target"].as_str().unwrap_or("");
        let source_handle = canvas_edge["sourceHandle"].as_str().unwrap_or("output");
        let target_handle = canvas_edge["targetHandle"].as_str().unwrap_or("input");

        if let (Some(&from_id), Some(&to_id)) = (id_map.get(source), id_map.get(target)) {
            let edge = Edge::new(from_id, source_handle, to_id, target_handle);
            flow.edges.push(edge);
        } else {
            warn!(source = source, target = target, "Skipping edge with unknown nodes");
        }
    }

    Ok((flow, id_map))
}

fn parse_port_type(s: &str) -> PortType {
    match s {
        "string" => PortType::String,
        "number" => PortType::Number,
        "boolean" => PortType::Boolean,
        "object" => PortType::Object,
        "array" => PortType::Array,
        "binary" => PortType::Binary,
        _ => PortType::Any,
    }
}

/// GET /api/v1/vault
/// List all credential keys (not values!)
async fn list_credentials(
    State(state): State<Arc<AppState>>,
    axum::Extension(_claims): axum::Extension<Claims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = state.vault.list_keys().await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "keys": keys })))
}

/// POST /api/v1/vault
/// Store a credential
/// Body: { "key": "openai_api_key", "value": "sk-..." }
async fn store_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(_claims): axum::Extension<Claims>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key = body["key"].as_str()
        .ok_or_else(|| ApiError::bad_request("Missing 'key' field"))?;
    let value = body["value"].as_str()
        .ok_or_else(|| ApiError::bad_request("Missing 'value' field"))?;

    state.vault.store(key, value).await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;

    Ok(Json(serde_json::json!({ "status": "stored", "key": key })))
}

/// GET /api/v1/vault/:key
/// Retrieve a credential value
async fn get_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(_claims): axum::Extension<Claims>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let value = state.vault.retrieve(&key).await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "key": key, "value": value })))
}

/// DELETE /api/v1/vault/:key
/// Delete a credential
async fn delete_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(_claims): axum::Extension<Claims>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.vault.delete(&key).await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "status": "deleted", "key": key })))
}

/// POST /api/v1/flows/:id/stop
async fn stop_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(_claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.engine.stop(id).await.map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "status": "stopped",
    })))
}

/// ANY /hook/{flow_id} or /hook/{flow_id}/{*path}
///
/// Unified hook handler for all input node types (http-in, webhook, etc.).
/// Each flow gets its own namespace under /hook/{flow_id}, preventing
/// route collisions between flows — ready for multi-tenant SaaS.
///
/// Examples:
///   POST /hook/{flow_id}              → triggers the flow (root path)
///   POST /hook/{flow_id}/branch       → matches http-in with path="/branch"
///   GET  /hook/{flow_id}/users?id=5   → matches http-in with path="/users"
async fn hook_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    Query(query_params): Query<HashMap<String, String>>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Extract flow_id and optional sub-path
    let flow_id_str = match params.get("flow_id") {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing flow_id"})),
            );
        }
    };

    let flow_id: Uuid = match flow_id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid flow_id: {}", flow_id_str)})),
            );
        }
    };

    // Sub-path from the wildcard capture (e.g. "branch" or "users/123")
    let sub_path = params.get("path")
        .map(|p| format!("/{}", p))
        .unwrap_or_else(|| "/".to_string());

    let method_str = method.to_string().to_uppercase();

    info!(
        flow_id = %flow_id,
        method = %method_str,
        path = %sub_path,
        "Hook triggered"
    );

    // Load the flow
    let stored_flow = match state.storage.get_flow(flow_id).await {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, "Failed to load flow");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Flow not found: {}", e)})),
            );
        }
    };

    // Build executable flow
    let (exec_flow, _id_map) = match canvas_to_flow(&stored_flow) {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Failed to compile flow: {}", e.message)})),
            );
        }
    };

    // Parse body as JSON (or wrap raw string)
    let body_json: serde_json::Value = serde_json::from_str(&body)
        .unwrap_or_else(|_| if body.is_empty() { serde_json::json!(null) } else { serde_json::json!(body) });

    // Convert headers to JSON map
    let headers_json: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|v| {
                (name.to_string(), serde_json::Value::String(v.to_string()))
            })
        })
        .collect();

    // Convert query params to JSON
    let query_json: serde_json::Value = serde_json::to_value(&query_params)
        .unwrap_or(serde_json::json!({}));

    // Create the trigger message with real HTTP data
    let trace_id = Uuid::now_v7();
    let trigger_payload = serde_json::json!({
        "method": method_str,
        "path": sub_path,
        "headers": headers_json,
        "query": query_json,
        "body": body_json,
    });

    let trigger_msg = FlowMessage::new(
        Uuid::nil(),
        "hook",
        trigger_payload,
        trace_id,
    );

    // Create oneshot channel for the response
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.webhook_responders.write().await.insert(trace_id, tx);

    // Execute the flow with the trigger message
    match state.engine.execute_with_trigger(exec_flow, Some(trigger_msg)).await {
        Ok(_) => {}
        Err(e) => {
            state.webhook_responders.write().await.remove(&trace_id);
            error!(error = %e, "Failed to execute flow");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Flow execution failed: {}", e)})),
            );
        }
    }

    // Wait for the response from http-out (with timeout)
    await_flow_response(flow_id, rx, &state).await
}

/// Shared logic: waits for the oneshot response from http-out node.
async fn await_flow_response(
    flow_id: Uuid,
    rx: tokio::sync::oneshot::Receiver<z8run_core::nodes::http_out::WebhookResponse>,
    state: &Arc<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(response)) => {
            info!(
                flow_id = %flow_id,
                status = response.status,
                "Flow response sent"
            );
            let status = StatusCode::from_u16(response.status)
                .unwrap_or(StatusCode::OK);
            (status, Json(response.body))
        }
        Ok(Err(_)) => {
            warn!(flow_id = %flow_id, "Response channel dropped");
            state.webhook_responders.write().await.remove(&flow_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Flow completed without sending a response"})),
            )
        }
        Err(_) => {
            warn!(flow_id = %flow_id, "Flow timed out after 10 seconds");
            state.webhook_responders.write().await.remove(&flow_id);
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({"error": "Flow execution timed out (10s)"})),
            )
        }
    }
}

/// GET /api/v1/flows/:id/export — Export a flow as a portable JSON document.
///
/// The exported JSON includes flow metadata and the full canvas state
/// (nodes, edges, viewport) but strips internal fields like user_id
/// so it can be imported by any user.
async fn export_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flow = state.storage.get_flow_for_user(id, claims.sub).await.map_err(ApiError::from)?;

    let canvas_nodes = flow.metadata.positions.get("canvas_nodes")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let canvas_edges = flow.metadata.positions.get("canvas_edges")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let viewport = flow.metadata.positions.get("viewport")
        .cloned()
        .unwrap_or(serde_json::json!({"x": 0, "y": 0, "zoom": 1}));

    let export = serde_json::json!({
        "z8run_version": env!("CARGO_PKG_VERSION"),
        "export_format": 1,
        "flow": {
            "name": flow.name,
            "description": flow.description,
            "version": flow.version,
            "canvas_nodes": canvas_nodes,
            "canvas_edges": canvas_edges,
            "viewport": viewport,
        }
    });

    info!(flow_id = %id, "Flow exported");
    Ok(Json(export))
}

/// POST /api/v1/flows/import — Import a flow from an exported JSON document.
///
/// Creates a brand-new flow (new UUID) owned by the authenticated user,
/// populated with the canvas state from the export.
async fn import_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate export format
    let flow_data = payload.get("flow")
        .ok_or_else(|| ApiError::bad_request("Invalid export: missing 'flow' key"))?;

    let name = flow_data["name"].as_str()
        .ok_or_else(|| ApiError::bad_request("Invalid export: missing flow name"))?;
    let description = flow_data["description"].as_str().unwrap_or("");
    let version = flow_data["version"].as_str().unwrap_or("0.1.0");

    // Validate node types — reject unknown nodes before creating the flow
    const VALID_NODE_TYPES: &[&str] = &[
        "http-in", "http-out", "http-request", "function", "debug",
        "switch", "filter", "delay", "timer", "webhook", "json", "database", "mqtt",
        "llm", "embeddings", "classifier", "prompt-template", "text-splitter",
        "vector-store", "structured-output", "summarizer", "ai-agent", "image-gen",
    ];

    if let Some(canvas_nodes) = flow_data["canvas_nodes"].as_array() {
        let mut unknown_types: Vec<String> = Vec::new();

        for node in canvas_nodes {
            let node_type = node["data"]["type"].as_str().unwrap_or("unknown");
            if !VALID_NODE_TYPES.contains(&node_type) {
                unknown_types.push(node_type.to_string());
            }
        }

        if !unknown_types.is_empty() {
            // Deduplicate
            unknown_types.sort();
            unknown_types.dedup();
            return Err(ApiError::bad_request(format!(
                "Flow contains unsupported node types: {}. Supported types: {}",
                unknown_types.join(", "),
                VALID_NODE_TYPES.join(", "),
            )));
        }
    }

    // Create a new flow with a fresh ID
    let mut flow = Flow::new(name);
    flow.description = description.to_string();
    flow.version = version.to_string();

    // Restore canvas state into metadata
    if let Some(nodes) = flow_data.get("canvas_nodes") {
        flow.metadata.positions.insert("canvas_nodes".to_string(), nodes.clone());
    }
    if let Some(edges) = flow_data.get("canvas_edges") {
        flow.metadata.positions.insert("canvas_edges".to_string(), edges.clone());
    }
    if let Some(vp) = flow_data.get("viewport") {
        flow.metadata.positions.insert("viewport".to_string(), vp.clone());
    }

    // Count imported nodes/edges for the response
    let node_count = flow_data["canvas_nodes"].as_array().map(|a| a.len()).unwrap_or(0);
    let edge_count = flow_data["canvas_edges"].as_array().map(|a| a.len()).unwrap_or(0);

    // Save with user ownership
    state.storage.save_flow_with_user(&flow, claims.sub).await.map_err(ApiError::from)?;

    info!(flow_id = %flow.id, name = %flow.name, nodes = node_count, edges = edge_count, "Flow imported");

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "nodes": node_count,
        "edges": edge_count,
        "status": "idle",
        "created_at": flow.created_at.to_rfc3339(),
    })))
}

