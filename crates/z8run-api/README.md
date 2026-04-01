# z8run-api

HTTP/WebSocket server for [z8run](https://github.com/z8run/z8run), built on [Axum](https://github.com/tokio-rs/axum).

## Overview

`z8run-api` exposes the z8run engine as a web service with:

- **REST API** — CRUD for flows, credential vault, import/export
- **WebSocket** — real-time streaming of execution events
- **Webhook endpoints** — trigger flows via HTTP with HMAC signature validation
- **JWT authentication** — user registration, login, and protected routes
- **Rate limiting** — configurable per-IP token bucket (API, auth, hooks)

## REST API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/health` | Health check |
| `GET` | `/api/v1/info` | Server information |
| `GET` | `/api/v1/flows` | List all flows |
| `POST` | `/api/v1/flows` | Create a new flow |
| `GET` | `/api/v1/flows/{id}` | Get flow by ID |
| `PUT` | `/api/v1/flows/{id}` | Update flow |
| `DELETE` | `/api/v1/flows/{id}` | Delete flow |
| `POST` | `/api/v1/flows/{id}/start` | Start flow execution |
| `POST` | `/api/v1/flows/{id}/stop` | Stop flow execution |
| `GET` | `/api/v1/flows/{id}/export` | Export flow as JSON |
| `POST` | `/api/v1/flows/import` | Import flow from JSON |
| `GET` | `/api/v1/vault` | List credential keys |
| `POST` | `/api/v1/vault` | Store a credential |
| `DELETE` | `/api/v1/vault/{key}` | Delete a credential |
| `POST` | `/api/v1/auth/register` | Register user |
| `POST` | `/api/v1/auth/login` | Login |

## WebSocket

Connect to `ws://localhost:7700/ws/engine` to receive real-time execution events:

```json
{"event": "NodeCompleted", "node_id": "llm-1", "duration_ms": 1200}
```

## Webhooks

Trigger flows via HTTP:

```bash
curl -X POST http://localhost:7700/hook/{flow_id}/my-path \
  -H "Content-Type: application/json" \
  -d '{"message": "hello"}'
```

## Rate limiting

| Endpoint | Default limit |
|----------|--------------|
| API routes | 100 req/min |
| Auth routes | 20 req/min |
| Hook routes | 200 req/min |

Configurable via `Z8_RATE_LIMIT_API`, `Z8_RATE_LIMIT_AUTH`, `Z8_RATE_LIMIT_HOOK` env vars.

## Usage

```toml
[dependencies]
z8run-api = "0.1"
```

```rust
use z8run_api::build_router;

let app = build_router(app_state).await;
let listener = tokio::net::TcpListener::bind("0.0.0.0:7700").await?;
axum::serve(listener, app).await?;
```

## License

Apache-2.0 OR MIT
