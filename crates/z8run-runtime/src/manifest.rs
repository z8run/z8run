//! WASM plugin manifest.
//!
//! Each plugin is distributed with a manifest that declares
//! metadata, ports, required host capabilities, etc.

use serde::{Deserialize, Serialize};

/// WASM plugin/node manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin name (e.g., "http-request").
    pub name: String,
    /// Semantic version of the plugin.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Plugin author.
    pub author: String,
    /// License.
    #[serde(default)]
    pub license: String,
    /// Category for the editor (e.g., "network", "transform", "io").
    pub category: String,
    /// Node icon in the editor (name or URL).
    #[serde(default)]
    pub icon: String,
    /// Input port definitions.
    pub inputs: Vec<ManifestPort>,
    /// Output port definitions.
    pub outputs: Vec<ManifestPort>,
    /// Required WASI capabilities.
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    /// WASM file relative to manifest.
    pub wasm_file: String,
    /// Minimum z8run runtime version required.
    #[serde(default)]
    pub min_runtime_version: String,
}

/// Port declared in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPort {
    pub name: String,
    #[serde(rename = "type")]
    pub port_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// WASI capabilities that the plugin requests from the host.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Network access (making HTTP requests, etc.).
    #[serde(default)]
    pub network: bool,
    /// Filesystem access (reading/writing files).
    #[serde(default)]
    pub filesystem: bool,
    /// Allowed directories if filesystem = true.
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Environment variable access.
    #[serde(default)]
    pub env_vars: bool,
    /// Specific environment variables allowed.
    #[serde(default)]
    pub allowed_env: Vec<String>,
    /// Memory limit in MB (0 = use system default).
    #[serde(default)]
    pub memory_limit_mb: u64,
}

impl PluginManifest {
    /// Loads a manifest from a TOML file.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serializes the manifest to TOML.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}
