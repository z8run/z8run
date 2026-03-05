//! WebSocket server for real-time communication.

use std::sync::Arc;
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::time::{interval, Duration};
use tracing::{info, warn, debug};

use crate::state::AppState;
use z8run_core::engine::EngineEvent;

/// Mounts the WebSocket routes.
pub fn ws_routes() -> Router<Arc<AppState>> {
    Router::new().route("/engine", get(ws_handler))
}

/// WebSocket upgrade handler.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
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
        EngineEvent::NodeCompleted { flow_id, node_id, duration_us, output_preview } => {
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
        },
        EngineEvent::NodeSkipped { flow_id, node_id } => serde_json::json!({
            "type": "node_skipped",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
        }),
        EngineEvent::NodeError { flow_id, node_id, error } => serde_json::json!({
            "type": "node_error",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
            "error": error,
        }),
        EngineEvent::MessageSent { flow_id, from_node, to_node, message_id, payload_preview } => {
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
        },
        EngineEvent::FlowCompleted { flow_id, trace_id, duration_ms } => serde_json::json!({
            "type": "flow_completed",
            "flow_id": flow_id.to_string(),
            "trace_id": trace_id.to_string(),
            "duration_ms": duration_ms,
        }),
        EngineEvent::FlowError { flow_id, trace_id, error } => serde_json::json!({
            "type": "flow_error",
            "flow_id": flow_id.to_string(),
            "trace_id": trace_id.to_string(),
            "error": error,
        }),
        EngineEvent::StreamChunk { flow_id, node_id, chunk, done } => serde_json::json!({
            "type": "stream_chunk",
            "flow_id": flow_id.to_string(),
            "node_id": node_id.to_string(),
            "chunk": chunk,
            "done": done,
        }),
    }
}

/// Handles an active WebSocket connection with keepalive pings.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    info!("New WebSocket connection established");

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
                        // Client responded to our ping — connection is alive
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
                        // Broadcast sender dropped — engine was shut down.
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
