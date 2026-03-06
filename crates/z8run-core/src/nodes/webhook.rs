//! Webhook node: listens for incoming webhook events with signature validation.
//!
//! Unlike HTTP In (a generic endpoint), Webhook is designed for event-driven
//! integrations: GitHub webhooks, Stripe events, pub/sub pushes, etc.
//!
//! Features:
//! - HMAC-SHA256 signature validation (X-Hub-Signature-256 style)
//! - Event type extraction from headers or payload
//! - Event filtering (only process specific event types)
//! - Raw body passthrough for signature verification
//!
//! Config example:
//! ```json
//! {
//!   "path": "/github",
//!   "method": "POST",
//!   "secret": "whsec_abc123",
//!   "signatureHeader": "X-Hub-Signature-256",
//!   "eventHeader": "X-GitHub-Event",
//!   "events": ["push", "pull_request"]
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::{debug, warn};

pub struct WebhookNode {
    name: String,
    path: String,
    method: String,
    secret: String,
    signature_header: String,
    event_header: String,
    /// If non-empty, only these event types are accepted. Empty = accept all.
    events: Vec<String>,
}

/// Verify HMAC-SHA256 signature.
/// `signature` should be like "sha256=abcdef1234..."
fn verify_hmac_sha256(secret: &str, body: &str, signature: &str) -> bool {
    use std::fmt::Write;

    let sig_hex = signature.strip_prefix("sha256=").unwrap_or(signature);

    // Compute HMAC-SHA256 using a simple implementation
    // In production you'd use `hmac` + `sha2` crates, but we can compute
    // it with the raw algorithm to avoid extra deps for now.
    let expected = hmac_sha256(secret.as_bytes(), body.as_bytes());

    let mut expected_hex = String::with_capacity(64);
    for byte in &expected {
        let _ = write!(expected_hex, "{:02x}", byte);
    }

    // Constant-time comparison
    if expected_hex.len() != sig_hex.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected_hex.bytes().zip(sig_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Minimal HMAC-SHA256 implementation.
/// key = secret, message = body
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use std::io::Write;

    const BLOCK_SIZE: usize = 64;
    const IPAD: u8 = 0x36;
    const OPAD: u8 = 0x5c;

    // If key is longer than block size, hash it first
    let key_block = if key.len() > BLOCK_SIZE {
        let hash = sha256(key);
        let mut block = [0u8; BLOCK_SIZE];
        block[..32].copy_from_slice(&hash);
        block
    } else {
        let mut block = [0u8; BLOCK_SIZE];
        block[..key.len()].copy_from_slice(key);
        block
    };

    // Inner hash: SHA256((key XOR ipad) || message)
    let mut inner_input = Vec::with_capacity(BLOCK_SIZE + message.len());
    for &b in key_block.iter().take(BLOCK_SIZE) {
        inner_input.push(b ^ IPAD);
    }
    inner_input.write_all(message).unwrap();
    let inner_hash = sha256(&inner_input);

    // Outer hash: SHA256((key XOR opad) || inner_hash)
    let mut outer_input = Vec::with_capacity(BLOCK_SIZE + 32);
    for &b in key_block.iter().take(BLOCK_SIZE) {
        outer_input.push(b ^ OPAD);
    }
    outer_input.write_all(&inner_hash).unwrap();
    sha256(&outer_input)
}

/// Minimal SHA-256 implementation.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
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

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: padding
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
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

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
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
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

#[async_trait::async_trait]
impl NodeExecutor for WebhookNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, path = %self.path, "Processing webhook event");

        let headers = msg.payload.get("req").and_then(|r| r.get("headers"));
        let body = msg.payload.get("req").and_then(|r| r.get("body"));

        // 1. Signature validation (if secret is configured)
        if !self.secret.is_empty() {
            let sig = headers
                .and_then(|h| h.get(self.signature_header.to_lowercase().as_str()))
                .or_else(|| headers.and_then(|h| h.get(&self.signature_header)))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if sig.is_empty() {
                warn!(node = %self.name, "Missing signature header: {}", self.signature_header);
                let err = serde_json::json!({
                    "error": "Missing webhook signature",
                    "header": self.signature_header,
                });
                let out = msg.derive(msg.source_node, "rejected", err);
                return Ok(vec![out]);
            }

            let raw_body = match body {
                Some(Value::String(s)) => s.clone(),
                Some(v) => serde_json::to_string(v).unwrap_or_default(),
                None => String::new(),
            };

            if !verify_hmac_sha256(&self.secret, &raw_body, sig) {
                warn!(node = %self.name, "Invalid webhook signature");
                let err = serde_json::json!({
                    "error": "Invalid webhook signature",
                    "header": self.signature_header,
                });
                let out = msg.derive(msg.source_node, "rejected", err);
                return Ok(vec![out]);
            }

            debug!(node = %self.name, "Signature verified");
        }

        // 2. Extract event type
        let event_type = if !self.event_header.is_empty() {
            headers
                .and_then(|h| h.get(self.event_header.to_lowercase().as_str()))
                .or_else(|| headers.and_then(|h| h.get(&self.event_header)))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        } else {
            // Try to get event from payload body
            body.and_then(|b| b.get("event"))
                .or_else(|| body.and_then(|b| b.get("type")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        };

        // 3. Event filtering
        if !self.events.is_empty() && !self.events.contains(&event_type) {
            debug!(node = %self.name, event = %event_type, "Event type not in filter list, skipping");
            let out = serde_json::json!({
                "skipped": true,
                "event": event_type,
                "reason": "Event type not in allowed list",
            });
            let msg_out = msg.derive(msg.source_node, "filtered", out);
            return Ok(vec![msg_out]);
        }

        // 4. Build output payload
        let payload = serde_json::json!({
            "event": event_type,
            "body": body.cloned().unwrap_or(Value::Null),
            "headers": headers.cloned().unwrap_or(Value::Null),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "verified": !self.secret.is_empty(),
        });

        debug!(node = %self.name, event = %event_type, "Webhook event accepted");
        let out = msg.derive(msg.source_node, "payload", payload);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
            self.path = path.to_string();
        }
        if let Some(method) = config.get("method").and_then(|v| v.as_str()) {
            self.method = method.to_uppercase();
        }
        if let Some(secret) = config.get("secret").and_then(|v| v.as_str()) {
            self.secret = secret.to_string();
        }
        if let Some(sh) = config.get("signatureHeader").and_then(|v| v.as_str()) {
            self.signature_header = sh.to_string();
        }
        if let Some(eh) = config.get("eventHeader").and_then(|v| v.as_str()) {
            self.event_header = eh.to_string();
        }
        if let Some(events) = config.get("events").and_then(|v| v.as_array()) {
            self.events = events
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.path.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "Webhook node requires a 'path'".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "webhook"
    }
}

pub struct WebhookNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for WebhookNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = WebhookNode {
            name: "Webhook".to_string(),
            path: "/hook".to_string(),
            method: "POST".to_string(),
            secret: String::new(),
            signature_header: "X-Hub-Signature-256".to_string(),
            event_header: String::new(),
            events: vec![],
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "webhook"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hello() {
        let hash = sha256(b"hello");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_hmac_sha256_rfc4231_vector() {
        // Test vector from RFC 4231 - Test Case 2
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let result = hmac_sha256(key, data);
        let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn test_verify_signature() {
        let secret = "mysecret";
        let body = r#"{"action":"push"}"#;
        let mac = hmac_sha256(secret.as_bytes(), body.as_bytes());
        let sig_hex: String = mac.iter().map(|b| format!("{:02x}", b)).collect();
        let signature = format!("sha256={}", sig_hex);

        assert!(verify_hmac_sha256(secret, body, &signature));
        assert!(!verify_hmac_sha256(secret, body, "sha256=deadbeef"));
    }

    #[tokio::test]
    async fn test_webhook_accepts_valid_event() {
        let node = WebhookNode {
            name: "test".into(),
            path: "/github".into(),
            method: "POST".into(),
            secret: String::new(), // No signature check
            signature_header: String::new(),
            event_header: "x-github-event".into(),
            events: vec!["push".into()],
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({
                "req": {
                    "headers": { "x-github-event": "push" },
                    "body": { "ref": "refs/heads/main" }
                }
            }),
            uuid::Uuid::now_v7(),
        );

        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].source_port, "payload");
        assert_eq!(results[0].payload["event"], "push");
    }

    #[tokio::test]
    async fn test_webhook_filters_unwanted_event() {
        let node = WebhookNode {
            name: "test".into(),
            path: "/github".into(),
            method: "POST".into(),
            secret: String::new(),
            signature_header: String::new(),
            event_header: "x-github-event".into(),
            events: vec!["push".into()],
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({
                "req": {
                    "headers": { "x-github-event": "star" },
                    "body": {}
                }
            }),
            uuid::Uuid::now_v7(),
        );

        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].source_port, "filtered");
    }
}
