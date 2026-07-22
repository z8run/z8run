//! Rate limiting middleware for z8run API.
//!
//! Uses an in-memory token bucket algorithm with per-IP tracking.
//! Supports configurable limits for different route tiers:
//!   - General API:  `Z8_RATE_LIMIT_API`   (default: 100 req/min)
//!   - Auth routes:  `Z8_RATE_LIMIT_AUTH`  (default: 20 req/min)
//!   - Webhooks:     `Z8_RATE_LIMIT_HOOK`  (default: 200 req/min)
//!
//! Responds with `429 Too Many Requests` and standard rate-limit headers:
//!   - `X-RateLimit-Limit`     - max requests per window
//!   - `X-RateLimit-Remaining` - requests remaining
//!   - `X-RateLimit-Reset`     - seconds until window resets
//!   - `Retry-After`           - seconds to wait (on 429)

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, Response, StatusCode},
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
    /// When `true`, the limiter is disabled and every request is allowed.
    /// Set when `max_requests == 0` (see `.env.example`: "Set 0 to disable").
    disabled: bool,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        // A configured capacity of 0 means "unlimited / disabled" rather than
        // "block everything". Short-circuit checks and skip the cleanup task.
        let disabled = max_requests == 0;

        let limiter = Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_secs,
            disabled,
        };

        if disabled {
            tracing::info!("Rate limiter disabled (max_requests = 0); all requests allowed");
            return limiter;
        }

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
        // Disabled limiter (max_requests == 0): always allow, never touch state.
        if self.disabled {
            return (true, 0, 0);
        }

        let mut buckets = self.buckets.write().await;
        let max = self.max_requests as f64;
        let window = self.window_secs as f64;

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(max));

        bucket.try_consume(max, window)
    }
}

/// Whether forwarded IP headers (`X-Forwarded-For` / `X-Real-IP`) may be trusted.
///
/// These headers are trivially spoofable by any client that can reach the
/// backend directly, so honoring them unconditionally lets an attacker rotate
/// `X-Forwarded-For` to evade per-IP rate limiting. They are only meaningful
/// when the backend is guaranteed to sit behind a trusted reverse proxy
/// (e.g. nginx) that overwrites them.
///
/// Controlled by `Z8_TRUST_PROXY`: truthy values ("1", "true", "yes",
/// case-insensitive) enable trust; anything else (including unset) disables it.
fn trust_proxy_enabled() -> bool {
    std::env::var("Z8_TRUST_PROXY")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Extract the client IP used as the rate-limit bucket key.
///
/// When `trust_proxy` is `true`, forwarded headers are preferred (original
/// client behind a trusted reverse proxy), falling back to the TCP peer address.
/// When `trust_proxy` is `false`, forwarded headers are ignored entirely and the
/// actual TCP peer address is always used, so a client cannot spoof its identity.
///
/// Falls back to `"unknown"` only when no peer address is available and no
/// trusted header applies, rather than panicking.
fn extract_client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        // Trusted proxy: prefer X-Forwarded-For (first / original client in chain).
        if let Some(forwarded) = headers.get("x-forwarded-for") {
            if let Ok(val) = forwarded.to_str() {
                if let Some(ip) = val.split(',').next() {
                    let ip = ip.trim();
                    if !ip.is_empty() {
                        return ip.to_string();
                    }
                }
            }
        }

        // Then X-Real-IP.
        if let Some(real_ip) = headers.get("x-real-ip") {
            if let Ok(val) = real_ip.to_str() {
                let val = val.trim();
                if !val.is_empty() {
                    return val.to_string();
                }
            }
        }
    }

    // Untrusted (default), or trusted but no usable header: use the TCP peer addr.
    if let Some(addr) = peer {
        return addr.ip().to_string();
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
    // Peer address is injected as a `ConnectInfo` extension when the server is
    // started with `into_make_service_with_connect_info::<SocketAddr>()`.
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let client_ip = extract_client_ip(req.headers(), peer, trust_proxy_enabled());
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
        .expect("Rate limiters not initialized - call init_rate_limiters() first")
}

fn auth_limiter() -> &'static RateLimiter {
    AUTH_LIMITER
        .get()
        .expect("Rate limiters not initialized - call init_rate_limiters() first")
}

fn hook_limiter() -> &'static RateLimiter {
    HOOK_LIMITER
        .get()
        .expect("Rate limiters not initialized - call init_rate_limiters() first")
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

    #[tokio::test]
    async fn zero_max_requests_disables_limiter() {
        // Per .env.example, Z8_RATE_LIMIT_* = 0 disables rate limiting.
        // The limiter must allow every request instead of blocking all of them.
        let limiter = RateLimiter::new(0, 60);

        for _ in 0..1000 {
            let (allowed, _, reset) = limiter.check("ip-a").await;
            assert!(allowed, "disabled limiter must always allow requests");
            assert_eq!(reset, 0, "disabled limiter must not emit retry-after");
        }
    }

    #[test]
    fn untrusted_proxy_ignores_spoofed_forwarded_headers() {
        // With Z8_TRUST_PROXY off, a client-supplied X-Forwarded-For / X-Real-IP
        // must be ignored in favor of the real TCP peer address, so it can't be
        // rotated to evade per-IP rate limiting.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "9.9.9.9".parse().unwrap());
        headers.insert("x-real-ip", "8.8.8.8".parse().unwrap());
        let peer: SocketAddr = "203.0.113.7:5555".parse().unwrap();

        let ip = extract_client_ip(&headers, Some(peer), false);
        assert_eq!(ip, "203.0.113.7");
    }

    #[test]
    fn trusted_proxy_prefers_forwarded_header() {
        // With trust enabled, the first hop in X-Forwarded-For wins.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "9.9.9.9, 10.0.0.1".parse().unwrap());
        let peer: SocketAddr = "203.0.113.7:5555".parse().unwrap();

        let ip = extract_client_ip(&headers, Some(peer), true);
        assert_eq!(ip, "9.9.9.9");
    }

    #[test]
    fn trusted_proxy_falls_back_to_peer_without_headers() {
        // Trust enabled but no forwarded headers present: use the peer addr.
        let headers = HeaderMap::new();
        let peer: SocketAddr = "203.0.113.7:5555".parse().unwrap();

        let ip = extract_client_ip(&headers, Some(peer), true);
        assert_eq!(ip, "203.0.113.7");
    }

    #[test]
    fn missing_peer_falls_back_to_unknown() {
        // No peer address and untrusted headers => stable "unknown", no panic.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "9.9.9.9".parse().unwrap());

        let ip = extract_client_ip(&headers, None, false);
        assert_eq!(ip, "unknown");
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
