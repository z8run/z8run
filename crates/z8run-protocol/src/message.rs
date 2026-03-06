//! Message types of the z8run protocol.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message types of the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum MessageType {
    // ── Control (0x00xx) ──
    Ping = 0x0001,
    Pong = 0x0002,
    Authenticate = 0x0003,
    AuthResult = 0x0004,
    VersionNegotiation = 0x0005,

    // ── Flow Commands (0x01xx) ──
    FlowCreate = 0x0100,
    FlowUpdate = 0x0101,
    FlowDelete = 0x0102,
    FlowDeploy = 0x0103,
    FlowStart = 0x0104,
    FlowStop = 0x0105,
    FlowPause = 0x0106,
    FlowResume = 0x0107,
    FlowList = 0x0108,
    FlowGet = 0x0109,

    // ── Execution Events (0x02xx) ──
    ExecStarted = 0x0200,
    ExecNodeStarted = 0x0201,
    ExecNodeCompleted = 0x0202,
    ExecNodeError = 0x0203,
    ExecMessageSent = 0x0204,
    ExecCompleted = 0x0205,
    ExecError = 0x0206,

    // ── Debug (0x03xx) ──
    DebugSetBreakpoint = 0x0300,
    DebugRemoveBreakpoint = 0x0301,
    DebugStep = 0x0302,
    DebugInspect = 0x0303,
    DebugInspectResult = 0x0304,

    // ── Editor Sync (0x04xx) ──
    EditorNodeMoved = 0x0400,
    EditorZoomChanged = 0x0401,
    EditorSelectionChanged = 0x0402,
    EditorCursorPosition = 0x0403,

    // ── Response (0x0Fxx) ──
    Ok = 0x0F00,
    Error = 0x0F01,
}

impl MessageType {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0001 => Some(Self::Ping),
            0x0002 => Some(Self::Pong),
            0x0003 => Some(Self::Authenticate),
            0x0004 => Some(Self::AuthResult),
            0x0005 => Some(Self::VersionNegotiation),
            0x0100 => Some(Self::FlowCreate),
            0x0101 => Some(Self::FlowUpdate),
            0x0102 => Some(Self::FlowDelete),
            0x0103 => Some(Self::FlowDeploy),
            0x0104 => Some(Self::FlowStart),
            0x0105 => Some(Self::FlowStop),
            0x0106 => Some(Self::FlowPause),
            0x0107 => Some(Self::FlowResume),
            0x0108 => Some(Self::FlowList),
            0x0109 => Some(Self::FlowGet),
            0x0200 => Some(Self::ExecStarted),
            0x0201 => Some(Self::ExecNodeStarted),
            0x0202 => Some(Self::ExecNodeCompleted),
            0x0203 => Some(Self::ExecNodeError),
            0x0204 => Some(Self::ExecMessageSent),
            0x0205 => Some(Self::ExecCompleted),
            0x0206 => Some(Self::ExecError),
            0x0300 => Some(Self::DebugSetBreakpoint),
            0x0301 => Some(Self::DebugRemoveBreakpoint),
            0x0302 => Some(Self::DebugStep),
            0x0303 => Some(Self::DebugInspect),
            0x0304 => Some(Self::DebugInspectResult),
            0x0400 => Some(Self::EditorNodeMoved),
            0x0401 => Some(Self::EditorZoomChanged),
            0x0402 => Some(Self::EditorSelectionChanged),
            0x0403 => Some(Self::EditorCursorPosition),
            0x0F00 => Some(Self::Ok),
            0x0F01 => Some(Self::Error),
            _ => None,
        }
    }
}

/// Protocol message with typed payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ProtocolMessage {
    // ── Control ──
    Ping,
    Pong,
    Authenticate {
        token: String,
    },
    AuthResult {
        success: bool,
        user_id: Option<Uuid>,
    },

    // ── Flow Commands ──
    FlowCreate {
        flow: serde_json::Value,
    },
    FlowUpdate {
        flow_id: Uuid,
        changes: serde_json::Value,
    },
    FlowDelete {
        flow_id: Uuid,
    },
    FlowStart {
        flow_id: Uuid,
    },
    FlowStop {
        flow_id: Uuid,
    },
    FlowList,
    FlowGet {
        flow_id: Uuid,
    },

    // ── Execution Events ──
    ExecStarted {
        flow_id: Uuid,
        trace_id: Uuid,
    },
    ExecNodeStarted {
        flow_id: Uuid,
        node_id: Uuid,
    },
    ExecNodeCompleted {
        flow_id: Uuid,
        node_id: Uuid,
        duration_us: u64,
    },
    ExecNodeError {
        flow_id: Uuid,
        node_id: Uuid,
        error: String,
    },
    ExecCompleted {
        flow_id: Uuid,
        trace_id: Uuid,
        duration_ms: u64,
    },

    // ── Debug ──
    DebugSetBreakpoint {
        flow_id: Uuid,
        node_id: Uuid,
    },
    DebugRemoveBreakpoint {
        flow_id: Uuid,
        node_id: Uuid,
    },
    DebugInspect {
        flow_id: Uuid,
        node_id: Uuid,
    },
    DebugInspectResult {
        node_id: Uuid,
        data: serde_json::Value,
    },

    // ── Responses ──
    Ok {
        correlation_id: u32,
        data: Option<serde_json::Value>,
    },
    Error {
        correlation_id: u32,
        code: u16,
        message: String,
    },
}

impl ProtocolMessage {
    /// Serializes the message to bincode.
    pub fn to_bincode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserializes a message from bincode.
    pub fn from_bincode(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }

    /// Serializes to JSON (debug mode).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes from JSON (debug mode).
    pub fn from_json(data: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(data)
    }
}
