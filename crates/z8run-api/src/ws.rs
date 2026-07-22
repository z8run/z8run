//! WebSocket server for real-time communication.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::auth::decode_jwt;
use crate::state::AppState;
use z8run_core::engine::EngineEvent;

/// WebSocket subprotocol used to carry the auth token.
const AUTH_PROTOCOL: &str = "z8.jwt";

/// Mounts the WebSocket routes.
pub fn ws_routes() -> Router<Arc<AppState>> {
    Router::new().route("/engine", get(ws_handler))
}

/// Extracts the JWT from the `Sec-WebSocket-Protocol` header.
///
/// The client offers two subprotocols: our marker (`z8.jwt`) and the token
/// itself. We read the token from the header rather than the query string so
/// it never lands in access logs or proxy logs.
fn token_from_protocols(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("sec-websocket-protocol")?.to_str().ok()?;
    let mut parts = raw.split(',').map(|p| p.trim());
    // First entry must be our marker; the second is the token.
    if parts.next() != Some(AUTH_PROTOCOL) {
        return None;
    }
    parts.next().filter(|t| !t.is_empty()).map(str::to_string)
}

/// WebSocket upgrade handler.
///
/// Requires authentication. Browsers cannot set an `Authorization` header on a
/// WebSocket handshake, so the JWT is passed via the `Sec-WebSocket-Protocol`
/// header and validated before the connection is upgraded. Unauthenticated
/// clients are rejected with `401` instead of receiving the engine event stream.
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let token = match token_from_protocols(&headers) {
        Some(t) => t,
        None => {
            warn!("WebSocket rejected: missing token");
            return (StatusCode::UNAUTHORIZED, "Missing authentication token").into_response();
        }
    };

    let claims = match decode_jwt(&token, &state.jwt_secret) {
        Ok(c) if !c.is_expired() => c,
        _ => {
            warn!("WebSocket rejected: invalid or expired token");
            return (StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response();
        }
    };

    let user_id = claims.sub;
    // Echo back the accepted subprotocol so the browser completes the handshake.
    ws.protocols([AUTH_PROTOCOL])
        .on_upgrade(move |socket| handle_socket(socket, state, user_id))
}

/// Converts an EngineEvent to a JSON value for the frontend.
fn event_to_json(event: &EngineEvent) -> serde_json::Value {
    match event {
        EngineEvent::FlowStarted { flow_id, trace_id } => serde_json::json!({
            "type": "flow_started",
            "flow_id": flow_id.to_string(),
            "trace_id": trace_id.to_string(),
        }),
        EngineEvent::NodeStarted { flow_id, node_id } => serde_json::json!({
            "type": "node_started",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
        }),
        EngineEvent::NodeCompleted {
            flow_id,
            node_id,
            duration_us,
            output_preview,
        } => {
            let mut v = serde_json::json!({
                "type": "node_completed",
                "flow_id": flow_id.to_string(),
                "node_id": node_id.to_string(),
                "duration_us": duration_us,
            });
            if let Some(preview) = output_preview {
                v["output"] = preview.clone();
            }
            v
        }
        EngineEvent::NodeSkipped { flow_id, node_id } => serde_json::json!({
            "type": "node_skipped",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
        }),
        EngineEvent::NodeError {
            flow_id,
            node_id,
            error,
        } => serde_json::json!({
            "type": "node_error",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
            "error": error,
        }),
        EngineEvent::MessageSent {
            flow_id,
            from_node,
            to_node,
            message_id,
            payload_preview,
        } => {
            let mut v = serde_json::json!({
                "type": "message_sent",
                "flow_id": flow_id.to_string(),
                "from_node": from_node.to_string(),
                "to_node": to_node.to_string(),
                "message_id": message_id.to_string(),
            });
            if let Some(preview) = payload_preview {
                v["payload"] = preview.clone();
            }
            v
        }
        EngineEvent::FlowCompleted {
            flow_id,
            trace_id,
            duration_ms,
        } => serde_json::json!({
            "type": "flow_completed",
            "flow_id": flow_id.to_string(),
            "trace_id": trace_id.to_string(),
            "duration_ms": duration_ms,
        }),
        EngineEvent::FlowError {
            flow_id,
            trace_id,
            error,
        } => serde_json::json!({
            "type": "flow_error",
            "flow_id": flow_id.to_string(),
            "trace_id": trace_id.to_string(),
            "error": error,
        }),
        EngineEvent::StreamChunk {
            flow_id,
            node_id,
            chunk,
            done,
        } => serde_json::json!({
            "type": "stream_chunk",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
            "chunk": chunk,
            "done": done,
        }),
    }
}

/// Handles an active WebSocket connection with keepalive pings.
///
/// `user_id` identifies the authenticated client. Engine events are not yet
/// filtered per user (that requires threading the owner through the engine's
/// event model); for now authentication gates who may receive the stream.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, user_id: Uuid) {
    info!(user_id = %user_id, "New WebSocket connection established");

    // Subscribe to engine events
    let mut event_rx = state.engine.subscribe_events();

    // Ping interval to keep the connection alive
    let mut ping_interval = interval(Duration::from_secs(30));
    ping_interval.tick().await; // consume the immediate first tick

    loop {
        tokio::select! {
            // Keepalive ping
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![1, 2, 3, 4].into())).await.is_err() {
                    warn!("Failed to send ping, closing WebSocket");
                    break;
                }
            }

            // Client messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        debug!(text = %text, "Client message received");
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to our ping - connection is alive
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket connection closed by client");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket recv error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            // Engine events -> forward to client as JSON
            event = event_rx.recv() => {
                match event {
                    Ok(engine_event) => {
                        let json = event_to_json(&engine_event);
                        let text = serde_json::to_string(&json).unwrap_or_default();
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            warn!("Failed to send event to WebSocket client");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(lagged = n, "WebSocket client lagged, missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Broadcast sender dropped - engine was shut down.
                        // Sleep to avoid busy-loop; ping/recv arms handle disconnect.
                        debug!("Broadcast channel closed, waiting...");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    info!("WebSocket handler exiting");
}
