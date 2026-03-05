//! WASM sandbox for secure plugin execution.
//!
//! Configures wasmtime with appropriate resource limits
//! and WASI capabilities declared in the manifest.

use crate::manifest::PluginCapabilities;
use crate::RuntimeError;
use tracing::debug;
use wasmtime::{Engine, Linker, Memory, Module, Store};

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

/// WASI context holder (minimal for now, can be extended).
pub struct WasiState;

/// WASM sandbox that executes an individual module.
pub struct WasmSandbox {
    engine: Engine,
    config: SandboxConfig,
}

impl WasmSandbox {
    /// Creates a new sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Result<Self, RuntimeError> {
        let mut wasmtime_config = wasmtime::Config::new();

        // Configure wasmtime
        wasmtime_config.wasm_simd(true);
        wasmtime_config.wasm_bulk_memory(true);
        wasmtime_config.wasm_reference_types(true);
        wasmtime_config.wasm_multi_value(true);

        // Set fuel limit if specified
        if config.fuel_limit > 0 {
            wasmtime_config.consume_fuel(true);
        }

        let engine = Engine::new(&wasmtime_config)
            .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to create wasmtime engine: {}", e)))?;

        debug!(memory_limit_mb = config.memory_limit / 1024 / 1024, fuel_limit = config.fuel_limit, "WASM sandbox created");

        Ok(Self { engine, config })
    }

    /// Creates a sandbox with default configuration.
    pub fn default_sandbox() -> Result<Self, RuntimeError> {
        Self::new(SandboxConfig::default())
    }

    /// Returns the sandbox configuration.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Returns a reference to the wasmtime engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Load and instantiate a WASM module.
    pub fn instantiate(&self, wasm_bytes: &[u8]) -> Result<WasmInstance, RuntimeError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to load WASM module: {}", e)))?;

        let mut store = Store::new(&self.engine, WasiState);

        // Set fuel if configured
        if self.config.fuel_limit > 0 {
            store
                .set_fuel(self.config.fuel_limit)
                .map_err(|e| RuntimeError::Instantiation(format!("Failed to set fuel: {}", e)))?;
        }

        // Create linker
        let linker = Linker::new(&self.engine);

        // Instantiate the module
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| RuntimeError::Instantiation(format!("Failed to instantiate module: {}", e)))?;

        // Get the module's exported memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| RuntimeError::Instantiation("Module does not export 'memory'".to_string()))?;

        debug!("WASM module instantiated successfully");

        Ok(WasmInstance {
            store,
            instance,
            memory,
        })
    }
}

/// A live WASM instance ready for execution.
pub struct WasmInstance {
    store: Store<WasiState>,
    instance: wasmtime::Instance,
    memory: Memory,
}

impl WasmInstance {
    /// Write bytes to WASM linear memory at a given pointer, returns the pointer.
    pub fn write_to_memory(&mut self, data: &[u8]) -> Result<i32, RuntimeError> {
        let size = data.len();

        // Call z8_alloc to get a pointer
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "z8_alloc")
            .map_err(|e| RuntimeError::FunctionNotFound(format!("z8_alloc: {}", e)))?;

        let ptr = alloc_fn
            .call(&mut self.store, size as i32)
            .map_err(|e| RuntimeError::Execution(format!("z8_alloc failed: {}", e)))?;

        if ptr < 0 {
            return Err(RuntimeError::Execution("z8_alloc returned negative pointer".to_string()));
        }

        // Write data to the allocated memory
        self.memory
            .write(&mut self.store, ptr as usize, data)
            .map_err(|e| RuntimeError::Execution(format!("Failed to write to memory: {}", e)))?;

        Ok(ptr)
    }

    /// Read bytes from WASM linear memory at a pointer.
    /// Expected format: 4-byte length prefix followed by data.
    pub fn read_from_memory(&mut self, ptr: i32) -> Result<Vec<u8>, RuntimeError> {
        if ptr < 0 {
            return Err(RuntimeError::Execution(format!("Invalid pointer: {}", ptr)));
        }

        let ptr_usize = ptr as usize;

        // Read the 4-byte length prefix
        let mut len_bytes = [0u8; 4];
        self.memory
            .read(&self.store, ptr_usize, &mut len_bytes)
            .map_err(|e| RuntimeError::Execution(format!("Failed to read length from memory: {}", e)))?;

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Sanity check
        if len > 100 * 1024 * 1024 {
            return Err(RuntimeError::MemoryLimitExceeded {
                used_mb: len as u64 / 1024 / 1024,
                limit_mb: 100,
            });
        }

        // Read the actual data
        let mut data = vec![0u8; len];
        self.memory
            .read(&self.store, ptr_usize + 4, &mut data)
            .map_err(|e| RuntimeError::Execution(format!("Failed to read data from memory: {}", e)))?;

        // Free the memory
        let dealloc_fn = self
            .instance
            .get_typed_func::<(i32, i32), ()>(&mut self.store, "z8_dealloc")
            .map_err(|_| {
                // z8_dealloc is optional
                RuntimeError::Execution("z8_dealloc not exported".to_string())
            });

        if let Ok(fn_ref) = dealloc_fn {
            let _ = fn_ref.call(&mut self.store, (ptr, len as i32 + 4));
        }

        Ok(data)
    }

    /// Call z8_process with a JSON payload.
    pub fn call_process(&mut self, payload_json: &str) -> Result<String, RuntimeError> {
        debug!(payload_len = payload_json.len(), "Calling z8_process");

        // Write payload to memory
        let ptr = self.write_to_memory(payload_json.as_bytes())?;

        // Call z8_process
        let process_fn = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "z8_process")
            .map_err(|e| RuntimeError::FunctionNotFound(format!("z8_process: {}", e)))?;

        let result_ptr = process_fn
            .call(&mut self.store, (ptr, payload_json.len() as i32))
            .map_err(|e| RuntimeError::Execution(format!("z8_process failed: {}", e)))?;

        // Read result
        let result_json_bytes = self.read_from_memory(result_ptr)?;
        let result_json =
            String::from_utf8(result_json_bytes).map_err(|e| RuntimeError::Execution(e.to_string()))?;

        Ok(result_json)
    }

    /// Call z8_configure with configuration JSON.
    pub fn call_configure(&mut self, config_json: &str) -> Result<(), RuntimeError> {
        debug!(config_len = config_json.len(), "Calling z8_configure");

        let ptr = self.write_to_memory(config_json.as_bytes())?;

        let configure_fn = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "z8_configure")
            .map_err(|e| RuntimeError::FunctionNotFound(format!("z8_configure: {}", e)))?;

        let result = configure_fn
            .call(&mut self.store, (ptr, config_json.len() as i32))
            .map_err(|e| RuntimeError::Execution(format!("z8_configure failed: {}", e)))?;

        if result != 0 {
            return Err(RuntimeError::Execution(format!(
                "z8_configure returned non-zero status: {}",
                result
            )));
        }

        Ok(())
    }

    /// Call z8_validate to validate the current configuration.
    pub fn call_validate(&mut self) -> Result<(), RuntimeError> {
        debug!("Calling z8_validate");

        let validate_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "z8_validate")
            .map_err(|e| RuntimeError::FunctionNotFound(format!("z8_validate: {}", e)))?;

        let result = validate_fn
            .call(&mut self.store, ())
            .map_err(|e| RuntimeError::Execution(format!("z8_validate failed: {}", e)))?;

        if result != 0 {
            return Err(RuntimeError::Execution(format!(
                "z8_validate returned non-zero status: {}",
                result
            )));
        }

        Ok(())
    }

    /// Get the node type string exported by the module.
    pub fn call_node_type(&mut self) -> Result<String, RuntimeError> {
        debug!("Calling z8_node_type");

        let node_type_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "z8_node_type")
            .map_err(|e| RuntimeError::FunctionNotFound(format!("z8_node_type: {}", e)))?;

        let ptr = node_type_fn
            .call(&mut self.store, ())
            .map_err(|e| RuntimeError::Execution(format!("z8_node_type failed: {}", e)))?;

        let node_type_bytes = self.read_from_memory(ptr)?;
        let node_type =
            String::from_utf8(node_type_bytes).map_err(|e| RuntimeError::Execution(e.to_string()))?;

        Ok(node_type)
    }
}
