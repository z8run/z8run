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

use crate::error::ApiError;
use crate::state::AppState;
use z8run_core::flow::{Flow, Edge};
use z8run_core::message::FlowMessage;
use z8run_core::node::{Node, PortType};
use tracing::{info, warn, error};

/// Mounts the REST API routes.
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Flows
        .route("/flows", get(list_flows).post(create_flow))
        .route("/flows/{id}", get(get_flow).put(update_flow).delete(delete_flow))
        .route("/flows/{id}/start", post(start_flow))
        .route("/flows/{id}/stop", post(stop_flow))
        // Health check
        .route("/health", get(health_check))
        .route("/info", get(server_info))
}

/// Mounts webhook routes (outside /api/v1 namespace).
pub fn webhook_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{flow_id}", post(webhook_handler))
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

/// PUT /api/v1/flows/:id — Update flow with canvas state (nodes, edges, metadata)
async fn update_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Load existing flow
    let mut flow = state.storage.get_flow(id).await.map_err(ApiError::from)?;

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
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flow = state.storage.get_flow(id).await.map_err(ApiError::from)?;

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
    let stored_flow = state.storage.get_flow(id).await.map_err(ApiError::from)?;

    // Build an executable Flow from canvas state (returns id_map for frontend feedback)
    let (exec_flow, id_map) = canvas_to_flow(&stored_flow)?;

    info!(
        flow_id = %id,
        nodes = exec_flow.nodes.len(),
        edges = exec_flow.edges.len(),
        "Starting flow execution"
    );

    let trace_id = state.engine.execute(exec_flow).await.map_err(ApiError::from)?;

    // Return canvas_id → core UUID mapping so the frontend can
    // correlate engine events back to canvas nodes for visual feedback.
    let node_map: serde_json::Map<String, serde_json::Value> = id_map
        .into_iter()
        .map(|(canvas_id, uuid)| (canvas_id, serde_json::Value::String(uuid.to_string())))
        .collect();

    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "trace_id": trace_id.to_string(),
        "status": "running",
        "node_map": node_map,
    })))
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

/// POST /webhooks/:flow_id — Execute a flow synchronously via webhook.
///
/// Accepts an HTTP request, runs the flow, and returns the HTTP response
/// generated by the http-out node (or times out after 10 seconds).
async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    Path(flow_id): Path<Uuid>,
    Query(query_params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    info!(flow_id = %flow_id, "Webhook triggered");

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
        .unwrap_or_else(|_| serde_json::json!(body));

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
        "method": "POST",
        "path": format!("/webhooks/{}", flow_id),
        "headers": headers_json,
        "query": query_json,
        "body": body_json,
    });

    let trigger_msg = FlowMessage::new(
        Uuid::nil(),  // source_node will be overridden by engine
        "webhook",
        trigger_payload,
        trace_id,
    );

    // Create oneshot channel for the response
    let (tx, rx) = tokio::sync::oneshot::channel();

    // Store the sender so http-out can find it
    {
        state.webhook_responders.write().await.insert(trace_id, tx);
    }

    // Execute the flow with the trigger message
    match state.engine.execute_with_trigger(exec_flow, Some(trigger_msg)).await {
        Ok(_) => {}
        Err(e) => {
            // Clean up responder
            state.webhook_responders.write().await.remove(&trace_id);
            error!(error = %e, "Failed to execute flow");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Flow execution failed: {}", e)})),
            );
        }
    }

    // Wait for the response from http-out (with timeout)
    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(response)) => {
            info!(
                flow_id = %flow_id,
                status = response.status,
                "Webhook response sent"
            );
            let status = StatusCode::from_u16(response.status)
                .unwrap_or(StatusCode::OK);
            (status, Json(response.body))
        }
        Ok(Err(_)) => {
            // Sender was dropped (flow completed without http-out?)
            warn!(flow_id = %flow_id, "Webhook response channel dropped");
            state.webhook_responders.write().await.remove(&trace_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Flow completed without sending a response"})),
            )
        }
        Err(_) => {
            // Timeout
            warn!(flow_id = %flow_id, "Webhook timed out after 10 seconds");
            state.webhook_responders.write().await.remove(&trace_id);
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({"error": "Flow execution timed out (10s)"})),
            )
        }
    }
}
