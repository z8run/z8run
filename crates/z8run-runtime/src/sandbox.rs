//! WASM sandbox for secure plugin execution.
//!
//! Configures wasmtime with appropriate resource limits
//! and WASI capabilities declared in the manifest.

use crate::manifest::PluginCapabilities;
use crate::RuntimeError;

/// Sandbox configuration for a WASM module.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Memory limit in bytes.
    pub memory_limit: u64,
    /// Fuel limit (WASM instructions, 0 = unlimited).
    pub fuel_limit: u64,
    /// Capabilities granted to the module.
    pub capabilities: PluginCapabilities,
    /// Enable debug mode (more logs, no strict limits).
    pub debug_mode: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 256 * 1024 * 1024, // 256 MB
            fuel_limit: 0,                     // no limit by default
            capabilities: PluginCapabilities::default(),
            debug_mode: false,
        }
    }
}

/// WASM sandbox that executes an individual module.
pub struct WasmSandbox {
    config: SandboxConfig,
    // wasmtime::Engine and Store are created at runtime
    // when the full wasmtime integration is implemented.
}

impl WasmSandbox {
    /// Creates a new sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Result<Self, RuntimeError> {
        Ok(Self { config })
    }

    /// Creates a sandbox with default configuration.
    pub fn default_sandbox() -> Result<Self, RuntimeError> {
        Self::new(SandboxConfig::default())
    }

    /// Returns the sandbox configuration.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
}
