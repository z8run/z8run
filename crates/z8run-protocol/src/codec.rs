//! Codec for encoding/decoding frames of the z8run protocol.
//!
//! Integrates with tokio-tungstenite to send/receive
//! binary frames over WebSockets.

use crate::frame::{Frame, ProtocolError};
use crate::message::{MessageType, ProtocolMessage};

use std::sync::atomic::{AtomicU32, Ordering};

/// Codec of the z8run protocol.
/// Handles request/response correlation and serialization.
pub struct Z8Codec {
    /// Atomic counter for correlation IDs.
    next_correlation_id: AtomicU32,
    /// Debug mode: serializes payloads as JSON instead of bincode.
    debug_mode: bool,
}

impl Z8Codec {
    pub fn new() -> Self {
        Self {
            next_correlation_id: AtomicU32::new(1),
            debug_mode: false,
        }
    }

    pub fn with_debug_mode(mut self, debug: bool) -> Self {
        self.debug_mode = debug;
        self
    }

    /// Generates a new correlation ID.
    pub fn next_id(&self) -> u32 {
        self.next_correlation_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Encodes a ProtocolMessage into a binary Frame.
    pub fn encode(&self, msg: &ProtocolMessage) -> Result<Frame, ProtocolError> {
        let msg_type = self.message_type_id(msg);
        let correlation_id = self.next_id();

        let payload = if self.debug_mode {
            serde_json::to_vec(msg).map_err(|e| ProtocolError::Serialization(e.to_string()))?
        } else {
            bincode::serialize(msg).map_err(|e| ProtocolError::Serialization(e.to_string()))?
        };

        Frame::new(msg_type, correlation_id, payload)
    }

    /// Decodes a binary Frame into a ProtocolMessage.
    pub fn decode(&self, frame: &Frame) -> Result<ProtocolMessage, ProtocolError> {
        if self.debug_mode {
            serde_json::from_slice(&frame.payload)
                .map_err(|e| ProtocolError::Deserialization(e.to_string()))
        } else {
            bincode::deserialize(&frame.payload)
                .map_err(|e| ProtocolError::Deserialization(e.to_string()))
        }
    }

    /// Encodes directly to bytes (header + payload).
    pub fn encode_bytes(&self, msg: &ProtocolMessage) -> Result<Vec<u8>, ProtocolError> {
        let frame = self.encode(msg)?;
        Ok(frame.to_bytes())
    }

    /// Decodes directly from bytes.
    pub fn decode_bytes(&self, data: &[u8]) -> Result<ProtocolMessage, ProtocolError> {
        let frame = Frame::from_bytes(data)?;
        self.decode(&frame)
    }

    /// Gets the message type ID for the header.
    fn message_type_id(&self, msg: &ProtocolMessage) -> u16 {
        match msg {
            ProtocolMessage::Ping => MessageType::Ping as u16,
            ProtocolMessage::Pong => MessageType::Pong as u16,
            ProtocolMessage::Authenticate { .. } => MessageType::Authenticate as u16,
            ProtocolMessage::AuthResult { .. } => MessageType::AuthResult as u16,
            ProtocolMessage::FlowCreate { .. } => MessageType::FlowCreate as u16,
            ProtocolMessage::FlowUpdate { .. } => MessageType::FlowUpdate as u16,
            ProtocolMessage::FlowDelete { .. } => MessageType::FlowDelete as u16,
            ProtocolMessage::FlowStart { .. } => MessageType::FlowStart as u16,
            ProtocolMessage::FlowStop { .. } => MessageType::FlowStop as u16,
            ProtocolMessage::FlowList => MessageType::FlowList as u16,
            ProtocolMessage::FlowGet { .. } => MessageType::FlowGet as u16,
            ProtocolMessage::ExecStarted { .. } => MessageType::ExecStarted as u16,
            ProtocolMessage::ExecNodeStarted { .. } => MessageType::ExecNodeStarted as u16,
            ProtocolMessage::ExecNodeCompleted { .. } => MessageType::ExecNodeCompleted as u16,
            ProtocolMessage::ExecNodeError { .. } => MessageType::ExecNodeError as u16,
            ProtocolMessage::ExecCompleted { .. } => MessageType::ExecCompleted as u16,
            ProtocolMessage::DebugSetBreakpoint { .. } => MessageType::DebugSetBreakpoint as u16,
            ProtocolMessage::DebugRemoveBreakpoint { .. } => {
                MessageType::DebugRemoveBreakpoint as u16
            }
            ProtocolMessage::DebugInspect { .. } => MessageType::DebugInspect as u16,
            ProtocolMessage::DebugInspectResult { .. } => MessageType::DebugInspectResult as u16,
            ProtocolMessage::Ok { .. } => MessageType::Ok as u16,
            ProtocolMessage::Error { .. } => MessageType::Error as u16,
        }
    }
}

impl Default for Z8Codec {
    fn default() -> Self {
        Self::new()
    }
}
