//! Encrypted credentials vault (AES-256-GCM).
//!
//! Stores API keys, tokens, and secrets securely.
//! WASM nodes only receive temporary tokens, never the actual key.

use crate::StorageError;

/// Trait for the credentials vault.
#[async_trait::async_trait]
pub trait CredentialVault: Send + Sync {
    /// Stores an encrypted credential.
    async fn store(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Retrieves a decrypted credential.
    async fn retrieve(&self, key: &str) -> Result<String, StorageError>;

    /// Deletes a credential.
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Lists stored keys (without values).
    async fn list_keys(&self) -> Result<Vec<String>, StorageError>;

    /// Issues a short-lived temporary token for a WASM node.
    async fn issue_temporary_token(
        &self,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError>;
}
