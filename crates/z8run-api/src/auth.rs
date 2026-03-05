//! JWT authentication and security middlewares.

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// JWT token claims.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// User ID.
    pub sub: Uuid,
    /// User name.
    pub name: String,
    /// User email.
    pub email: String,
    /// User roles.
    pub roles: Vec<String>,
    /// Expiration timestamp (epoch seconds).
    pub exp: i64,
    /// Issued at timestamp.
    pub iat: i64,
}

impl Claims {
    /// Creates claims for a user.
    pub fn new(user_id: Uuid, name: String, email: String, roles: Vec<String>, ttl_hours: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            sub: user_id,
            name,
            email,
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

/// Encodes a Claims struct into a JWT token.
pub fn encode_jwt(claims: &Claims, secret: &str) -> Result<String, ApiError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::internal(format!("JWT encode error: {}", e)))
}

/// Decodes and validates a JWT token.
pub fn decode_jwt(token: &str, secret: &str) -> Result<Claims, ApiError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| ApiError::unauthorized(format!("Invalid token: {}", e)))?;
    Ok(data.claims)
}

/// Request payload for user registration.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

/// Request payload for user login.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Response payload for auth success.
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

/// User information returned in responses.
#[derive(Debug, Serialize, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub username: String,
    pub roles: Vec<String>,
}

/// Registers a new user.
/// POST /auth/register
async fn register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Validate inputs
    if payload.email.is_empty() {
        return Err(ApiError::bad_request("Email cannot be empty"));
    }
    if payload.username.len() < 3 {
        return Err(ApiError::bad_request("Username must be at least 3 characters"));
    }
    if payload.password.len() < 8 {
        return Err(ApiError::bad_request("Password must be at least 8 characters"));
    }

    // Hash password with argon2
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|e| ApiError::internal(format!("Password hashing failed: {}", e)))?
        .to_string();

    // Create user record
    let user_id = Uuid::now_v7();
    let user = z8run_storage::repository::UserRecord {
        id: user_id,
        email: payload.email.clone(),
        username: payload.username.clone(),
        password_hash,
        roles: vec!["user".to_string()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Save to database
    state
        .user_storage
        .create_user(&user)
        .await
        .map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("unique") || msg.contains("duplicate") || msg.contains("already exists") {
                if msg.contains("email") {
                    ApiError::conflict("An account with this email already exists")
                } else if msg.contains("username") {
                    ApiError::conflict("This username is already taken")
                } else {
                    ApiError::conflict("An account with these credentials already exists")
                }
            } else {
                ApiError::from(e)
            }
        })?;

    // Create JWT token
    let claims = Claims::new(
        user_id,
        payload.username.clone(),
        payload.email.clone(),
        vec!["user".to_string()],
        24, // 24-hour expiration
    );
    let token = encode_jwt(&claims, &state.jwt_secret)?;

    Ok(Json(AuthResponse {
        token,
        user: UserInfo {
            id: user_id.to_string(),
            email: payload.email,
            username: payload.username,
            roles: vec!["user".to_string()],
        },
    }))
}

/// Authenticates a user and returns a JWT token.
/// POST /auth/login
async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Validate inputs
    if payload.email.is_empty() {
        return Err(ApiError::bad_request("Email cannot be empty"));
    }
    if payload.password.is_empty() {
        return Err(ApiError::bad_request("Password cannot be empty"));
    }

    // Look up user by email
    let user = state
        .user_storage
        .get_user_by_email(&payload.email)
        .await
        .map_err(|_| ApiError::unauthorized("Invalid email or password"))?;

    // Verify password
    let password_hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| ApiError::internal("Invalid password hash"))?;
    Argon2::default()
        .verify_password(payload.password.as_bytes(), &password_hash)
        .map_err(|_| ApiError::unauthorized("Invalid email or password"))?;

    // Create JWT token
    let claims = Claims::new(
        user.id,
        user.username.clone(),
        user.email.clone(),
        user.roles.clone(),
        24, // 24-hour expiration
    );
    let token = encode_jwt(&claims, &state.jwt_secret)?;

    Ok(Json(AuthResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            username: user.username,
            roles: user.roles,
        },
    }))
}

/// Returns the authenticated user's information.
/// GET /auth/me
async fn me(
    axum::Extension(claims): axum::Extension<Claims>,
) -> Result<Json<UserInfo>, ApiError> {
    Ok(Json(UserInfo {
        id: claims.sub.to_string(),
        email: claims.email,
        username: claims.name,
        roles: claims.roles,
    }))
}

/// JWT middleware that validates tokens and inserts Claims into request extensions.
pub async fn jwt_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let token = auth_header.ok_or_else(|| ApiError::unauthorized("Missing authorization header"))?;
    let claims = decode_jwt(token, &state.jwt_secret)?;

    // Check if token is expired
    if claims.is_expired() {
        return Err(ApiError::unauthorized("Token has expired"));
    }

    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

/// Mounts authentication routes (public).
pub fn auth_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
}

/// Mounts protected authentication routes (requires JWT).
pub fn auth_protected_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/me", get(me))
}
