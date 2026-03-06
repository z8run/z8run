# Contributing to z8run

Thank you for your interest in contributing to z8run! This document explains how to get involved, whether you want to fix a bug, add a feature, improve documentation, or build a plugin.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Commit Convention](#commit-convention)
- [Pull Request Process](#pull-request-process)
- [Writing Plugins (WASM)](#writing-plugins-wasm)
- [Frontend Contributions](#frontend-contributions)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)

---

## Code of Conduct

This project follows our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold it. Please report unacceptable behavior to [hello@z8run.org](mailto:hello@z8run.org).

---

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/z8run.git
   cd z8run
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/z8run/z8run.git
   ```

---

## Project Structure

```
z8run/
├── crates/
│   ├── z8run-core       # Flow engine, DAG scheduler, built-in nodes
│   ├── z8run-protocol   # Binary WebSocket protocol
│   ├── z8run-storage    # SQLite / PostgreSQL persistence
│   ├── z8run-runtime    # WASM plugin sandbox (wasmtime)
│   └── z8run-api        # REST + WebSocket server (Axum)
├── bins/
│   ├── z8run-cli        # CLI binary
│   └── z8run-server     # Server with embedded frontend
├── frontend/            # React + TypeScript visual editor
└── Cargo.toml           # Workspace root
```

---

## Development Setup

### Requirements

- [Rust](https://rustup.rs/) 1.91+ (see `rust-toolchain.toml` for the pinned version)
- Node.js 20+ and `pnpm` (for frontend work)
- Optional: Docker and Docker Compose (for containerized development)
- Optional: `wasm-pack` or a WASM target if working on plugins

### Environment Setup

```bash
cp .env.example .env
```

Edit `.env` and set at minimum:
- `Z8_JWT_SECRET` — required for PostgreSQL (generate with `openssl rand -base64 32`)
- `POSTGRES_PASSWORD` — if using Docker with PostgreSQL

For local development with SQLite, the defaults work out of the box.

### Backend

```bash
# Build all crates
cargo build

# Run tests
cargo test --workspace

# Run the server in development mode
cargo run --bin z8run -- serve
```

### Frontend

```bash
cd frontend
pnpm install
pnpm dev
```

### With Docker (local build)

```bash
docker compose -f docker-compose.yml -f docker-compose.build.yml up -d
```

This uses `docker-compose.build.yml` as an override to build images locally instead of pulling from GHCR.

### Linting & Formatting

```bash
# Rust
cargo fmt --all
cargo clippy --workspace -- -D warnings

# Frontend
pnpm lint
pnpm format
```

All CI checks must pass before a PR can be merged.

---

## How to Contribute

### Pick an Issue

- Look for issues labeled **`good first issue`** if you are new to the project.
- Issues labeled **`help wanted`** are open for anyone to work on.
- Comment on the issue to let maintainers know you are working on it.

### Create a Branch

```bash
git checkout -b feat/my-new-feature   # new feature
git checkout -b fix/bug-description   # bug fix
git checkout -b docs/improve-readme   # documentation
```

---

## Commit Convention

z8run follows [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]

[optional footer]
```

| Type | When to use |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation only changes |
| `refactor` | Code change that is not a feature or fix |
| `test` | Adding or updating tests |
| `chore` | Build process, tooling, dependencies |
| `perf` | Performance improvement |

Examples:

```
feat(core): add retry policy to HTTP Request node
fix(storage): handle concurrent writes on SQLite
docs: update Quick Start section in README
```

---

## Pull Request Process

1. Ensure your branch is up to date with `main`:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```
2. Make sure all tests pass and there are no clippy warnings.
3. Open a Pull Request against the `main` branch.
4. Fill in the PR template -- describe **what** changed and **why**.
5. Link any related issue (e.g., `Closes #42`).
6. A maintainer will review your PR. Please respond to review comments within a reasonable time.
7. Once approved and CI is green, a maintainer will merge it.

---

## Writing Plugins (WASM)

z8run supports plugins compiled to WebAssembly. To create one:

1. Implement the z8run plugin interface (see `crates/z8run-runtime` for the ABI spec).
2. Compile your plugin to `wasm32-wasi`:
   ```bash
   cargo build --target wasm32-wasi --release
   ```
3. Install the plugin using the CLI:
   ```bash
   z8run plugin install ./target/wasm32-wasi/release/my_plugin.wasm
   ```
   Or place the `.wasm` file in the `data/plugins/` directory and use `z8run plugin scan` to register it.
4. Add tests and documentation for your plugin nodes and capabilities.

---

## Frontend Contributions

The frontend lives in `frontend/` and is built with React, TypeScript, React Flow, Zustand, and Tailwind CSS.

- Node definitions are in `frontend/src/lib/`
- The editor canvas and node palette are in `frontend/src/features/`
- State management uses Zustand stores in `frontend/src/stores/`

When adding a new built-in node, update both the backend crate (`z8run-core`) and the frontend node definition.

---

## Reporting Bugs

Please [open an issue](https://github.com/z8run/z8run/issues/new) and include:

- z8run version (`z8run info`)
- Operating system and architecture
- Steps to reproduce
- Expected vs. actual behavior
- Relevant logs or screenshots

For **security vulnerabilities**, do **not** open a public issue -- see [SECURITY.md](SECURITY.md) instead.

---

## Suggesting Features

Open a [GitHub Discussion](https://github.com/z8run/z8run/discussions) or an issue with the `enhancement` label. Describe the use case and the problem it solves.

---

## Questions?

- GitHub Discussions: [z8run/z8run/discussions](https://github.com/z8run/z8run/discussions)
- Email: [hello@z8run.org](mailto:hello@z8run.org)

We appreciate every contribution, big or small. Thank you!
