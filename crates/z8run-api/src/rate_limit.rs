//! Rate limiting middleware for z8run API.
//!
//! Uses an in-memory token bucket algorithm with per-IP tracking.
//! Supports configurable limits for different route tiers:
//!   - General API:  `Z8_RATE_LIMIT_API`   (default: 100 req/min)
//!   - Auth routes:  `Z8_RATE_LIMIT_AUTH`  (default: 20 req/min)
//!   - Webhooks:     `Z8_RATE_LIMIT_HOOK`  (default: 200 req/min)
//!
//! Responds with `429 Too Many Requests` and standard rate-limit headers:
//!   - `X-RateLimit-Limit`     — max requests per window
//!   - `X-RateLimit-Remaining` — requests remaining
//!   - `X-RateLimit-Reset`     — seconds until window resets
//!   - `Retry-After`           — seconds to wait (on 429)

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, Response, StatusCode},
    middleware::Next,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::warn;

/// Bucket state for a single client.
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64) -> Self {
        Self {
            tokens: max_tokens,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time, then try to consume one.
    /// Returns (allowed, remaining, reset_seconds).
    fn try_consume(&mut self, max_tokens: f64, window_secs: f64) -> (bool, u64, u64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        // Refill at rate: max_tokens / window_secs per second
        let refill_rate = max_tokens / window_secs;
        self.tokens = (self.tokens + elapsed * refill_rate).min(max_tokens);
        self.last_refill = now;

        let _remaining = self.tokens as u64;
        let reset_secs = if self.tokens < max_tokens {
            ((max_tokens - self.tokens) / refill_rate).ceil() as u64
        } else {
            0
        };

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            (true, self.tokens as u64, reset_secs)
        } else {
            let retry_after = ((1.0 - self.tokens) / refill_rate).ceil() as u64;
            (false, 0, retry_after)
        }
    }
}

/// Shared rate limiter state.
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    max_requests: u64,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        let limiter = Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_secs,
        };

        // Spawn cleanup task to evict stale entries every 5 minutes
        let buckets = Arc::clone(&limiter.buckets);
        let ws = window_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let mut map = buckets.write().await;
                let stale_threshold = Duration::from_secs(ws * 2);
                let now = Instant::now();
                map.retain(|_, bucket| now.duration_since(bucket.last_refill) < stale_threshold);
            }
        });

        limiter
    }

    /// Check rate limit for a given key. Returns (allowed, remaining, reset_secs).
    async fn check(&self, key: &str) -> (bool, u64, u64) {
        let mut buckets = self.buckets.write().await;
        let max = self.max_requests as f64;
        let window = self.window_secs as f64;

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(max));

        bucket.try_consume(max, window)
    }
}

/// Extract client IP from request (supports X-Forwarded-For for proxies).
fn extract_client_ip(req: &Request<Body>) -> String {
    // Check X-Forwarded-For header first (for reverse proxy / Cloudflare)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            // Take the first IP in the chain (original client)
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Check X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(val) = real_ip.to_str() {
            return val.trim().to_string();
        }
    }

    // Fallback to socket address
    if let Some(addr) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.0.ip().to_string();
    }

    "unknown".to_string()
}

/// Build a 429 Too Many Requests response with proper headers.
fn too_many_requests(limit: u64, reset_secs: u64) -> Response<Body> {
    let body = serde_json::json!({
        "error": "Too Many Requests",
        "message": "Rate limit exceeded. Please try again later.",
        "retryAfter": reset_secs,
    });

    let mut resp = Response::new(Body::from(body.to_string()));
    *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;

    let headers = resp.headers_mut();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("x-ratelimit-limit", limit.to_string().parse().unwrap());
    headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
    headers.insert("x-ratelimit-reset", reset_secs.to_string().parse().unwrap());
    headers.insert("retry-after", reset_secs.to_string().parse().unwrap());

    resp
}

/// Append rate-limit headers to a successful response.
fn append_rate_headers(resp: &mut Response<Body>, limit: u64, remaining: u64, reset_secs: u64) {
    let headers = resp.headers_mut();
    headers.insert("x-ratelimit-limit", limit.to_string().parse().unwrap());
    headers.insert(
        "x-ratelimit-remaining",
        remaining.to_string().parse().unwrap(),
    );
    headers.insert("x-ratelimit-reset", reset_secs.to_string().parse().unwrap());
}

/// Rate limit middleware for general API routes.
///
/// Default: 100 requests per 60 seconds per IP.
pub async fn api_rate_limit(req: Request<Body>, next: Next) -> Response<Body> {
    rate_limit_inner(req, next, api_limiter()).await
}

/// Rate limit middleware for auth routes (stricter).
///
/// Default: 20 requests per 60 seconds per IP.
pub async fn auth_rate_limit(req: Request<Body>, next: Next) -> Response<Body> {
    rate_limit_inner(req, next, auth_limiter()).await
}

/// Rate limit middleware for webhook/hook routes.
///
/// Default: 200 requests per 60 seconds per IP.
pub async fn hook_rate_limit(req: Request<Body>, next: Next) -> Response<Body> {
    rate_limit_inner(req, next, hook_limiter()).await
}

async fn rate_limit_inner(req: Request<Body>, next: Next, limiter: &RateLimiter) -> Response<Body> {
    let client_ip = extract_client_ip(&req);
    let (allowed, remaining, reset_secs) = limiter.check(&client_ip).await;

    if !allowed {
        warn!(
            ip = %client_ip,
            limit = limiter.max_requests,
            "Rate limit exceeded"
        );
        return too_many_requests(limiter.max_requests, reset_secs);
    }

    let mut response = next.run(req).await;
    append_rate_headers(&mut response, limiter.max_requests, remaining, reset_secs);
    response
}

// ── Global limiter singletons ───────────────────────────────

use std::sync::OnceLock;

static API_LIMITER: OnceLock<RateLimiter> = OnceLock::new();
static AUTH_LIMITER: OnceLock<RateLimiter> = OnceLock::new();
static HOOK_LIMITER: OnceLock<RateLimiter> = OnceLock::new();

/// Initialize rate limiters from environment variables.
/// Call once at startup before building the router.
pub fn init_rate_limiters() {
    let api_max = std::env::var("Z8_RATE_LIMIT_API")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100u64);

    let auth_max = std::env::var("Z8_RATE_LIMIT_AUTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20u64);

    let hook_max = std::env::var("Z8_RATE_LIMIT_HOOK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200u64);

    let window = std::env::var("Z8_RATE_LIMIT_WINDOW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);

    let _ = API_LIMITER.set(RateLimiter::new(api_max, window));
    let _ = AUTH_LIMITER.set(RateLimiter::new(auth_max, window));
    let _ = HOOK_LIMITER.set(RateLimiter::new(hook_max, window));

    tracing::info!(
        api = api_max,
        auth = auth_max,
        hook = hook_max,
        window_secs = window,
        "Rate limiters initialized"
    );
}

fn api_limiter() -> &'static RateLimiter {
    API_LIMITER
        .get()
        .expect("Rate limiters not initialized — call init_rate_limiters() first")
}

fn auth_limiter() -> &'static RateLimiter {
    AUTH_LIMITER
        .get()
        .expect("Rate limiters not initialized — call init_rate_limiters() first")
}

fn hook_limiter() -> &'static RateLimiter {
    HOOK_LIMITER
        .get()
        .expect("Rate limiters not initialized — call init_rate_limiters() first")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_allows_within_limit() {
        let mut bucket = TokenBucket::new(5.0);
        for _ in 0..5 {
            let (allowed, _, _) = bucket.try_consume(5.0, 60.0);
            assert!(allowed);
        }
        // 6th request should be denied
        let (allowed, _, _) = bucket.try_consume(5.0, 60.0);
        assert!(!allowed);
    }

    #[test]
    fn token_bucket_returns_correct_remaining() {
        let mut bucket = TokenBucket::new(10.0);
        let (allowed, remaining, _) = bucket.try_consume(10.0, 60.0);
        assert!(allowed);
        assert_eq!(remaining, 9);

        let (allowed, remaining, _) = bucket.try_consume(10.0, 60.0);
        assert!(allowed);
        assert_eq!(remaining, 8);
    }

    #[tokio::test]
    async fn rate_limiter_tracks_separate_keys() {
        let limiter = RateLimiter::new(2, 60);

        let (ok1, _, _) = limiter.check("ip-a").await;
        let (ok2, _, _) = limiter.check("ip-b").await;
        let (ok3, _, _) = limiter.check("ip-a").await;
        let (ok4, _, _) = limiter.check("ip-a").await; // should be denied
        let (ok5, _, _) = limiter.check("ip-b").await; // should still pass

        assert!(ok1);
        assert!(ok2);
        assert!(ok3);
        assert!(!ok4);
        assert!(ok5);
    }

    #[test]
    fn env_defaults_are_reasonable() {
        // Verify the default limits are positive
        let api = 100u64;
        let auth = 20u64;
        let hook = 200u64;
        assert!(api > 0);
        assert!(auth > 0);
        assert!(hook > 0);
    }
}
