//! # z8run-core
//!
//! Next-generation visual flow engine.
//! This crate contains the data model, flow compiler,
//! scheduler, and motor executor.

pub mod error;
pub mod flow;
pub mod node;
pub mod message;
pub mod engine;
pub mod scheduler;

pub use error::{Z8Error, Z8Result};
pub use flow::{Flow, FlowConfig, FlowMeta, FlowStatus};
pub use node::{Node, NodeType, Port, PortDirection, PortType};
pub use message::FlowMessage;
pub use engine::FlowEngine;
