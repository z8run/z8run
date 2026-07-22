# z8run-storage

Dual SQLite/PostgreSQL persistence layer for [z8run](https://github.com/z8run/z8run), with an AES-256-GCM encrypted credential vault.

## Overview

`z8run-storage` provides trait-based abstractions for persistence, making the database backend interchangeable:

- **Flow storage** - save, load, list, search, delete flows (with multi-user support)
- **User storage** - user accounts with email/username lookups
- **Execution history** - record flow execution start/completion times
- **Credential vault** - AES-256-GCM encrypted storage for API keys and secrets
- **Automatic migrations** - schema setup for both SQLite and PostgreSQL

## Architecture

```
FlowRepository trait ──► SqliteStorage
                     ──► PgStorage

CredentialVault trait ──► SqliteCredentialVault
                     ──► PgCredentialVault
                             │
                         VaultCrypto (AES-256-GCM)
```

## Key traits

| Trait | Methods |
|-------|---------|
| `FlowRepository` | `save_flow`, `get_flow`, `list_flows`, `delete_flow`, `search_flows` |
| `UserRepository` | `create_user`, `get_user_by_id`, `get_user_by_email` |
| `ExecutionRepository` | `record_start`, `record_completion`, `get_history` |
| `CredentialVault` | `store`, `retrieve`, `delete`, `list_keys`, `issue_temporary_token` |

## Credential vault

The vault encrypts all stored credentials with AES-256-GCM. WASM plugins never receive raw credentials - they get short-lived temporary tokens instead.

```rust
use z8run_storage::credential_vault::VaultCrypto;

let vault = VaultCrypto::from_secret("my-encryption-key");
let (ciphertext, nonce) = vault.encrypt(b"sk-my-api-key")?;
let plaintext = vault.decrypt(&ciphertext, &nonce)?;
```

## Usage

```toml
[dependencies]
z8run-storage = "0.1"
```

### SQLite (default, zero config)

```rust
let pool = sqlx::SqlitePool::connect("sqlite:data/z8run.db").await?;
let storage = z8run_storage::sqlite::SqliteStorage::new(pool.clone());
let vault = z8run_storage::credential_vault::SqliteCredentialVault::new(pool, "my-secret");
```

### PostgreSQL

```rust
let pool = sqlx::PgPool::connect("postgres://user:pass@localhost/z8run").await?;
let storage = z8run_storage::postgres::PgStorage::new(pool.clone());
let vault = z8run_storage::credential_vault::PgCredentialVault::new(pool, "my-secret");
```

## License

Apache-2.0 OR MIT
