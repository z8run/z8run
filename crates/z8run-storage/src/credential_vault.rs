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
    async fn store(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) = self.crypto.encrypt(value.as_bytes())?;
        let now = chrono::Utc::now();

        sqlx::query(
            r#"INSERT INTO credentials (key, encrypted_value, nonce, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT(key) DO UPDATE SET
                   encrypted_value = EXCLUDED.encrypted_value,
                   nonce = EXCLUDED.nonce,
                   updated_at = EXCLUDED.updated_at"#,
        )
        .bind(key)
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn retrieve(&self, key: &str) -> Result<String, StorageError> {
        let row: (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT encrypted_value, nonce FROM credentials WHERE key = $1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| {
                    StorageError::Encryption(format!("Credential not found: {}", key))
                })?;

        let plaintext = self.crypto.decrypt(&row.0, &row.1)?;
        String::from_utf8(plaintext)
            .map_err(|e| StorageError::Encryption(format!("Invalid UTF-8: {}", e)))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM credentials WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM credentials ORDER BY key")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn issue_temporary_token(
        &self,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError> {
        // Retrieve the actual credential
        let value = self.retrieve(credential_key).await?;

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
    async fn store(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) = self.crypto.encrypt(value.as_bytes())?;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"INSERT INTO credentials (key, encrypted_value, nonce, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(key) DO UPDATE SET
                   encrypted_value = excluded.encrypted_value,
                   nonce = excluded.nonce,
                   updated_at = excluded.updated_at"#,
        )
        .bind(key)
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now.clone())
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn retrieve(&self, key: &str) -> Result<String, StorageError> {
        let row: (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT encrypted_value, nonce FROM credentials WHERE key = ?1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| {
                    StorageError::Encryption(format!("Credential not found: {}", key))
                })?;

        let plaintext = self.crypto.decrypt(&row.0, &row.1)?;
        String::from_utf8(plaintext)
            .map_err(|e| StorageError::Encryption(format!("Invalid UTF-8: {}", e)))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM credentials WHERE key = ?1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM credentials ORDER BY key")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn issue_temporary_token(
        &self,
        credential_key: &str,
        ttl_seconds: u64,
    ) -> Result<String, StorageError> {
        let value = self.retrieve(credential_key).await?;
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
}
