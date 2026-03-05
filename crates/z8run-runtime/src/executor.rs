//! WASM node execution layer.
//!
//! Wraps a WASM instance as a NodeExecutor that integrates
//! with the z8run flow engine.

use crate::manifest::PluginManifest;
use crate::sandbox::{WasmInstance, WasmSandbox, SandboxConfig};
use crate::RuntimeError;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;
use z8run_core::engine::{NodeExecutor, NodeExecutorFactory};
use z8run_core::error::{Z8Error, Z8Result};
use z8run_core::message::FlowMessage;

/// Wraps a WASM instance as a NodeExecutor for the flow engine.
pub struct WasmNodeExecutor {
    /// The WASM instance is behind a Mutex because wasmtime's Store is not Sync.
    instance: Arc<Mutex<WasmInstance>>,
    node_type_name: String,
}

#[async_trait::async_trait]
impl NodeExecutor for WasmNodeExecutor {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let mut instance = self.instance.lock().await;

        // Serialize the message payload to JSON
        let payload_json = serde_json::to_string(&msg.payload)
            .map_err(|e| Z8Error::Serialization(e))?;

        debug!(
            payload_len = payload_json.len(),
            node_type = %self.node_type_name,
            "Processing message in WASM node"
        );

        // Call z8_process
        let result_json = instance
            .call_process(&payload_json)
            .map_err(|e| Z8Error::Internal(format!("WASM process failed: {}", e)))?;

        // Parse the response as JSON
        let response: serde_json::Value = serde_json::from_str(&result_json)
            .map_err(|e| Z8Error::Internal(format!("Failed to parse WASM response: {}", e)))?;

        // Expect an array of output messages: [{port: string, payload: Value}, ...]
        let outputs = response
            .as_array()
            .ok_or_else(|| Z8Error::Internal("WASM process must return a JSON array".to_string()))?;

        let mut messages = Vec::new();
        for output in outputs {
            let port = output
                .get("port")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Z8Error::Internal("Missing or invalid 'port' field in output".to_string()))?
                .to_string();

            let payload = output
                .get("payload")
                .cloned()
                .ok_or_else(|| Z8Error::Internal("Missing 'payload' field in output".to_string()))?;

            messages.push(msg.derive(msg.source_node, port, payload));
        }

        debug!(output_count = messages.len(), "WASM process completed");
        Ok(messages)
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        let mut instance = self.instance.lock().await;

        let config_json = serde_json::to_string(&config)
            .map_err(|e| Z8Error::Serialization(e))?;

        debug!(config_len = config_json.len(), "Configuring WASM node");

        instance
            .call_configure(&config_json)
            .map_err(|e| Z8Error::Internal(format!("WASM configure failed: {}", e)))?;

        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        let mut instance = self.instance.lock().await;

        debug!("Validating WASM node configuration");

        instance
            .call_validate()
            .map_err(|e| Z8Error::Internal(format!("WASM validate failed: {}", e)))?;

        Ok(())
    }

    fn node_type(&self) -> &str {
        &self.node_type_name
    }
}

/// Factory that creates WasmNodeExecutor instances from a WASM module.
pub struct WasmNodeFactory {
    wasm_bytes: Vec<u8>,
    sandbox_config: SandboxConfig,
    manifest: PluginManifest,
}

impl WasmNodeFactory {
    /// Creates a new WASM node factory.
    pub fn new(
        wasm_bytes: Vec<u8>,
        sandbox_config: SandboxConfig,
        manifest: PluginManifest,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            wasm_bytes,
            sandbox_config,
            manifest,
        })
    }

    /// Creates a factory from a manifest and WASM bytes, with default sandbox config.
    pub fn from_manifest_and_bytes(
        manifest: PluginManifest,
        wasm_bytes: Vec<u8>,
    ) -> Result<Self, RuntimeError> {
        let mut config = SandboxConfig::default();

        // Apply manifest capabilities to sandbox config
        if manifest.capabilities.memory_limit_mb > 0 {
            config.memory_limit = manifest.capabilities.memory_limit_mb * 1024 * 1024;
        }

        Self::new(wasm_bytes, config, manifest)
    }
}

#[async_trait::async_trait]
impl NodeExecutorFactory for WasmNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        debug!(
            node_type = %self.manifest.name,
            "Creating WASM node executor instance"
        );

        // Create sandbox
        let sandbox = WasmSandbox::new(self.sandbox_config.clone())
            .map_err(|e| Z8Error::Internal(format!("Failed to create sandbox: {}", e)))?;

        // Instantiate the module
        let instance = sandbox
            .instantiate(&self.wasm_bytes)
            .map_err(|e| Z8Error::Internal(format!("Failed to instantiate module: {}", e)))?;

        let mut executor = WasmNodeExecutor {
            instance: Arc::new(Mutex::new(instance)),
            node_type_name: self.manifest.name.clone(),
        };

        // Configure the node if config is not empty
        if !config.is_null() && config != serde_json::Value::Object(Default::default()) {
            executor.configure(config).await?;
            executor.validate().await?;
        }

        Ok(Box::new(executor))
    }

    fn node_type(&self) -> &str {
        &self.manifest.name
    }
}
