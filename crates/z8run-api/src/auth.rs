//! JWT authentication and security middlewares.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT token claims.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User ID.
    pub sub: Uuid,
    /// User name.
    pub name: String,
    /// User roles.
    pub roles: Vec<String>,
    /// Expiration timestamp (epoch seconds).
    pub exp: i64,
    /// Issued at timestamp.
    pub iat: i64,
}

impl Claims {
    /// Creates claims for a user.
    pub fn new(user_id: Uuid, name: String, roles: Vec<String>, ttl_hours: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            sub: user_id,
            name,
            roles,
            exp: now + (ttl_hours * 3600),
            iat: now,
        }
    }

    /// Verifies if the token has expired.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.exp
    }

    /// Verifies if the user has a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}
