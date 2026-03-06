//! # z8run-core
//!
//! Next-generation visual flow engine.
//! This crate contains the data model, flow compiler,
//! scheduler, and motor executor.

pub mod engine;
pub mod error;
pub mod flow;
pub mod message;
pub mod node;
pub mod nodes;
pub mod scheduler;

pub use engine::FlowEngine;
pub use error::{Z8Error, Z8Result};
pub use flow::{Flow, FlowConfig, FlowMeta, FlowStatus};
pub use message::FlowMessage;
pub use node::{Node, NodeType, Port, PortDirection, PortType};
