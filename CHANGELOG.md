# Changelog

All notable changes to z8run are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/) and this project adheres to [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

### Added
- Plugin install/remove via CLI (`z8run plugin install`, `z8run plugin remove`)
- `Z8_JWT_SECRET` environment variable for secure JWT signing (required for PostgreSQL/MySQL)
- GHCR-based CI/CD pipeline (`deploy.yml`) — builds in GitHub Actions, pushes to `ghcr.io/z8run`
- Release workflow (`release.yml`) — Docker images, cross-compiled binaries, crates.io publish, GitHub Release on tag
- `docker-compose.build.yml` for local Docker builds
- Domain separation: `z8run.org` (landing page) + `app.z8run.org` (application)
- Landing page at `deploy/landing/index.html`
- Public `/api/v1/health` and `/api/v1/info` endpoints (no auth required)

### Fixed
- JWT secret was hardcoded as `"z8run-dev-secret"` — now reads from `Z8_JWT_SECRET` env var
- Plugin install and remove were stubs with TODO — now fully implemented
- Docker health check pointed to auth-protected endpoint — now uses `/api/v1/health`
- Dockerfile cargo cache not invalidated on source changes — added `find ... -exec touch`

### Changed
- Docker images moved from Docker Hub to GitHub Container Registry (GHCR)
- CI/CD no longer compiles on the server — images are pre-built in GitHub Actions
- Rust version bumped to 1.91 (required by wasmtime)
- README badges updated to reflect GHCR, crates.io, issues, and contributors

---

## [0.1.0] — 2025-03-06

Initial release of z8run.

### Core Engine
- Flow engine with DAG validation and topological scheduling
- 23 built-in nodes across 6 categories (Input, Process, Output, Logic, Data, AI)
- Binary WebSocket protocol (11-byte header) for real-time editor sync
- WASM plugin sandbox using wasmtime with capability controls

### Nodes
- **Input:** HTTP In, Timer, Webhook (HMAC-SHA256 validation)
- **Process:** Function, JSON Transform, HTTP Request, Filter
- **Output:** Debug, HTTP Response
- **Logic:** Switch (multi-rule routing), Delay
- **Data:** Database (PostgreSQL, MySQL, SQLite), MQTT (publish/subscribe with TLS)
- **AI:** LLM, Embeddings, Classifier, Prompt Template, Text Splitter, Vector Store, Structured Output, Summarizer, AI Agent, Image Gen

### API & Server
- REST API with Axum 0.8 (flows CRUD, start/stop execution, health, info)
- WebSocket engine at `/ws/engine`
- Namespaced webhook routes (`/hook/{flow_id}/{path}`)
- JWT authentication with argon2 password hashing
- AES-256-GCM encrypted credential vault

### Storage
- SQLite persistence (embedded, zero-config for development)
- PostgreSQL persistence (recommended for production)
- Flow import/export (JSON)

### Frontend
- Visual node editor with React Flow + Zustand + Tailwind CSS
- Drag-and-drop node palette with 6 categories
- Smart config UI (dropdowns, password fields, code editors)
- Flow management (list, create, delete, deploy, stop)
- Credential vault UI
- Real-time execution log with payload tracing

### Deployment
- Docker multi-stage build (Rust 1.91 + Node.js)
- Docker Compose with PostgreSQL
- Nginx reverse proxy with WebSocket support
- Cloudflare DNS integration (Flexible SSL)

---

[Unreleased]: https://github.com/z8run/z8run/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/z8run/z8run/releases/tag/v0.1.0
