//! WASM plugin registry.
//!
//! Manages loading, unloading, and discovery of WASM modules
//! available for the flow engine.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::manifest::PluginManifest;
use crate::RuntimeError;

/// Information about a registered plugin.
#[derive(Debug, Clone)]
pub struct RegisteredPlugin {
    /// Plugin manifest.
    pub manifest: PluginManifest,
    /// Path to the WASM file.
    pub wasm_path: PathBuf,
    /// Whether the module is preloaded in memory.
    pub preloaded: bool,
}

/// Registry of available WASM plugins.
pub struct PluginRegistry {
    /// Plugins registered by name.
    plugins: Arc<RwLock<HashMap<String, RegisteredPlugin>>>,
    /// Base directory where plugins are stored.
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    /// Creates a new registry.
    pub fn new(plugins_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugins_dir: plugins_dir.into(),
        }
    }

    /// Scans the plugins directory and registers found plugins.
    pub async fn scan(&self) -> Result<usize, RuntimeError> {
        let dir = &self.plugins_dir;
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| {
                RuntimeError::ModuleLoad(format!("Could not create plugins directory: {}", e))
            })?;
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(dir)
            .map_err(|e| RuntimeError::ModuleLoad(format!("Could not read directory: {}", e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Look for manifest.toml in the plugin directory
                let manifest_path = path.join("manifest.toml");
                if manifest_path.exists() {
                    match self.register_from_dir(&path).await {
                        Ok(name) => {
                            tracing::info!(plugin = %name, "Plugin registered");
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "Plugin ignored");
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    /// Registers a plugin from its directory.
    async fn register_from_dir(&self, dir: &Path) -> Result<String, RuntimeError> {
        let manifest_content = std::fs::read_to_string(dir.join("manifest.toml"))
            .map_err(|e| RuntimeError::Manifest(e.to_string()))?;

        let manifest = PluginManifest::from_toml(&manifest_content)
            .map_err(|e| RuntimeError::Manifest(e.to_string()))?;

        let wasm_path = dir.join(&manifest.wasm_file);
        if !wasm_path.exists() {
            return Err(RuntimeError::ModuleNotFound(
                wasm_path.display().to_string(),
            ));
        }

        let name = manifest.name.clone();
        self.plugins.write().await.insert(
            name.clone(),
            RegisteredPlugin {
                manifest,
                wasm_path,
                preloaded: false,
            },
        );

        Ok(name)
    }

    /// Gets a plugin by name.
    pub async fn get(&self, name: &str) -> Option<RegisteredPlugin> {
        self.plugins.read().await.get(name).cloned()
    }

    /// Lists all registered plugins.
    pub async fn list(&self) -> Vec<RegisteredPlugin> {
        self.plugins.read().await.values().cloned().collect()
    }

    /// Returns the count of registered plugins.
    pub async fn count(&self) -> usize {
        self.plugins.read().await.len()
    }
}
