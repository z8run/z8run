# z8run-cli

Main CLI binary for [z8run](https://github.com/z8run/z8run). Wires together all crates to provide a single executable for running the server, managing plugins, and validating flows.

## Installation

### From crates.io

```bash
cargo install z8run-cli
```

### From source

```bash
git clone https://github.com/z8run/z8run.git
cd z8run
cargo build --release --bin z8run
```

### With Docker

```bash
docker pull ghcr.io/z8run/z8run-api:latest
docker run -p 7700:7700 ghcr.io/z8run/z8run-api:latest
```

## Commands

```bash
z8run serve                            # Start the server (default port 7700)
z8run serve -p 8080                    # Custom port
z8run migrate                          # Run database migrations
z8run plugin list                      # List installed plugins
z8run plugin install ./my-plugin.wasm  # Install a WASM plugin
z8run plugin install ./plugin-dir/     # Install from directory with manifest.toml
z8run plugin remove my-plugin          # Remove a plugin
z8run plugin scan                      # Scan plugin directory
z8run validate flow.json               # Validate a flow file (DAG check)
z8run info                             # Show version and system info
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `Z8_PORT` | `7700` | HTTP/WebSocket port |
| `Z8_BIND` | `0.0.0.0` | Bind address |
| `Z8_DATA_DIR` | `./data` | Data directory (database, plugins) |
| `Z8_DB_URL` | SQLite auto | Database URL (`sqlite://` or `postgres://`) |
| `Z8_LOG_LEVEL` | `info` | Log level (trace, debug, info, warn, error) |
| `Z8_JWT_SECRET` | - | JWT signing secret (**required** for PostgreSQL) |
| `Z8_VAULT_SECRET` | - | Encryption key for the credential vault |

## Quick start

```bash
cp .env.example .env
z8run serve
```

Open `http://localhost:7700` in your browser to access the visual flow editor.

## License

Apache-2.0 OR MIT
