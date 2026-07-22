//! Encrypted credentials vault (AES-256-GCM).
//!
//! Stores API keys, tokens, and secrets securely.
//! WASM nodes only receive temporary tokens, never the actual key.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::StorageError;

/// Trait for the credentials vault.
///
/// Every operation is scoped to a `user_id` so that credentials are isolated
/// per user: one user can never read, overwrite, or delete another user's
/// secrets, and two users may reuse the same key name independently.
#[async_trait::async_trait]
pub trait CredentialVault: Send + Sync {
    /// Stores an encrypted credential for the given user.
    async fn store(&self, user_id: Uuid, key: &str, value: &str) -> Result<(), StorageError>;

    /// Retrieves a decrypted credential owned by the given user.
    async fn retrieve(&self, user_id: Uuid, key: &str) -> Result<String, StorageError>;

    /// Deletes a credential owned by the given user.
    async fn delete(&self, user_id: Uuid, key: &str) -> Result<(), StorageError>;

    /// Lists the keys owned by the given user (without values).
    async fn list_keys(&self, user_id: Uuid) -> Result<Vec<String>, StorageError>;

    /// Issues a short-lived temporary token for a WASM node, scoped to the
    /// credential's owner.
    async fn issue_temporary_token(
        &self,
        user_id: Uuid,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError>;
}

/// AES-256-GCM encryption helper.
pub struct VaultCrypto {
    cipher: Aes256Gcm,
}

impl VaultCrypto {
    /// Creates a new VaultCrypto from a 32-byte key.
    /// Panics if the key is not exactly 32 bytes. Use `from_secret` for
    /// arbitrary-length passphrases (it hashes them to 32 bytes via SHA-256).
    pub fn new(key: &[u8]) -> Self {
        assert!(
            key.len() == 32,
            "AES-256-GCM key must be exactly 32 bytes, got {}",
            key.len()
        );
        let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key must be 32 bytes");
        Self { cipher }
    }

    /// Derives a vault key from a string secret (e.g. JWT secret or env var).
    pub fn from_secret(secret: &str) -> Self {
        let hash = Sha256::digest(secret.as_bytes());
        Self::new(&hash)
    }

    /// Encrypts plaintext. Returns (ciphertext, nonce).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), StorageError> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| StorageError::Encryption(format!("Encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    /// Decrypts ciphertext using the provided nonce.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, StorageError> {
        if nonce.len() != 12 {
            return Err(StorageError::Encryption("Invalid nonce length".to_string()));
        }
        let nonce = Nonce::from_slice(nonce);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StorageError::Encryption(format!("Decryption failed: {}", e)))
    }
}

/// PostgreSQL-backed credential vault using AES-256-GCM.
pub struct PgCredentialVault {
    pool: sqlx::PgPool,
    crypto: VaultCrypto,
}

impl PgCredentialVault {
    pub fn new(pool: sqlx::PgPool, encryption_key: &str) -> Self {
        Self {
            pool,
            crypto: VaultCrypto::from_secret(encryption_key),
        }
    }
}

#[async_trait::async_trait]
impl CredentialVault for PgCredentialVault {
    async fn store(&self, user_id: Uuid, key: &str, value: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) = self.crypto.encrypt(value.as_bytes())?;
        let now = chrono::Utc::now();

        sqlx::query(
            r#"INSERT INTO credentials (user_id, key, encrypted_value, nonce, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT(user_id, key) DO UPDATE SET
                   encrypted_value = EXCLUDED.encrypted_value,
                   nonce = EXCLUDED.nonce,
                   updated_at = EXCLUDED.updated_at"#,
        )
        .bind(user_id.to_string())
        .bind(key)
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn retrieve(&self, user_id: Uuid, key: &str) -> Result<String, StorageError> {
        let row: (Vec<u8>, Vec<u8>) = sqlx::query_as(
            "SELECT encrypted_value, nonce FROM credentials WHERE user_id = $1 AND key = $2",
        )
        .bind(user_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::Encryption(format!("Credential not found: {}", key)))?;

        let plaintext = self.crypto.decrypt(&row.0, &row.1)?;
        String::from_utf8(plaintext)
            .map_err(|e| StorageError::Encryption(format!("Invalid UTF-8: {}", e)))
    }

    async fn delete(&self, user_id: Uuid, key: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM credentials WHERE user_id = $1 AND key = $2")
            .bind(user_id.to_string())
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_keys(&self, user_id: Uuid) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT key FROM credentials WHERE user_id = $1 ORDER BY key")
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn issue_temporary_token(
        &self,
        user_id: Uuid,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError> {
        // Retrieve the actual credential
        let value = self.retrieve(user_id, credential_key).await?;

        // Create a temporary token: base64(encrypted(value + expiry))
        let expiry = chrono::Utc::now().timestamp() + ttl_seconds as i64;
        let token_data = format!("{}|{}", value, expiry);
        let (ciphertext, nonce) = self.crypto.encrypt(token_data.as_bytes())?;

        // Encode as base64: nonce:ciphertext
        use std::fmt::Write;
        let mut token = String::new();
        for b in &nonce {
            write!(token, "{:02x}", b).unwrap();
        }
        token.push(':');
        for b in &ciphertext {
            write!(token, "{:02x}", b).unwrap();
        }

        Ok(token)
    }
}

/// SQLite-backed credential vault using AES-256-GCM.
pub struct SqliteCredentialVault {
    pool: sqlx::SqlitePool,
    crypto: VaultCrypto,
}

impl SqliteCredentialVault {
    pub fn new(pool: sqlx::SqlitePool, encryption_key: &str) -> Self {
        Self {
            pool,
            crypto: VaultCrypto::from_secret(encryption_key),
        }
    }
}

#[async_trait::async_trait]
impl CredentialVault for SqliteCredentialVault {
    async fn store(&self, user_id: Uuid, key: &str, value: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) = self.crypto.encrypt(value.as_bytes())?;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"INSERT INTO credentials (user_id, key, encrypted_value, nonce, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(user_id, key) DO UPDATE SET
                   encrypted_value = excluded.encrypted_value,
                   nonce = excluded.nonce,
                   updated_at = excluded.updated_at"#,
        )
        .bind(user_id.to_string())
        .bind(key)
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now.clone())
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn retrieve(&self, user_id: Uuid, key: &str) -> Result<String, StorageError> {
        let row: (Vec<u8>, Vec<u8>) = sqlx::query_as(
            "SELECT encrypted_value, nonce FROM credentials WHERE user_id = ?1 AND key = ?2",
        )
        .bind(user_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::Encryption(format!("Credential not found: {}", key)))?;

        let plaintext = self.crypto.decrypt(&row.0, &row.1)?;
        String::from_utf8(plaintext)
            .map_err(|e| StorageError::Encryption(format!("Invalid UTF-8: {}", e)))
    }

    async fn delete(&self, user_id: Uuid, key: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM credentials WHERE user_id = ?1 AND key = ?2")
            .bind(user_id.to_string())
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_keys(&self, user_id: Uuid) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT key FROM credentials WHERE user_id = ?1 ORDER BY key")
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn issue_temporary_token(
        &self,
        user_id: Uuid,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError> {
        let value = self.retrieve(user_id, credential_key).await?;
        let expiry = chrono::Utc::now().timestamp() + ttl_seconds as i64;
        let token_data = format!("{}|{}", value, expiry);
        let (ciphertext, nonce) = self.crypto.encrypt(token_data.as_bytes())?;

        use std::fmt::Write;
        let mut token = String::new();
        for b in &nonce {
            write!(token, "{:02x}", b).unwrap();
        }
        token.push(':');
        for b in &ciphertext {
            write!(token, "{:02x}", b).unwrap();
        }

        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_crypto_roundtrip() {
        let crypto = VaultCrypto::from_secret("test-secret-key");
        let plaintext = b"my-api-key-12345";

        let (ciphertext, nonce) = crypto.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext);

        let decrypted = crypto.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_vault_crypto_different_nonces() {
        let crypto = VaultCrypto::from_secret("test-secret");
        let plaintext = b"same-plaintext";

        let (ct1, n1) = crypto.encrypt(plaintext).unwrap();
        let (ct2, n2) = crypto.encrypt(plaintext).unwrap();

        // Different nonces → different ciphertexts
        assert_ne!(n1, n2);
        assert_ne!(ct1, ct2);

        // Both decrypt to the same plaintext
        assert_eq!(crypto.decrypt(&ct1, &n1).unwrap(), plaintext);
        assert_eq!(crypto.decrypt(&ct2, &n2).unwrap(), plaintext);
    }

    #[test]
    fn test_vault_crypto_wrong_key() {
        let crypto1 = VaultCrypto::from_secret("key-one");
        let crypto2 = VaultCrypto::from_secret("key-two");

        let (ciphertext, nonce) = crypto1.encrypt(b"secret").unwrap();
        assert!(crypto2.decrypt(&ciphertext, &nonce).is_err());
    }

    /// Builds an in-memory SQLite vault with a single shared connection so the
    /// in-memory database survives across queries.
    async fn memory_vault() -> SqliteCredentialVault {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        crate::migration::run_sqlite_migrations(&pool)
            .await
            .expect("run migrations");
        SqliteCredentialVault::new(pool, "test-vault-secret")
    }

    #[tokio::test]
    async fn test_vault_is_isolated_per_user() {
        let vault = memory_vault().await;
        let alice = Uuid::now_v7();
        let bob = Uuid::now_v7();

        // Both users store a credential under the SAME key name.
        vault.store(alice, "api_key", "alice-secret").await.unwrap();
        vault.store(bob, "api_key", "bob-secret").await.unwrap();

        // Each user retrieves only their own value.
        assert_eq!(
            vault.retrieve(alice, "api_key").await.unwrap(),
            "alice-secret"
        );
        assert_eq!(vault.retrieve(bob, "api_key").await.unwrap(), "bob-secret");

        // A user cannot read a key they never stored.
        assert!(vault.retrieve(alice, "bob_only").await.is_err());

        // list_keys is scoped per user.
        assert_eq!(vault.list_keys(alice).await.unwrap(), vec!["api_key"]);
        assert_eq!(vault.list_keys(bob).await.unwrap(), vec!["api_key"]);
    }

    #[tokio::test]
    async fn test_vault_delete_only_affects_owner() {
        let vault = memory_vault().await;
        let alice = Uuid::now_v7();
        let bob = Uuid::now_v7();

        vault
            .store(alice, "shared_name", "alice-secret")
            .await
            .unwrap();
        vault.store(bob, "shared_name", "bob-secret").await.unwrap();

        // Alice deletes her key; Bob's identically-named key is untouched.
        vault.delete(alice, "shared_name").await.unwrap();

        assert!(vault.retrieve(alice, "shared_name").await.is_err());
        assert_eq!(
            vault.retrieve(bob, "shared_name").await.unwrap(),
            "bob-secret"
        );
    }
}
