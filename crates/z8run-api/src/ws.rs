//! WebSocket server for real-time communication.

use std::sync::Arc;
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use tracing::{info, warn, debug};

use crate::state::AppState;
use z8run_protocol::codec::Z8Codec;

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

/// Handles an active WebSocket connection.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    info!("New WebSocket connection established");

    let codec = Z8Codec::new();

    // Subscribe to engine events
    let mut event_rx = state.engine.subscribe_events();

    loop {
        tokio::select! {
            // Client messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        debug!(bytes = data.len(), "Binary message received");
                        match codec.decode_bytes(&data) {
                            Ok(protocol_msg) => {
                                debug!(?protocol_msg, "Message decoded");
                                // TODO: process the message and respond
                            }
                            Err(e) => {
                                warn!(error = %e, "Error decoding message");
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        debug!("Text message received (debug mode)");
                        // In debug mode, accept JSON
                        match z8run_protocol::ProtocolMessage::from_json(text.as_str()) {
                            Ok(protocol_msg) => {
                                debug!(?protocol_msg, "JSON message decoded");
                            }
                            Err(e) => {
                                warn!(error = %e, "Error parsing JSON");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket connection closed");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }
            // Engine events -> forward to client
            event = event_rx.recv() => {
                match event {
                    Ok(engine_event) => {
                        // TODO: convert EngineEvent to ProtocolMessage and send
                        debug!(?engine_event, "Engine event");
                    }
                    Err(_) => {
                        // Channel closed, reconnect
                        event_rx = state.engine.subscribe_events();
                    }
                }
            }
        }
    }
}
