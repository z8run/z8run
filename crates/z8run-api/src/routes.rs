//! REST API v1 routes.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::Claims;
use crate::error::ApiError;
use crate::state::AppState;
use tracing::{error, info, warn};
use z8run_core::flow::{Edge, Flow};
use z8run_core::message::FlowMessage;
use z8run_core::node::{Node, PortType};
use z8run_storage::credential_vault::CredentialVault;
use z8run_storage::repository::HookRoute;

/// Mounts the REST API routes (protected by JWT).
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Flows
        .route("/flows", get(list_flows).post(create_flow))
        .route(
            "/flows/{id}",
            get(get_flow).put(update_flow).delete(delete_flow),
        )
        .route("/flows/{id}/start", post(start_flow))
        .route("/flows/{id}/stop", post(stop_flow))
        .route("/flows/{id}/export", get(export_flow))
        .route("/flows/{id}/executions", get(get_executions))
        .route("/flows/import", post(import_flow))
        // Plugin nodes for the editor palette
        .route("/plugins", get(list_plugins))
        // Vault
        .route("/vault", get(list_credentials).post(store_credential))
        .route(
            "/vault/{key}",
            get(get_credential).delete(delete_credential),
        )
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

/// Port types the editor knows; anything else is shown as `any`.
const EDITOR_PORT_TYPES: &[&str] = &[
    "any", "string", "number", "boolean", "object", "array", "binary",
];

/// GET /api/v1/plugins
///
/// Plugins registered with the engine, shaped like the editor's node
/// definitions so they can be dragged onto the canvas.
async fn list_plugins(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ports = |ports: &[z8run_runtime::ManifestPort]| -> Vec<serde_json::Value> {
        ports
            .iter()
            .map(|p| {
                let port_type = if EDITOR_PORT_TYPES.contains(&p.port_type.as_str()) {
                    p.port_type.as_str()
                } else {
                    "any"
                };
                serde_json::json!({
                    "id": p.name,
                    "name": p.name,
                    "type": port_type,
                    "required": p.required,
                })
            })
            .collect()
    };
    let mut plugins = state.plugin_nodes();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    let plugins: Vec<serde_json::Value> = plugins
        .iter()
        .map(|m| {
            serde_json::json!({
                "type": m.name,
                "label": m.name,
                "description": m.description,
                "version": m.version,
                "author": m.author,
                "icon": m.icon,
                "inputs": ports(&m.inputs),
                "outputs": ports(&m.outputs),
                "defaultConfig": if m.config.is_object() { m.config.clone() } else { serde_json::json!({}) },
            })
        })
        .collect();
    Json(serde_json::json!({ "plugins": plugins }))
}

/// GET /api/v1/info
async fn server_info(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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
    let flows = state
        .storage
        .list_flows_by_user(claims.sub)
        .await
        .map_err(ApiError::from)?;

    let mut flow_summaries: Vec<serde_json::Value> = Vec::with_capacity(flows.len());
    for f in &flows {
        // Count canvas nodes/edges from metadata (where the frontend stores them)
        let canvas_node_count = f
            .metadata
            .positions
            .get("canvas_nodes")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(f.nodes.len());
        let canvas_edge_count = f
            .metadata
            .positions
            .get("canvas_edges")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(f.edges.len());

        // Derive the shown status from the last persisted execution (FUNC-008),
        // falling back to the flow's stored status when it has never run.
        let last_run = state
            .executions
            .get_history(f.id, 1)
            .await
            .ok()
            .and_then(|v| v.into_iter().next());
        let (status, last_run_at) = match last_run {
            Some(rec) => (rec.status, Some(rec.started_at.to_rfc3339())),
            None => (f.status.to_string(), None),
        };

        flow_summaries.push(serde_json::json!({
            "id": f.id.to_string(),
            "name": f.name,
            "description": f.description,
            "status": status,
            "last_run_at": last_run_at,
            "nodes": canvas_node_count,
            "edges": canvas_edge_count,
            "created_at": f.created_at.to_rfc3339(),
            "updated_at": f.updated_at.to_rfc3339(),
            // Surfaced on the summary so the list can group and order a chain
            // without fetching every flow in full.
            "tags": f.metadata.tags,
            "notes": f.metadata.notes,
        }));
    }

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
    state
        .storage
        .save_flow_with_user(&flow, claims.sub)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "status": "idle",
        "created_at": flow.created_at.to_rfc3339(),
    })))
}

/// Validates the structural shape of a canvas payload before it is stored.
///
/// Catches malformed data early with actionable errors instead of letting it
/// degrade silently at execution time (nodes defaulting to `function`, edges
/// being dropped). Structure only — node *types* are validated against the
/// registry on import/execute, not here.
fn validate_canvas(payload: &serde_json::Value) -> Result<(), ApiError> {
    if let Some(nodes) = payload.get("canvas_nodes") {
        let arr = nodes
            .as_array()
            .ok_or_else(|| ApiError::bad_request("'canvas_nodes' must be an array"))?;
        for (i, node) in arr.iter().enumerate() {
            if !node.is_object() {
                return Err(ApiError::bad_request(format!(
                    "canvas node at index {i} must be an object"
                )));
            }
            let id_ok = node
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            if !id_ok {
                return Err(ApiError::bad_request(format!(
                    "canvas node at index {i} is missing a non-empty string 'id'"
                )));
            }
            let data = &node["data"];
            let type_ok = data
                .get("type")
                .or_else(|| data.get("nodeType"))
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            if !type_ok {
                return Err(ApiError::bad_request(format!(
                    "canvas node '{}' is missing a string 'data.type'",
                    node["id"].as_str().unwrap_or("?")
                )));
            }
        }
    }

    if let Some(edges) = payload.get("canvas_edges") {
        let arr = edges
            .as_array()
            .ok_or_else(|| ApiError::bad_request("'canvas_edges' must be an array"))?;
        for (i, edge) in arr.iter().enumerate() {
            let src_ok = edge
                .get("source")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            let tgt_ok = edge
                .get("target")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            if !src_ok || !tgt_ok {
                return Err(ApiError::bad_request(format!(
                    "canvas edge at index {i} must have string 'source' and 'target'"
                )));
            }
        }
    }

    Ok(())
}

/// PUT /api/v1/flows/:id - Update flow with canvas state (nodes, edges, metadata)
async fn update_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Reject structurally malformed canvas data with actionable errors (FUNC-009)
    validate_canvas(&payload)?;

    // Load existing flow (only if owned by user)
    let mut flow = state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    // Update name if provided
    if let Some(name) = payload["name"].as_str() {
        flow.name = name.to_string();
    }

    // Update description if provided
    if let Some(desc) = payload["description"].as_str() {
        flow.description = desc.to_string();
    }

    // Tags and notes ride the import payload. Without this an exported flow
    // loses both on re-import, which is how a chain quietly becomes a pile of
    // unrelated flows.
    // In update_flow the payload IS the flow object (no export envelope), so
    // these read straight off it. import_flow has to unwrap `flow` first.
    if let Some(tags) = payload.get("tags").and_then(|v| v.as_array()) {
        flow.metadata.tags = tags
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect();
    }
    if let Some(notes) = payload.get("notes").and_then(|v| v.as_array()) {
        flow.metadata.notes = notes
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect();
    }

    // Store the React Flow canvas state in metadata
    // This preserves the full frontend state (positions, data, selections)
    if let Some(canvas_nodes) = payload.get("canvas_nodes") {
        flow.metadata
            .positions
            .insert("canvas_nodes".to_string(), canvas_nodes.clone());
    }

    if let Some(canvas_edges) = payload.get("canvas_edges") {
        flow.metadata
            .positions
            .insert("canvas_edges".to_string(), canvas_edges.clone());
    }

    if let Some(viewport) = payload.get("viewport") {
        flow.metadata
            .positions
            .insert("viewport".to_string(), viewport.clone());
    }

    // Update timestamp
    flow.updated_at = chrono::Utc::now();

    // Persist
    state
        .storage
        .save_flow(&flow)
        .await
        .map_err(ApiError::from)?;

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
    let flow = state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    // Extract canvas state from metadata for the frontend
    let canvas_nodes = flow
        .metadata
        .positions
        .get("canvas_nodes")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let canvas_edges = flow
        .metadata
        .positions
        .get("canvas_edges")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let viewport = flow
        .metadata
        .positions
        .get("viewport")
        .cloned()
        .unwrap_or(serde_json::json!({"x": 0, "y": 0, "zoom": 1}));

    // Whether this flow's public hooks are live, so the UI can offer to stop
    // them (a deployed hook flow is idle between requests, not "running").
    let deployed = state
        .storage
        .get_deployment(id)
        .await
        .map_err(ApiError::from)?
        .is_some();

    Ok(Json(serde_json::json!({
        "id": flow.id.to_string(),
        "name": flow.name,
        "description": flow.description,
        "version": flow.version,
        "status": flow.status.to_string(),
        "deployed": deployed,
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
    state
        .storage
        .delete_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;
    state.hook_limits.release(id);

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
    let stored_flow = state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    // Build an executable Flow from canvas state (returns id_map for frontend feedback)
    let (exec_flow, id_map) =
        canvas_to_flow(&stored_flow, claims.sub, state.vault.as_ref()).await?;

    info!(
        flow_id = %id,
        nodes = exec_flow.nodes.len(),
        edges = exec_flow.edges.len(),
        "Starting flow execution"
    );

    // Determine the HTTP entry points this flow exposes.
    let hook_routes = collect_hook_routes(&stored_flow)?;
    let has_input_nodes = !hook_routes.is_empty();

    // Return canvas_id → core UUID mapping so the frontend can
    // correlate engine events back to canvas nodes for visual feedback.
    let node_map: serde_json::Map<String, serde_json::Value> = id_map
        .into_iter()
        .map(|(canvas_id, uuid)| (canvas_id, serde_json::Value::String(uuid.to_string())))
        .collect();

    if has_input_nodes {
        // Flow has input nodes (http-in, webhook, etc.) - don't execute now.
        // Persist an immutable snapshot plus the exact routes bound to their
        // trigger nodes. Hooks run this snapshot, so later canvas edits don't
        // change what a published URL executes until the next deploy (A-05).
        state
            .storage
            .deploy_flow(id, claims.sub, &stored_flow, &hook_routes)
            .await
            .map_err(ApiError::from)?;

        let registered_routes = hook_route_urls(&hook_routes, id);
        info!(flow_id = %id, routes = hook_routes.len(), "Flow deployed - waiting for hook triggers");
        Ok(Json(serde_json::json!({
            "flow_id": id.to_string(),
            "status": "deployed",
            "node_map": node_map,
            "routes": registered_routes,
        })))
    } else {
        // No input nodes - execute immediately (manual/cron flow). If an
        // earlier version of this flow was deployed with hooks, retire them:
        // otherwise its old URLs would stay live after the triggers were removed.
        state
            .storage
            .undeploy_flow(id)
            .await
            .map_err(ApiError::from)?;
        state.hook_limits.release(id);

        let trace_id = state
            .engine
            .execute(exec_flow)
            .await
            .map_err(ApiError::from)?;
        Ok(Json(serde_json::json!({
            "flow_id": id.to_string(),
            "trace_id": trace_id.to_string(),
            "status": "running",
            "node_map": node_map,
        })))
    }
}

/// Canvas node types that expose an HTTP hook entry point.
const HOOK_NODE_TYPES: [&str; 3] = ["http-in", "webhook", "webhook-trigger"];

/// Normalizes a configured hook path to the sub-path form used for matching:
/// empty or `"/"` becomes `"/"`, otherwise a single leading slash is ensured
/// (e.g. `"branch"` → `"/branch"`). This must mirror how `hook_handler`
/// derives the incoming sub-path so persisted routes and requests compare equal.
fn normalize_hook_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_start_matches('/'))
    }
}

/// Scans canvas_nodes for HTTP-trigger nodes and returns their exact routes.
/// Each flow gets its own namespace: /hook/{flow_id}/{path}.
///
/// Each route is bound to the canvas id of the trigger that declared it, so a
/// request is later authorized and executed against that node only (A-01).
/// Two triggers declaring the same method + path would make the route
/// ambiguous, so the deploy is rejected instead of picking one silently.
fn collect_hook_routes(stored: &Flow) -> Result<Vec<HookRoute>, ApiError> {
    let mut routes: Vec<HookRoute> = Vec::new();

    let canvas_nodes = match stored
        .metadata
        .positions
        .get("canvas_nodes")
        .and_then(|v| v.as_array())
    {
        Some(nodes) => nodes,
        None => return Ok(routes),
    };

    for node in canvas_nodes {
        let data = &node["data"];
        let node_type = data["type"].as_str().unwrap_or("");
        if !HOOK_NODE_TYPES.contains(&node_type) {
            continue;
        }

        // A trigger without an id cannot be bound to its route; skipping it
        // leaves no route, which fails closed.
        let Some(node_id) = node["id"].as_str().filter(|s| !s.is_empty()) else {
            warn!(node_type, "Skipping hook trigger without a canvas id");
            continue;
        };

        let config = &data["config"];
        let method = config["method"].as_str().unwrap_or("POST").to_uppercase();
        let path = normalize_hook_path(config["path"].as_str().unwrap_or("/"));

        if let Some(existing) = routes.iter().find(|r| r.method == method && r.path == path) {
            return Err(ApiError::bad_request(format!(
                "Duplicate hook route {method} {path}: declared by nodes '{}' and '{node_id}'",
                existing.node_id
            )));
        }

        routes.push(HookRoute {
            method,
            path,
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
        });
    }

    Ok(routes)
}

/// Builds the user-facing hook URLs (`/hook/{flow_id}{path}`) for display.
fn hook_route_urls(routes: &[HookRoute], flow_id: Uuid) -> Vec<serde_json::Value> {
    routes
        .iter()
        .map(|r| {
            let hook_path = if r.path == "/" {
                format!("/hook/{}", flow_id)
            } else {
                format!("/hook/{}{}", flow_id, r.path)
            };
            serde_json::json!({ "method": r.method, "path": hook_path })
        })
        .collect()
}

/// Recursively resolves `"vault:key-name"` references in a JSON value.
/// Strings starting with `"vault:"` are replaced with the decrypted value
/// from the credential vault. Other values pass through unchanged.
async fn resolve_vault_refs(
    value: serde_json::Value,
    user_id: Uuid,
    vault: &dyn CredentialVault,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(ref s) if s.starts_with("vault:") => {
            let key = &s[6..]; // strip "vault:" prefix
            match vault.retrieve(user_id, key).await {
                Ok(secret) => serde_json::Value::String(secret),
                Err(e) => {
                    warn!(vault_key = key, error = %e, "Failed to resolve vault reference");
                    // Return the original reference so the node can report a clear error
                    value
                }
            }
        }
        serde_json::Value::Object(map) => {
            let mut resolved = serde_json::Map::new();
            for (k, v) in map {
                resolved.insert(k, Box::pin(resolve_vault_refs(v, user_id, vault)).await);
            }
            serde_json::Value::Object(resolved)
        }
        serde_json::Value::Array(arr) => {
            let mut resolved = Vec::with_capacity(arr.len());
            for v in arr {
                resolved.push(Box::pin(resolve_vault_refs(v, user_id, vault)).await);
            }
            serde_json::Value::Array(resolved)
        }
        // Numbers, bools, null - pass through
        other => other,
    }
}

/// Converts the frontend canvas state (stored in metadata) into
/// an executable core Flow with proper Nodes and Edges.
async fn canvas_to_flow(
    stored: &Flow,
    user_id: Uuid,
    vault: &dyn CredentialVault,
) -> Result<(Flow, std::collections::HashMap<String, Uuid>), ApiError> {
    let canvas_nodes = stored
        .metadata
        .positions
        .get("canvas_nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No canvas nodes found. Save the flow first."))?;

    let canvas_edges = stored
        .metadata
        .positions
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
        let node_type_str = data["type"]
            .as_str()
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

        // Pass the node config (resolve vault references under the flow owner)
        if let Some(config) = data.get("config") {
            let resolved = resolve_vault_refs(config.clone(), user_id, vault).await;
            core_node = core_node.with_config(resolved);
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
            warn!(
                source = source,
                target = target,
                "Skipping edge with unknown nodes"
            );
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
    axum::Extension(claims): axum::Extension<Claims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = state
        .vault
        .list_keys(claims.sub)
        .await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "keys": keys })))
}

/// POST /api/v1/vault
/// Store a credential
/// Body: { "key": "openai_api_key", "value": "sk-..." }
async fn store_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key = body["key"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Missing 'key' field"))?;
    let value = body["value"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Missing 'value' field"))?;

    state
        .vault
        .store(claims.sub, key, value)
        .await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;

    Ok(Json(serde_json::json!({ "status": "stored", "key": key })))
}

/// GET /api/v1/vault/:key
/// Retrieve a credential value
async fn get_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let value = state
        .vault
        .retrieve(claims.sub, &key)
        .await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "key": key, "value": value })))
}

/// DELETE /api/v1/vault/:key
/// Delete a credential
async fn delete_credential(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .vault
        .delete(claims.sub, &key)
        .await
        .map_err(|e| ApiError::internal(format!("Vault error: {}", e)))?;
    Ok(Json(serde_json::json!({ "status": "deleted", "key": key })))
}

/// GET /api/v1/flows/:id/executions
/// Returns the persisted execution history for a flow (owner only).
async fn get_executions(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Authorize: the flow must belong to the caller.
    state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    let history = state
        .executions
        .get_history(id, 50)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "executions": history,
    })))
}

/// POST /api/v1/flows/:id/stop
async fn stop_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Only the owner may stop a flow (A-02): another user's id 404s exactly
    // like a missing flow, so ownership can't be probed.
    state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    state.engine.stop(id).await.map_err(ApiError::from)?;

    // Stopping also retires the flow's public hooks; before this, a "stopped"
    // flow kept accepting webhook requests.
    state
        .storage
        .undeploy_flow(id)
        .await
        .map_err(ApiError::from)?;
    state.hook_limits.release(id);
    Ok(Json(serde_json::json!({
        "flow_id": id.to_string(),
        "status": "stopped",
    })))
}

/// Returns the `data.config` of canvas node `node_id` in `flow`, if present.
fn trigger_node_config(flow: &Flow, node_id: &str) -> Option<serde_json::Value> {
    flow.metadata
        .positions
        .get("canvas_nodes")?
        .as_array()?
        .iter()
        .find(|n| n["id"].as_str() == Some(node_id))
        .map(|n| n["data"]["config"].clone())
}

/// Why a hook request failed its trigger's auth policy.
#[derive(Debug, PartialEq, Eq)]
enum HookAuthError {
    /// The request did not present valid credentials (401).
    Unauthorized(&'static str),
    /// The trigger's own configuration is unusable (500). Never fails open.
    Misconfigured(&'static str),
}

/// Constant-time byte comparison. Only the length (not a secret) can leak.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Verifies a request against a trigger's auth policy, given the already
/// resolved `expected` secret. Unknown auth types are rejected (fail closed).
fn verify_hook_auth(
    auth_type: &str,
    expected: &str,
    headers: &HeaderMap,
    body: &str,
) -> Result<(), HookAuthError> {
    use HookAuthError::{Misconfigured, Unauthorized};

    if auth_type == "none" {
        return Ok(());
    }
    if expected.is_empty() {
        return Err(Misconfigured("Webhook auth token not configured"));
    }

    let authorization = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match auth_type {
        "bearer" => {
            let token = authorization
                .strip_prefix("Bearer ")
                .or_else(|| authorization.strip_prefix("bearer "));
            match token {
                Some(t) if ct_eq(t.as_bytes(), expected.as_bytes()) => Ok(()),
                _ => Err(Unauthorized(
                    "Unauthorized: invalid or missing Bearer token",
                )),
            }
        }
        "basic" => {
            // Expected secret is "username:password"; the header carries it base64-encoded.
            let decoded = authorization
                .strip_prefix("Basic ")
                .or_else(|| authorization.strip_prefix("basic "))
                .and_then(|b64| {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.decode(b64).ok()
                });
            match decoded {
                Some(bytes) if ct_eq(&bytes, expected.as_bytes()) => Ok(()),
                _ => Err(Unauthorized(
                    "Unauthorized: invalid or missing Basic credentials",
                )),
            }
        }
        "hmac" => {
            // HMAC-SHA256 of the raw body, from X-Signature or X-Hub-Signature-256.
            use hmac::{Hmac, Mac};
            let raw_sig = headers
                .get("x-signature")
                .or_else(|| headers.get("x-hub-signature-256"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let provided = raw_sig.strip_prefix("sha256=").unwrap_or(raw_sig);
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(expected.as_bytes())
                .map_err(|_| Misconfigured("Invalid HMAC key configuration"))?;
            mac.update(body.as_bytes());
            // `verify_slice` is constant-time; malformed hex decodes to empty and fails.
            mac.verify_slice(&hex::decode(provided).unwrap_or_default())
                .map_err(|_| Unauthorized("Unauthorized: HMAC signature mismatch"))
        }
        _ => Err(Misconfigured("Unsupported webhook auth type")),
    }
}

/// Enforces a trigger's auth policy for an incoming hook request.
///
/// Resolves `vault:` token references under the flow owner, then delegates to
/// [`verify_hook_auth`]. The error is the ready-to-send HTTP response.
async fn authorize_trigger(
    config: &serde_json::Value,
    headers: &HeaderMap,
    body: &str,
    owner_id: Uuid,
    vault: &dyn CredentialVault,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let reject = |status: StatusCode, msg: &str| (status, Json(serde_json::json!({"error": msg})));

    let auth_type = config["authType"].as_str().unwrap_or("none");
    match auth_type {
        "none" => return Ok(()),
        "bearer" | "basic" | "hmac" => {}
        other => {
            // Fail closed: an auth type we don't understand must not let requests through.
            warn!(
                auth_type = other,
                "Hook rejected: unsupported webhook auth type"
            );
            return Err(reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unsupported webhook auth type",
            ));
        }
    }

    let raw_token = config["authToken"].as_str().unwrap_or("");
    let expected = match raw_token.strip_prefix("vault:") {
        Some(key) => vault.retrieve(owner_id, key).await.map_err(|e| {
            warn!(error = %e, key, "Failed to resolve vault ref for webhook auth");
            reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resolve auth credential",
            )
        })?,
        None => raw_token.to_string(),
    };

    verify_hook_auth(auth_type, &expected, headers, body).map_err(|e| match e {
        HookAuthError::Unauthorized(msg) => reject(StatusCode::UNAUTHORIZED, msg),
        HookAuthError::Misconfigured(msg) => reject(StatusCode::INTERNAL_SERVER_ERROR, msg),
    })
}

/// Restricts `flow` to `trigger` and everything reachable downstream of it.
///
/// The engine delivers the trigger message to every root node, so without this
/// a request to one trigger would also fire every other trigger's branch.
/// Edges coming from pruned nodes are dropped too, so a shared downstream node
/// only receives this branch's messages.
fn restrict_to_trigger(flow: &mut Flow, trigger: Uuid) {
    let mut keep = std::collections::HashSet::from([trigger]);
    let mut queue = std::collections::VecDeque::from([trigger]);
    while let Some(node) = queue.pop_front() {
        for edge in flow.edges.iter().filter(|e| e.from_node == node) {
            if keep.insert(edge.to_node) {
                queue.push_back(edge.to_node);
            }
        }
    }
    flow.nodes.retain(|n| keep.contains(&n.id));
    flow.edges
        .retain(|e| keep.contains(&e.from_node) && keep.contains(&e.to_node));
}

/// ANY /hook/{flow_id} or /hook/{flow_id}/{*path}
///
/// Unified hook handler for all input node types (http-in, webhook, etc.).
/// Each flow gets its own namespace under /hook/{flow_id}, preventing
/// route collisions between flows - ready for multi-tenant SaaS.
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
    let sub_path = params
        .get("path")
        .map(|p| format!("/{}", p))
        .unwrap_or_else(|| "/".to_string());

    let method_str = method.to_string().to_uppercase();

    info!(
        flow_id = %flow_id,
        method = %method_str,
        path = %sub_path,
        "Hook triggered"
    );

    // Authorize against the persisted hook routes BEFORE loading or running
    // anything: the flow must be deployed AND expose this exact method + path.
    // This blocks triggering an arbitrary flow by id, using the wrong HTTP
    // method, or hitting a path the flow never configured. The matched route
    // also yields the owner, which scopes credential resolution to their vault.
    let hook = match state
        .storage
        .find_hook_route(flow_id, &method_str, &sub_path)
        .await
    {
        Ok(Some(hook)) => hook,
        Ok(None) => {
            warn!(flow_id = %flow_id, method = %method_str, path = %sub_path,
                "Hook rejected: no matching deployed route");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "No matching hook route for this flow, method, and path"
                })),
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to look up hook route");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            );
        }
    };
    let owner_id = hook.user_id;

    // Run the deployed snapshot, never the live (editable) flow (A-05).
    let snapshot = match state.storage.get_deployment(flow_id).await {
        Ok(Some(flow)) => flow,
        Ok(None) => {
            warn!(flow_id = %flow_id, "Hook rejected: route has no deployment snapshot");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Flow is not deployed"})),
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to load deployment snapshot");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            );
        }
    };

    // Enforce the auth policy of the trigger bound to THIS route (A-01), not
    // of whichever trigger happens to come first on the canvas.
    let Some(trigger_config) = trigger_node_config(&snapshot, &hook.node_id) else {
        error!(flow_id = %flow_id, node_id = %hook.node_id, "Deployed trigger node missing from snapshot");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Deployed trigger not found"})),
        );
    };
    if let Err(rejection) = authorize_trigger(
        &trigger_config,
        &headers,
        &body,
        owner_id,
        state.vault.as_ref(),
    )
    .await
    {
        info!(flow_id = %flow_id, node_id = %hook.node_id, "Hook rejected by trigger auth policy");
        return rejection;
    }

    // Bound concurrent executions per flow (A-06). The slot is held until this
    // handler returns; by then the execution has finished or been cancelled.
    let Some(_slot) = state.hook_limits.try_acquire(flow_id) else {
        warn!(flow_id = %flow_id, "Hook rejected: flow at its concurrent execution limit");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Too many concurrent executions for this flow"})),
        );
    };

    // Build executable flow (vault refs resolved under the flow owner)
    let (mut exec_flow, id_map) = match canvas_to_flow(&snapshot, owner_id, state.vault.as_ref())
        .await
    {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": format!("Failed to compile flow: {}", e.message)}),
                ),
            );
        }
    };

    // Execute only the matched trigger's branch (A-01): other roots, including
    // other triggers with different auth policies, must not fire.
    let Some(&trigger_id) = id_map.get(&hook.node_id) else {
        error!(flow_id = %flow_id, node_id = %hook.node_id, "Trigger node not compiled");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Deployed trigger not found"})),
        );
    };
    restrict_to_trigger(&mut exec_flow, trigger_id);

    // Parse body as JSON (or wrap raw string)
    let body_json: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| {
        if body.is_empty() {
            serde_json::json!(null)
        } else {
            serde_json::json!(body)
        }
    });

    // Convert headers to JSON map
    let headers_json: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), serde_json::Value::String(v.to_string())))
        })
        .collect();

    // Convert query params to JSON
    let query_json: serde_json::Value =
        serde_json::to_value(&query_params).unwrap_or(serde_json::json!({}));

    // Create the trigger message with real HTTP data
    let trace_id = Uuid::now_v7();
    let trigger_payload = serde_json::json!({
        "method": method_str,
        "path": sub_path,
        "headers": headers_json,
        "query": query_json,
        "body": body_json,
    });

    let trigger_msg = FlowMessage::new(Uuid::nil(), "hook", trigger_payload, trace_id);

    // Create oneshot channel for the response
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.webhook_responders.write().await.insert(trace_id, tx);

    // Execute the flow with the trigger message
    match state
        .engine
        .execute_with_trigger(exec_flow, Some(trigger_msg))
        .await
    {
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
    await_flow_response(flow_id, trace_id, rx, &state).await
}

/// Shared logic: waits for the oneshot response from http-out node.
///
/// `trace_id` is the key under which the responder was registered in
/// `state.webhook_responders` (see `hook_handler`), so cleanup on
/// drop/timeout MUST remove with `trace_id` — removing by `flow_id`
/// would leak this responder and could evict an unrelated entry.
/// `flow_id` is retained purely for logging correlation.
async fn await_flow_response(
    flow_id: Uuid,
    trace_id: Uuid,
    rx: tokio::sync::oneshot::Receiver<z8run_core::nodes::http_out::WebhookResponse>,
    state: &Arc<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let timeout = state.hook_limits.timeout;
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(response)) => {
            info!(
                flow_id = %flow_id,
                status = response.status,
                "Flow response sent"
            );
            let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK);
            (status, Json(response.body))
        }
        Ok(Err(_)) => {
            warn!(flow_id = %flow_id, "Response channel dropped");
            state.webhook_responders.write().await.remove(&trace_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Flow completed without sending a response"})),
            )
        }
        Err(_) => {
            // The caller is gone: stop the execution instead of letting it keep
            // spending CPU, network and third-party quota (A-06).
            let cancelled = state.engine.cancel_execution(trace_id).await;
            warn!(flow_id = %flow_id, timeout_secs = timeout.as_secs(), cancelled,
                "Hook timed out; execution cancelled");
            state.webhook_responders.write().await.remove(&trace_id);
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": format!("Flow execution timed out ({}s)", timeout.as_secs())
                })),
            )
        }
    }
}

/// GET /api/v1/flows/:id/export - Export a flow as a portable JSON document.
///
/// The exported JSON includes flow metadata and the full canvas state
/// (nodes, edges, viewport) but strips internal fields like user_id
/// so it can be imported by any user.
async fn export_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flow = state
        .storage
        .get_flow_for_user(id, claims.sub)
        .await
        .map_err(ApiError::from)?;

    let canvas_nodes = flow
        .metadata
        .positions
        .get("canvas_nodes")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let canvas_edges = flow
        .metadata
        .positions
        .get("canvas_edges")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let viewport = flow
        .metadata
        .positions
        .get("viewport")
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

/// POST /api/v1/flows/import - Import a flow from an exported JSON document.
///
/// Creates a brand-new flow (new UUID) owned by the authenticated user,
/// populated with the canvas state from the export.
async fn import_flow(
    State(state): State<Arc<AppState>>,
    axum::Extension(claims): axum::Extension<Claims>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate export format
    let flow_data = payload
        .get("flow")
        .ok_or_else(|| ApiError::bad_request("Invalid export: missing 'flow' key"))?;

    let name = flow_data["name"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Invalid export: missing flow name"))?;
    let description = flow_data["description"].as_str().unwrap_or("");
    let version = flow_data["version"].as_str().unwrap_or("0.1.0");

    // Validate node types - reject unknown nodes before creating the flow.
    //
    // This is the complete canonical set of node types the UI/engine support.
    // It MUST stay in sync with the frontend node registry
    // (`frontend/src/lib/nodeDefinitions.ts`) and the engine registration
    // (`crates/z8run-core/src/nodes/mod.rs`). Deriving this list from a shared
    // registry source is a future improvement.
    const VALID_NODE_TYPES: &[&str] = &[
        "aggregator",
        "ai-agent",
        "batch",
        "classifier",
        "conversation-memory",
        "crm",
        "cron-trigger",
        "csv",
        "database",
        "debug",
        "delay",
        "embeddings",
        "filter",
        "function",
        "http-in",
        "http-out",
        "http-request",
        "human-handoff",
        "if-else",
        "image-gen",
        "json",
        "llm",
        "loop",
        "mapper",
        "mqtt",
        "prompt-template",
        "sanitize",
        "structured-output",
        "stt",
        "summarizer",
        "switch",
        "text-splitter",
        "timer",
        "tts",
        "twilio",
        "vector-store",
        "webhook",
        "webhook-trigger",
        "whatsapp",
    ];

    if let Some(canvas_nodes) = flow_data["canvas_nodes"].as_array() {
        let mut unknown_types: Vec<String> = Vec::new();

        for node in canvas_nodes {
            let node_type = node["data"]["type"].as_str().unwrap_or("unknown");
            // VALID_NODE_TYPES covers the built-ins, which are known at compile
            // time. WASM plugins are registered at startup and cannot be in a
            // const, so ask the engine too — otherwise an installed, loadable
            // plugin is rejected as an unsupported type.
            if !VALID_NODE_TYPES.contains(&node_type)
                && !state.engine.has_node_type(node_type).await
            {
                unknown_types.push(node_type.to_string());
            }
        }

        if !unknown_types.is_empty() {
            // Deduplicate
            unknown_types.sort();
            unknown_types.dedup();
            // Report what is ACTUALLY available, plugins included. The old
            // message listed only the const, so an installed plugin looked
            // like it did not exist.
            let available = state.engine.registered_node_types().await;
            return Err(ApiError::bad_request(format!(
                "Flow contains unsupported node types: {}. Supported types: {}",
                unknown_types.join(", "),
                available.join(", "),
            )));
        }
    }

    // Create a new flow with a fresh ID
    let mut flow = Flow::new(name);
    flow.description = description.to_string();
    flow.version = version.to_string();

    // Tags and notes come off the FLOW object, not the envelope: `name` and
    // `description` above are read from `flow_data`, and reading these from
    // `payload` yields nothing — the key persisted empty, which reads as "no
    // tags were set" rather than as a wiring bug. Accepted at the flow's top
    // level (where an author writes them) or under `metadata` (where an export
    // round-trips them).
    if let Some(tags) = flow_data
        .get("tags")
        .or_else(|| flow_data.pointer("/metadata/tags"))
        .and_then(|v| v.as_array())
    {
        flow.metadata.tags = tags
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect();
    }
    if let Some(notes) = flow_data
        .get("notes")
        .or_else(|| flow_data.pointer("/metadata/notes"))
        .and_then(|v| v.as_array())
    {
        flow.metadata.notes = notes
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect();
    }

    // Restore canvas state into metadata
    if let Some(nodes) = flow_data.get("canvas_nodes") {
        flow.metadata
            .positions
            .insert("canvas_nodes".to_string(), nodes.clone());
    }
    if let Some(edges) = flow_data.get("canvas_edges") {
        flow.metadata
            .positions
            .insert("canvas_edges".to_string(), edges.clone());
    }
    if let Some(vp) = flow_data.get("viewport") {
        flow.metadata
            .positions
            .insert("viewport".to_string(), vp.clone());
    }

    // Count imported nodes/edges for the response
    let node_count = flow_data["canvas_nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let edge_count = flow_data["canvas_edges"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    // Save with user ownership
    state
        .storage
        .save_flow_with_user(&flow, claims.sub)
        .await
        .map_err(ApiError::from)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_canvas_accepts_wellformed_and_empty_payloads() {
        // Empty payload (no canvas fields) is fine.
        assert!(validate_canvas(&json!({})).is_ok());
        // Well-formed nodes + edges.
        let ok = json!({
            "canvas_nodes": [
                {"id": "n1", "data": {"type": "http-in"}},
                {"id": "n2", "data": {"nodeType": "http-out"}}
            ],
            "canvas_edges": [{"source": "n1", "target": "n2"}]
        });
        assert!(validate_canvas(&ok).is_ok());
    }

    #[test]
    fn validate_canvas_rejects_malformed_shapes() {
        // canvas_nodes must be an array.
        assert!(validate_canvas(&json!({"canvas_nodes": {}})).is_err());
        // node missing id.
        assert!(validate_canvas(&json!({"canvas_nodes": [{"data": {"type": "debug"}}]})).is_err());
        // node missing data.type / data.nodeType.
        assert!(validate_canvas(&json!({"canvas_nodes": [{"id": "n1", "data": {}}]})).is_err());
        // edge missing target.
        assert!(validate_canvas(&json!({
            "canvas_nodes": [{"id": "n1", "data": {"type": "debug"}}],
            "canvas_edges": [{"source": "n1"}]
        }))
        .is_err());
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    /// A-01: only the matched trigger's branch survives; edges from other
    /// roots into shared nodes are dropped, so those branches cannot fire.
    #[test]
    fn restrict_to_trigger_keeps_only_the_triggered_branch() {
        let mut flow = Flow::new("f");
        let node = |name: &str| Node::new(name, "debug");
        let (a, b, shared, a_only, b_only) =
            (node("A"), node("B"), node("S"), node("A1"), node("B1"));
        let ids = (a.id, b.id, shared.id, a_only.id, b_only.id);
        for n in [a, b, shared, a_only, b_only] {
            flow.nodes.push(n);
        }
        flow.edges.push(Edge::new(ids.0, "output", ids.2, "input")); // A -> S
        flow.edges.push(Edge::new(ids.1, "output", ids.2, "input")); // B -> S
        flow.edges.push(Edge::new(ids.0, "output", ids.3, "input")); // A -> A1
        flow.edges.push(Edge::new(ids.1, "output", ids.4, "input")); // B -> B1

        restrict_to_trigger(&mut flow, ids.0);

        let mut kept: Vec<Uuid> = flow.nodes.iter().map(|n| n.id).collect();
        kept.sort();
        let mut expected = vec![ids.0, ids.2, ids.3];
        expected.sort();
        assert_eq!(kept, expected, "only A and its downstream nodes remain");
        assert_eq!(flow.edges.len(), 2, "B -> S and B -> B1 are dropped");
        assert!(flow.edges.iter().all(|e| e.from_node == ids.0));
    }

    #[test]
    fn verify_hook_auth_bearer_and_basic() {
        use base64::Engine;
        assert_eq!(verify_hook_auth("none", "", &HeaderMap::new(), ""), Ok(()));

        let ok = headers(&[("authorization", "Bearer s3cret")]);
        assert_eq!(verify_hook_auth("bearer", "s3cret", &ok, ""), Ok(()));
        for bad in [
            headers(&[("authorization", "Bearer nope")]),
            headers(&[("authorization", "Bearer s3cre")]),
            HeaderMap::new(),
        ] {
            assert!(matches!(
                verify_hook_auth("bearer", "s3cret", &bad, ""),
                Err(HookAuthError::Unauthorized(_))
            ));
        }

        let creds = base64::engine::general_purpose::STANDARD.encode("user:pass");
        let basic = headers(&[("authorization", &format!("Basic {creds}"))]);
        assert_eq!(verify_hook_auth("basic", "user:pass", &basic, ""), Ok(()));
        assert!(verify_hook_auth("basic", "user:other", &basic, "").is_err());
    }

    #[test]
    fn verify_hook_auth_hmac() {
        use hmac::{Hmac, Mac};
        let body = r#"{"event":"push"}"#;
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"key").unwrap();
        mac.update(body.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());

        let good = headers(&[("x-hub-signature-256", &format!("sha256={sig}"))]);
        assert_eq!(verify_hook_auth("hmac", "key", &good, body), Ok(()));
        // Same signature over a tampered body must fail.
        assert!(verify_hook_auth("hmac", "key", &good, r#"{"event":"x"}"#).is_err());
    }

    /// A-01: misconfiguration never lets a request through.
    #[test]
    fn verify_hook_auth_fails_closed() {
        let any = headers(&[("authorization", "Bearer x")]);
        assert!(matches!(
            verify_hook_auth("magic", "secret", &any, ""),
            Err(HookAuthError::Misconfigured(_))
        ));
        assert!(matches!(
            verify_hook_auth("bearer", "", &any, ""),
            Err(HookAuthError::Misconfigured(_))
        ));
    }

    #[test]
    fn ct_eq_compares_exactly() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(ct_eq(b"", b""));
    }
}
