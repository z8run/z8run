# Changelog

All notable changes to z8run are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/) and this project adheres to [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [0.2.0] — 2026-04-01

### Added
- Per-crate README.md files with documentation for crates.io
- Crate metadata: keywords, categories, homepage, repository for all 6 crates
- PR template, feature request template, and CODEOWNERS
- `.editorconfig` and `.node-version` for cross-editor consistency
- Docker targets in Makefile (`docker-build`, `docker-up`, `docker-down`, `setup`)
- Demo GIF in README with comparison table vs Node-RED and n8n
- Ko-fi sponsorship, GitHub Discussions welcome post
- `LICENSE` root file (MIT full text) for GitHub license detection

### Fixed
- All CodeQL security alerts: HTTPS-only clients, API keys in headers, prototype pollution guard
- Security patches: `aws-lc-sys` 0.39.1, `rustls-webpki` 0.103.10
- Biome lint/format errors across frontend
- `rust-version` MSRV corrected from 1.75 to 1.91
- All `pnpm` references changed to `npm`
- Node.js updated from 20 to 22 LTS
- Docker image `unknown/unknown` platform fixed with `provenance: false`
- jscpd threshold raised from 5% to 15%

### Changed
- Dependabot: groups patches/minor, ignores major versions, weekly on Monday
- Deploy workflow: concurrency, environment protection, SSH cleanup, scoped image pruning
- GitHub Actions updated to latest versions (setup-node v6, cache v5, etc.)
- CI and Code Quality run only on PRs, not push to main
- Branch protection: required checks, code owner reviews
- crates.io publish: version check, skip-if-published, 3 retries
- Repo topics optimized for discoverability (20 topics)

### Removed
- Redundant `LICENSE-MIT` (MIT text now in `LICENSE`)
- Manual SHA-256 implementation (replaced by `sha2` crate)
- 26 stale Dependabot PRs (merged safe ones, closed breaking ones)

---

## [0.1.0] — 2026-03-06

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

[Unreleased]: https://github.com/z8run/z8run/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/z8run/z8run/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/z8run/z8run/releases/tag/v0.1.0
