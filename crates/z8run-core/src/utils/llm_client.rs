//! Shared LLM client for simple text completion calls.
//!
//! Used by classifier, structured_output, and summarizer nodes.
//! Supports OpenAI-compatible, Anthropic, and Ollama providers.
//!
//! NOTE: The main LLM node (`nodes/llm.rs`) uses its own implementation
//! because it needs streaming, vision, and custom event emitter support.

use crate::utils::http_helpers::{check_json_response, resolve_api_url};

/// Parameters for a simple LLM text completion call.
pub struct LlmCallParams<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub max_tokens: u64,
    pub temperature: f64,
    pub timeout: std::time::Duration,
}

/// Call an LLM provider and return the text response.
///
/// Replaces the identical `call_llm()` functions in classifier.rs,
/// structured_output.rs, and summarizer.rs (~130 lines each).
///
/// # Providers
/// - `"openai"` (default): OpenAI-compatible API (also works with Azure, Groq, etc.)
/// - `"anthropic"`: Anthropic Messages API
/// - `"ollama"`: Local Ollama instance
pub async fn call_llm(
    client: &reqwest::Client,
    params: &LlmCallParams<'_>,
) -> Result<String, String> {
    match params.provider {
        "anthropic" => call_anthropic(client, params).await,
        "ollama" => call_ollama(client, params).await,
        _ => call_openai(client, params).await,
    }
}

async fn call_openai(client: &reqwest::Client, p: &LlmCallParams<'_>) -> Result<String, String> {
    let url = resolve_api_url(p.base_url, "https://api.openai.com/v1", "/chat/completions");

    let body = serde_json::json!({
        "model": p.model,
        "messages": [
            {"role": "system", "content": p.system_prompt},
            {"role": "user", "content": p.user_prompt},
        ],
        "temperature": p.temperature,
        "max_tokens": p.max_tokens,
    });

    let resp = client
        .post(&url)
        .bearer_auth(p.api_key)
        .header("Content-Type", "application/json")
        .timeout(p.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI request failed: {}", e))?;

    let json = check_json_response(resp, "OpenAI").await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

async fn call_anthropic(client: &reqwest::Client, p: &LlmCallParams<'_>) -> Result<String, String> {
    let url = resolve_api_url(p.base_url, "https://api.anthropic.com/v1", "/messages");

    let body = serde_json::json!({
        "model": p.model,
        "max_tokens": p.max_tokens,
        "system": p.system_prompt,
        "messages": [{"role": "user", "content": p.user_prompt}],
    });

    let resp = client
        .post(&url)
        .header("x-api-key", p.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .timeout(p.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Anthropic request failed: {}", e))?;

    let json = check_json_response(resp, "Anthropic").await?;
    Ok(json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

async fn call_ollama(client: &reqwest::Client, p: &LlmCallParams<'_>) -> Result<String, String> {
    let url = resolve_api_url(p.base_url, "http://localhost:11434", "/api/chat");

    let body = serde_json::json!({
        "model": p.model,
        "messages": [
            {"role": "system", "content": p.system_prompt},
            {"role": "user", "content": p.user_prompt},
        ],
        "stream": false,
        "options": {"temperature": p.temperature, "num_predict": p.max_tokens},
    });

    let resp = client
        .post(&url)
        .timeout(p.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    let json = check_json_response(resp, "Ollama").await?;
    Ok(json["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}
