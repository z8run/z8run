# z8run-runtime

WASM plugin runtime for [z8run](https://github.com/z8run/z8run), built on [wasmtime](https://wasmtime.dev/).

## Overview

`z8run-runtime` allows extending z8run with custom nodes compiled to WebAssembly. It provides:

- **Sandboxed execution** - memory limits, fuel metering, and controlled capabilities
- **Plugin registry** - discover, install, remove, and scan WASM plugins
- **Manifest system** - TOML-based plugin metadata (ports, capabilities, author)
- **NodeExecutor bridge** - WASM modules implement the same interface as built-in nodes

## Plugin structure

```
my-plugin/
├── manifest.toml    # Plugin metadata
└── my_plugin.wasm   # Compiled WASM module
```

### manifest.toml

```toml
[plugin]
name = "csv-parser"
version = "1.0.0"
description = "Parse CSV data into JSON"
author = "Your Name"
category = "data"
wasm_file = "csv_parser.wasm"

[[inputs]]
name = "in"
type = "String"

[[outputs]]
name = "out"
type = "Object"

[capabilities]
network = false
filesystem = false
memory_limit_mb = 64
```

## WASM ABI

> The ABI is defined by the sandbox in [`crates/z8run-runtime/src/`](src/) (see `sandbox.rs`) — that source is the source of truth.

Plugins must export these functions:

| Export | Signature | Description |
|--------|-----------|-------------|
| `z8_alloc` | `(size: i32) -> i32` | Allocate `size` bytes and return a pointer |
| `z8_dealloc` | `(ptr: i32, size: i32) -> ()` | Free memory (optional; called if exported) |
| `z8_process` | `(ptr: i32, len: i32) -> i32` | Process a message; returns a pointer to a result buffer |
| `z8_node_type` | `() -> i32` | Return a pointer to the node type string |
| `z8_configure` | `(ptr: i32, len: i32) -> i32` | Apply configuration (return 0 = ok, non-zero = error) |
| `z8_validate` | `() -> i32` | Validate the current configuration (return 0 = ok, non-zero = error) |

Buffers returned by `z8_process` and `z8_node_type` must be laid out as a
4-byte little-endian length prefix followed by that many bytes of UTF-8 data.
Input passed to `z8_process`/`z8_configure` is the raw UTF-8 payload written at
`ptr` with length `len` (no prefix).

## Usage

```toml
[dependencies]
z8run-runtime = "0.1"
```

```rust
use z8run_runtime::{PluginRegistry, WasmSandbox};

// Scan plugins directory
let mut registry = PluginRegistry::new("./data/plugins");
registry.scan().await?;

// List installed plugins
for plugin in registry.list() {
    println!("{}: {}", plugin.manifest.name, plugin.manifest.version);
}

// Install a plugin
registry.install_local("./my-plugin.wasm").await?;
```

## Sandbox configuration

```rust
use z8run_runtime::SandboxConfig;

let config = SandboxConfig {
    memory_limit_bytes: 256 * 1024 * 1024, // 256 MB
    fuel_limit: Some(1_000_000),
    capabilities: PluginCapabilities { network: false, filesystem: false, .. },
    debug: false,
};
```

## License

Apache-2.0 OR MIT
