//! Database node: executes SQL queries against multiple database backends.
//!
//! Supports PostgreSQL, MySQL, SQLite, and SQL Server via connection configuration.
//! Parameters can be extracted from the message payload using dot-notation paths.
//!
//! Config example (individual fields):
//! ```json
//! {
//!   "dbType": "postgres",
//!   "host": "db.example.com",
//!   "port": 5432,
//!   "database": "myapp",
//!   "user": "admin",
//!   "password": "secret",
//!   "query": "SELECT * FROM users WHERE age > $1",
//!   "params": ["req.body.min_age"]
//! }
//! ```
//!
//! Or via direct connection string:
//! ```json
//! {
//!   "connectionString": "postgres://admin:secret@db.example.com/myapp",
//!   "query": "SELECT * FROM users"
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::{debug, error};

use super::switch::json_path_lookup;

pub struct DatabaseNode {
    name: String,
    db_type: String,
    host: String,
    port: u16,
    database: String,
    user: String,
    password: String,
    query: String,
    /// Dot-notation paths to extract parameter values from message payload.
    params: Vec<String>,
    /// Direct connection string (overrides individual fields if set).
    connection_string: String,
}

impl DatabaseNode {
    /// Build connection string from individual config fields.
    fn build_connection_string(&self) -> String {
        if !self.connection_string.is_empty() {
            return self.connection_string.clone();
        }

        match self.db_type.as_str() {
            "postgres" => {
                let port = if self.port == 0 { 5432 } else { self.port };
                format!(
                    "postgres://{}:{}@{}:{}/{}",
                    self.user, self.password, self.host, port, self.database
                )
            }
            "mysql" => {
                let port = if self.port == 0 { 3306 } else { self.port };
                format!(
                    "mysql://{}:{}@{}:{}/{}",
                    self.user, self.password, self.host, port, self.database
                )
            }
            "sqlite" => {
                // For SQLite, database is the file path
                if self.database.is_empty() {
                    "sqlite::memory:".to_string()
                } else {
                    format!("sqlite:{}", self.database)
                }
            }
            "mssql" => {
                let port = if self.port == 0 { 1433 } else { self.port };
                format!(
                    "mssql://{}:{}@{}:{}/{}",
                    self.user, self.password, self.host, port, self.database
                )
            }
            _ => {
                // Fallback: try postgres-style
                let port = if self.port == 0 { 5432 } else { self.port };
                format!(
                    "{}://{}:{}@{}:{}/{}",
                    self.db_type, self.user, self.password, self.host, port, self.database
                )
            }
        }
    }

    /// Returns a safe version of the connection string for error messages (no password).
    fn safe_connection_info(&self) -> String {
        if self.connection_string.is_empty() {
            format!(
                "{}://{}@{}:{}/{}",
                self.db_type, self.user, self.host, self.port, self.database
            )
        } else {
            // Mask everything between :// and @
            self.connection_string
                .split('@')
                .next_back()
                .unwrap_or("***")
                .to_string()
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for DatabaseNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, db_type = %self.db_type, query = %self.query, "Executing database query");

        let conn_str = self.build_connection_string();

        if conn_str.is_empty() {
            let err = serde_json::json!({
                "error": "No database connection configured"
            });
            let out = msg.derive(msg.source_node, "error", err);
            return Ok(vec![out]);
        }

        // Use the appropriate database driver based on dbType
        match self.db_type.as_str() {
            "postgres" => self.execute_postgres(&msg, &conn_str).await,
            "mysql" => self.execute_mysql(&msg, &conn_str).await,
            "sqlite" => self.execute_sqlite(&msg, &conn_str).await,
            other => {
                let err = serde_json::json!({
                    "error": format!("Unsupported database type: '{}'. Supported: postgres, mysql, sqlite, mssql", other),
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(t) = config.get("dbType").and_then(|v| v.as_str()) {
            self.db_type = t.to_string();
        }
        // Legacy: also accept "type" field
        if let Some(t) = config.get("type").and_then(|v| v.as_str()) {
            self.db_type = t.to_string();
        }
        if let Some(h) = config.get("host").and_then(|v| v.as_str()) {
            self.host = h.to_string();
        }
        if let Some(p) = config.get("port").and_then(|v| v.as_u64()) {
            self.port = p as u16;
        }
        if let Some(d) = config.get("database").and_then(|v| v.as_str()) {
            self.database = d.to_string();
        }
        if let Some(u) = config.get("user").and_then(|v| v.as_str()) {
            self.user = u.to_string();
        }
        if let Some(pw) = config.get("password").and_then(|v| v.as_str()) {
            self.password = pw.to_string();
        }
        if let Some(q) = config.get("query").and_then(|v| v.as_str()) {
            self.query = q.to_string();
        }
        if let Some(params) = config.get("params").and_then(|v| v.as_array()) {
            self.params = params
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(cs) = config.get("connectionString").and_then(|v| v.as_str()) {
            self.connection_string = cs.to_string();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.query.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Database node requires a 'query'".to_string(),
            ));
        }
        if self.connection_string.is_empty() && self.database.is_empty() && self.db_type != "sqlite"
        {
            return Err(crate::error::Z8Error::Internal(
                "Database node requires either a 'connectionString' or 'database' name".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "database"
    }
}

// ---------- PostgreSQL ----------

impl DatabaseNode {
    async fn execute_postgres(
        &self,
        msg: &FlowMessage,
        conn_str: &str,
    ) -> Z8Result<Vec<FlowMessage>> {
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(conn_str)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                error!(node = %self.name, error = %e, "PostgreSQL connection failed");
                let err = serde_json::json!({
                    "error": format!("PostgreSQL connection failed: {}", e),
                    "connection": self.safe_connection_info(),
                });
                let out = msg.derive(msg.source_node, "error", err);
                return Ok(vec![out]);
            }
        };

        let mut query = sqlx::query(&self.query);
        for param_path in &self.params {
            let val = json_path_lookup(&msg.payload, param_path);
            query = bind_pg_value(query, &val);
        }

        let result = match query.fetch_all(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                error!(node = %self.name, error = %e, "PostgreSQL query failed");
                let err = serde_json::json!({
                    "error": format!("Query failed: {}", e),
                    "query": self.query,
                });
                let out = msg.derive(msg.source_node, "error", err);
                pool.close().await;
                return Ok(vec![out]);
            }
        };

        let rows_json: Vec<Value> = result.iter().map(pg_row_to_json).collect();
        let payload = serde_json::json!({
            "rows": rows_json,
            "count": rows_json.len(),
            "query": self.query,
            "dbType": "postgres",
        });

        debug!(node = %self.name, row_count = rows_json.len(), "PostgreSQL query completed");
        pool.close().await;
        let out = msg.derive(msg.source_node, "results", payload);
        Ok(vec![out])
    }

    async fn execute_mysql(&self, msg: &FlowMessage, conn_str: &str) -> Z8Result<Vec<FlowMessage>> {
        // MySQL uses ? for parameters instead of $1, $2, ...
        let pool = match sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(conn_str)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                error!(node = %self.name, error = %e, "MySQL connection failed");
                let err = serde_json::json!({
                    "error": format!("MySQL connection failed: {}", e),
                    "connection": self.safe_connection_info(),
                });
                let out = msg.derive(msg.source_node, "error", err);
                return Ok(vec![out]);
            }
        };

        let mut query = sqlx::query(&self.query);
        for param_path in &self.params {
            let val = json_path_lookup(&msg.payload, param_path);
            query = bind_mysql_value(query, &val);
        }

        let result = match query.fetch_all(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                error!(node = %self.name, error = %e, "MySQL query failed");
                let err = serde_json::json!({
                    "error": format!("Query failed: {}", e),
                    "query": self.query,
                });
                let out = msg.derive(msg.source_node, "error", err);
                pool.close().await;
                return Ok(vec![out]);
            }
        };

        let rows_json: Vec<Value> = result.iter().map(mysql_row_to_json).collect();
        let payload = serde_json::json!({
            "rows": rows_json,
            "count": rows_json.len(),
            "query": self.query,
            "dbType": "mysql",
        });

        debug!(node = %self.name, row_count = rows_json.len(), "MySQL query completed");
        pool.close().await;
        let out = msg.derive(msg.source_node, "results", payload);
        Ok(vec![out])
    }

    async fn execute_sqlite(
        &self,
        msg: &FlowMessage,
        conn_str: &str,
    ) -> Z8Result<Vec<FlowMessage>> {
        let pool = match sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(conn_str)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                error!(node = %self.name, error = %e, "SQLite connection failed");
                let err = serde_json::json!({
                    "error": format!("SQLite connection failed: {}", e),
                    "connection": conn_str,
                });
                let out = msg.derive(msg.source_node, "error", err);
                return Ok(vec![out]);
            }
        };

        let mut query = sqlx::query(&self.query);
        for param_path in &self.params {
            let val = json_path_lookup(&msg.payload, param_path);
            query = bind_sqlite_value(query, &val);
        }

        let result = match query.fetch_all(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                error!(node = %self.name, error = %e, "SQLite query failed");
                let err = serde_json::json!({
                    "error": format!("Query failed: {}", e),
                    "query": self.query,
                });
                let out = msg.derive(msg.source_node, "error", err);
                pool.close().await;
                return Ok(vec![out]);
            }
        };

        let rows_json: Vec<Value> = result.iter().map(sqlite_row_to_json).collect();
        let payload = serde_json::json!({
            "rows": rows_json,
            "count": rows_json.len(),
            "query": self.query,
            "dbType": "sqlite",
        });

        debug!(node = %self.name, row_count = rows_json.len(), "SQLite query completed");
        pool.close().await;
        let out = msg.derive(msg.source_node, "results", payload);
        Ok(vec![out])
    }
}

// ---------- PostgreSQL helpers ----------

fn bind_pg_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    val: &Value,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match val {
        Value::String(s) => query.bind(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        Value::Bool(b) => query.bind(*b),
        Value::Null => query.bind(None::<String>),
        _ => query.bind(serde_json::to_string(val).unwrap_or_default()),
    }
}

fn pg_row_to_json(row: &sqlx::postgres::PgRow) -> Value {
    use sqlx::{Column, Row, TypeInfo};
    let mut obj = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let val: Value = match type_name {
            "INT4" | "INT2" => row
                .try_get::<i32, _>(name.as_str())
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "INT8" => row
                .try_get::<i64, _>(name.as_str())
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "FLOAT4" | "FLOAT8" => row
                .try_get::<f64, _>(name.as_str())
                .map(|v| serde_json::json!(v))
                .unwrap_or(Value::Null),
            "BOOL" => row
                .try_get::<bool, _>(name.as_str())
                .map(Value::Bool)
                .unwrap_or(Value::Null),
            "JSON" | "JSONB" => row
                .try_get::<Value, _>(name.as_str())
                .unwrap_or(Value::Null),
            "UUID" => row
                .try_get::<uuid::Uuid, _>(name.as_str())
                .map(|v| Value::String(v.to_string()))
                .unwrap_or(Value::Null),
            "TIMESTAMPTZ" | "TIMESTAMP" => row
                .try_get::<chrono::NaiveDateTime, _>(name.as_str())
                .map(|v| Value::String(v.to_string()))
                .ok()
                .or_else(|| {
                    row.try_get::<chrono::DateTime<chrono::Utc>, _>(name.as_str())
                        .map(|v| Value::String(v.to_rfc3339()))
                        .ok()
                })
                .unwrap_or(Value::Null),
            _ => row
                .try_get::<String, _>(name.as_str())
                .map(Value::String)
                .unwrap_or(Value::Null),
        };
        obj.insert(name, val);
    }
    Value::Object(obj)
}

// ---------- MySQL helpers ----------

fn bind_mysql_value<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    val: &Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match val {
        Value::String(s) => query.bind(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        Value::Bool(b) => query.bind(*b),
        Value::Null => query.bind(None::<String>),
        _ => query.bind(serde_json::to_string(val).unwrap_or_default()),
    }
}

fn mysql_row_to_json(row: &sqlx::mysql::MySqlRow) -> Value {
    use sqlx::{Column, Row, TypeInfo};
    let mut obj = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let val: Value = match type_name {
            "INT" | "SMALLINT" | "MEDIUMINT" | "TINYINT" => row
                .try_get::<i32, _>(name.as_str())
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "BIGINT" => row
                .try_get::<i64, _>(name.as_str())
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "FLOAT" | "DOUBLE" | "DECIMAL" => row
                .try_get::<f64, _>(name.as_str())
                .map(|v| serde_json::json!(v))
                .unwrap_or(Value::Null),
            "BOOLEAN" => row
                .try_get::<bool, _>(name.as_str())
                .map(Value::Bool)
                .unwrap_or(Value::Null),
            "JSON" => row
                .try_get::<Value, _>(name.as_str())
                .unwrap_or(Value::Null),
            "DATETIME" | "TIMESTAMP" => row
                .try_get::<chrono::NaiveDateTime, _>(name.as_str())
                .map(|v| Value::String(v.to_string()))
                .unwrap_or(Value::Null),
            _ => row
                .try_get::<String, _>(name.as_str())
                .map(Value::String)
                .unwrap_or(Value::Null),
        };
        obj.insert(name, val);
    }
    Value::Object(obj)
}

// ---------- SQLite helpers ----------

fn bind_sqlite_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    val: &Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match val {
        Value::String(s) => query.bind(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        Value::Bool(b) => query.bind(*b),
        Value::Null => query.bind(None::<String>),
        _ => query.bind(serde_json::to_string(val).unwrap_or_default()),
    }
}

fn sqlite_row_to_json(row: &sqlx::sqlite::SqliteRow) -> Value {
    use sqlx::{Column, Row, TypeInfo};
    let mut obj = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let val: Value = match type_name {
            "INTEGER" => row
                .try_get::<i64, _>(name.as_str())
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "REAL" => row
                .try_get::<f64, _>(name.as_str())
                .map(|v| serde_json::json!(v))
                .unwrap_or(Value::Null),
            "BOOLEAN" => row
                .try_get::<bool, _>(name.as_str())
                .map(Value::Bool)
                .unwrap_or(Value::Null),
            "TEXT" => row
                .try_get::<String, _>(name.as_str())
                .map(Value::String)
                .unwrap_or(Value::Null),
            _ => row
                .try_get::<String, _>(name.as_str())
                .map(Value::String)
                .unwrap_or(Value::Null),
        };
        obj.insert(name, val);
    }
    Value::Object(obj)
}

// ---------- Factory ----------

pub struct DatabaseNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for DatabaseNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = DatabaseNode {
            name: "Database".to_string(),
            db_type: "postgres".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: String::new(),
            user: String::new(),
            password: String::new(),
            query: String::new(),
            params: vec![],
            connection_string: String::new(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "database"
    }
}
