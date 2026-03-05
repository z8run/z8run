//! Automatic schema migration system.
//! Supports both SQLite and PostgreSQL.

use crate::StorageError;

/// Migration SQL for PostgreSQL.
pub const PG_MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS flows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '0.1.0',
    data JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'idle',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_flows_name ON flows(name);
CREATE INDEX IF NOT EXISTS idx_flows_status ON flows(status);

CREATE TABLE IF NOT EXISTS executions (
    id TEXT PRIMARY KEY,
    flow_id TEXT NOT NULL REFERENCES flows(id) ON DELETE CASCADE,
    trace_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_ms BIGINT,
    error TEXT,
    node_logs JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_executions_flow_id ON executions(flow_id);
CREATE INDEX IF NOT EXISTS idx_executions_trace_id ON executions(trace_id);

CREATE TABLE IF NOT EXISTS credentials (
    key TEXT PRIMARY KEY,
    encrypted_value BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL
);
"#;

/// Migration SQL for PostgreSQL V2 (users and connections).
pub const PG_MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    roles TEXT NOT NULL DEFAULT 'user',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    conn_type TEXT NOT NULL,
    encrypted_data BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connections_user_id ON connections(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_connections_user_name ON connections(user_id, name);

ALTER TABLE flows ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES users(id);
CREATE INDEX IF NOT EXISTS idx_flows_user_id ON flows(user_id);
"#;

/// Migration SQL for SQLite.
pub const SQLITE_MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS flows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '0.1.0',
    data JSON NOT NULL,
    status TEXT NOT NULL DEFAULT 'idle',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_flows_name ON flows(name);
CREATE INDEX IF NOT EXISTS idx_flows_status ON flows(status);

CREATE TABLE IF NOT EXISTS executions (
    id TEXT PRIMARY KEY,
    flow_id TEXT NOT NULL REFERENCES flows(id) ON DELETE CASCADE,
    trace_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_ms INTEGER,
    error TEXT,
    node_logs JSON NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_executions_flow_id ON executions(flow_id);
CREATE INDEX IF NOT EXISTS idx_executions_trace_id ON executions(trace_id);

CREATE TABLE IF NOT EXISTS credentials (
    key TEXT PRIMARY KEY,
    encrypted_value BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);
"#;

/// Migration SQL for SQLite V2 (users and connections).
pub const SQLITE_MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    roles TEXT NOT NULL DEFAULT 'user',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    conn_type TEXT NOT NULL,
    encrypted_data BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connections_user_id ON connections(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_connections_user_name ON connections(user_id, name);

ALTER TABLE flows ADD COLUMN user_id TEXT REFERENCES users(id);
CREATE INDEX IF NOT EXISTS idx_flows_user_id ON flows(user_id);
"#;

/// Helper: split SQL into individual statements, strip comments.
fn split_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|chunk| {
            chunk
                .lines()
                .filter(|line| !line.trim().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Run migrations on a PostgreSQL pool.
pub async fn run_pg_migrations(pool: &sqlx::PgPool) -> Result<(), StorageError> {
    tracing::info!("Checking PostgreSQL migrations...");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let applied_v1: Option<(i32,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = 1")
            .fetch_optional(pool)
            .await?;

    if applied_v1.is_none() {
        tracing::info!("Applying PostgreSQL migration V1...");

        for stmt in &split_statements(PG_MIGRATION_V1) {
            sqlx::query(stmt).execute(pool).await.map_err(|e| {
                StorageError::Migration(format!("Failed to execute: {}", e))
            })?;
        }

        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (1, NOW())")
            .execute(pool)
            .await?;

        tracing::info!("PostgreSQL migration V1 applied successfully");
    } else {
        tracing::debug!("PostgreSQL migration V1 already applied");
    }

    let applied_v2: Option<(i32,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = 2")
            .fetch_optional(pool)
            .await?;

    if applied_v2.is_none() {
        tracing::info!("Applying PostgreSQL migration V2...");

        for stmt in &split_statements(PG_MIGRATION_V2) {
            sqlx::query(stmt).execute(pool).await.map_err(|e| {
                StorageError::Migration(format!("Failed to execute: {}", e))
            })?;
        }

        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (2, NOW())")
            .execute(pool)
            .await?;

        tracing::info!("PostgreSQL migration V2 applied successfully");
    } else {
        tracing::debug!("PostgreSQL migration V2 already applied");
    }

    Ok(())
}

/// Run migrations on a SQLite pool.
pub async fn run_sqlite_migrations(pool: &sqlx::SqlitePool) -> Result<(), StorageError> {
    tracing::info!("Checking SQLite migrations...");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let applied_v1: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = 1")
            .fetch_optional(pool)
            .await?;

    if applied_v1.is_none() {
        tracing::info!("Applying SQLite migration V1...");

        for stmt in &split_statements(SQLITE_MIGRATION_V1) {
            sqlx::query(stmt).execute(pool).await.map_err(|e| {
                StorageError::Migration(format!("Failed to execute: {}", e))
            })?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)")
            .bind(&now)
            .execute(pool)
            .await?;

        tracing::info!("SQLite migration V1 applied successfully");
    } else {
        tracing::debug!("SQLite migration V1 already applied");
    }

    let applied_v2: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = 2")
            .fetch_optional(pool)
            .await?;

    if applied_v2.is_none() {
        tracing::info!("Applying SQLite migration V2...");

        for stmt in &split_statements(SQLITE_MIGRATION_V2) {
            match sqlx::query(stmt).execute(pool).await {
                Ok(_) => {},
                Err(e) => {
                    // For SQLite, some statements like ALTER TABLE ADD COLUMN may fail
                    // if the column already exists. Log and continue.
                    tracing::debug!("Migration statement possibly already applied: {}", e);
                }
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (2, ?1)")
            .bind(&now)
            .execute(pool)
            .await?;

        tracing::info!("SQLite migration V2 applied successfully");
    } else {
        tracing::debug!("SQLite migration V2 already applied");
    }

    Ok(())
}
