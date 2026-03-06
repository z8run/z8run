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

    /// Returns the plugins directory path.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Installs a plugin from a local .wasm file or directory.
    ///
    /// If `source` is a directory, it must contain manifest.toml + .wasm file.
    /// If `source` is a .wasm file, a minimal manifest is auto-generated.
    pub async fn install_local(&self, source: &Path) -> Result<String, RuntimeError> {
        if !source.exists() {
            return Err(RuntimeError::ModuleNotFound(source.display().to_string()));
        }

        if source.is_dir() {
            // Source is a plugin directory — copy it into plugins_dir
            let dir_name = source
                .file_name()
                .ok_or_else(|| RuntimeError::Manifest("Invalid directory name".into()))?;
            let dest = self.plugins_dir.join(dir_name);

            if dest.exists() {
                return Err(RuntimeError::Manifest(format!(
                    "Plugin directory '{}' already exists. Remove it first.",
                    dest.display()
                )));
            }

            copy_dir_recursive(source, &dest)?;
            self.register_from_dir(&dest).await
        } else if source.extension().map(|e| e == "wasm").unwrap_or(false) {
            // Source is a single .wasm file — create a plugin directory with auto-manifest
            let stem = source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let plugin_dir = self.plugins_dir.join(stem);
            std::fs::create_dir_all(&plugin_dir)
                .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to create plugin dir: {}", e)))?;

            let wasm_filename = source
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("plugin.wasm");

            // Copy the .wasm file
            std::fs::copy(source, plugin_dir.join(wasm_filename))
                .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to copy wasm: {}", e)))?;

            // Generate a manifest
            let manifest = format!(
                r#"name = "{}"
version = "0.1.0"
description = "Installed from {}"
wasm_file = "{}"
"#,
                stem,
                source.display(),
                wasm_filename
            );
            std::fs::write(plugin_dir.join("manifest.toml"), manifest)
                .map_err(|e| RuntimeError::Manifest(format!("Failed to write manifest: {}", e)))?;

            self.register_from_dir(&plugin_dir).await
        } else {
            Err(RuntimeError::ModuleLoad(
                "Source must be a .wasm file or a directory with manifest.toml".into(),
            ))
        }
    }

    /// Removes an installed plugin by name.
    pub async fn remove(&self, name: &str) -> Result<(), RuntimeError> {
        // Check if plugin exists in registry
        let exists = self.plugins.read().await.contains_key(name);
        if !exists {
            return Err(RuntimeError::ModuleNotFound(format!(
                "Plugin '{}' is not installed",
                name
            )));
        }

        // Remove plugin directory
        let plugin_dir = self.plugins_dir.join(name);
        if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir).map_err(|e| {
                RuntimeError::ModuleLoad(format!("Failed to remove plugin directory: {}", e))
            })?;
        }

        // Unregister from memory
        self.plugins.write().await.remove(name);

        Ok(())
    }
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(dst)
        .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to create dir: {}", e)))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| RuntimeError::ModuleLoad(format!("Failed to read dir: {}", e)))?
        .flatten()
    {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                RuntimeError::ModuleLoad(format!("Failed to copy file: {}", e))
            })?;
        }
    }

    Ok(())
}
