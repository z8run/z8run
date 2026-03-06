//! PostgreSQL implementation of the storage repositories.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use z8run_core::flow::Flow;

use crate::repository::{
    ExecutionRecord, ExecutionRepository, FlowRepository, UserRecord, UserRepository,
};
use crate::StorageError;

/// PostgreSQL-backed storage for flows and executions.
#[derive(Clone)]
pub struct PgStorage {
    pool: PgPool,
}

impl PgStorage {
    /// Creates a new PostgreSQL storage.
    /// Example URL: `postgres://user:pass@localhost:5432/z8run`
    pub async fn new(database_url: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| {
                StorageError::Database(format!("Failed to connect to PostgreSQL: {}", e))
            })?;

        Ok(Self { pool })
    }

    /// Returns a reference to the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Runs migrations on this pool.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        crate::migration::run_pg_migrations(&self.pool).await
    }
}

#[async_trait::async_trait]
impl FlowRepository for PgStorage {
    async fn save_flow(&self, flow: &Flow) -> Result<(), StorageError> {
        let id = flow.id.to_string();
        let data =
            serde_json::to_value(flow).map_err(|e| StorageError::Serialization(e.to_string()))?;
        let status = flow.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO flows (id, name, description, version, data, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            ON CONFLICT(id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                version = EXCLUDED.version,
                data = EXCLUDED.data,
                status = EXCLUDED.status,
                updated_at = NOW()
            "#,
        )
        .bind(&id)
        .bind(&flow.name)
        .bind(&flow.description)
        .bind(&flow.version)
        .bind(&data)
        .bind(&status)
        .bind(&flow.created_at)
        .execute(&self.pool)
        .await?;

        tracing::debug!(flow_id = %id, "Flow saved to PostgreSQL");
        Ok(())
    }

    async fn get_flow(&self, id: Uuid) -> Result<Flow, StorageError> {
        let id_str = id.to_string();

        let row: (serde_json::Value,) = sqlx::query_as("SELECT data FROM flows WHERE id = $1")
            .bind(&id_str)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StorageError::FlowNotFound(id))?;

        let flow: Flow = serde_json::from_value(row.0)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        Ok(flow)
    }

    async fn list_flows(&self) -> Result<Vec<Flow>, StorageError> {
        let rows: Vec<(serde_json::Value,)> =
            sqlx::query_as("SELECT data FROM flows ORDER BY updated_at DESC")
                .fetch_all(&self.pool)
                .await?;

        let mut flows = Vec::with_capacity(rows.len());
        for (data,) in rows {
            let flow: Flow = serde_json::from_value(data)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            flows.push(flow);
        }

        Ok(flows)
    }

    async fn delete_flow(&self, id: Uuid) -> Result<(), StorageError> {
        let id_str = id.to_string();

        let result = sqlx::query("DELETE FROM flows WHERE id = $1")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::FlowNotFound(id));
        }

        tracing::debug!(flow_id = %id, "Flow deleted from PostgreSQL");
        Ok(())
    }

    async fn search_flows(&self, query: &str) -> Result<Vec<Flow>, StorageError> {
        let pattern = format!("%{}%", query);

        let rows: Vec<(serde_json::Value,)> =
            sqlx::query_as("SELECT data FROM flows WHERE name ILIKE $1 ORDER BY updated_at DESC")
                .bind(&pattern)
                .fetch_all(&self.pool)
                .await?;

        let mut flows = Vec::with_capacity(rows.len());
        for (data,) in rows {
            let flow: Flow = serde_json::from_value(data)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            flows.push(flow);
        }

        Ok(flows)
    }

    async fn save_flow_with_user(&self, flow: &Flow, user_id: Uuid) -> Result<(), StorageError> {
        let id = flow.id.to_string();
        let user_id_str = user_id.to_string();
        let data =
            serde_json::to_value(flow).map_err(|e| StorageError::Serialization(e.to_string()))?;
        let status = flow.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO flows (id, name, description, version, data, status, user_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT(id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                version = EXCLUDED.version,
                data = EXCLUDED.data,
                status = EXCLUDED.status,
                updated_at = NOW()
            "#,
        )
        .bind(&id)
        .bind(&flow.name)
        .bind(&flow.description)
        .bind(&flow.version)
        .bind(&data)
        .bind(&status)
        .bind(&user_id_str)
        .bind(&flow.created_at)
        .execute(&self.pool)
        .await?;

        tracing::debug!(flow_id = %id, user_id = %user_id, "Flow saved with user");
        Ok(())
    }

    async fn list_flows_by_user(&self, user_id: Uuid) -> Result<Vec<Flow>, StorageError> {
        let user_id_str = user_id.to_string();

        let rows: Vec<(serde_json::Value,)> =
            sqlx::query_as("SELECT data FROM flows WHERE user_id = $1 ORDER BY updated_at DESC")
                .bind(&user_id_str)
                .fetch_all(&self.pool)
                .await?;

        let mut flows = Vec::with_capacity(rows.len());
        for (data,) in rows {
            let flow: Flow = serde_json::from_value(data)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            flows.push(flow);
        }

        Ok(flows)
    }

    async fn get_flow_for_user(&self, id: Uuid, user_id: Uuid) -> Result<Flow, StorageError> {
        let id_str = id.to_string();
        let user_id_str = user_id.to_string();

        let row: (serde_json::Value,) =
            sqlx::query_as("SELECT data FROM flows WHERE id = $1 AND user_id = $2")
                .bind(&id_str)
                .bind(&user_id_str)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(StorageError::FlowNotFound(id))?;

        let flow: Flow = serde_json::from_value(row.0)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        Ok(flow)
    }

    async fn delete_flow_for_user(&self, id: Uuid, user_id: Uuid) -> Result<(), StorageError> {
        let id_str = id.to_string();
        let user_id_str = user_id.to_string();

        let result = sqlx::query("DELETE FROM flows WHERE id = $1 AND user_id = $2")
            .bind(&id_str)
            .bind(&user_id_str)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::FlowNotFound(id));
        }

        tracing::debug!(flow_id = %id, user_id = %user_id, "Flow deleted");
        Ok(())
    }
}

#[async_trait::async_trait]
impl ExecutionRepository for PgStorage {
    async fn record_start(&self, flow_id: Uuid, trace_id: Uuid) -> Result<Uuid, StorageError> {
        let id = Uuid::now_v7();
        let id_str = id.to_string();
        let flow_id_str = flow_id.to_string();
        let trace_id_str = trace_id.to_string();

        sqlx::query(
            r#"
            INSERT INTO executions (id, flow_id, trace_id, status, started_at, node_logs)
            VALUES ($1, $2, $3, 'running', NOW(), '{}')
            "#,
        )
        .bind(&id_str)
        .bind(&flow_id_str)
        .bind(&trace_id_str)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn record_completion(
        &self,
        execution_id: Uuid,
        status: &str,
        duration_ms: u64,
        error: Option<&str>,
    ) -> Result<(), StorageError> {
        let id_str = execution_id.to_string();
        let dur = duration_ms as i64;

        sqlx::query(
            r#"
            UPDATE executions
            SET status = $1, completed_at = NOW(), duration_ms = $2, error = $3
            WHERE id = $4
            "#,
        )
        .bind(status)
        .bind(dur)
        .bind(error)
        .bind(&id_str)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_history(
        &self,
        flow_id: Uuid,
        limit: usize,
    ) -> Result<Vec<ExecutionRecord>, StorageError> {
        let flow_id_str = flow_id.to_string();
        let limit_i64 = limit as i64;

        let rows: Vec<(String, String, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, Option<i64>, Option<String>, serde_json::Value)> =
            sqlx::query_as(
                r#"
                SELECT id, flow_id, trace_id, status, started_at, completed_at, duration_ms, error, node_logs
                FROM executions
                WHERE flow_id = $1
                ORDER BY started_at DESC
                LIMIT $2
                "#,
            )
            .bind(&flow_id_str)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let record = ExecutionRecord {
                id: Uuid::parse_str(&row.0)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?,
                flow_id: Uuid::parse_str(&row.1)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?,
                trace_id: Uuid::parse_str(&row.2)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?,
                status: row.3.clone(),
                started_at: row.4,
                completed_at: row.5,
                duration_ms: row.6.map(|v| v as u64),
                error: row.7.clone(),
                node_logs: row.8.clone(),
            };
            records.push(record);
        }

        Ok(records)
    }
}

#[async_trait::async_trait]
impl UserRepository for PgStorage {
    async fn create_user(&self, user: &UserRecord) -> Result<(), StorageError> {
        sqlx::query(
            r#"INSERT INTO users (id, email, username, password_hash, roles, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(&user.id.to_string())
        .bind(&user.email)
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(&user.roles.join(","))
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .execute(&self.pool)
        .await?;

        tracing::debug!(user_id = %user.id, "User created in PostgreSQL");
        Ok(())
    }

    async fn get_user_by_id(&self, id: Uuid) -> Result<UserRecord, StorageError> {
        let row: (String, String, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT id, email, username, password_hash, roles, created_at, updated_at FROM users WHERE id = $1")
                .bind(&id.to_string())
                .fetch_optional(&self.pool)
                .await?
                .ok_or(StorageError::UserNotFound(id.to_string()))?;

        Ok(UserRecord {
            id: Uuid::parse_str(&row.0).map_err(|e| StorageError::Serialization(e.to_string()))?,
            email: row.1,
            username: row.2,
            password_hash: row.3,
            roles: row
                .4
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            created_at: row.5,
            updated_at: row.6,
        })
    }

    async fn get_user_by_email(&self, email: &str) -> Result<UserRecord, StorageError> {
        let row: (String, String, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT id, email, username, password_hash, roles, created_at, updated_at FROM users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(StorageError::UserNotFound(email.to_string()))?;

        Ok(UserRecord {
            id: Uuid::parse_str(&row.0).map_err(|e| StorageError::Serialization(e.to_string()))?,
            email: row.1,
            username: row.2,
            password_hash: row.3,
            roles: row
                .4
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            created_at: row.5,
            updated_at: row.6,
        })
    }

    async fn get_user_by_username(&self, username: &str) -> Result<UserRecord, StorageError> {
        let row: (String, String, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT id, email, username, password_hash, roles, created_at, updated_at FROM users WHERE username = $1")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(StorageError::UserNotFound(username.to_string()))?;

        Ok(UserRecord {
            id: Uuid::parse_str(&row.0).map_err(|e| StorageError::Serialization(e.to_string()))?,
            email: row.1,
            username: row.2,
            password_hash: row.3,
            roles: row
                .4
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            created_at: row.5,
            updated_at: row.6,
        })
    }
}
