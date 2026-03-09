//! Shared HTTP response handling utilities.
//!
//! Replaces 28+ duplicated patterns for reading and validating HTTP responses.

/// Resolve an API base URL with a default fallback and endpoint path.
///
/// Replaces the repeated pattern:
/// ```ignore
/// let base = if self.base_url.is_empty() { "https://api.openai.com/v1" } else { &self.base_url };
/// let url = format!("{}/chat/completions", base);
/// ```
///
/// Usage:
/// ```ignore
/// let url = resolve_api_url(&self.base_url, "https://api.openai.com/v1", "/chat/completions");
/// ```
pub fn resolve_api_url(base_url: &str, default_base: &str, endpoint: &str) -> String {
    let base = if base_url.is_empty() {
        default_base
    } else {
        base_url
    };
    format!("{}{}", base.trim_end_matches('/'), endpoint)
}

/// Read an HTTP response, checking for non-200 status codes.
///
/// Returns `(status, body_text)` on success, or an error string on failure.
///
/// Replaces the repeated pattern:
/// ```ignore
/// let status = resp.status().as_u16();
/// let text = resp.text().await.map_err(|e| format!("Read error: {}", e))?;
/// if status != 200 {
///     return Err(format!("API error ({}): {}", status, text));
/// }
/// ```
pub async fn check_response(resp: reqwest::Response, context: &str) -> Result<String, String> {
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("{} read error: {}", context, e))?;
    if status != 200 {
        return Err(format!("{} API error ({}): {}", context, status, text));
    }
    Ok(text)
}

/// Read an HTTP response and parse as JSON, with error handling.
///
/// Combines `check_response` + JSON parsing into one call.
///
/// Usage:
/// ```ignore
/// let json = check_json_response(resp, "OpenAI").await?;
/// ```
pub async fn check_json_response(
    resp: reqwest::Response,
    context: &str,
) -> Result<serde_json::Value, String> {
    let text = check_response(resp, context).await?;
    serde_json::from_str(&text).map_err(|e| format!("{} parse error: {}", context, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_api_url_default() {
        let url = resolve_api_url("", "https://api.openai.com/v1", "/chat/completions");
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_resolve_api_url_custom() {
        let url = resolve_api_url(
            "https://custom.api.com/v2",
            "https://api.openai.com/v1",
            "/chat/completions",
        );
        assert_eq!(url, "https://custom.api.com/v2/chat/completions");
    }

    #[test]
    fn test_resolve_api_url_trailing_slash() {
        let url = resolve_api_url(
            "https://custom.api.com/v2/",
            "https://api.openai.com/v1",
            "/messages",
        );
        assert_eq!(url, "https://custom.api.com/v2/messages");
    }

    #[test]
    fn test_resolve_api_url_ollama() {
        let url = resolve_api_url("", "http://localhost:11434", "/api/chat");
        assert_eq!(url, "http://localhost:11434/api/chat");
    }
}
