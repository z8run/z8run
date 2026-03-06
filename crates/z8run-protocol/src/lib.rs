//! # z8run-protocol
//!
//! Custom binary protocol over WebSockets.
//! Defines frame format, message types,
//! and payload serialization/deserialization.

pub mod codec;
pub mod frame;
pub mod message;

pub use frame::{Frame, FrameHeader, PROTOCOL_VERSION};
pub use message::ProtocolMessage;
