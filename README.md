<p align="center">
  <img src="assets/z8run-logo.svg" alt="z8run" width="280" />
</p>

<h3 align="center">Next Generation Visual Flow Engine</h3>

<p align="center">
  Build, connect, and automate anything — visually.
</p>

<p align="center">
  <a href="https://github.com/z8run/z8run/actions/workflows/ci.yml"><img src="https://github.com/z8run/z8run/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/z8run/z8run/releases"><img src="https://img.shields.io/github/v/release/z8run/z8run?style=flat-square&color=06B6D4" alt="Release" /></a>
  <a href="https://github.com/z8run/z8run/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue?style=flat-square" alt="License" /></a>
  <a href="https://github.com/z8run/z8run/stargazers"><img src="https://img.shields.io/github/stars/z8run/z8run?style=flat-square" alt="Stars" /></a>
  <a href="https://hub.docker.com/r/z8run/z8run"><img src="https://img.shields.io/docker/pulls/z8run/z8run?style=flat-square&color=2496ED" alt="Docker Pulls" /></a>
</p>

---

## What is z8run?

z8run is an open-source visual flow engine built from the ground up in **Rust** for performance, safety, and extensibility. Think of it as the next evolution of tools like Node-RED — designed for developers who need real-time automation with a modern stack.

**Key principles:**

- **Fast** — Rust + Tokio async runtime, compiled to native code
- **Visual** — Drag-and-drop node editor with real-time WebSocket sync
- **Extensible** — WebAssembly plugin sandbox (write plugins in any language that compiles to WASM)
- **Lightweight** — Single binary, embedded SQLite, zero external dependencies to get started
- **Secure** — AES-256-GCM credential vault, JWT auth, sandboxed plugin execution

## Quick Start

### Requirements

- [Rust](https://rustup.rs/) 1.75+

### Build & Run

```bash
git clone https://github.com/z8run/z8run.git
cd z8run
cargo build --release
cargo run --bin z8run -- serve
```

The server starts on `http://localhost:7700`.

### Test the API

```bash
# Health check
curl http://localhost:7700/api/v1/health

# Create a flow
curl -X POST http://localhost:7700/api/v1/flows \
  -H "Content-Type: application/json" \
  -d '{"name": "My First Flow"}'

# List all flows
curl http://localhost:7700/api/v1/flows
```

## Architecture

z8run is organized as a Rust workspace with focused crates:

```
z8run/
├── crates/
│   ├── z8run-core       # Flow engine, DAG validation, scheduler, 23 built-in nodes
│   ├── z8run-protocol   # Binary WebSocket protocol (11-byte header)
│   ├── z8run-storage    # SQLite / PostgreSQL persistence layer
│   ├── z8run-runtime    # WASM plugin sandbox (wasmtime)
│   └── z8run-api        # REST + WebSocket server (Axum)
├── bins/
│   ├── z8run-cli        # Main CLI binary
│   └── z8run-server     # Server with embedded frontend
├── frontend/            # React + TypeScript visual editor
│   ├── src/features/    # Editor canvas, node palette, config panel
│   ├── src/stores/      # Zustand state management
│   └── src/lib/         # Node definitions, utilities
└── Cargo.toml           # Workspace root
```

### How it works

1. **Flows** are directed acyclic graphs (DAGs) of nodes connected by typed ports
2. **Nodes** process messages and pass them to connected outputs
3. **The scheduler** compiles flows into parallel execution plans using topological ordering
4. **Plugins** run inside a WebAssembly sandbox with controlled capabilities (network, filesystem, memory limits)
5. **The protocol** uses a compact binary format over WebSockets for real-time editor sync

## CLI

```bash
z8run serve              # Start the server (default port 7700)
z8run serve -p 8080      # Custom port
z8run migrate            # Run database migrations
z8run plugin list        # List installed plugins
z8run plugin scan        # Scan plugin directory
z8run validate flow.json # Validate a flow file
z8run info               # Show system information
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `Z8_PORT` | `7700` | HTTP/WebSocket port |
| `Z8_BIND` | `0.0.0.0` | Bind address |
| `Z8_DATA_DIR` | `./data` | Data directory (database, plugins) |
| `Z8_DB_URL` | SQLite auto | Database URL (sqlite:// or postgres://) |
| `Z8_LOG_LEVEL` | `info` | Log level (trace, debug, info, warn, error) |

## API

### REST Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/v1/health` | Health check |
| `GET` | `/api/v1/info` | Server information |
| `GET` | `/api/v1/flows` | List all flows |
| `POST` | `/api/v1/flows` | Create a new flow |
| `GET` | `/api/v1/flows/{id}` | Get flow by ID |
| `DELETE` | `/api/v1/flows/{id}` | Delete a flow |
| `POST` | `/api/v1/flows/{id}/start` | Start flow execution |
| `POST` | `/api/v1/flows/{id}/stop` | Stop flow execution |

### WebSocket

Connect to `ws://localhost:7700/ws/engine` for real-time communication using the z8run binary protocol.

## Built-in Nodes

z8run ships with 23 native nodes across 6 categories:

| Category | Nodes |
|---|---|
| **Input** | HTTP In, Timer, Webhook (HMAC-SHA256 signature validation) |
| **Process** | Function, JSON Transform (parse/stringify/extract), HTTP Request (outbound), Filter |
| **Output** | Debug, HTTP Response |
| **Logic** | Switch (multi-rule routing), Delay |
| **Data** | Database (PostgreSQL, MySQL, SQLite), MQTT (publish/subscribe) |
| **AI** | LLM, Embeddings, Classifier, Prompt Template, Text Splitter, Vector Store, Structured Output, Summarizer, AI Agent, Image Gen |

## Roadmap

- [x] Core engine with DAG validation and topological scheduling
- [x] Binary WebSocket protocol
- [x] REST API (Axum 0.8)
- [x] SQLite / PostgreSQL persistence
- [x] Visual node editor (React Flow + Zustand + Tailwind)
- [x] 23 built-in nodes (HTTP In/Out/Request, Debug, Function, Switch, Filter, Delay, Timer, Webhook, JSON Transform, Database, MQTT + 10 AI nodes)
- [x] Real-time WebSocket execution events
- [x] Namespaced hook routes (`/hook/{flow_id}/{path}`)
- [x] Smart config UI (dropdowns, password fields, code editors)
- [x] Multi-database support (PostgreSQL, MySQL, SQLite)
- [x] Flow management UI (list, create, delete from browser)
- [x] Deploy & test from UI (save, deploy, stop buttons)
- [x] Authentication & multi-user (JWT + argon2)
- [x] Credential vault (AES-256-GCM encrypted connections)
- [x] Flow import/export (JSON)
- [x] WASM plugin execution (wasmtime sandbox with capabilities)
- [x] MQTT node (publish/subscribe with TLS)
- [x] AI suite: LLM, Embeddings, Classifier, Prompt Template, Text Splitter, Vector Store, Structured Output, Summarizer, AI Agent, Image Gen
- [ ] Cloud deployment mode (Docker, Helm)
- [ ] Plugin marketplace

## Contributing

z8run is in early development. Contributions, ideas, and feedback are welcome!

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Security

If you discover a security vulnerability, please email **security@z8run.org** instead of opening a public issue. We take security seriously and will respond promptly.

## License

z8run is dual-licensed under [Apache 2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT). You may choose either license.

## Support

- Website: [z8run.org](https://z8run.org)
- Email: [hello@z8run.org](mailto:hello@z8run.org)
- GitHub Issues: [z8run/z8run/issues](https://github.com/z8run/z8run/issues)
- Sponsor: [GitHub Sponsors](https://github.com/sponsors/z8run)

---

<p align="center">
  Built with Rust and a lot of coffee.
</p>
