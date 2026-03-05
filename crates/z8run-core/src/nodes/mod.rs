//! Built-in node executor implementations.
//!
//! These are the native nodes shipped with z8run.
//! Each implements `NodeExecutor` and has a corresponding factory.

pub mod debug;
pub mod function;
pub mod http_in;
pub mod http_out;
pub mod delay;

use std::sync::Arc;
use crate::engine::{FlowEngine, NodeExecutorFactory};

/// Registers all built-in node types with the engine.
pub async fn register_builtin_nodes(engine: &FlowEngine) {
    engine
        .register_node_type(Arc::new(debug::DebugNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(function::FunctionNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(http_in::HttpInNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(http_out::HttpOutNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(delay::DelayNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
}
