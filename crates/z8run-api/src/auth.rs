//! JWT authentication and security middlewares.

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::body::Body;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, Request};
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
    pub fn new(
        user_id: Uuid,
        name: String,
        email: String,
        roles: Vec<String>,
        ttl_hours: i64,
    ) -> Self {
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

/// Name of the HttpOnly session cookie that carries the JWT (SEC-009).
pub(crate) const SESSION_COOKIE: &str = "z8_session";

/// Session lifetime in hours (matches the JWT expiry).
const SESSION_TTL_HOURS: i64 = 24;

/// Whether to mark the session cookie `Secure` (HTTPS only).
///
/// Enable in production via `Z8_COOKIE_SECURE=true`. Off by default so local
/// http dev keeps working (a `Secure` cookie is not sent over plain http).
fn cookie_secure() -> bool {
    matches!(
        std::env::var("Z8_COOKIE_SECURE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

/// Builds the `Set-Cookie` value that stores the session token.
fn session_cookie(token: &str) -> String {
    let max_age = SESSION_TTL_HOURS * 3600;
    let secure = if cookie_secure() { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}{secure}")
}

/// Builds the `Set-Cookie` value that clears the session token.
fn clear_session_cookie() -> String {
    let secure = if cookie_secure() { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure}")
}

/// Extracts the session token from the `Cookie` request header, if present.
pub(crate) fn token_from_cookies(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(SESSION_COOKIE)
            .and_then(|rest| rest.strip_prefix('='))
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    })
}

/// Wraps a JSON auth response with a `Set-Cookie` header for the session.
fn auth_response_with_cookie(
    token: &str,
    body: AuthResponse,
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&session_cookie(token))
            .map_err(|e| ApiError::internal(format!("Invalid session cookie: {}", e)))?,
    );
    Ok((headers, Json(body)))
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
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
    // Validate inputs
    if payload.email.is_empty() {
        return Err(ApiError::bad_request("Email cannot be empty"));
    }
    if payload.username.len() < 3 {
        return Err(ApiError::bad_request(
            "Username must be at least 3 characters",
        ));
    }
    if payload.password.len() < 8 {
        return Err(ApiError::bad_request(
            "Password must be at least 8 characters",
        ));
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
    state.user_storage.create_user(&user).await.map_err(|e| {
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
        SESSION_TTL_HOURS,
    );
    let token = encode_jwt(&claims, &state.jwt_secret)?;

    let body = AuthResponse {
        token: token.clone(),
        user: UserInfo {
            id: user_id.to_string(),
            email: payload.email,
            username: payload.username,
            roles: vec!["user".to_string()],
        },
    };
    auth_response_with_cookie(&token, body)
}

/// Authenticates a user and returns a JWT token.
/// POST /auth/login
async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<AuthResponse>), ApiError> {
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
        SESSION_TTL_HOURS,
    );
    let token = encode_jwt(&claims, &state.jwt_secret)?;

    let body = AuthResponse {
        token: token.clone(),
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            username: user.username,
            roles: user.roles,
        },
    };
    auth_response_with_cookie(&token, body)
}

/// Returns the authenticated user's information.
/// GET /auth/me
async fn me(axum::Extension(claims): axum::Extension<Claims>) -> Result<Json<UserInfo>, ApiError> {
    Ok(Json(UserInfo {
        id: claims.sub.to_string(),
        email: claims.email,
        username: claims.name,
        roles: claims.roles,
    }))
}

/// Clears the session cookie.
/// POST /auth/logout
async fn logout() -> Result<(HeaderMap, Json<serde_json::Value>), ApiError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie())
            .map_err(|e| ApiError::internal(format!("Invalid session cookie: {}", e)))?,
    );
    Ok((headers, Json(serde_json::json!({ "status": "logged_out" }))))
}

/// JWT middleware that validates tokens and inserts Claims into request extensions.
///
/// The token is taken from the `Authorization: Bearer` header (for CLI/API
/// clients) or, failing that, the HttpOnly `z8_session` cookie (browsers).
pub async fn jwt_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let bearer = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    let token = bearer
        .or_else(|| token_from_cookies(req.headers()))
        .ok_or_else(|| ApiError::unauthorized("Missing authentication"))?;

    let claims = decode_jwt(&token, &state.jwt_secret)?;

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
        .route("/logout", post(logout))
}

/// Mounts protected authentication routes (requires JWT).
pub fn auth_protected_routes() -> Router<Arc<AppState>> {
    Router::new().route("/me", get(me))
}
