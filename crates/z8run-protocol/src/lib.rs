//! # z8run-protocol
//!
//! Custom binary protocol over WebSockets.
//! Defines frame format, message types,
//! and payload serialization/deserialization.

pub mod frame;
pub mod message;
pub mod codec;

pub use frame::{Frame, FrameHeader, PROTOCOL_VERSION};
pub use message::ProtocolMessage;
