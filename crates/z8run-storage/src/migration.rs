//! Automatic schema migration system.

use sqlx::sqlite::SqlitePool;

use crate::StorageError;

/// Initial migration SQL for SQLite and PostgreSQL.
pub const MIGRATION_V1: &str = r#"
-- Flows table
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

-- Index by name for searches
CREATE INDEX IF NOT EXISTS idx_flows_name ON flows(name);
CREATE INDEX IF NOT EXISTS idx_flows_status ON flows(status);

-- Execution history table
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

-- Encrypted credentials table
CREATE TABLE IF NOT EXISTS credentials (
    key TEXT PRIMARY KEY,
    encrypted_value BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Applied migrations table
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);
"#;

/// Applies pending migrations using the given connection pool.
pub async fn run_migrations_with_pool(pool: &SqlitePool) -> Result<(), StorageError> {
    tracing::info!("Checking database migrations...");

    // First, ensure the schema_migrations table exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Check if V1 has been applied
    let applied: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = 1")
            .fetch_optional(pool)
            .await?;

    if applied.is_none() {
        tracing::info!("Applying migration V1...");

        // Execute each statement separately (SQLite doesn't support multi-statement exec).
        // Split by semicolon, strip comment lines, skip empty results.
        let statements: Vec<String> = MIGRATION_V1
            .split(';')
            .map(|chunk| {
                chunk
                    .lines()
                    .filter(|line| !line.trim().starts_with("--"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        for stmt in &statements {
            sqlx::query(stmt).execute(pool).await.map_err(|e| {
                StorageError::Migration(format!("Failed to execute: {}", e))
            })?;
        }

        // Record migration
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)")
            .bind(&now)
            .execute(pool)
            .await?;

        tracing::info!("Migration V1 applied successfully");
    } else {
        tracing::debug!("Migration V1 already applied");
    }

    Ok(())
}

/// Convenience function: connects and runs migrations.
pub async fn run_migrations(db_url: &str) -> Result<(), StorageError> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(db_url)
        .await
        .map_err(|e| StorageError::Database(format!("Migration connection failed: {}", e)))?;

    run_migrations_with_pool(&pool).await
}
