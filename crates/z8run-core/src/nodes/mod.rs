//! Built-in node executor implementations.
//!
//! These are the native nodes shipped with z8run.
//! Each implements `NodeExecutor` and has a corresponding factory.

pub mod database;
pub mod debug;
pub mod delay;
pub mod filter;
pub mod function;
pub mod http_in;
pub mod http_out;
pub mod http_request;
pub mod json_transform;
pub mod switch;
pub mod timer;
pub mod webhook;

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
    engine
        .register_node_type(Arc::new(switch::SwitchNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(filter::FilterNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(http_request::HttpRequestNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(json_transform::JsonTransformNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(timer::TimerNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(webhook::WebhookNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(database::DatabaseNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
}
