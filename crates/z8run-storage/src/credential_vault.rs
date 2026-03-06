//! Encrypted credentials vault (AES-256-GCM).
//!
//! Stores API keys, tokens, and secrets securely.
//! WASM nodes only receive temporary tokens, never the actual key.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

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
    /// If the provided key is shorter, it is padded with zeros.
    /// If longer, it is truncated.
    pub fn new(key: &[u8]) -> Self {
        let mut key_bytes = [0u8; 32];
        let len = key.len().min(32);
        key_bytes[..len].copy_from_slice(&key[..len]);
        let cipher =
            Aes256Gcm::new_from_slice(&key_bytes).expect("AES-256-GCM key must be 32 bytes");
        Self { cipher }
    }

    /// Derives a vault key from a string secret (e.g. JWT secret or env var).
    pub fn from_secret(secret: &str) -> Self {
        // Simple key derivation: SHA-256 of the secret string.
        // For production, use a proper KDF like argon2 or HKDF.
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        let hash = hasher.finalize();
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

/// Minimal SHA-256 for key derivation (avoids adding sha2 crate dependency).
struct Sha256 {
    data: Vec<u8>,
}

impl Sha256 {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn update(&mut self, input: &[u8]) {
        self.data.extend_from_slice(input);
    }

    fn finalize(self) -> [u8; 32] {
        // SHA-256 implementation using the constants and algorithm
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];

        let k: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        // Pre-processing: padding
        let msg_len = self.data.len();
        let bit_len = (msg_len as u64) * 8;
        let mut padded = self.data;
        padded.push(0x80);
        while (padded.len() % 64) != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&bit_len.to_be_bytes());

        // Process each 512-bit block
        for chunk in padded.chunks(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[i * 4],
                    chunk[i * 4 + 1],
                    chunk[i * 4 + 2],
                    chunk[i * 4 + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }

            let mut a = h[0];
            let mut b = h[1];
            let mut c = h[2];
            let mut d = h[3];
            let mut e = h[4];
            let mut f = h[5];
            let mut g = h[6];
            let mut hh = h[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(k[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }

        let mut result = [0u8; 32];
        for (i, val) in h.iter().enumerate() {
            result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
        }
        result
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
