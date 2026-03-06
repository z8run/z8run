//! # z8run-storage
//!
//! Dual SQLite/PostgreSQL persistence layer.
//! Provides a common abstraction that makes backend switching
//! transparent to the rest of the system.

pub mod credential_vault;
pub mod migration;
pub mod postgres;
pub mod repository;
pub mod sqlite;

use thiserror::Error;

/// Errors from the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Flow not found: {0}")]
    FlowNotFound(uuid::Uuid),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Migration error: {0}")]
    Migration(String),
}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// Storage backend type.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// SQLite for standalone/edge (local file).
    Sqlite { path: String },
    /// PostgreSQL for cloud/multi-tenant.
    Postgres { url: String },
}

impl StorageBackend {
    /// Creates a SQLite backend with default path.
    pub fn sqlite_default() -> Self {
        Self::Sqlite {
            path: "./data/z8run.db".to_string(),
        }
    }

    /// Returns the connection URL for sqlx.
    pub fn connection_url(&self) -> String {
        match self {
            Self::Sqlite { path } => format!("sqlite://{}?mode=rwc", path),
            Self::Postgres { url } => url.clone(),
        }
    }
}
