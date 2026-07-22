//! HTTP Request node: makes outbound HTTP calls to external APIs.
//!
//! Supports all HTTP methods, custom headers, body extraction from
//! the incoming message payload, URL template interpolation, and
//! configurable timeout.
//!
//! Outputs:
//!   - "response" port: successful HTTP response (any status code)
//!   - "error" port: network/timeout/parse errors

use super::switch::json_path_lookup;
use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::node_helpers::require_non_empty;
use tracing::{info, warn};

/// Regex for `{path.to.field}` placeholders in URLs.
fn resolve_template(template: &str, data: &serde_json::Value) -> String {
    let re = regex::Regex::new(r"\{([^}]+)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let path = &caps[1];
        let val = json_path_lookup(data, path);
        match &val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string().trim_matches('"').to_string(),
        }
    })
    .to_string()
}

/// Returns a log-safe version of a URL for server-side logging: keeps only
/// scheme + host + path, dropping the query string, fragment, and any
/// `user:pass@` userinfo so tokens/credentials/PII don't leak into logs.
fn sanitize_url(url: &str) -> String {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    match without_query.split_once("://") {
        Some((scheme, rest)) => {
            let (authority, path) = match rest.split_once('/') {
                Some((a, p)) => (a, Some(p)),
                None => (rest, None),
            };
            // Strip `user:pass@` userinfo from the authority component.
            let host = authority
                .rsplit_once('@')
                .map(|(_, h)| h)
                .unwrap_or(authority);
            match path {
                Some(p) => format!("{scheme}://{host}/{p}"),
                None => format!("{scheme}://{host}"),
            }
        }
        None => without_query.to_string(),
    }
}

pub struct HttpRequestNode {
    name: String,
    url: String,
    method: String,
    headers: serde_json::Value,
    body_path: String,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for HttpRequestNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Resolve URL templates: e.g. "https://api.example.com/{req.body.id}"
        let resolved_url = resolve_template(&self.url, &msg.payload);

        info!(
            node = %self.name,
            method = %self.method,
            url = %sanitize_url(&resolved_url),
            "HTTP Request outbound"
        );

        let client = reqwest::Client::new();

        // Build request with method
        let mut request = match self.method.as_str() {
            "POST" => client.post(&resolved_url),
            "PUT" => client.put(&resolved_url),
            "PATCH" => client.patch(&resolved_url),
            "DELETE" => client.delete(&resolved_url),
            "HEAD" => client.head(&resolved_url),
            _ => client.get(&resolved_url),
        };

        // Set timeout
        request = request.timeout(std::time::Duration::from_millis(self.timeout_ms));

        // Add custom headers from config
        if let serde_json::Value::Object(headers) = &self.headers {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    request = request.header(key.as_str(), val_str);
                }
            }
        }

        // Extract body from the incoming message payload using body_path
        if !self.body_path.is_empty() && self.method != "GET" && self.method != "HEAD" {
            let body_value = json_path_lookup(&msg.payload, &self.body_path);
            if !body_value.is_null() {
                request = request
                    .header("Content-Type", "application/json")
                    .json(&body_value);
            }
        }

        // Execute the request
        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let resp_headers: serde_json::Map<String, serde_json::Value> = response
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .to_str()
                            .ok()
                            .map(|v| (name.to_string(), serde_json::Value::String(v.to_string())))
                    })
                    .collect();

                // Parse response body as JSON, fallback to string
                let body_text = response.text().await.unwrap_or_default();
                let body_json: serde_json::Value =
                    serde_json::from_str(&body_text).unwrap_or(if body_text.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(body_text)
                    });

                info!(
                    node = %self.name,
                    status = status,
                    url = %sanitize_url(&resolved_url),
                    "HTTP Request completed"
                );

                let payload = serde_json::json!({
                    "status": status,
                    "headers": resp_headers,
                    "body": body_json,
                    "url": resolved_url,
                });

                let out = msg.derive(msg.source_node, "response", payload);
                Ok(vec![out])
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    error = %e,
                    url = %sanitize_url(&resolved_url),
                    "HTTP Request failed"
                );

                let is_timeout = e.is_timeout();
                let error_payload = serde_json::json!({
                    "error": e.to_string(),
                    "url": resolved_url,
                    "timeout": is_timeout,
                });

                let out = msg.derive(msg.source_node, "error", error_payload);
                Ok(vec![out])
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "url" => url: str,
            "method" => method: str_upper,
            "bodyPath" => body_path: str,
            "timeout" => timeout_ms: u64,
            "headers" => headers: value,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.url, "HTTP Request node requires a URL")?;
        Ok(())
    }

    fn node_type(&self) -> &str {
        "http-request"
    }
}

node_factory!(HttpRequestNodeFactory, HttpRequestNode, "http-request", {
    name: "HTTP Request".to_string(),
    url: String::new(),
    method: "GET".to_string(),
    headers: serde_json::json!({}),
    body_path: "req.body".to_string(),
    timeout_ms: 5000
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_template() {
        let data = serde_json::json!({
            "req": {
                "body": { "id": 42, "name": "test" },
                "query": { "city": "London" }
            }
        });

        let url = "https://api.example.com/users/{req.body.id}?city={req.query.city}";
        let resolved = resolve_template(url, &data);
        assert_eq!(resolved, "https://api.example.com/users/42?city=London");
    }

    #[test]
    fn test_resolve_template_missing_field() {
        let data = serde_json::json!({"req": {}});
        let url = "https://api.example.com/{req.body.id}";
        let resolved = resolve_template(url, &data);
        assert_eq!(resolved, "https://api.example.com/");
    }

    #[test]
    fn test_resolve_template_no_placeholders() {
        let data = serde_json::json!({});
        let url = "https://api.example.com/static";
        let resolved = resolve_template(url, &data);
        assert_eq!(resolved, "https://api.example.com/static");
    }

    #[test]
    fn test_sanitize_url_drops_query() {
        let url = "https://api.example.com/users/42?token=secret&city=London";
        assert_eq!(sanitize_url(url), "https://api.example.com/users/42");
    }

    #[test]
    fn test_sanitize_url_drops_userinfo() {
        let url = "https://user:pass@api.example.com/path?x=1";
        assert_eq!(sanitize_url(url), "https://api.example.com/path");
    }

    #[test]
    fn test_sanitize_url_no_path_or_query() {
        let url = "https://api.example.com";
        assert_eq!(sanitize_url(url), "https://api.example.com");
    }
}
