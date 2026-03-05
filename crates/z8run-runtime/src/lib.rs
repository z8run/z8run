//! # z8run-runtime
//!
//! WASM runtime for executing nodes/plugins in a secure sandbox.
//! Uses wasmtime as the WebAssembly execution engine.

pub mod executor;
pub mod manifest;
pub mod registry;
pub mod sandbox;

use std::sync::Arc;
use thiserror::Error;
use tracing::info;
use z8run_core::engine::FlowEngine;

pub use executor::{WasmNodeExecutor, WasmNodeFactory};
pub use manifest::{PluginCapabilities, PluginManifest, ManifestPort};
pub use registry::{PluginRegistry, RegisteredPlugin};
pub use sandbox::{SandboxConfig, WasmSandbox};

/// WASM runtime errors.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("WASM module not found: {0}")]
    ModuleNotFound(String),

    #[error("Error loading WASM module: {0}")]
    ModuleLoad(String),

    #[error("Error instantiating WASM module: {0}")]
    Instantiation(String),

    #[error("Exported function not found: {0}")]
    FunctionNotFound(String),

    #[error("WASM execution error: {0}")]
    Execution(String),

    #[error("Module exceeded memory limit: {used_mb}MB (maximum: {limit_mb}MB)")]
    MemoryLimitExceeded { used_mb: u64, limit_mb: u64 },

    #[error("Manifest error: {0}")]
    Manifest(String),
}

/// Loads all plugins from a registry and registers them with the flow engine.
///
/// Returns the count of successfully registered plugins.
pub async fn register_plugins(
    engine: &FlowEngine,
    registry: &PluginRegistry,
) -> Result<usize, RuntimeError> {
    let plugins = registry.list().await;
    let mut count = 0;

    for plugin in &plugins {
        match load_and_register_plugin(engine, plugin).await {
            Ok(_) => {
                info!(
                    plugin = %plugin.manifest.name,
                    version = %plugin.manifest.version,
                    "Plugin registered with engine"
                );
                count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin.manifest.name,
                    error = %e,
                    "Failed to register plugin"
                );
            }
        }
    }

    Ok(count)
}

/// Helper function to load a plugin from disk and register it with the engine.
async fn load_and_register_plugin(
    engine: &FlowEngine,
    plugin: &RegisteredPlugin,
) -> Result<(), RuntimeError> {
    // Load WASM bytes from disk
    let wasm_bytes = tokio::fs::read(&plugin.wasm_path)
        .await
        .map_err(|e| RuntimeError::ModuleLoad(e.to_string()))?;

    // Create factory from manifest and bytes
    let factory = WasmNodeFactory::from_manifest_and_bytes(plugin.manifest.clone(), wasm_bytes)?;

    // Register with engine
    engine.register_node_type(Arc::new(factory)).await;

    Ok(())
}
