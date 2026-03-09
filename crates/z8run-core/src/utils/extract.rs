//! Shared text/data extraction utilities for z8run-core nodes.
//!
//! Replaces 10+ duplicated `extract_text` implementations across
//! LLM, TTS, STT, embeddings, classifier, summarizer, etc.

use serde_json::Value;

/// Default field names to search for text content.
pub const TEXT_FIELDS: &[&str] = &["text", "input", "content", "prompt", "body", "message"];

/// Default field names to search for prompts (prioritizes "prompt").
pub const PROMPT_FIELDS: &[&str] = &["prompt", "text", "body", "message", "content", "input"];

/// Extract a text string from a JSON payload by searching common field names.
///
/// Search order:
/// 1. If payload is a string, return it directly.
/// 2. Try each `field_names` key at the top level.
/// 3. Try nested under `req.body.<key>`.
/// 4. Try `req.body` as a string.
/// 5. Return empty string if nothing found.
///
/// # Examples
/// ```ignore
/// let payload = json!({"text": "hello world"});
/// assert_eq!(extract_text(&payload, &TEXT_FIELDS), "hello world");
/// ```
pub fn extract_text(payload: &Value, field_names: &[&str]) -> String {
    // Direct string payload
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }

    // Try top-level field names
    for key in field_names {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }

    // Try nested: req.body.<key>
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in field_names {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        // If body itself is a string
        if let Some(s) = body.as_str() {
            return s.to_string();
        }
    }

    String::new()
}

/// Extract a specific named field from a payload, trying multiple alternative names.
///
/// Useful for extracting phone numbers, audio data, etc.
pub fn extract_field(payload: &Value, field_names: &[&str]) -> String {
    for field_name in field_names {
        if let Some(s) = payload.get(field_name).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_direct_string() {
        let payload = json!("hello world");
        assert_eq!(extract_text(&payload, TEXT_FIELDS), "hello world");
    }

    #[test]
    fn test_extract_top_level_field() {
        let payload = json!({"text": "hello"});
        assert_eq!(extract_text(&payload, TEXT_FIELDS), "hello");
    }

    #[test]
    fn test_extract_nested_req_body() {
        let payload = json!({"req": {"body": {"text": "nested"}}});
        assert_eq!(extract_text(&payload, TEXT_FIELDS), "nested");
    }

    #[test]
    fn test_extract_req_body_string() {
        let payload = json!({"req": {"body": "raw string body"}});
        assert_eq!(extract_text(&payload, TEXT_FIELDS), "raw string body");
    }

    #[test]
    fn test_extract_empty_when_not_found() {
        let payload = json!({"unrelated": 42});
        assert_eq!(extract_text(&payload, TEXT_FIELDS), "");
    }

    #[test]
    fn test_extract_field_simple() {
        let payload = json!({"phone": "+1234567890", "number": "+0987654321"});
        assert_eq!(
            extract_field(&payload, &["phone", "number", "to"]),
            "+1234567890"
        );
    }

    #[test]
    fn test_extract_with_prompt_fields() {
        let payload = json!({"prompt": "translate this"});
        assert_eq!(extract_text(&payload, PROMPT_FIELDS), "translate this");
    }
}
