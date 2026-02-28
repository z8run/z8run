//! # z8run-runtime
//!
//! WASM runtime for executing nodes/plugins in a secure sandbox.
//! Uses wasmtime as the WebAssembly execution engine.

pub mod manifest;
pub mod sandbox;
pub mod registry;

use thiserror::Error;

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
